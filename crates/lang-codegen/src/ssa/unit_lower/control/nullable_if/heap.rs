//! A fresh owned HeapOwner If proof, independent of any retired When generation.
//! Only one direct root-Borrow call and an empty opposite arm are admitted here.
mod field;

use super::*;
use lang_frontend::{
    ownership_checking::UnitCallArgumentOwnershipKind,
    type_checking::{
        CallableResultSource, ParameterMode, UnitCallTarget, UnitNullComparisonDescriptor,
        UnitTypeId,
    },
};

#[derive(Clone, Copy)]
pub(super) struct NamedNullableIfRoot {
    pub(super) symbol: UnitSymbolId,
    pub(super) declared: UnitTypeId,
    pub(super) narrowed: UnitTypeId,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn prepare_heap_nullable_if_entry(
        &self,
        control: UnitExpressionId,
        condition: ExpressionId,
        span: Span,
    ) -> Result<Option<NullableIfEntry>, LoweringError> {
        // The original no-else, While, Rc and Map entries retain their own rules.
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
            else_branch: Some(else_branch),
            ..
        } = *node.payload()
        else {
            return Ok(None);
        };
        if control.source_unit() != self.source_unit || actual != condition || node.span() != span {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // Shape is a capability boundary, not a substitute for typed null facts.
        let Some(comparison) = self.map_borrow_condition(condition, span)? else {
            return Ok(None);
        };
        let root = comparison.symbol();
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
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
        ) || self.borrow_bindings.contains_key(&root)
            || super::super::super::map_borrow::owned_remove_origin(self.function, owner)
        {
            return Ok(None);
        }
        let Some(UnitTypeKind::Nullable(narrowed)) =
            self.typed.types().get(comparison.nullable_type())
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let root = NamedNullableIfRoot {
            symbol: root,
            declared: comparison.nullable_type(),
            narrowed: *narrowed,
        };
        let (non_null, null) = if comparison.non_null_when_true() {
            (then_branch, else_branch)
        } else {
            (else_branch, then_branch)
        };
        if !self.map_if_body_is_empty(Some(null), span)? {
            return Ok(None);
        }
        let Some(call) = self.heap_if_direct_borrow_call(non_null, span)? else {
            return Ok(None);
        };
        if !self.heap_if_source_context(control, span)?
            || !self.heap_if_call_qualifies(call, root, span)?
        {
            return Ok(None);
        }
        self.validate_named_nullable_if_condition(condition, comparison, root, span)?;
        // Reuse the existing actual storage/current-Value check, not its != parser.
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

    fn heap_if_source_context(
        &self,
        control: UnitExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let key = self
            .source_plan
            .source(self.source_token)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.function_ids.get(key) != Some(&self.function.id()) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if key.deinit_owner().is_some() || !key.type_arguments().is_empty() {
            return Ok(false);
        }
        let UnitCallableTarget::Declaration(target) = key.target() else {
            return Ok(false);
        };
        let declaration = self
            .names
            .names()
            .index()
            .declarations()
            .get(target.index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if declaration.id() != target
            || declaration.source_unit() != self.source_unit
            || declaration.name_span().source_id() != self.parsed.source_id()
            || span.source_id() != self.parsed.source_id()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (_, item, _) = unwrap_modified(self.parsed, declaration.root())?;
        let Item::Function { .. } = item else {
            return Ok(false);
        };
        // An enclosing call, constructor, assignment RHS or lambda may hold
        // operands outside CFG carriers. Only ordinary control ancestors qualify.
        Ok(!self.parsed.ast().expressions().iter().any(|(id, node)| {
            id != control.expression()
                && super::super::super::map_iteration::contains(node.span(), span)
                && !matches!(
                    node.payload(),
                    Expression::Group { .. } | Expression::If { .. } | Expression::When { .. }
                )
        }))
    }

    fn heap_if_direct_borrow_call(
        &self,
        body: StatementId,
        span: Span,
    ) -> Result<Option<ExpressionId>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .statements()
            .get(body)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::ControlBody { elements } = node.payload() else {
            return Ok(None);
        };
        let [statement] = elements.as_slice() else {
            return Ok(None);
        };
        let node = self
            .parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Statement::Expression { expression } = *node.payload() else {
            return Ok(None);
        };
        let expression = self.ungroup_map_if_expression(expression, span)?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        Ok(matches!(node.payload(), Expression::Call { .. }).then_some(expression))
    }

    pub(super) fn validate_named_nullable_if_condition(
        &self,
        condition: ExpressionId,
        descriptor: UnitNullComparisonDescriptor,
        root: NamedNullableIfRoot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let comparison = self.ungroup_map_if_expression(condition, span)?;
        if descriptor.expression() != UnitExpressionId::new(self.source_unit, comparison)
            || root.symbol.source_unit() != self.source_unit
            || self.typed.symbol_type(root.symbol) != Some(root.declared)
            || self.expression_builtin_type(condition, span)? != Some(BuiltinType::Boolean)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let nodes = self.parsed.ast().expressions();
        let node = nodes
            .get(comparison)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Binary {
            left,
            operator,
            right,
            ..
        } = *node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if !matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual)
            || descriptor.non_null_when_true() != (operator == BinaryOperator::NotEqual)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let left_leaf = self.ungroup_map_if_expression(left, span)?;
        let right_leaf = self.ungroup_map_if_expression(right, span)?;
        let is_null = |id| {
            nodes
                .get(id)
                .is_ok_and(|node| matches!(node.payload(), Expression::Literal(LiteralKind::Null)))
        };
        let subject = match (is_null(left_leaf), is_null(right_leaf)) {
            (true, false) => right,
            (false, true) => left,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if !self.heap_if_root_operand(subject, root, false, span)? {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    fn heap_if_call_qualifies(
        &self,
        expression: ExpressionId,
        root: NamedNullableIfRoot,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let call = UnitExpressionId::new(self.source_unit, expression);
        let nodes = self.parsed.ast().expressions();
        let node = nodes
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call {
            callee, arguments, ..
        } = node.payload()
        else {
            return Ok(false);
        };
        let callee = self.ungroup_map_if_expression(*callee, span)?;
        if !nodes
            .get(callee)
            .is_ok_and(|node| matches!(node.payload(), Expression::Name))
        {
            return Ok(false);
        }
        let descriptor = self
            .typed
            .call(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            return Ok(false);
        };
        if descriptor.receiver().is_some()
            || descriptor.aborts()
            || descriptor.prints_line()
            || descriptor.result_source() != CallableResultSource::Owned
            || !descriptor.instance().type_arguments().is_empty()
            || self.typed.construction(call).is_some()
            || self.typed.ownership_primitive(call).is_some()
            || arguments.is_empty()
            || descriptor
                .arguments()
                .iter()
                .any(|mapping| mapping.mode() != ParameterMode::Borrow || mapping.crosses_thread())
            || self.typed.copyability(descriptor.return_type()) != Copyability::Copyable
            || builtin_type(self.typed, descriptor.return_type()) == Some(BuiltinType::Nothing)
        {
            return Ok(false);
        }
        let route = self
            .source_plan
            .call_site(self.source_token, call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if descriptor.expression() != call
            || descriptor.arguments().len() != arguments.len()
            || route.key().target() != UnitCallableTarget::Declaration(target)
            || !route.key().type_arguments().is_empty()
            || !self.function_ids.contains_key(route.key())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // No nested control or mutation can occur between the condition and this
        // exact call: the arm contains only this call, with pure Name/Group args.
        for (index, argument) in arguments.iter().enumerate() {
            if argument.mode_marker.is_some()
                || !self.heap_if_root_operand(argument.value, root, true, argument.span)?
            {
                return Ok(false);
            }
            let mut mappings = descriptor
                .arguments()
                .iter()
                .filter(|mapping| mapping.argument_index() == index);
            let Some(mapping) = mappings.next() else {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            };
            if mappings.next().is_some()
                || mapping.category() != ExpressionCategory::Place
                || mapping.parameter_type() != root.narrowed
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
            let argument_id = UnitExpressionId::new(self.source_unit, argument.value);
            let argument_span = nodes
                .get(argument.value)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, argument.span))?
                .span();
            let mut contracts = self
                .owned
                .call_argument_contracts_for(call)
                .filter(|contract| contract.argument() == argument_id);
            let Some(contract) = contracts.next() else {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            };
            let mut loans = self
                .owned
                .loans()
                .iter()
                .filter(|loan| loan.call() == call && loan.argument() == argument_id);
            let Some(loan) = loans.next() else {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            };
            if contracts.next().is_some()
                || loans.next().is_some()
                || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
                || contract.category() != ExpressionCategory::Place
                || contract.parameter_index() != mapping.parameter_index()
                || contract.parameter_type() != root.narrowed
                || contract.crosses_thread()
                || contract.argument_span() != argument_span
                || contract.loan_begin_span() != argument_span
                || contract.call_span() != node.span()
                || contract.parameter_span().is_none()
                || loan.kind() != FrontendLoanKind::Shared
                || loan.begin_span() != contract.loan_begin_span()
                || loan.end_span() != node.span()
                || loan.parameter_span() != contract.parameter_span()
                || !matches!(loan.target(), UnitLoanTarget::Place(place)
                    if place.is_root() && place.root() == root.symbol)
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
        }
        Ok(true)
    }

    fn heap_if_root_operand(
        &self,
        mut expression: ExpressionId,
        root: NamedNullableIfRoot,
        narrowed: bool,
        span: Span,
    ) -> Result<bool, LoweringError> {
        loop {
            let id = UnitExpressionId::new(self.source_unit, expression);
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(node.payload(), Expression::Name | Expression::Group { .. }) {
                return Ok(false);
            }
            let expected = if narrowed {
                root.narrowed
            } else {
                root.declared
            };
            if self.typed.expression_type(id) != Some(expected)
                || self.typed.expression_category(id) != Some(ExpressionCategory::Place)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let fact = self.typed.non_null_use(id);
            if narrowed {
                if fact.is_none_or(|fact| {
                    fact.expression() != id
                        || fact.symbol() != root.symbol
                        || fact.declared_type() != root.declared
                        || fact.narrowed_type() != root.narrowed
                }) {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            } else if fact.is_some() {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            match node.payload() {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Name => {
                    if self.references.get(&span_key(node.span())) != Some(&root.symbol) {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    return Ok(true);
                }
                _ => return Ok(false),
            }
        }
    }
}
