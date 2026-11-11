//! Selected Value delivery and future ASAP Drop qualification for retained Map sources.
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    type_checking::{CallableTarget, ExpressionCategory},
};

pub(in crate::ssa::lower_frontend::map_iteration::source_owner) mod future {
    use super::super::*;
    use lang_frontend::name_resolution::ReferenceTarget;

    pub(in crate::ssa::lower_frontend::map_iteration::source_owner) fn qualify(
        parsed: &ParsedFile,
        names: &NameResolution,
        owned: &OwnershipCheckedFile,
        plan: &MapIterationOwnershipPlan,
        root: (
            SymbolId,
            Option<&crate::ssa::map_source::Formation<SymbolId>>,
        ),
        body_exit: IterationExitKind,
        span: Span,
    ) -> Result<Proof, LoweringError> {
        let (symbol, birth) = root;
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        if !matches!(
            body_exit,
            IterationExitKind::Fallthrough
                | IterationExitKind::Continue(_)
                | IterationExitKind::Break(_)
        ) || plan.closure_phis().len() != 2
            || plan.closure_phi_incomings().len() != 3
            || !plan.capture_graph().nodes().is_empty()
        {
            return Err(unsupported());
        }
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
        let [drop] = drops.as_slice() else {
            return Err(unsupported());
        };
        let point = match drop.point() {
            DropPoint::CallReturn(e)
            | DropPoint::AfterExpression(e)
            | DropPoint::ControlTransfer(e) => {
                parsed.ast().expressions().get(e).map_err(|_| bad())?.span()
            }
            DropPoint::LoopExit(s) | DropPoint::AfterStatement(s) => {
                parsed.ast().statements().get(s).map_err(|_| bad())?.span()
            }
            _ => return Err(unsupported()),
        };
        if !crate::ssa::map_source::future_reference(
            parsed,
            span,
            point,
            names
                .references()
                .iter()
                .filter_map(|r| (r.target() == &ReferenceTarget::Symbol(symbol)).then_some(r.span())),
        ) {
            return Err(unsupported());
        }
        let origin = names.symbols().get(symbol.index()).ok_or_else(bad)?.span();
        let conditions = owned.cleanup_conditions();
        let live = |owner| match birth {
            Some(birth) => super::super::formed_condition(parsed, owned, (symbol, birth.birth), owner, span),
            None => super::super::provenance::parameter(owned, symbol, origin, owner, span),
        };
        for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
            let phi = super::super::phi(conditions, plan, symbol, boundary, span)?;
            live(phi.owner())?;
        }
        let condition = live(drop.owner().ok_or_else(bad)?)?;
        if drop.value_origin() != origin
            || drop.condition() != Some(condition)
            || drop.capture_slot().is_some()
            || drop.instance_address().is_some()
        {
            return Err(bad());
        }
        if plan.exits().iter().any(|exit| {
            exit.actions().iter().any(|action| {
                matches!(action, IterationCleanupAction::Drop(fact)
                    if fact.target() == DropTarget::Named(symbol))
            })
        }) {
            return Err(bad());
        }
        let exhaustion = unique(
            plan.exits()
                .iter()
                .filter(|e| e.kind() == IterationExitKind::Exhaustion),
        )
        .ok_or_else(bad)?;
        if exhaustion.actions()
            != [
                IterationCleanupAction::FinishProvider(plan.descriptor().statement()),
                IterationCleanupAction::EndSource(plan.descriptor().statement()),
            ]
        {
            return Err(bad());
        }
        Ok(Proof {
            flow: None,
            disposition: source_owner::Disposition::Drop(*drop),
            transfer: None,
            conditional_return: None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Delivery {
    pub(super) root: SymbolId,
    pub(super) call: ExpressionId,
    pub(super) argument: ExpressionId,
    pub(super) parameter: usize,
    pub(super) owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
    pub(super) origin: Span,
}

pub(in super::super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    flow: Flow,
    span: Span,
) -> Result<Delivery, LoweringError> {
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let plan = owned.map_iteration(flow.statement).ok_or_else(bad)?;
    let root = match flow.source {
        SourceIdentity::Parameter(root)
        | SourceIdentity::Local { symbol: root, .. }
        | SourceIdentity::Formed { symbol: root, .. } => root,
        _ => return Err(unsupported()),
    };
    let has_drop = match flow.source {
        SourceIdentity::Formed { symbol, .. } => {
            let formation = formed::formation(
                parsed,
                names,
                typed,
                owned,
                symbol,
                plan.descriptor().source_type(),
                span,
            )?;
            !formed::drops(parsed, owned, symbol, &formation)?.is_empty()
        }
        _ => owned
            .drops()
            .iter()
            .any(|f| f.target() == DropTarget::Named(root)),
    };
    if has_drop {
        return Err(unsupported());
    }
    let storage = plan.descriptor().source_type();
    let origin = names.symbols().get(root.index()).ok_or_else(bad)?.span();
    let exit = super::phi(
        owned.cleanup_conditions(),
        plan,
        root,
        IterationPhiBoundary::Exit,
        span,
    )?;
    let live = super::flow_condition(parsed, names, owned, flow, exit.owner(), span)?;
    if live != exit.availability_condition()
        || owned.cleanup_conditions().get(live) == Some(&CleanupCondition::Never)
    {
        return Err(bad());
    }
    let mut candidates = Vec::new();
    for call in typed.calls() {
        let node = parsed
            .ast()
            .expressions()
            .get(call.expression())
            .map_err(|_| bad())?;
        if !crate::ssa::map_source::future_reference(
            parsed,
            span,
            node.span(),
            names
                .references()
                .iter()
                .filter_map(|r| (r.target() == &ReferenceTarget::Symbol(root)).then_some(r.span())),
        ) {
            continue;
        }
        let Expression::Call { arguments, .. } = node.payload() else {
            return Err(bad());
        };
        for mapping in call.arguments() {
            let argument = arguments.get(mapping.argument_index()).ok_or_else(bad)?;
            let canonical =
                crate::ssa::map_source::transparent_expression(parsed, argument.value, |id| {
                    typed.expression_type(id).zip(typed.expression_category(id))
                })?;
            let operand = parsed
                .ast()
                .expressions()
                .get(canonical)
                .map_err(|_| bad())?;
            if operand.payload() != &Expression::Name {
                continue;
            }
            let reference = unique(
                names
                    .references()
                    .iter()
                    .filter(|r| r.span() == operand.span()),
            )
            .ok_or_else(bad)?;
            if reference.target() != &ReferenceTarget::Symbol(root)
                || mapping.mode() != ParameterMode::Value
            {
                continue;
            }
            if !matches!(call.target(), CallableTarget::Source(_)) || call.aborts() {
                return Err(unsupported());
            }
            if mapping.parameter_type() != storage
                || mapping.category() != ExpressionCategory::Place
                || typed.expression_type(argument.value) != Some(storage)
                || typed.copyability(storage) != Some(Copyability::MoveOnly)
            {
                return Err(bad());
            }
            candidates.push(Delivery {
                root,
                call: call.expression(),
                argument: argument.value,
                parameter: mapping.parameter_index(),
                owner: exit.owner(),
                origin,
            });
        }
    }
    let [delivery] = candidates.as_slice() else {
        return Err(unsupported());
    };
    Ok(*delivery)
}

impl ExpressionLowerer<'_> {
    pub(in super::super::super) fn validate_map_value_delivery(
        &self,
        argument: ExpressionId,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, span);
        for plan in self.owned.map_iterations() {
            let descriptor = plan.descriptor();
            let qualified =
                qualify::qualify(self.parsed, self.names, self.typed, self.owned, *descriptor)?;
            let Some(Proof {
                disposition: Disposition::Delivery(delivery),
                ..
            }) = qualified.owner
            else {
                continue;
            };
            if delivery.argument != argument {
                continue;
            }
            let provider_span = self
                .parsed
                .ast()
                .statements()
                .get(descriptor.statement())
                .map_err(|_| bad())?
                .span();
            let exit = super::phi(
                self.owned.cleanup_conditions(),
                qualified.plan,
                delivery.root,
                IterationPhiBoundary::Exit,
                provider_span,
            )?;
            if exit.owner() != delivery.owner {
                return Err(bad());
            }
            let call = self.typed.call(delivery.call).ok_or_else(bad)?;
            let mapping = unique(
                call.arguments()
                    .iter()
                    .filter(|m| m.parameter_index() == delivery.parameter),
            )
            .ok_or_else(bad)?;
            if mapping.mode() != ParameterMode::Value
                || qualified.source.current_owner(self) != Some(value)
                || self
                    .bindings
                    .iter()
                    .any(|(&root, v)| root != delivery.root && *v == LoweredValue::Value(value))
                || self.temporaries.values().any(|&v| v == value)
            {
                return Err(bad());
            }
            match qualified.source {
                SourceIdentity::Parameter(root) => {
                    let anchor = *self
                        .map_iteration_state
                        .entries
                        .get(&root)
                        .ok_or_else(bad)?;
                    nested::transport(self.function, EntityId::Value(value), anchor, span)?;
                }
                SourceIdentity::Formed { symbol, .. } => {
                    let formation = formed::formation(
                        self.parsed,
                        self.names,
                        self.typed,
                        self.owned,
                        symbol,
                        descriptor.source_type(),
                        self.parsed
                            .ast()
                            .statements()
                            .get(descriptor.statement())
                            .map_err(|_| bad())?
                            .span(),
                    )?;
                    self.formed_map_anchor(&formation, Some(value), span)?;
                }
                _ => return Err(bad()),
            }
        }
        Ok(())
    }
}
