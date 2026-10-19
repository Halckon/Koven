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
