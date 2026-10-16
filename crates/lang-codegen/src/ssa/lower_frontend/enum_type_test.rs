//! 普通 enum 类型测试只比较同一具体 root 的已登记 case tag。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{DropPoint, DropTarget},
    parser::{Expression, TypeRef},
    source::Span,
    type_checking::{BuiltinType, Copyability, ExpressionCategory, NominalKind, TypeKind},
};

use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, builtin_type, error, value,
};
use crate::ssa::model::{
    ComparisonOperator, EntityId, EntityType, Operation, ScalarConstant, SsaTypeKind, ValueId,
};

impl ExpressionLowerer<'_> {
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
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::TypeTest {
            expression: operand,
            type_ref,
            negated,
            ..
        } = *node.payload()
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let result_type = self
            .typed
            .expression_type(expression)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if builtin_type(self.typed, result_type) != Some(BuiltinType::Boolean)
            || self.expression_ssa_type(expression, span)?
                != self.ssa_builtin(BuiltinType::Boolean, span)?
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }

        let operand_type = self
            .typed
            .expression_type(operand)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let operand_root = match self.typed.types().get(operand_type) {
            Some(TypeKind::EnumCase { root, .. }) => *root,
            _ => operand_type,
        };
        let operand_root = self.resolve_type(operand_root, span)?;
        let Some(TypeKind::Nominal { nominal, .. }) = self.typed.types().get(operand_root) else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !self.typed.nominals().iter().any(|descriptor| {
            descriptor.id() == *nominal && descriptor.kind() == NominalKind::EnumClass
        }) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let tagged = self
            .type_ids
            .get(&operand_root)
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(
            self.ssa_types.get(tagged.index()),
            Some(SsaTypeKind::TaggedUnion { .. })
        ) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }

        let target = self
            .typed
            .type_ref_type(type_ref)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(TypeKind::EnumCase { case, root }) = self.typed.types().get(target) else {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        };
        let target_root = self.resolve_type(*root, span)?;
        if target_root != operand_root || self.type_ids.get(&target_root) != Some(&tagged) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let case = self
            .typed
            .enum_cases()
            .iter()
            .find(|descriptor| descriptor.id() == *case && descriptor.root() == *nominal)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let type_node = self
            .parsed
            .ast()
            .type_refs()
            .get(type_ref)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let TypeRef::Qualified { segments, .. } = type_node.payload() else {
            return Err(error(LoweringErrorKind::UnsupportedNode, type_node.span()));
        };
        let symbol = segments
            .last()
            .and_then(|segment| self.references.get(&super::span_key(segment.name_span)))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, type_node.span()))?;
        if symbol != case.type_symbol() {
            return Err(error(LoweringErrorKind::MissingFact, type_node.span()));
        }
        let (variant, _) = self
            .enum_payloads
            .get(&(tagged, case.id()))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, type_node.span()))?;

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
                .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
            let Expression::Group { expression } = *node.payload() else {
                break;
            };
            evaluated = expression;
            operands.push(evaluated);
        }
        // TypeTest currently publishes named Read cleanup, not a general
        // temporary-result cleanup plan. Never invent the missing drop.
        let temporary = self.typed.copyability(operand_root) == Some(Copyability::MoveOnly)
            && (self.typed.expression_category(evaluated) == Some(ExpressionCategory::Temporary)
                || self.typed.construction(evaluated).is_some());
        if temporary
            && !self.owned.drops().iter().any(|fact| {
                let DropTarget::Temporary(target) = fact.target() else {
                    return false;
                };
                let DropPoint::AfterExpression(point) = fact.point() else {
                    return false;
                };
                operands.contains(&target) && (point == expression || operands.contains(&point))
            })
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = match self.lower_expression(evaluated)? {
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Value(value) => value,
            LoweredValue::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        for &operand in operands.iter().rev() {
            if self.typed.expression_category(operand) == Some(ExpressionCategory::Temporary)
                && self
                    .typed
                    .expression_type(operand)
                    .and_then(|ty| self.typed.copyability(ty))
                    == Some(Copyability::MoveOnly)
            {
                self.temporaries.insert(operand.index(), owner);
            }
        }
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|data| data.ty)
            != Some(EntityType::Value(tagged))
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        let result = self.lower_enum_tag_test(owner, variant, negated, span)?;
        for operand in operands.into_iter().rev() {
            self.emit_drops(DropPoint::AfterExpression(operand))?;
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
        let (_, tag) = self.append(
            Operation::TaggedDiscriminant { owner },
            vec![EntityType::Value(integer)],
            span,
        )?;
        let (_, expected) = self.append(
            Operation::Constant(ScalarConstant::Integer(variant as i128)),
            vec![EntityType::Value(integer)],
            span,
        )?;
        let (_, matches) = self.append(
            Operation::Compare {
                operator: ComparisonOperator::Equal,
                left: value(tag[0]),
                right: value(expected[0]),
            },
            vec![EntityType::Value(boolean)],
            span,
        )?;
        let matches = value(matches[0]);
        if !negated {
            return Ok(matches);
        }
        let (_, result) = self.append(
            Operation::BooleanNot { operand: matches },
            vec![EntityType::Value(boolean)],
            span,
        )?;
        Ok(value(result[0]))
    }
}
