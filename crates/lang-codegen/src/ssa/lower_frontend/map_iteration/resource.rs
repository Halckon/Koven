//! One normal owned Resource Map parameter, retained to its real callable scope drop.
//! Presence is proved from Parameter -> Header -> Exit incoming values, never a bit alone.
use super::*;
use lang_frontend::{
    ownership_checking::{
        CleanupCondition, CleanupOwnerValue, DropFact, DropTarget, IterationPhiBoundary,
        IterationPhiIncomingKind,
    },
    parser::{FunctionBody, FunctionForm},
    type_checking::NominalKind,
};

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    symbol: SymbolId,
    body_exit: IterationExitKind,
) -> Result<source_owner::Proof, LoweringError> {
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
    let Some(TypeKind::Nominal { nominal, arguments }) = typed.types().get(d.value_type()) else {
        return Err(unsupported());
    };
    if d.kind() != lang_frontend::type_checking::MapIterationKind::Map
        || body_exit != IterationExitKind::Fallthrough
        || typed.is_resource_type(d.source_type()) != Some(true)
        || builtin_type(typed, d.key_type()) != Some(BuiltinType::String)
        || !arguments.is_empty()
        || !typed.nominals().iter().any(|n| {
            n.id() == *nominal
                && n.kind() == NominalKind::Class
                && n.has_deinit()
                && n.fields().is_empty()
                && n.type_parameters().is_empty()
                && n.interfaces().is_empty()
        })
    {
        return Err(unsupported());
    }
    let scope = scope(parsed, d.statement(), origin, span)?;
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|f| f.target() == DropTarget::Named(symbol))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        return Err(unsupported());
    };
    let conditions = owned.cleanup_conditions();
    if plan.closure_phis().len() != 2
        || plan.closure_phi_incomings().len() != 3
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
    if conditions.get(entry.available_when()) != Some(&CleanupCondition::Always)
        || seed.condition() != entry.available_when()
        || !matches!(conditions.owner_value(seed.source()), Some(CleanupOwnerValue::Parameter { symbol: s, origin: o }) if *s == symbol && *o == origin)
    {
        return Err(bad());
    }
    let fallthrough = unique(
        plan.exits()
            .iter()
            .filter(|e| e.kind() == IterationExitKind::Fallthrough),
    )
    .ok_or_else(bad)?;
    for (phi, kind, point) in [
        (
            h,
            IterationPhiIncomingKind::Fallthrough,
            fallthrough.point(),
        ),
        (
            x,
            IterationPhiIncomingKind::Exhaustion,
            DropPoint::LoopExit(d.statement()),
        ),
    ] {
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
    let related = owned
        .drops()
        .iter()
        .copied()
        .filter(|f| {
            f.owner()
                .is_some_and(|o| [seed.source(), h.owner(), x.owner()].contains(&o))
                || f.target() == DropTarget::Named(symbol)
        })
        .collect::<Vec<_>>();
    if related != drops
        || drop.point() != DropPoint::AfterStatement(scope)
        || drop.value_origin() != origin
        || drop.owner() != Some(x.owner())
        || drop.condition() != Some(x.availability_condition())
        || drop.capture_slot().is_some()
        || drop.instance_address().is_some()
    {
        return Err(bad());
    }
    Ok(source_owner::Proof {
        drop: *drop,
        transfer: None,
        conditional_return: None,
    })
}

// Resolve the callable scope independently of the number or kind of continuation statements.
fn scope(
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
        let Some((proof, SourceIdentity::Parameter(root), current)) =
            owner.filter(|(p, _, _)| matches!(p.drop.point(), DropPoint::AfterStatement(_)))
        else {
            return Ok(());
        };
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let EntityId::Value(anchor) = seed else {
            return Err(bad());
        };
        if self.bindings.get(&root) != Some(&LoweredValue::Value(current))
            || self.borrow_bindings.contains_key(&root)
        {
            return Err(bad());
        }
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
