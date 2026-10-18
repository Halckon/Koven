//! Validate the actual conditional edges and independent cleanup/Next/Return witnesses.
use super::*;

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn validate_conditional_map_cleanup(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let Some((statement, state)) = self.conditional_map() else {
            return Ok(());
        };
        let branch = state.conditional.ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let Some(index) = branch.point_index(state.carrier, point) else {
            return Ok(());
        };
        let span = self.statement_span(statement.statement())?;
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let witness = branch.exits[index].ok_or_else(bad)?;
        let block = self.function.block(witness.roles.block).ok_or_else(bad)?;
        if self.block != witness.roles.block || witness.cleanup_end.is_some() {
            return Err(bad());
        }
        let actions = self.owned.iteration_cleanup_at(point).ok_or_else(bad)?;
        let mut operations = Vec::new();
        for action in actions {
            let expected = match action {
                Action::Drop(fact) => {
                    let actual = block
                        .instructions
                        .get(witness.cleanup_start + operations.len())
                        .and_then(|id| self.function.instruction(*id))
                        .ok_or_else(bad)?;
                    let Operation::Drop { owner } = actual.operation else {
                        return Err(bad());
                    };
                    if actual.origin != Origin::Source(fact.value_origin()) {
                        return Err(bad());
                    }
                    let (local, _) = branch.shape.local.ok_or_else(bad)?;
                    match fact.target() {
                        lang_frontend::ownership_checking::UnitDropTarget::Named(symbol)
                            if symbol == state.carrier.root
                                && EntityId::Value(owner) == witness.roles.seed => {}
                        lang_frontend::ownership_checking::UnitDropTarget::Named(symbol)
                            if symbol == local && witness.roles.local == Some(owner) => {}
                        _ => return Err(bad()),
                    }
                    Operation::Drop { owner }
                }
                Action::EndBinding {
                    statement: actual,
                    symbol,
                } if *actual == statement => {
                    let loan = if *symbol == state.value_binding {
                        witness.roles.value
                    } else if *symbol == state.key_binding {
                        witness.roles.key
                    } else {
                        None
                    };
                    Operation::BorrowEnd {
                        loan: loan.ok_or_else(bad)?,
                    }
                }
                Action::FinishProvider(actual) if *actual == statement => continue,
                Action::EndSource(actual) if *actual == statement => Operation::BorrowEnd {
                    loan: witness.roles.source,
                },
                _ => return Err(bad()),
            };
            operations.push(expected);
        }
        let actual = block
            .instructions
            .get(witness.cleanup_start..)
            .ok_or_else(bad)?;
        if actual.len() != operations.len()
            || actual.iter().zip(&operations).any(|(id, expected)| {
                self.function
                    .instruction(*id)
                    .is_none_or(|instruction| instruction.operation != *expected)
            })
        {
            return Err(bad());
        }
        let end = block.instructions.len();
        self.conditional_map_mut(statement, span)?.exits[index]
            .as_mut()
            .ok_or_else(bad)?
            .cleanup_end = Some(end);
        Ok(())
    }

    pub(super) fn conditional_map_local_birth(
        &self,
        branch: ConditionalState,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        use lang_frontend::ownership_checking::ConstructionRootKind;
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let (symbol, expression) = branch.shape.local.ok_or_else(bad)?;
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
            return Err(bad());
        };
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| bad())?;
        let descriptor = self.typed.construction(expression).ok_or_else(bad)?;
        let plans = self
            .owned
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == expression)
            .collect::<Vec<_>>();
        let [plan] = plans.as_slice() else {
            return Err(bad());
        };
        let obligation = plan.root_obligation().ok_or_else(bad)?;
        let ty = self.expression_ssa_type(expression.expression(), span)?;
        let instruction = carrier::instruction(self.function, EntityId::Value(owner), span)?;
        let actual_type = match (obligation.kind(), &instruction.operation) {
            (ConstructionRootKind::HeapOwner, Operation::HeapAllocate { owner, .. }) => *owner,
            (ConstructionRootKind::SharedOwner, Operation::SharedAllocate { owner, .. }) => *owner,
            (ConstructionRootKind::Inline, Operation::AggregateConstruct { aggregate, .. }) => {
                *aggregate
            }
            (ConstructionRootKind::Inline, Operation::TaggedConstruct { tagged, .. }) => *tagged,
            _ => return Err(bad()),
        };
        if expression.source_unit() != self.source_unit
            || symbol.source_unit() != self.source_unit
            || instruction.block != branch.birth.block
            || instruction.block != self.block
            || instruction.origin != Origin::Source(node.span())
            || actual_type != ty
            || self
                .function
                .entity(EntityId::Value(owner))
                .map(|data| data.ty)
                != Some(EntityType::Value(ty))
            || descriptor.result_type() != obligation.result_type()
            || plan.target() != descriptor.target()
        {
            return Err(bad());
        }
        Ok(owner)
    }

    pub(super) fn validate_conditional_map_return(
        &self,
        state: MapState,
        witness: ExitWitness,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let branch = state.conditional.ok_or_else(bad)?;
        let UnitIterationExitKind::Return(expression) = branch.shape.transfers[2] else {
            return Err(bad());
        };
        if witness.point != UnitDropPoint::ControlTransfer(expression) || witness.next.is_some() {
            return Err(bad());
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| bad())?;
        let lang_frontend::parser::Expression::Return {
            value: Some(operand),
            ..
        } = node.payload()
        else {
            return Err(bad());
        };
        let operand_span = self
            .parsed
            .ast()
            .expressions()
            .get(*operand)
            .map_err(|_| bad())?
            .span();
        let block = self.function.block(witness.roles.block).ok_or_else(bad)?;
        let Some(TerminatorKind::Return { values }) =
            block.terminator.as_ref().map(|term| &term.kind)
        else {
            return Err(bad());
        };
        let [result] = values.as_slice() else {
            return Err(bad());
        };
        let call = carrier::instruction(self.function, EntityId::Value(*result), span)?;
        let position = block
            .instructions
            .iter()
            .position(|id| *id == call.id)
            .ok_or_else(bad)?;
        let (Some(key), Some(value)) = (witness.roles.key, witness.roles.value) else {
            return Err(bad());
        };
        if call.block != witness.roles.block
            || call.origin != Origin::Source(operand_span)
            || position >= witness.cleanup_start
            || !matches!(&call.operation, Operation::DirectCall { receiver: None, arguments, .. }
                if arguments.as_slice() == [EntityId::Loan(key), EntityId::Loan(value)])
            || call.results.as_slice() != [EntityId::Value(*result)]
            || block.instructions.iter().any(|id| {
                self.function.instruction(*id).is_some_and(|instruction| {
                    matches!(instruction.operation, Operation::MapIterationNext { .. })
                })
            })
        {
            return Err(bad());
        }
        Ok(*result)
    }

    pub(in super::super) fn validate_conditional_map_body(
        &self,
        context: &LoopContext,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let Some(iteration) = &context.iteration else {
            return Err(bad());
        };
        let IterationState::Map(state) = &iteration.state else {
            return Err(bad());
        };
        let Some(branch) = state.conditional else {
            return Ok(());
        };
        if branch.completed != 3
            || branch.checkpoint.is_some()
            || context.continues.len() != 2
            || context.breaks.len() != 1
            || branch.exits[4].is_some()
        {
            return Err(bad());
        }
        let mut previous = branch.birth;
        for index in 0..3 {
            let [Some(when_true), Some(when_false)] = branch.transports[index] else {
                return Err(bad());
            };
            if when_true.from != previous
                || when_false.from != previous
                || when_true.to.block == when_false.to.block
            {
                return Err(bad());
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(branch.shape.controls[index].expression())
                .map_err(|_| bad())?;
            let header = self.function.block(previous.block).ok_or_else(bad)?;
            let terminator = header.terminator.as_ref().ok_or_else(bad)?;
            let TerminatorKind::Conditional {
                condition,
                when_true: true_edge,
                when_false: false_edge,
            } = &terminator.kind
            else {
                return Err(bad());
            };
            if terminator.origin != Origin::Source(node.span())
                || previous.transported(self.function, true_edge, span)? != when_true.to
                || previous.transported(self.function, false_edge, span)? != when_false.to
            {
                return Err(bad());
            }
            let condition = carrier::instruction(self.function, EntityId::Value(*condition), span)?;
            if condition.block != previous.block
                || !matches!(condition.operation,
                Operation::StringEqual { left: EntityId::Loan(loan), .. } if Some(loan) == previous.key)
            {
                return Err(bad());
            }
            let witness = branch.exits[index].ok_or_else(bad)?;
            if witness.roles != when_true.to || witness.cleanup_end.is_none() {
                return Err(bad());
            }
            previous = when_false.to;
        }
        let tail = branch.exits[3].ok_or_else(bad)?;
        if tail.roles != previous
            || tail.roles != branch.roles
            || tail.cleanup_end.is_none()
            || tail.point != state.carrier.body_point
        {
            return Err(bad());
        }
        for (jump, index) in context.continues.iter().zip([0, 3]) {
            let witness = branch.exits[index].ok_or_else(bad)?;
            if jump.state.block != witness.roles.block {
                return Err(bad());
            }
            self.validate_conditional_map_backedge(**state, witness, &jump.state, span)?;
        }
        let breaking = branch.exits[1].ok_or_else(bad)?;
        let returned = branch.exits[2].ok_or_else(bad)?;
        if context.breaks[0].state.block != breaking.roles.block
            || breaking.next.is_some()
            || breaking.returned.is_some()
            || returned.next.is_some()
            || returned.returned
                != Some(self.validate_conditional_map_return(**state, returned, span)?)
            || context
                .breaks
                .iter()
                .chain(&context.continues)
                .any(|jump| jump.state.block == returned.roles.block)
        {
            return Err(bad());
        }
        Ok(())
    }

    fn validate_conditional_map_backedge(
        &self,
        state: MapState,
        witness: ExitWitness,
        jump: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let next = witness.next.ok_or_else(bad)?;
        let block = self.function.block(witness.roles.block).ok_or_else(bad)?;
        let Some(TerminatorKind::Branch(edge)) = block.terminator.as_ref().map(|term| &term.kind)
        else {
            return Err(bad());
        };
        let [seed, source, cursor] = state.carrier.header_entities(self.function, span)?;
        if edge.target != state.carrier.header
            || transport_entity(self.function, edge, witness.roles.seed, span)? != seed
            || transport_entity(
                self.function,
                edge,
                EntityId::Loan(witness.roles.source),
                span,
            )? != source
            || transport_entity(self.function, edge, EntityId::Value(next), span)? != cursor
            || jump.pending_operands.get(state.source_slot)
                != Some(&EntityId::Loan(witness.roles.source))
            || jump.pending_operands.get(state.cursor_slot) != Some(&EntityId::Value(next))
            || witness
                .roles
                .key
                .is_some_and(|loan| edge.arguments.contains(&EntityId::Loan(loan)))
            || witness
                .roles
                .value
                .is_some_and(|loan| edge.arguments.contains(&EntityId::Loan(loan)))
        {
            return Err(bad());
        }
        let advance = carrier::instruction(self.function, EntityId::Value(next), span)?;
        let position = block
            .instructions
            .iter()
            .position(|id| *id == advance.id)
            .ok_or_else(bad)?;
        if advance.block != witness.roles.block
            || Some(position) != witness.cleanup_end
            || advance.operation
                != (Operation::MapIterationNext {
                    source: witness.roles.source,
                    cursor: witness.roles.cursor,
                })
            || position + 1 != block.instructions.len()
        {
            return Err(bad());
        }
        Ok(())
    }

    pub(in super::super) fn validate_conditional_map_complete(
        &self,
        context: &LoopContext,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let Some(iteration) = &context.iteration else {
            return Err(bad());
        };
        let IterationState::Map(state) = &iteration.state else {
            return Err(bad());
        };
        let Some(branch) = state.conditional else {
            return Ok(());
        };
        if branch
            .exits
            .iter()
            .any(|witness| witness.is_none_or(|witness| witness.cleanup_end.is_none()))
        {
            return Err(bad());
        }
        let exhausted = branch.exits[4].ok_or_else(bad)?;
        let breaking = branch.exits[1].ok_or_else(bad)?;
        if exhausted.roles.block != state.carrier.exhaustion
            || self.block != exhausted.roles.block
            || exhausted.point != state.carrier.exhaustion_point
            || exhausted.roles.key.is_some()
            || exhausted.roles.value.is_some()
            || exhausted.roles.local.is_some()
            || exhausted.next.is_some()
            || exhausted.returned.is_some()
            || context.breaks.len() != 1
            || context.breaks[0].state.block != breaking.roles.block
            || self
                .function
                .block(exhausted.roles.block)
                .is_none_or(|block| {
                    block.instructions.iter().any(|id| {
                        self.function.instruction(*id).is_some_and(|instruction| {
                            matches!(
                                instruction.operation,
                                Operation::MapIterationKeyLoan { .. }
                                    | Operation::MapIterationValueLoan { .. }
                                    | Operation::MapIterationNext { .. }
                            )
                        })
                    })
                })
        {
            return Err(bad());
        }
        Ok(())
    }
}
