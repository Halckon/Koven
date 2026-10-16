//! Source-qualified enum 类型测试只比较同一具体 root 的已登记 case tag。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{Expression, TypeRef},
    source::Span,
    type_checking::{
        BuiltinType, Copyability, ExpressionCategory, NominalKind, UnitExpressionId, UnitTypeKind,
        UnitTypeRefId,
    },
};

use super::{
    LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error, require_value,
    resolve_concrete_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        ComparisonOperator, EntityId, EntityType, Operation, Origin, ScalarConstant, SsaTypeKind,
        ValueId,
    },
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_enum_type_test(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::TypeTest {
            expression: operand,
            type_ref,
            negated,
            ..
        } = *node.payload()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let result_type = self
            .typed
            .expression_type(UnitExpressionId::new(self.source_unit, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if builtin_type(self.typed, result_type) != Some(BuiltinType::Boolean)
            || self.expression_ssa_type(expression, span)?
                != self.ssa_builtin(BuiltinType::Boolean, span)?
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }

        let operand_type = self
            .typed
            .expression_type(UnitExpressionId::new(self.source_unit, operand))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let operand_root = match self.typed.types().get(operand_type) {
            Some(UnitTypeKind::EnumCase { root, .. }) => *root,
            _ => operand_type,
        };
        let operand_root = resolve_concrete_type(
            self.typed,
            operand_root,
            self.substitutions,
            self.static_self,
            span,
        )?;
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.typed.types().get(operand_root)
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .filter(|nominal| nominal.kind() == NominalKind::EnumClass)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let tagged = self
            .type_ids
            .get(&operand_root)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(
            self.ssa_types.get(tagged.index()),
            Some(SsaTypeKind::TaggedUnion { .. })
        ) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }

        let target = self
            .typed
            .type_ref_type(UnitTypeRefId::new(self.source_unit, type_ref))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(UnitTypeKind::EnumCase { case, root }) = self.typed.types().get(target) else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let target_root = resolve_concrete_type(
            self.typed,
            *root,
            self.substitutions,
            self.static_self,
            span,
        )?;
        if target_root != operand_root || self.type_ids.get(&target_root) != Some(&tagged) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let case = nominal
            .enum_cases()
            .iter()
            .find(|descriptor| descriptor.type_symbol() == *case)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let type_node = self
            .parsed
            .ast()
            .type_refs()
            .get(type_ref)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let TypeRef::Qualified { segments, .. } = type_node.payload() else {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                type_node.span(),
            ));
        };
        let symbol = segments
            .last()
            .and_then(|segment| {
                self.type_references
                    .get(&super::span_key(segment.name_span))
            })
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, type_node.span()))?;
        if symbol != case.type_symbol() {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                type_node.span(),
            ));
        }
        let (variant, _) = self
            .enum_payloads
            .get(&(tagged, case.type_symbol()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, type_node.span()))?;

        // The final Read of a named owner can have an ASAP drop on the leaf.
        // Evaluate once, retaining transparent aliases, and defer that cleanup
        // until the discriminator has read the owner.
        let mut operands = vec![operand];
        let mut evaluated = operand;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(evaluated)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Group { expression } = *node.payload() else {
                break;
            };
            evaluated = expression;
            operands.push(evaluated);
        }
        // TypeTest currently publishes named Read cleanup, not a general
        // temporary-result cleanup plan. Never invent the missing drop.
        let evaluated_id = UnitExpressionId::new(self.source_unit, evaluated);
        let temporary = self.typed.copyability(operand_root) == Copyability::MoveOnly
            && (self.typed.expression_category(evaluated_id)
                == Some(ExpressionCategory::Temporary)
                || self.typed.construction(evaluated_id).is_some());
        if temporary
            && !self.owned.drops().iter().any(|fact| {
                let UnitDropTarget::Temporary(target) = fact.target() else {
                    return false;
                };
                let UnitDropPoint::AfterExpression(point) = fact.point() else {
                    return false;
                };
                target.source_unit() == self.source_unit
                    && point.source_unit() == self.source_unit
                    && operands.contains(&target.expression())
                    && (point.expression() == expression || operands.contains(&point.expression()))
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = match self.lower_expression(evaluated)? {
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Value(value) => value,
            LoweredValue::Unit => {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        };
        for &operand in operands.iter().rev() {
            let operand = UnitExpressionId::new(self.source_unit, operand);
            if self
                .typed
                .expression_type(operand)
                .map(|ty| self.typed.copyability(ty))
                == Some(Copyability::MoveOnly)
                && (self.typed.expression_category(operand) == Some(ExpressionCategory::Temporary)
                    || self.typed.construction(operand).is_some())
            {
                self.temporaries.insert(operand, owner);
            }
        }
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|data| data.ty)
            != Some(EntityType::Value(tagged))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let result = self.lower_enum_tag_test(owner, variant, negated, span)?;
        for operand in operands.into_iter().rev() {
            self.emit_drops(UnitDropPoint::AfterExpression(UnitExpressionId::new(
                self.source_unit,
                operand,
            )))?;
        }
        Ok(LoweredValue::Value(result))
    }

    pub(super) fn lower_enum_tag_test(
        &mut self,
        owner: ValueId,
        variant: usize,
        negated: bool,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let integer = self.ssa_builtin(BuiltinType::Int, span)?;
        let boolean = self.ssa_builtin(BuiltinType::Boolean, span)?;
        let (_, tag) = self
            .function
            .append_instruction(
                self.block,
                Operation::TaggedDiscriminant { owner },
                vec![EntityType::Value(integer)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let (_, expected) = self
            .function
            .append_instruction(
                self.block,
                Operation::Constant(ScalarConstant::Integer(
                    i128::try_from(variant)
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?,
                )),
                vec![EntityType::Value(integer)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let (_, matches) = self
            .function
            .append_instruction(
                self.block,
                Operation::Compare {
                    operator: ComparisonOperator::Equal,
                    left: require_value(tag[0], span)?,
                    right: require_value(expected[0], span)?,
                },
                vec![EntityType::Value(boolean)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let matches = require_value(matches[0], span)?;
        if !negated {
            return Ok(matches);
        }
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::BooleanNot { operand: matches },
                vec![EntityType::Value(boolean)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        require_value(results[0], span)
    }
}
