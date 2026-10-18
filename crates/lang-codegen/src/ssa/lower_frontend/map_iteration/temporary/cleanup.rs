//! Exact temporary owner, lexical Guard and complete point sequences from Phase 3.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupOwnerValue, DropFact, DropTarget, IterationPhiBoundary, IterationPhiIncomingKind,
};

pub(super) fn prove(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    body: StatementId,
    kind: IterationExitKind,
    guard: Option<nested::tail::Local>,
    span: Span,
) -> Result<source_owner::Proof, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    let IterationSource::BorrowedSource(LoanTarget::Temporary(source)) = plan.source_kind() else {
        return Err(missing());
    };
    let source = *source;
    let origin = parsed
        .ast()
        .expressions()
        .get(source)
        .map_err(|_| missing())?
        .span();
    // Keep every pre-existing normal qualification and its U/M classification unchanged.
    if kind == IterationExitKind::Fallthrough {
        return normal(owned, plan, body, origin, span);
    }
    let body_point = exit_shape::point(kind, statement, body);
    let exhaustion_point = DropPoint::LoopExit(statement);
    if !plan.closure_phis().is_empty()
        || !plan.closure_flow().header().is_empty()
        || !plan.closure_flow().exit().is_empty()
        || !plan.capture_graph().nodes().is_empty()
    {
        return Err(unsupported());
    }
    let conditions = owned.cleanup_conditions();
    let mut incomings = vec![(
        IterationPhiIncomingKind::Entry,
        DropPoint::AfterExpression(descriptor.source()),
        IterationPhiBoundary::Header,
    )];
    match kind {
        IterationExitKind::Fallthrough => incomings.push((
            IterationPhiIncomingKind::Fallthrough,
            body_point,
            IterationPhiBoundary::Header,
        )),
        IterationExitKind::Continue(expression) => incomings.push((
            IterationPhiIncomingKind::Continue(expression),
            body_point,
            IterationPhiBoundary::Header,
        )),
        IterationExitKind::Break(expression) => incomings.push((
            IterationPhiIncomingKind::Break(expression),
            body_point,
            IterationPhiBoundary::Exit,
        )),
        // Return leaves the function and has no boundary incoming.
        IterationExitKind::Return(_) => {}
        IterationExitKind::Exhaustion => return Err(missing()),
    }
    incomings.push((
        IterationPhiIncomingKind::Exhaustion,
        exhaustion_point,
        IterationPhiBoundary::Exit,
    ));
    for &(expected_kind, point, boundary) in &incomings {
        let incoming = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|incoming| incoming.kind() == expected_kind),
        )
        .ok_or_else(missing)?;
        if incoming.point() != point
            || incoming.boundary() != boundary
            || conditions.get(incoming.condition()) != Some(&CleanupCondition::Always)
            || !incoming.bindings().is_empty()
        {
            return Err(missing());
        }
    }
    if plan.closure_phi_incomings().len() != incomings.len() {
        return Err(missing());
    }
    let source_drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == DropTarget::Temporary(source))
        .collect::<Vec<_>>();
    let drop = unique(
        source_drops
            .iter()
            .copied()
            .filter(|fact| fact.point() == exhaustion_point),
    )
    .ok_or_else(missing)?;
    let owner = drop.owner().ok_or_else(missing)?;
    if conditions.owner_value(owner)
        != Some(&CleanupOwnerValue::Expression {
            expression: source,
            origin,
        })
        || source_drops.iter().any(|fact| {
            fact.owner() != Some(owner) || fact.value_origin() != origin || !ordinary(*fact)
        })
    {
        return Err(missing());
    }
    let terminal_body = exit_shape::terminal(kind);
    let transfer = if terminal_body {
        Some(
            unique(
                source_drops
                    .iter()
                    .copied()
                    .filter(|fact| fact.point() == body_point),
            )
            .ok_or_else(missing)?,
        )
    } else {
        None
    };
    if source_drops.iter().any(|fact| {
        fact.point() != exhaustion_point && (!terminal_body || fact.point() != body_point)
    }) {
        return Err(missing());
    }
    if source_drops.len() != 1 + usize::from(terminal_body)
        || owned
            .drops()
            .iter()
            .filter(|fact| fact.owner() == Some(owner))
            .copied()
            .collect::<Vec<_>>()
            != source_drops
    {
        return Err(missing());
    }
    let local_drop = guard
        .map(|guard| guard_drop(owned, guard, body_point, span))
        .transpose()?;
    if (kind == IterationExitKind::Fallthrough) != guard.is_none()
        || local_drop.is_some_and(|fact| fact.owner() == Some(owner))
    {
        return Err(missing());
    }
    exit_shape::cleanup(owned, plan, body, kind, span)?;
    let terminal = vec![
        IterationCleanupAction::FinishProvider(statement),
        IterationCleanupAction::EndSource(statement),
        IterationCleanupAction::Drop(drop),
    ];
    let mut body_actions = local_drop
        .into_iter()
        .map(IterationCleanupAction::Drop)
        .collect::<Vec<_>>();
    body_actions.extend([
        IterationCleanupAction::EndBinding {
            statement,
            symbol: descriptor.value_binding(),
        },
        IterationCleanupAction::EndBinding {
            statement,
            symbol: descriptor.key_binding(),
        },
    ]);
    if let Some(transfer) = transfer {
        body_actions.extend([
            IterationCleanupAction::FinishProvider(statement),
            IterationCleanupAction::EndSource(statement),
            IterationCleanupAction::Drop(transfer),
        ]);
    }
    for (expected_kind, point, actions) in [
        (kind, body_point, body_actions),
        (IterationExitKind::Exhaustion, exhaustion_point, terminal),
    ] {
        let exit = unique(
            plan.exits()
                .iter()
                .filter(|exit| exit.kind() == expected_kind),
        )
        .ok_or_else(missing)?;
        if exit.actions() != actions {
            // Preserve the pre-existing normal slice's no-local-cleanup capability boundary.
            if kind == IterationExitKind::Fallthrough && expected_kind == kind {
                return Err(unsupported());
            }
            return Err(missing());
        }
        let expected = actions
            .iter()
            .filter_map(|action| match action {
                IterationCleanupAction::Drop(fact) => Some(*fact),
                _ => None,
            })
            .collect::<Vec<_>>();
        let actual = owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        if actual.len() != expected.len() || expected.iter().any(|fact| !actual.contains(fact)) {
            return Err(missing());
        }
    }
    Ok(source_owner::Proof {
        drop,
        transfer,
        conditional_return: None,
    })
}

fn ordinary(fact: DropFact) -> bool {
    fact.condition().is_none() && fact.capture_slot().is_none() && fact.instance_address().is_none()
}

fn guard_drop(
    owned: &OwnershipCheckedFile,
    guard: nested::tail::Local,
    point: DropPoint,
    span: Span,
) -> Result<DropFact, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let fact = unique(
        owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == DropTarget::Named(guard.symbol)),
    )
    .ok_or_else(missing)?;
    let owner = fact.owner().ok_or_else(missing)?;
    if fact.point() != point
        || fact.value_origin() != guard.origin
        || !ordinary(fact)
        || owned.cleanup_conditions().owner_value(owner)
            != Some(&CleanupOwnerValue::Expression {
                expression: guard.initializer,
                origin: guard.initializer_span,
            })
        || !matches!(owned.drops().iter().filter(|fact| fact.owner() == Some(owner))
            .collect::<Vec<_>>().as_slice(), [actual] if **actual == fact)
    {
        return Err(missing());
    }
    Ok(fact)
}

impl ExpressionLowerer<'_> {
    /// The admitted local must still be bound and must emit its exact Drop once.
    /// The general ordinary-drop path may skip absent locals; this bounded path must not.
    pub(in super::super) fn emit_temporary_map_local_drop(
        &mut self,
        fact: DropFact,
        point: DropPoint,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let DropTarget::Named(symbol) = fact.target() else {
            return Err(missing());
        };
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(missing());
        };
        let Some(CleanupOwnerValue::Expression { expression, origin }) = fact
            .owner()
            .and_then(|owner| self.owned.cleanup_conditions().owner_value(owner))
        else {
            return Err(missing());
        };
        let construction = self.typed.construction(*expression).ok_or_else(missing)?;
        let ty = self.expression_ssa_type(*expression, span)?;
        let resolved = self.resolve_type(construction.result_type(), span)?;
        let birth =
            carrier::definition(self.function, EntityId::Value(owner)).ok_or_else(missing)?;
        let Operation::HeapAllocate {
            owner: owner_type,
            payload,
        } = birth.operation
        else {
            return Err(missing());
        };
        let payload =
            carrier::definition(self.function, EntityId::Value(payload)).ok_or_else(missing)?;
        let aggregate = self.heap_payloads.get(&ty).ok_or_else(missing)?;
        if fact.point() != point
            || !ordinary(fact)
            || construction.expression() != *expression
            || Some(construction.result_type()) != self.typed.symbol_type(symbol)
            || birth.block != self.block
            || birth.origin != Origin::Source(*origin)
            || owner_type != ty
            || self.type_ids.get(&resolved) != Some(&ty)
            || self
                .function
                .entity(EntityId::Value(owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(ty))
            || payload.block != self.block
            || payload.origin != birth.origin
            || !matches!(&payload.operation, Operation::AggregateConstruct { aggregate: actual, fields }
                if actual == aggregate && fields.is_empty())
            || unique(self.function.instructions.iter().filter(|instruction| {
                instruction.origin == birth.origin
                    && matches!(instruction.operation, Operation::HeapAllocate { .. })
            }))
            .map(|instruction| instruction.id)
                != Some(birth.id)
        {
            return Err(missing());
        }
        let before = self
            .function
            .block(self.block)
            .ok_or_else(missing)?
            .instructions
            .len();
        self.emit_iteration_cleanup_action(&IterationCleanupAction::Drop(fact), span)?;
        let block = self.function.block(self.block).ok_or_else(missing)?;
        let [instruction] = block.instructions.get(before..).ok_or_else(missing)? else {
            return Err(missing());
        };
        let instruction = self
            .function
            .instruction(*instruction)
            .ok_or_else(missing)?;
        if instruction.operation != (Operation::Drop { owner })
            || instruction.origin != Origin::Source(fact.value_origin())
            || self.bindings.contains_key(&symbol)
            || block
                .instructions
                .iter()
                .filter(|id| {
                    self.function.instruction(**id).is_some_and(|instruction| {
                        instruction.operation == (Operation::Drop { owner })
                    })
                })
                .count()
                != 1
        {
            return Err(missing());
        }
        Ok(())
    }
}

// The prior normal temporary proof is preserved verbatim below.
fn normal(
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    body: StatementId,
    origin: Span,
    span: Span,
) -> Result<source_owner::Proof, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    let IterationSource::BorrowedSource(LoanTarget::Temporary(source)) = plan.source_kind() else {
        return Err(missing());
    };
    let source = *source;
    if !plan.closure_phis().is_empty()
        || !plan.closure_flow().header().is_empty()
        || !plan.closure_flow().exit().is_empty()
        || !plan.capture_graph().nodes().is_empty()
    {
        return Err(unsupported());
    }
    let conditions = owned.cleanup_conditions();
    let incoming_shapes = [
        (
            IterationPhiIncomingKind::Entry,
            DropPoint::AfterExpression(descriptor.source()),
            IterationPhiBoundary::Header,
        ),
        (
            IterationPhiIncomingKind::Fallthrough,
            DropPoint::AfterStatement(body),
            IterationPhiBoundary::Header,
        ),
        (
            IterationPhiIncomingKind::Exhaustion,
            DropPoint::LoopExit(statement),
            IterationPhiBoundary::Exit,
        ),
    ];
    for (kind, point, boundary) in incoming_shapes {
        let incoming = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|incoming| incoming.kind() == kind),
        )
        .ok_or_else(missing)?;
        if incoming.point() != point
            || incoming.boundary() != boundary
            || conditions.get(incoming.condition()) != Some(&CleanupCondition::Always)
            || !incoming.bindings().is_empty()
        {
            return Err(missing());
        }
    }
    if plan.closure_phi_incomings().len() != incoming_shapes.len() {
        return Err(unsupported());
    }
    let drop = unique(
        owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == DropTarget::Temporary(source)),
    )
    .ok_or_else(missing)?;
    if drop.point() != DropPoint::LoopExit(statement)
        || drop.value_origin() != origin
        || drop.condition().is_some()
        || drop.capture_slot().is_some()
        || drop.instance_address().is_some()
    {
        return Err(missing());
    }
    let owner = drop.owner().ok_or_else(missing)?;
    if conditions.owner_value(owner)
        != Some(&CleanupOwnerValue::Expression {
            expression: source,
            origin,
        })
        || owned
            .drops()
            .iter()
            .any(|fact| fact.owner() == Some(owner) && *fact != drop)
    {
        return Err(missing());
    }
    exit_shape::cleanup(owned, plan, body, IterationExitKind::Fallthrough, span)?;
    let body_plan = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Fallthrough),
    )
    .ok_or_else(missing)?;
    let exhaustion = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
    )
    .ok_or_else(missing)?;
    let body_actions = [
        IterationCleanupAction::EndBinding {
            statement,
            symbol: descriptor.value_binding(),
        },
        IterationCleanupAction::EndBinding {
            statement,
            symbol: descriptor.key_binding(),
        },
    ];
    // Body-local owner combinations need their own first evidence. Do not silently
    // extend this temporary source slice merely because generic straight-line AST fits.
    if body_plan.actions() != body_actions {
        return Err(unsupported());
    }
    if exhaustion.actions()
        != [
            IterationCleanupAction::FinishProvider(statement),
            IterationCleanupAction::EndSource(statement),
            IterationCleanupAction::Drop(drop),
        ]
    {
        return Err(missing());
    }
    Ok(source_owner::Proof {
        drop,
        transfer: None,
        conditional_return: None,
    })
}
