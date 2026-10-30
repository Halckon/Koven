//! A fresh If view for one terminal Copyable Int field comparison.
//! The published ASAP drop closes the view before the nested If forks.
use super::*;
use lang_frontend::{
    ownership_checking::UnitDropTarget,
    type_checking::{NominalKind, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver},
};

#[cfg(test)]
mod tests;

impl UnitExpressionLowerer<'_> {
    pub(in crate::ssa::unit_lower::control::nullable_if) fn prepare_heap_field_nullable_if_entry(
        &self,
        control: UnitExpressionId,
        condition: ExpressionId,
        span: Span,
    ) -> Result<Option<NullableIfEntry>, LoweringError> {
        // Do not extend the existing While, Rc, Map, root-Borrow or live-proof entries.
        if self.thunk_expression.is_some()
            || !self.nullable_proofs.is_empty()
            || self.current_while_proof_scope().is_some()
            || !self.loops.is_empty()
            || !self.substitutions.is_empty()
        {
            return Ok(None);
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(control.expression())
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::If {
            condition: actual,
            then_branch,
            else_branch,
            ..
        } = *node.payload()
        else {
            return Ok(None);
        };
        if control.source_unit() != self.source_unit || actual != condition || node.span() != span {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let Some(comparison) = self.map_borrow_condition(condition, span)? else {
            return Ok(None);
        };
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&comparison.symbol()).copied()
        else {
            return Ok(None);
        };
        let Some(EntityType::Value(storage)) = self
            .function
            .entity(EntityId::Value(owner))
            .map(|data| data.ty)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(SsaTypeKind::NullableHandle { inner }) = self.ssa_types.get(storage.index())
        else {
            return Ok(None);
        };
        if !matches!(
            self.ssa_types.get(inner.index()),
            Some(SsaTypeKind::HeapOwner { .. })
        ) || self.borrow_bindings.contains_key(&comparison.symbol())
            || crate::ssa::unit_lower::map_borrow::owned_remove_origin(self.function, owner)
        {
            return Ok(None);
        }
        let Some(UnitTypeKind::Nullable(narrowed)) =
            self.typed.types().get(comparison.nullable_type())
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let root = NamedNullableIfRoot {
            symbol: comparison.symbol(),
            declared: comparison.nullable_type(),
            narrowed: *narrowed,
        };
        let (non_null, null) = if comparison.non_null_when_true() {
            (Some(then_branch), else_branch)
        } else {
            (else_branch, Some(then_branch))
        };
        let Some(non_null) = non_null else {
            return Ok(None);
        };
        if !self.map_if_body_is_empty(null, span)?
            || !self.heap_if_source_context(control, span)?
            || !self.heap_if_has_terminal_int_field(non_null, root, span)?
        {
            return Ok(None);
        }
        self.validate_named_nullable_if_condition(condition, comparison, root, span)?;
        // The current Value and registered storage are checked, never retyped.
        let (nullable, inner) = self.direct_nullable_while_entry(comparison, span)?;
        if storage != nullable
            || self.type_ids.get(&root.narrowed) != Some(&inner)
            || self.typed.copyability(root.declared) != Copyability::MoveOnly
            || self.typed.copyability(root.narrowed) != Copyability::MoveOnly
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.ensure_no_while_proof_carriers(span)?;
        if !self.pending_call_frames.is_empty() || !self.pending_operands.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(Some(NullableIfEntry {
            root: root.symbol,
            owner,
            inner,
            scope: control,
            non_null_when_true: comparison.non_null_when_true(),
            consume: None,
        }))
    }

    fn heap_if_has_terminal_int_field(
        &self,
        body: StatementId,
        root: NamedNullableIfRoot,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let statements = self.parsed.ast().statements();
        let node = statements
            .get(body)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::ControlBody { elements } = node.payload() else {
            return Ok(false);
        };
        let [statement] = elements.as_slice() else {
            return Ok(false);
        };
        let node = statements
            .get(*statement)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::Expression { expression } = *node.payload() else {
            return Ok(false);
        };
        let expressions = self.parsed.ast().expressions();
        let nested = self.ungroup_map_if_expression(expression, span)?;
        let node = expressions
            .get(nested)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::If { condition, .. } = *node.payload() else {
            return Ok(false);
        };
        let comparison = self.ungroup_map_if_expression(condition, span)?;
        let node = expressions
            .get(comparison)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = *node.payload()
        else {
            return Ok(false);
        };
        if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
            || self.expression_builtin_type(condition, span)? != Some(BuiltinType::Boolean)
        {
            return Ok(false);
        }
        let left_leaf = self.ungroup_map_if_expression(left, span)?;
        let right_leaf = self.ungroup_map_if_expression(right, span)?;
        let is_int = |id| {
            expressions.get(id).is_ok_and(|node| {
                matches!(node.payload(), Expression::Literal(LiteralKind::Integer(_)))
            })
        };
        let (mut field_expression, literal) = match (is_int(left_leaf), is_int(right_leaf)) {
            (true, false) => (right, left),
            (false, true) => (left, right),
            _ => return Ok(false),
        };
        if self.expression_builtin_type(literal, span)? != Some(BuiltinType::Int) {
            return Ok(false);
        }
        // Group can own the final-use drop, but no call/control can run before it.
        let mut field_chain = vec![UnitExpressionId::new(self.source_unit, field_expression)];
        loop {
            let node = expressions
                .get(field_expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression } => {
                    field_expression = *expression;
                    field_chain.push(UnitExpressionId::new(self.source_unit, field_expression));
                }
                _ => break,
            }
        }
        let field_id = UnitExpressionId::new(self.source_unit, field_expression);
        let Some(projection) = self.typed.aggregate_projection(field_id) else {
            return Ok(false);
        };
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Ok(false);
        };
        if receiver.source_unit() != self.source_unit
            || projection.expression() != field_id
            || projection.kind() != UnitAggregateProjectionKind::Field
            || self.typed.expression_type(field_id) != Some(projection.ty())
            || self.typed.copyability(projection.ty()) != Copyability::Copyable
            || self.expression_builtin_type(field_expression, span)? != Some(BuiltinType::Int)
            || !self.heap_if_root_operand(receiver.expression(), root, true, span)?
        {
            return Ok(false);
        }
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(root.narrowed)
        else {
            return Ok(false);
        };
        if !arguments.is_empty()
            || self
                .typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .is_none_or(|nominal| nominal.kind() != NominalKind::Class)
        {
            return Ok(false);
        }
        // This is an eligibility fact, not a new drop plan. Existing lower(field)
        // consumes it and retires the actual view before dropping its same owner.
        Ok(self
            .owned
            .drops()
            .iter()
            .filter(|fact| {
                fact.target() == UnitDropTarget::Named(root.symbol)
                    && matches!(fact.point(), UnitDropPoint::AfterExpression(expression)
                    if field_chain.contains(&expression))
            })
            .count()
            == 1)
    }
}
