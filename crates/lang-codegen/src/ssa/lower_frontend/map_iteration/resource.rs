//! An owned Resource Map retains its real parameter or formed birth through lexical cleanup.
//! Presence is proved from Parameter -> Header -> Exit incoming values, never a bit alone.
use super::*;
use lang_frontend::{
    ownership_checking::{
        CleanupCondition, CleanupOwnerValue, DropFact, DropTarget, IterationPhiBoundary,
        IterationPhiIncomingKind,
    },
    parser::{FunctionBody, FunctionForm},
};

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    root: (
        SymbolId,
        Option<&crate::ssa::map_source::Formation<SymbolId>>,
    ),
    body_exit: IterationExitKind,
) -> Result<source_owner::Proof, LoweringError> {
    let (symbol, birth) = root;
    let d = plan.descriptor();
    let span = parsed
        .ast()
        .statements()
        .get(d.statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?
        .span();
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let origin = names.symbols().get(symbol.index()).ok_or_else(bad)?.span();
    // Source ownership depends on validated Resource classification and its complete
    // cleanup graph. Existing storage planning owns payload/key representation admission.
    if !matches!(
        body_exit,
        IterationExitKind::Fallthrough
            | IterationExitKind::Continue(_)
            | IterationExitKind::Break(_)
            | IterationExitKind::Return(_)
    ) || plan.source_access() != MapIterationSourceAccess::Owned
        || typed.is_resource_type(d.source_type()) != Some(true)
    {
        return Err(unsupported());
    }
    let scope = scope(parsed, d.statement(), origin, span)?;
    let drops = if let Some(formation) = birth {
        formed::drops(parsed, owned, symbol, formation)?
    } else {
        owned
            .drops()
            .iter()
            .copied()
            .filter(|f| f.target() == DropTarget::Named(symbol))
            .collect::<Vec<_>>()
    };
    let returning = match body_exit {
        IterationExitKind::Return(expression) => Some(expression),
        _ => None,
    };
    let transfer = returning
        .map(|expression| {
            let node = parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| bad())?;
            if !matches!(node.payload(), Expression::Return { .. })
                || node.span().source_id() != span.source_id()
                || node.span().start() <= span.start()
                || node.span().end() >= span.end()
            {
                return Err(bad());
            }
            unique(
                drops
                    .iter()
                    .copied()
                    .filter(|d| d.point() == DropPoint::ControlTransfer(expression)),
            )
            .ok_or_else(bad)
        })
        .transpose()?;
    if drops.len() != 1 + usize::from(returning.is_some()) {
        return Err(unsupported());
    }
    let drop = unique(drops.iter().copied().filter(|d| Some(*d) != transfer)).ok_or_else(bad)?;
    let replacement = birth.and_then(|f| f.replacement);
    if !normal_point(parsed, scope, span, drop.point())
        && !matches!((replacement, drop.point()), (Some(expected), DropPoint::AfterExpression(actual)) if expected == actual)
    {
        return Err(bad());
    }
    let conditions = owned.cleanup_conditions();
    if plan.closure_phis().len() != 2
        || plan.closure_phi_incomings().len() != if returning.is_some() { 2 } else { 3 }
        || !plan.capture_graph().nodes().is_empty()
    {
        return Err(unsupported());
    }
    let h = source_owner::phi(conditions, plan, symbol, IterationPhiBoundary::Header, span)?;
    let x = source_owner::phi(conditions, plan, symbol, IterationPhiBoundary::Exit, span)?;
    if h.owner() == x.owner() || h.availability_selector() == x.availability_selector() {
        return Err(bad());
    }
    let entry = source_owner::incoming(
        conditions,
        plan,
        h,
        IterationPhiIncomingKind::Entry,
        DropPoint::AfterExpression(d.source()),
        span,
    )?;
    let [seed] = entry.values() else {
        return Err(bad());
    };
    let available = match birth {
        Some(birth) => source_owner::formed_condition(
            parsed,
            owned,
            (symbol, birth.producer),
            seed.source(),
            span,
        )?,
        None => {
            if conditions.get(entry.available_when()) != Some(&CleanupCondition::Always)
                || !matches!(conditions.owner_value(seed.source()), Some(CleanupOwnerValue::Parameter { symbol: s, origin: o }) if *s == symbol && *o == origin)
            {
                return Err(bad());
            }
            entry.available_when()
        }
    };
    if seed.condition() != entry.available_when() || seed.condition() != available {
        return Err(bad());
    }
    let mut incoming_edges = vec![(
        x,
        IterationPhiIncomingKind::Exhaustion,
        DropPoint::LoopExit(d.statement()),
    )];
    if returning.is_none() {
        let backedge =
            unique(plan.exits().iter().filter(|e| e.kind() == body_exit)).ok_or_else(bad)?;
        incoming_edges.push((
            if matches!(body_exit, IterationExitKind::Break(_)) {
                x
            } else {
                h
            },
            match body_exit {
                IterationExitKind::Continue(expression) => {
                    IterationPhiIncomingKind::Continue(expression)
                }
                IterationExitKind::Break(expression) => IterationPhiIncomingKind::Break(expression),
                _ => IterationPhiIncomingKind::Fallthrough,
            },
            backedge.point(),
        ));
    }
    for (phi, kind, point) in incoming_edges {
        let incoming = source_owner::incoming(conditions, plan, phi, kind, point, span)?;
        let [value] = incoming.values() else {
            return Err(bad());
        };
        if value.source() != h.owner()
            || value.condition() != h.availability_condition()
            || incoming.available_when() != h.availability_condition()
        {
            return Err(bad());
        }
    }
    if let Some(fact) = transfer {
        if fact.owner() != Some(h.owner()) || fact.condition() != Some(h.availability_condition()) {
            return Err(bad());
        }
        let exit = unique(plan.exits().iter().filter(|e| e.kind() == body_exit)).ok_or_else(bad)?;
        let suffix = [
            IterationCleanupAction::EndBinding {
                statement: d.statement(),
                symbol: d.value_binding(),
            },
            IterationCleanupAction::EndBinding {
                statement: d.statement(),
                symbol: d.key_binding(),
            },
            IterationCleanupAction::FinishProvider(d.statement()),
            IterationCleanupAction::EndSource(d.statement()),
            IterationCleanupAction::Drop(fact),
        ];
        if exit.point() != fact.point() || !exit.actions().ends_with(&suffix)
            || exit.actions()[..exit.actions().len()-suffix.len()].iter().any(|a|
                !matches!(a, IterationCleanupAction::Drop(local) if local.target()!=DropTarget::Named(symbol)))
        { return Err(bad()); }
    }
    let related = owned
        .drops()
        .iter()
        .copied()
        .filter(|f| {
            f.owner()
                .is_some_and(|o| [seed.source(), h.owner(), x.owner()].contains(&o))
                || drops.contains(f)
        })
        .collect::<Vec<_>>();
    let (drop_owner, drop_condition) = if let Some(birth) = birth {
        source_owner::formed_condition(parsed, owned, (symbol, birth.producer), h.owner(), span)?;
        source_owner::formed_condition(parsed, owned, (symbol, birth.producer), x.owner(), span)?;
        let owner = drop.owner().ok_or_else(bad)?;
        let condition =
            source_owner::formed_condition(parsed, owned, (symbol, birth.producer), owner, span)?;
        (owner, condition)
    } else {
        (x.owner(), x.availability_condition())
    };
    if related != drops
        || drop.value_origin() != origin
        || drop.owner() != Some(drop_owner)
        || drop.condition() != Some(drop_condition)
        || drops.iter().any(|d| {
            d.value_origin() != origin
                || d.capture_slot().is_some()
                || d.instance_address().is_some()
        })
    {
        return Err(bad());
    }
    Ok(source_owner::Proof {
        flow: None,
        drop,
        transfer,
        conditional_return: None,
    })
}

// Resolve the callable scope independently of the number or kind of continuation statements.
pub(super) fn scope(
    parsed: &ParsedFile,
    statement: StatementId,
    origin: Span,
    span: Span,
) -> Result<StatementId, LoweringError> {
    let bad = || error(LoweringErrorKind::UnsupportedNode, span);
    let bodies = parsed
        .roots()
        .iter()
        .filter_map(|id| {
            let item = parsed.ast().items().get(*id).ok()?;
            if item.span().source_id() != origin.source_id()
                || item.span().start() > origin.start()
                || item.span().end() < origin.end()
            {
                return None;
            }
            match item.payload() {
                Item::Function {
                    form:
                        FunctionForm::ImplicitUnitBlock(b)
                        | FunctionForm::Explicit {
                            body: FunctionBody::Block(b),
                            ..
                        },
                    ..
                } => Some(*b),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    let [body] = bodies.as_slice() else {
        return Err(bad());
    };
    // The validated Drop names the actual callable scope. Continuation statements
    // are lowered normally; their AST shape cannot grant or revoke source ownership.
    let node = parsed.ast().statements().get(*body).map_err(|_| bad())?;
    let iteration = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| bad())?;
    if !matches!(node.payload(), Statement::Block { .. })
        || node.span().source_id() != iteration.span().source_id()
        || node.span().start() > iteration.span().start()
        || node.span().end() < iteration.span().end()
    {
        return Err(bad());
    }
    Ok(*body)
}

// The fact selects its real cleanup point; statement count and operand shape are irrelevant.
pub(super) fn normal_point(
    parsed: &ParsedFile,
    scope: StatementId,
    iteration: Span,
    point: DropPoint,
) -> bool {
    if point == DropPoint::AfterStatement(scope) {
        return true;
    }
    let DropPoint::ControlTransfer(expression) = point else {
        return false;
    };
    let Ok(node) = parsed.ast().expressions().get(expression) else {
        return false;
    };
    let Ok(body) = parsed.ast().statements().get(scope) else {
        return false;
    };
    let at = node.span();
    let inside = |outer: Span| {
        outer.source_id() == at.source_id()
            && outer.start() <= at.start()
            && outer.end() >= at.end()
    };
    matches!(node.payload(), Expression::Return { .. })
        && inside(body.span())
        && at.start() >= iteration.end()
        && !parsed
            .ast()
            .expressions()
            .iter()
            .any(|(_, n)| matches!(n.payload(), Expression::Lambda { .. }) && inside(n.span()))
        && !parsed.ast().items().iter().any(|(_, n)| {
            matches!(n.payload(), Item::Function { .. } | Item::Deinit { .. })
                && n.span().start() >= body.span().start()
                && inside(n.span())
        })
}

#[derive(Clone, Copy)]
pub(super) struct Carrier {
    fact: DropFact,
    anchor: ValueId,
}

impl ExpressionLowerer<'_> {
    pub(super) fn retain_resource_map(
        &mut self,
        owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
        seed: EntityId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some((proof, SourceIdentity::Parameter(root), _)) = owner.filter(|(p, _, _)| {
            // Terminal flow Drops have already executed. Only a separately
            // validated late Drop grants the future-lifetime carrier.
            p.flow.is_none_or(|flow| flow.late == Some(p.drop))
                && matches!(
                    p.drop.point(),
                    DropPoint::AfterStatement(_) | DropPoint::ControlTransfer(_)
                )
        }) else {
            return Ok(());
        };
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let EntityId::Value(anchor) = seed else {
            return Err(bad());
        };
        let Some(LoweredValue::Value(current)) = self.bindings.get(&root).copied() else {
            return Err(bad());
        };
        if self.borrow_bindings.contains_key(&root) {
            return Err(bad());
        }
        // Break may merge the retained owner with the independent exhaustion edge.
        // Prove the actual current binding through every incoming to the original seed.
        nested::transport(self.function, EntityId::Value(current), seed, span)?;
        if self
            .map_iteration_state
            .resources
            .insert(
                root,
                Carrier {
                    fact: proof.drop,
                    anchor,
                },
            )
            .is_some()
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(in super::super) fn emit_resource_map_drop(
        &mut self,
        fact: DropFact,
    ) -> Result<bool, LoweringError> {
        let DropTarget::Named(root) = fact.target() else {
            return Ok(false);
        };
        let Some(state) = self.map_iteration_state.resources.get(&root).copied() else {
            return Ok(false);
        };
        let bad = || error(LoweringErrorKind::MissingFact, fact.value_origin());
        let Some(LoweredValue::Value(current)) = self.bindings.get(&root).copied() else {
            return Err(bad());
        };
        if state.fact != fact
            || self.borrow_bindings.contains_key(&root)
            || self.temporaries.values().any(|v| *v == current)
        {
            return Err(bad());
        }
        nested::transport(
            self.function,
            EntityId::Value(current),
            EntityId::Value(state.anchor),
            fact.value_origin(),
        )?;
        self.bindings.remove(&root);
        self.map_iteration_state.resources.remove(&root);
        self.append(
            Operation::Drop { owner: current },
            Vec::new(),
            fact.value_origin(),
        )?;
        Ok(true)
    }
}
