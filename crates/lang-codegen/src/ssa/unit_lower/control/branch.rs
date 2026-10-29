//! Branch entry state, tail delivery and source statement locations.
use super::*;
use crate::ssa::statement_core::{self, StatementLowering};
use lang_frontend::parser::Statement;

/// Retain shared statement ordering while materializing a Nothing? tail in its
/// already checked control-result representation, including nested null controls.
struct NullTail<'a, 'b> {
    lowerer: &'a mut UnitExpressionLowerer<'b>,
    tail: ExpressionId,
    string_result: (UnitTypeId, SsaTypeId),
}

impl StatementLowering for NullTail<'_, '_> {
    fn lower_inner(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError> {
        if matches!(self.lowerer.parsed.ast().statements().get(statement)
            .map(|node| node.payload()), Ok(Statement::Expression { expression }) if *expression == self.tail)
        {
            return self
                .lowerer
                .lower_joined_null(self.tail, self.string_result);
        }
        self.lowerer.lower_statement_inner(statement)
    }

    fn emit_after_statement(&mut self, statement: StatementId) -> Result<(), LoweringError> {
        StatementLowering::emit_after_statement(self.lowerer, statement)
    }
}

impl UnitExpressionLowerer<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn lower_if_branch(
        &mut self,
        block: BlockId,
        statement: StatementId,
        receiver: Option<super::super::ReceiverBinding>,
        consumed_receiver: Option<super::super::ConsumedReceiver>,
        bindings: BTreeMap<UnitSymbolId, LoweredValue>,
        borrow_bindings: BTreeMap<UnitSymbolId, crate::ssa::model::LoanId>,
        nullable_proofs: super::super::NullableProofs,
        result_source_loans: BTreeMap<UnitSymbolId, Vec<crate::ssa::model::LoanId>>,
        closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
        capture_loans: BTreeMap<(UnitExpressionId, usize), crate::ssa::model::LoanId>,
        pending_operands: Vec<EntityId>,
        pending_call_frames: Vec<super::super::call_lifetimes::PendingCallFrame>,
        temporaries: BTreeMap<UnitExpressionId, ValueId>,
        drop_point: UnitDropPoint,
        result_plan: ControlResult,
    ) -> Result<Option<BranchExit>, LoweringError> {
        if !matches!(drop_point, UnitDropPoint::BranchExit { control, .. } if control == result_plan.control)
        {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                self.statement_span(statement)?,
            ));
        }
        let result_required = result_plan.required;
        self.block = block;
        self.current_receiver = receiver;
        self.consumed_receiver = consumed_receiver;
        let entry_symbols = bindings.keys().copied().collect();
        self.bindings = bindings;
        self.borrow_bindings = borrow_bindings;
        self.nullable_proofs = nullable_proofs;
        self.result_source_loans = result_source_loans;
        self.closure_bindings = closure_bindings;
        self.capture_loans = capture_loans;
        self.pending_operands = pending_operands;
        self.pending_call_frames = pending_call_frames;
        let entry_temporaries = temporaries.keys().copied().collect::<Vec<_>>();
        self.temporaries = temporaries;
        let result_expression = result_required
            .then(|| self.control_tail_expression(statement))
            .transpose()?;
        let string_result = result_plan.string;
        let null_tail = result_expression
            .zip(string_result)
            .and_then(|(tail, string_result)| {
                let ty = self
                    .typed
                    .expression_type(UnitExpressionId::new(self.source_unit, tail))?;
                (matches!(self.typed.types().get(ty), Some(UnitTypeKind::Nullable(inner))
                        if builtin_type(self.typed, *inner) == Some(BuiltinType::Nothing)))
                .then_some((tail, string_result))
            });
        let mut result = if result_required {
            self.lower_tail_value_body_with_null(statement, null_tail)?
        } else {
            self.lower_statement(statement)?
        };
        if result == LoweredValue::Diverged {
            return Ok(None);
        }
        if let (Some(expression), LoweredValue::Value(value)) = (result_expression, result) {
            if let Some((expected, _)) = string_result.filter(|_| null_tail.is_none()) {
                let (value, transferred) = self.adapt_owned_value_to_expected(
                    expression,
                    value,
                    expected,
                    self.statement_span(statement)?,
                )?;
                if !transferred {
                    self.transfer_owned_expression(
                        expression,
                        value,
                        self.statement_span(statement)?,
                    )?;
                }
                result = LoweredValue::Value(value);
            } else if null_tail.is_none() {
                self.transfer_owned_expression(expression, value, self.statement_span(statement)?)?;
            }
        }
        let expected_result = if result_required {
            matches!(result, LoweredValue::Value(_))
        } else {
            result == LoweredValue::Unit
        };
        if !expected_result || self.temporaries.keys().copied().ne(entry_temporaries) {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                self.statement_span(statement)?,
            ));
        }
        self.emit_drops(drop_point)?;
        self.discard_non_entry_bindings(&entry_symbols, self.statement_span(statement)?)?;
        Ok(Some(BranchExit {
            block: self.block,
            result,
            receiver: self.current_receiver,
            consumed_receiver: self.consumed_receiver,
            bindings: self.bindings.clone(),
            borrow_bindings: self.borrow_bindings.clone(),
            nullable_proofs: self.nullable_proofs.clone(),
            closure_bindings: self.closure_bindings.clone(),
            capture_loans: self.capture_loans.clone(),
            result_source_loans: self.result_source_loans.clone(),
            pending_operands: self.pending_operands.clone(),
            pending_call_frames: self.pending_call_frames.clone(),
            temporaries: self.temporaries.clone(),
        }))
    }

    pub(in super::super) fn control_tail_expression(
        &self,
        statement: StatementId,
    ) -> Result<ExpressionId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let tail = match node.payload() {
            lang_frontend::parser::Statement::Expression { expression } => return Ok(*expression),
            lang_frontend::parser::Statement::ControlBody { elements } => elements.last(),
            _ => None,
        }
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
        let tail = self
            .parsed
            .ast()
            .statements()
            .get(*tail)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
        match tail.payload() {
            lang_frontend::parser::Statement::Expression { expression } => Ok(*expression),
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, tail.span())),
        }
    }

    pub(in super::super) fn lower_tail_value_body(
        &mut self,
        statement: StatementId,
    ) -> Result<LoweredValue, LoweringError> {
        self.lower_tail_value_body_with_null(statement, None)
    }

    fn lower_tail_value_body_with_null(
        &mut self,
        statement: StatementId,
        null_tail: Option<(ExpressionId, (UnitTypeId, SsaTypeId))>,
    ) -> Result<LoweredValue, LoweringError> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let elements = match node.payload() {
            lang_frontend::parser::Statement::ControlBody { elements }
            | lang_frontend::parser::Statement::LambdaBody { elements } => elements,
            _ => {
                return if let Some((tail, string_result)) = null_tail {
                    statement_core::lower_statement(
                        &mut NullTail {
                            lowerer: self,
                            tail,
                            string_result,
                        },
                        statement,
                    )
                } else {
                    self.lower_statement(statement)
                };
            }
        };
        let elements = elements.clone();
        if let Some((tail, string_result)) = null_tail {
            statement_core::lower_tail_sequence(
                &mut NullTail {
                    lowerer: self,
                    tail,
                    string_result,
                },
                statement,
                &elements,
            )
        } else {
            statement_core::lower_tail_sequence(self, statement, &elements)
        }
    }

    /// Preserve nested control evaluation and Group cleanup in the published joined type.
    fn lower_joined_null(
        &mut self,
        expression: ExpressionId,
        string_result: (UnitTypeId, SsaTypeId),
    ) -> Result<LoweredValue, LoweringError> {
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let operands = self
            .nullable_when_group_chain(UnitExpressionId::new(self.source_unit, expression), span)?;
        for &id in &operands {
            self.guard_nullable_when_expression(id, span)?;
            let ty = self
                .typed
                .expression_type(id)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(self.typed.types().get(ty), Some(UnitTypeKind::Nullable(inner))
                if builtin_type(self.typed, *inner) == Some(BuiltinType::Nothing))
                || self.typed.copyability(ty) != Copyability::Copyable
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let literal = operands
            .last()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(literal.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let terminal = literal.expression();
        let terminal_span = node.span();
        let payload = node.payload().clone();
        let result = match payload {
            Expression::Literal(LiteralKind::Null) => self.append_scalar(
                Operation::NullableNull {
                    nullable: string_result.1,
                },
                string_result.1,
                terminal_span,
            )?,
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let plan = self.control_result(terminal, Some(string_result), terminal_span)?;
                self.lower_conditional(
                    terminal,
                    condition,
                    then_branch,
                    else_branch,
                    0,
                    1,
                    terminal_span,
                    plan,
                )?
            }
            Expression::When {
                subject, entries, ..
            } => self.lower_boolean_when(
                terminal,
                subject,
                &entries,
                terminal_span,
                Some(string_result),
            )?,
            _ => {
                return Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    terminal_span,
                ));
            }
        };
        if result == LoweredValue::Diverged {
            return Ok(result);
        }
        for id in operands.into_iter().rev() {
            self.emit_drops(UnitDropPoint::AfterExpression(id))?;
        }
        Ok(result)
    }

    pub(in super::super) fn statement_span(
        &self,
        statement: StatementId,
    ) -> Result<Span, LoweringError> {
        self.parsed
            .ast()
            .statements()
            .get(statement)
            .map(|statement| statement.span())
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })
    }
}
