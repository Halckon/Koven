//! Complete incoming and body-use checks for the normal local provider, including zero rounds.
use super::*;
use crate::ssa::unit_plan::UnitFunctionInstanceKey;
use lang_frontend::{parser::Statement, type_checking::UnitCallTarget};

impl MapCarrier {
    pub(super) fn validate_local_transport(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let function = &lowerer.function;
        let [owner, source, cursor] = self.current_entities(function, self.body, span)?;
        let (EntityId::Loan(source), EntityId::Value(cursor)) = (source, cursor) else {
            return Err(bad());
        };
        let header_incoming = transport::incoming(function, self.header);
        if header_incoming.len() != 2
            || header_incoming
                .iter()
                .filter(|(block, _)| *block == self.preheader)
                .count()
                != 1
            || header_incoming
                .iter()
                .filter(|(block, _)| *block == self.body)
                .count()
                != 1
        {
            return Err(bad());
        }
        for block in [self.body, self.exhaustion] {
            let incoming = transport::incoming(function, block);
            let [(predecessor, edge)] = incoming.as_slice() else {
                return Err(bad());
            };
            if *predecessor != self.header {
                return Err(bad());
            }
            for column in [self.seed_column, self.source_column, self.cursor_column] {
                let header = self.parameter(function, self.header, column, span)?;
                if transport::along(function, edge, header, span)?
                    != self.parameter(function, block, column, span)?
                {
                    return Err(bad());
                }
            }
        }
        let entry = header_incoming
            .iter()
            .find(|(block, _)| *block == self.preheader)
            .ok_or_else(bad)?
            .1;
        let entry_source = *entry.arguments.get(self.source_column).ok_or_else(bad)?;
        for block in [self.header, self.body, self.exhaustion] {
            if function.block(block).ok_or_else(bad)?.origin != Origin::Source(span) {
                return Err(bad());
            }
            transport::from_anchor(
                function,
                self.parameter(function, block, self.seed_column, span)?,
                self.anchor,
                span,
            )?;
            transport::from_anchor(
                function,
                self.parameter(function, block, self.source_column, span)?,
                entry_source,
                span,
            )?;
        }
        let body = function.block(self.body).ok_or_else(bad)?;
        let instructions = body
            .instructions
            .iter()
            .map(|id| function.instruction(*id).ok_or_else(bad))
            .collect::<Result<Vec<_>, _>>()?;
        let projections = instructions
            .iter()
            .filter(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. }
                )
            })
            .collect::<Vec<_>>();
        let [key, value] = projections.as_slice() else {
            return Err(bad());
        };
        if key.operation != (Operation::MapIterationKeyLoan { source, cursor })
            || value.operation != (Operation::MapIterationValueLoan { source, cursor })
        {
            return Err(bad());
        }
        let [EntityId::Loan(key_loan)] = key.results.as_slice() else {
            return Err(bad());
        };
        let [EntityId::Loan(value_loan)] = value.results.as_slice() else {
            return Err(bad());
        };
        if key_loan == value_loan {
            return Err(bad());
        }
        let calls = instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| {
                matches!(instruction.operation, Operation::DirectCall { .. })
            })
            .collect::<Vec<_>>();
        let [(call_position, call)] = calls.as_slice() else {
            return Err(bad());
        };
        if !matches!(&call.operation, Operation::DirectCall { receiver: None, arguments, .. } if arguments.as_slice() == [EntityId::Loan(*key_loan), EntityId::Loan(*value_loan)])
        {
            return Err(bad());
        }
        let Origin::Source(call_span) = call.origin else {
            return Err(bad());
        };
        let UnitDropPoint::AfterStatement(body_id) = self.body_point else {
            return Err(bad());
        };
        if body_id.source_unit() != lowerer.source_unit {
            return Err(bad());
        }
        let body_node = lowerer
            .parsed
            .ast()
            .statements()
            .get(body_id.statement())
            .map_err(|_| bad())?;
        let Statement::Block { elements } = body_node.payload() else {
            return Err(bad());
        };
        let [statement] = elements.as_slice() else {
            return Err(bad());
        };
        let statement_node = lowerer
            .parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|_| bad())?;
        let Statement::Expression { expression } = statement_node.payload() else {
            return Err(bad());
        };
        let call_node = lowerer
            .parsed
            .ast()
            .expressions()
            .get(*expression)
            .map_err(|_| bad())?;
        let typed_call = lowerer
            .typed
            .call(UnitExpressionId::new(lowerer.source_unit, *expression))
            .ok_or_else(bad)?;
        if call_node.span() != call_span {
            return Err(bad());
        }
        let key_position = instructions
            .iter()
            .position(|instruction| instruction.id == key.id)
            .ok_or_else(bad)?;
        let value_position = instructions
            .iter()
            .position(|instruction| instruction.id == value.id)
            .ok_or_else(bad)?;
        if key_position >= value_position || value_position >= *call_position {
            return Err(bad());
        }
        let UnitCallTarget::Declaration(target) = typed_call.target() else {
            return Err(bad());
        };
        let key = UnitFunctionInstanceKey::new(target, Vec::new());
        let resolved = lowerer
            .source_plan
            .call_site(lowerer.source_token, typed_call.expression())
            .ok_or_else(bad)?;
        let expected_callee = *lowerer.function_ids.get(&key).ok_or_else(bad)?;
        if resolved.key() != &key
            || !resolved.delegation().is_empty()
            || !matches!(call.operation, Operation::DirectCall { callee, .. } if callee == expected_callee)
        {
            return Err(bad());
        }
        let ends = instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| match instruction.operation {
                Operation::BorrowEnd { loan } if loan == *key_loan || loan == *value_loan => {
                    Some((index, loan))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(value_end, actual_value), (key_end, actual_key)] = ends.as_slice() else {
            return Err(bad());
        };
        let next = instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| {
                matches!(instruction.operation, Operation::MapIterationNext { .. })
            })
            .collect::<Vec<_>>();
        let [(next_position, _)] = next.as_slice() else {
            return Err(bad());
        };
        if *actual_value != *value_loan || *actual_key != *key_loan || call_position >= value_end || key_end >= next_position
            || instructions.iter().any(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == source))
            || lowerer.borrow_bindings.values().any(|loan| loan == key_loan || loan == value_loan)
            || lowerer.temporaries.values().any(|value| EntityId::Value(*value) == owner)
        { return Err(bad()); }
        Ok(())
    }

    pub(in super::super) fn validate_local_exhaustion(
        &self,
        lowerer: &UnitExpressionLowerer<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let [owner, source, _] = self.current_entities(lowerer.function, self.exhaustion, span)?;
        let (EntityId::Value(owner), EntityId::Loan(source)) = (owner, source) else {
            return Err(bad());
        };
        let block = lowerer.function.block(self.exhaustion).ok_or_else(bad)?;
        let instructions = block
            .instructions
            .iter()
            .map(|id| lowerer.function.instruction(*id).ok_or_else(bad))
            .collect::<Result<Vec<_>, _>>()?;
        let Some((end, rest)) = instructions.split_first() else {
            return Err(bad());
        };
        if end.operation != (Operation::BorrowEnd { loan: source })
            || end.origin != Origin::Source(span)
        {
            return Err(bad());
        }
        match (self.drop, rest) {
            (None, []) => {}
            (Some(fact), [instruction])
                if instruction.operation == (Operation::Drop { owner })
                    && instruction.origin == Origin::Source(fact.value_origin()) => {}
            _ => return Err(bad()),
        }
        Ok(())
    }
}
