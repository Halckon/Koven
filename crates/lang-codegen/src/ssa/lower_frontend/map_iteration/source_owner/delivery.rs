//! A retained Map has a real selected Value delivery, never an inferred missing Drop.
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    type_checking::{CallableTarget, ExpressionCategory},
};

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
    /// This executes before ordinary argument detachment. The existing pending
    /// argument slots then transport and consume exactly this current owner.
    pub(in super::super::super) fn validate_map_value_delivery(
        &self,
        argument: ExpressionId,
        value: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, span);
        for descriptor in self.typed.map_iterations() {
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
            let exit = super::phi(
                self.owned.cleanup_conditions(),
                qualified.plan,
                delivery.root,
                IterationPhiBoundary::Exit,
                span,
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
