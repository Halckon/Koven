//! Branch entry state, tail delivery and source statement locations.
use super::*;
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
        result_source_loans: BTreeMap<UnitSymbolId, Vec<crate::ssa::model::LoanId>>,
        closure_bindings: BTreeMap<UnitSymbolId, UnitExpressionId>,
        capture_loans: BTreeMap<(UnitExpressionId, usize), crate::ssa::model::LoanId>,
        pending_operands: Vec<EntityId>,
        temporaries: BTreeMap<UnitExpressionId, ValueId>,
        drop_point: UnitDropPoint,
        result_required: bool,
    ) -> Result<Option<BranchExit>, LoweringError> {
        self.block = block;
        self.current_receiver = receiver;
        self.consumed_receiver = consumed_receiver;
        let entry_symbols = bindings.keys().copied().collect();
        self.bindings = bindings;
        self.borrow_bindings = borrow_bindings;
        self.result_source_loans = result_source_loans;
        self.closure_bindings = closure_bindings;
        self.capture_loans = capture_loans;
        self.pending_operands = pending_operands;
        let entry_temporaries = temporaries.keys().copied().collect::<Vec<_>>();
        self.temporaries = temporaries;
        let result_expression = result_required
            .then(|| self.control_tail_expression(statement))
            .transpose()?;
        let result = if result_required {
            self.lower_tail_value_body(statement)?
        } else {
            self.lower_statement(statement)?
        };
        if result == LoweredValue::Diverged {
            return Ok(None);
        }
        if let (Some(expression), LoweredValue::Value(value)) = (result_expression, result) {
            self.transfer_owned_expression(expression, value, self.statement_span(statement)?)?;
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
            closure_bindings: self.closure_bindings.clone(),
            capture_loans: self.capture_loans.clone(),
            result_source_loans: self.result_source_loans.clone(),
            pending_operands: self.pending_operands.clone(),
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
            _ => return self.lower_statement(statement),
        };
        let elements = elements.clone();
        let Some((&last, prefix)) = elements.split_last() else {
            return Ok(LoweredValue::Unit);
        };
        for &element in prefix {
            if self.lower_statement(element)? == LoweredValue::Diverged {
                return Ok(LoweredValue::Diverged);
            }
        }
        let result = self.lower_statement(last)?;
        if result != LoweredValue::Diverged {
            self.emit_drops(UnitDropPoint::AfterStatement(UnitStatementId::new(
                self.source_unit,
                statement,
            )))?;
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
