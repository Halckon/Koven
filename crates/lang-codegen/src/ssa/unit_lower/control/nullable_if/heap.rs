//! Shared condition validation for String? and HeapOwner field If proofs.
mod field;

use super::*;
use lang_frontend::type_checking::{UnitCallableTarget, UnitNullComparisonDescriptor, UnitTypeId};

#[derive(Clone, Copy)]
pub(super) struct NamedNullableIfRoot {
    pub(super) symbol: UnitSymbolId,
    pub(super) declared: UnitTypeId,
    pub(super) narrowed: UnitTypeId,
}

impl UnitExpressionLowerer<'_> {
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
