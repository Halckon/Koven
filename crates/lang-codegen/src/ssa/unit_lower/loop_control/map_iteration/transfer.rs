//! Finish a proved future disposition against the actual selected Call and current owner graph.
use super::*;

impl UnitExpressionLowerer<'_> {
    pub(super) fn finish_map_transfers(&self, function_span: Span) -> Result<(), LoweringError> {
        for descriptor in self.typed.map_iterations() {
            if descriptor.statement().source_unit() != self.source_unit {
                continue;
            }
            let span = self.statement_span(descriptor.statement().statement())?;
            if !map_iteration::contains(function_span, span) {
                continue;
            }
            let qualified = map_iteration::qualify(
                self.parsed,
                self.names,
                self.typed,
                self.owned,
                descriptor,
            )?;
            let Some(transfer) = qualified.future_transfer else {
                continue;
            };
            let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
            if transfer.statement != qualified.statement || transfer.source != qualified.source {
                return Err(bad());
            }
            let births = self
                .function
                .instructions
                .iter()
                .filter(|i| {
                    i.origin == Origin::Source(span)
                        && matches!(i.operation, Operation::MapIterationFirst { .. })
                })
                .collect::<Vec<_>>();
            let [birth] = births.as_slice() else {
                return Err(bad());
            };
            let Operation::MapIterationFirst { source } = birth.operation else {
                return Err(bad());
            };
            let lease = carrier::instruction(self.function, EntityId::Loan(source), span)?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = lease.operation
            else {
                return Err(bad());
            };
            let place = carrier::instruction(self.function, EntityId::Place(place), span)?;
            let Operation::RootPlace { owner: seed } = place.operation else {
                return Err(bad());
            };
            let anchor = match qualified.source {
                SourceIdentity::Parameter { slot, .. } => {
                    let entry = self.function.entry_block().ok_or_else(bad)?;
                    transport::parameter(self.function, entry, slot, span)?
                }
                SourceIdentity::Formed { root, birth } => {
                    local::validate_formed_birth(self, root, birth, EntityId::Value(seed), span)?
                }
                _ => return Err(bad()),
            };
            transport::from_anchor(self.function, EntityId::Value(seed), anchor, span)?;
            for fact in transfer.deliveries(self.parsed, self.names, self.typed, self.owned)? {
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(fact.call().expression())
                    .map_err(|_| bad())?;
                let lang_frontend::parser::Expression::Call {
                    arguments: syntax, ..
                } = node.payload()
                else {
                    return Err(bad());
                };
                let descriptor = self.typed.call(fact.call()).ok_or_else(bad)?;
                let arguments = descriptor
                    .arguments()
                    .iter()
                    .filter(|a| {
                        syntax.get(a.argument_index()).is_some_and(|arg| {
                            UnitExpressionId::new(self.source_unit, arg.value) == fact.argument()
                        })
                    })
                    .collect::<Vec<_>>();
                let [argument] = arguments.as_slice() else {
                    return Err(bad());
                };
                let resolved = self
                    .source_plan
                    .call_site(self.source_token, fact.call())
                    .ok_or_else(bad)?;
                let callee = *self.function_ids.get(resolved.key()).ok_or_else(bad)?;
                let calls = self
                    .function
                    .instructions
                    .iter()
                    .filter(|i| {
                        i.origin == Origin::Source(node.span())
                            && matches!(i.operation, Operation::DirectCall { .. })
                    })
                    .collect::<Vec<_>>();
                let [call] = calls.as_slice() else {
                    return Err(bad());
                };
                let Operation::DirectCall {
                    callee: actual,
                    arguments,
                    ..
                } = &call.operation
                else {
                    return Err(bad());
                };
                let Some(EntityId::Value(owner)) =
                    arguments.get(argument.parameter_index()).copied()
                else {
                    return Err(bad());
                };
                let ty = self.iteration_type(argument.parameter_type(), span)?;
                if *actual != callee
                    || arguments.len() != descriptor.arguments().len()
                    || arguments
                        .iter()
                        .filter(|v| **v == EntityId::Value(owner))
                        .count()
                        != 1
                    || self.function.entity(EntityId::Value(owner)).map(|e| e.ty)
                        != Some(EntityType::Value(ty))
                    || self.temporaries.values().any(|v| *v == owner)
                    || self.pending_operands.contains(&EntityId::Value(owner))
                    || self
                        .bindings
                        .values()
                        .any(|v| *v == LoweredValue::Value(owner))
                {
                    return Err(bad());
                }
                // The ordinary call emitter consumed the exact Move fact and removed
                // its binding. Every incoming owner now delivered must reach this
                // provider's actual acquisition, including every loop backedge.
                transport::from_anchor(
                    self.function,
                    EntityId::Value(owner),
                    EntityId::Value(seed),
                    span,
                )?;
            }
        }
        Ok(())
    }
}
