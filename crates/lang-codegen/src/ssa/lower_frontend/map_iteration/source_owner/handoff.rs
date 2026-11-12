//! Ordinary Return/local Move terminal identities retain this provider's actual current owner.
use super::*;
use crate::ssa::map_source::{
    handoff::{self, Handoff},
    handoff_runtime::{self, Terminal},
};
use lang_frontend::{parser::NameMarker, type_checking::CallableResultSource};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Proof {
    root: SymbolId,
    terminal: Handoff<SymbolId>,
    pub(in super::super) origin: Span,
    drop_index: Option<usize>,
}
impl Proof {
    pub(in super::super) fn drop(
        self,
        owned: &OwnershipCheckedFile,
    ) -> Result<Option<DropFact>, LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, self.origin);
        let Handoff::Binding { root, marker, .. } = self.terminal else {
            return Ok(None);
        };
        let fact = *owned
            .drops()
            .get(self.drop_index.ok_or_else(bad)?)
            .ok_or_else(bad)?;
        if fact.target() != DropTarget::Named(root) || fact.value_origin() != marker {
            return Err(bad());
        }
        Ok(Some(fact))
    }
}

pub(in super::super) fn qualify(
    frontend: (
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
    ),
    flow: Flow,
    span: Span,
) -> Result<Option<Proof>, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let root = match flow.source {
        SourceIdentity::Parameter(root) | SourceIdentity::Formed { symbol: root, .. } => root,
        _ => return Ok(None),
    };
    let plan = owned.map_iteration(flow.statement).ok_or_else(bad)?;
    let storage = plan.descriptor().source_type();
    if plan.source_access() != MapIterationSourceAccess::Owned
        || owned
            .drops()
            .iter()
            .any(|d| d.target() == DropTarget::Named(root))
    {
        return Ok(None);
    }
    let resolve = |at| {
        unique(names.references().iter().filter(|r| r.span() == at))
            .and_then(|r| match r.target() {
                lang_frontend::name_resolution::ReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            })
            .ok_or_else(bad)
    };
    let declaration = |at| {
        unique(
            names
                .symbols()
                .iter()
                .filter(|s| s.span() == at && !s.is_synthetic()),
        )
        .map(|s| s.id())
        .ok_or_else(bad)
    };
    let Some(terminal) = handoff::find(parsed, span, root, storage, resolve, declaration, |id| {
        typed.expression_type(id).zip(typed.expression_category(id))
    })?
    else {
        return Ok(None);
    };
    let exit = super::phi(
        owned.cleanup_conditions(),
        plan,
        root,
        IterationPhiBoundary::Exit,
        span,
    )?;
    let live = super::flow_condition(parsed, names, owned, flow, exit.owner(), span)?;
    if live != exit.availability_condition()
        || typed.copyability(storage) != Some(Copyability::MoveOnly)
    {
        return Err(bad());
    }
    let drop_index = match terminal {
        Handoff::Return { .. } => {
            let function = parsed
                .ast()
                .items()
                .iter()
                .filter(|(_, n)| {
                    matches!(n.payload(), Item::Function { .. })
                        && n.span().start() <= span.start()
                        && span.end() <= n.span().end()
                })
                .min_by_key(|(_, n)| n.span().end() - n.span().start())
                .ok_or_else(bad)?;
            let Item::Function {
                name: NameMarker::Present(marker),
                ..
            } = function.1.payload()
            else {
                return Err(bad());
            };
            let symbol = declaration(*marker)?;
            let callable = unique(typed.callables().iter().filter(|c| c.symbol() == symbol))
                .ok_or_else(bad)?;
            if callable.return_type() != storage
                || callable.result_source() != CallableResultSource::Owned
            {
                return Err(bad());
            }
            None
        }
        Handoff::Binding {
            root: successor,
            marker,
            ..
        } => {
            if successor == root
                || typed.symbol_type(successor) != Some(storage)
                || !owned.formed_owned_locals().contains(&successor)
            {
                return Err(bad());
            }
            let (index, fact) = unique(
                owned
                    .drops()
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| d.target() == DropTarget::Named(successor)),
            )
            .ok_or_else(bad)?;
            if fact.value_origin() != marker
                || fact.owner() != Some(exit.owner())
                || fact.condition() != Some(live)
                || fact.capture_slot().is_some()
                || fact.instance_address().is_some()
                || !matches!(fact.point(), DropPoint::CallReturn(_))
            {
                return Err(bad());
            }
            Some(index)
        }
    };
    Ok(Some(Proof {
        root,
        terminal,
        origin: names.symbols().get(root.index()).ok_or_else(bad)?.span(),
        drop_index,
    }))
}

impl ExpressionLowerer<'_> {
    pub(in super::super::super) fn validate_map_handoff_drop(
        &self,
        fact: DropFact,
        owner: ValueId,
    ) -> Result<bool, LoweringError> {
        for plan in self.owned.map_iterations() {
            let descriptor = plan.descriptor();
            let qualified =
                qualify::qualify(self.parsed, self.names, self.typed, self.owned, *descriptor)?;
            let Some(super::Proof {
                disposition: Disposition::Handoff(proof),
                ..
            }) = qualified.owner
            else {
                continue;
            };
            if proof.drop(self.owned)? != Some(fact) {
                continue;
            }
            let bad = || error(LoweringErrorKind::MissingFact, fact.value_origin());
            let Handoff::Binding {
                root: successor, ..
            } = proof.terminal
            else {
                return Err(bad());
            };
            if self.bindings.contains_key(&proof.root)
                || self.bindings.contains_key(&successor)
                || self.borrow_bindings.contains_key(&proof.root)
                || self.borrow_bindings.contains_key(&successor)
                || self
                    .bindings
                    .values()
                    .any(|v| *v == LoweredValue::Value(owner))
                || self.temporaries.values().any(|v| *v == owner)
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
                    nested::transport(
                        self.function,
                        EntityId::Value(owner),
                        anchor,
                        fact.value_origin(),
                    )?;
                }
                SourceIdentity::Formed { symbol, .. } => {
                    let span = self.expression_source_provider_span(descriptor.statement())?;
                    let formation = formed::formation(
                        self.parsed,
                        self.names,
                        self.typed,
                        self.owned,
                        symbol,
                        descriptor.source_type(),
                        span,
                    )?;
                    self.formed_map_anchor(&formation, Some(owner), span)?;
                }
                _ => return Err(bad()),
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub(in super::super::super) fn finish_map_handoffs(&self) -> Result<(), LoweringError> {
        let function_span = self.function.origin.span();
        for plan in self.owned.map_iterations() {
            let descriptor = plan.descriptor();
            let span = self.expression_source_provider_span(descriptor.statement())?;
            if !handoff::same_callable(self.parsed, span, function_span) {
                continue;
            }
            let qualified =
                qualify::qualify(self.parsed, self.names, self.typed, self.owned, *descriptor)?;
            let Some(super::Proof {
                disposition: Disposition::Handoff(proof),
                ..
            }) = qualified.owner
            else {
                continue;
            };
            let bad = || error(LoweringErrorKind::MissingFact, span);
            let terminal = match proof.terminal {
                Handoff::Return {
                    expression,
                    operand,
                } => {
                    if !matches!(self.parsed.ast().expressions().get(expression).map_err(|_| bad())?.payload(),
                        Expression::Return { value: Some(actual), .. } if *actual == operand)
                    {
                        return Err(bad());
                    }
                    Terminal::Return(self.expression_span(expression)?)
                }
                Handoff::Binding { initializer, .. } => {
                    let canonical = crate::ssa::map_source::transparent_expression(
                        self.parsed,
                        initializer,
                        |id| {
                            self.typed
                                .expression_type(id)
                                .zip(self.typed.expression_category(id))
                        },
                    )?;
                    if self
                        .references
                        .get(&span_key(self.expression_span(canonical)?))
                        != Some(&proof.root)
                    {
                        return Err(bad());
                    }
                    let fact = proof.drop(self.owned)?.ok_or_else(bad)?;
                    let DropPoint::CallReturn(call) = fact.point() else {
                        return Err(bad());
                    };
                    let descriptor = self.typed.call(call).ok_or_else(bad)?;
                    let lang_frontend::type_checking::CallableTarget::Source(target) =
                        descriptor.target()
                    else {
                        return Err(bad());
                    };
                    let instance = if let Some(source) = self.source_token {
                        self.instance_plan
                            .call_site(source, call)
                            .cloned()
                            .ok_or_else(bad)?
                    } else {
                        let arguments = descriptor
                            .instance()
                            .type_arguments()
                            .iter()
                            .map(|ty| self.resolve_type(*ty, span))
                            .collect::<Result<Vec<_>, _>>()?;
                        FunctionInstanceKey::new(target, arguments)
                    };
                    let callee = *self.function_ids.get(&instance).ok_or_else(bad)?;
                    Terminal::Drop {
                        marker: fact.value_origin(),
                        call: self.expression_span(call)?,
                        callee,
                    }
                }
            };
            let (seed, _) = handoff_runtime::complete(self.function, span, terminal)?;
            match qualified.source {
                SourceIdentity::Parameter(root) => {
                    let anchor = *self
                        .map_iteration_state
                        .entries
                        .get(&root)
                        .ok_or_else(bad)?;
                    nested::transport(self.function, seed, anchor, span)?;
                }
                SourceIdentity::Formed { symbol, birth } => {
                    let formation = formed::formation(
                        self.parsed,
                        self.names,
                        self.typed,
                        self.owned,
                        symbol,
                        descriptor.source_type(),
                        span,
                    )?;
                    if formation.birth != birth {
                        return Err(bad());
                    }
                    let EntityId::Value(seed) = seed else {
                        return Err(bad());
                    };
                    self.formed_map_anchor(&formation, Some(seed), span)?;
                }
                _ => return Err(bad()),
            }
        }
        Ok(())
    }

    fn expression_source_provider_span(
        &self,
        statement: StatementId,
    ) -> Result<Span, LoweringError> {
        self.parsed
            .ast()
            .statements()
            .get(statement)
            .map(|n| n.span())
            .map_err(|_| error(LoweringErrorKind::MissingFact, self.function.origin.span()))
    }
}

pub(in crate::ssa::lower_frontend::map_iteration) mod delivery {
    //! Selected Value delivery and future ASAP Drop qualification for retained Map sources.
    use super::super::*;
    use lang_frontend::{
        name_resolution::ReferenceTarget,
        type_checking::{CallableTarget, ExpressionCategory},
    };

    pub(in crate::ssa::lower_frontend::map_iteration::source_owner) mod future {
        use super::super::super::*;
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
                names.references().iter().filter_map(|r| {
                    (r.target() == &ReferenceTarget::Symbol(symbol)).then_some(r.span())
                }),
            ) {
                return Err(unsupported());
            }
            let origin = names.symbols().get(symbol.index()).ok_or_else(bad)?.span();
            let conditions = owned.cleanup_conditions();
            let live = |owner| match birth {
                Some(birth) => super::super::super::formed_condition(
                    parsed,
                    owned,
                    (symbol, birth.birth),
                    owner,
                    span,
                ),
                None => {
                    super::super::super::provenance::parameter(owned, symbol, origin, owner, span)
                }
            };
            for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
                let phi = super::super::super::phi(conditions, plan, symbol, boundary, span)?;
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
    pub(in crate::ssa::lower_frontend::map_iteration) struct Delivery {
        pub(in crate::ssa::lower_frontend::map_iteration::source_owner) root: SymbolId,
        pub(in crate::ssa::lower_frontend::map_iteration::source_owner) call: ExpressionId,
        pub(in crate::ssa::lower_frontend::map_iteration::source_owner) argument: ExpressionId,
        pub(in crate::ssa::lower_frontend::map_iteration::source_owner) parameter: usize,
        pub(in crate::ssa::lower_frontend::map_iteration::source_owner) owner:
            lang_frontend::ownership_checking::CleanupOwnerValueId,
        pub(in crate::ssa::lower_frontend::map_iteration::source_owner) origin: Span,
    }

    pub(in crate::ssa::lower_frontend::map_iteration) fn qualify(
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
        let exit = super::super::phi(
            owned.cleanup_conditions(),
            plan,
            root,
            IterationPhiBoundary::Exit,
            span,
        )?;
        let live = super::super::flow_condition(parsed, names, owned, flow, exit.owner(), span)?;
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
                names.references().iter().filter_map(|r| {
                    (r.target() == &ReferenceTarget::Symbol(root)).then_some(r.span())
                }),
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
        pub(in crate::ssa::lower_frontend) fn validate_map_value_delivery(
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
                let exit = super::super::phi(
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
}
