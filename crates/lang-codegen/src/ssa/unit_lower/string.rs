//! Plain strings and the bounded compatibility Char interpolation path.

use super::{
    BuiltinType, EntityId, Expression, ExpressionId, LoweredValue, LoweringError,
    LoweringErrorKind, Operation, Origin, Span, UnitExpressionId, UnitExpressionLowerer,
    UnitTypeKind, lowering_error, string_literal,
};
use lang_frontend::type_checking::{UnitDisplayTarget, UnitInterpolationPart};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_string_literal(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let string = self.expression_ssa_type(expression, span)?;
        let source_text = self
            .sources
            .source_text(self.parsed.source_id())
            .map_err(|_| lowering_error(LoweringErrorKind::MismatchedSource, span))?;
        let id = UnitExpressionId::new(self.source_unit, expression);
        let Some(parts) = self.typed.interpolation_plan(id) else {
            let bytes = string_literal::decode_plain(self.parsed, source_text, expression)?
                .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
            return self.append_scalar(Operation::StringLiteral { string, bytes }, string, span);
        };

        // Only completing scalar name reads are admitted. A partial string
        // cannot be live across a source-level transfer on this route.
        let mut result = None;
        for part in parts {
            let pending = self.pending_operands.len();
            if let Some(prefix) = result {
                self.pending_operands.push(EntityId::Value(prefix));
            }
            let value = match *part {
                UnitInterpolationPart::Text(text) if text.source_id() == span.source_id() => {
                    let mut bytes = Vec::new();
                    string_literal::decode_text(source_text, text, &mut bytes)?;
                    self.append_scalar(Operation::StringLiteral { string, bytes }, string, text)?
                }
                UnitInterpolationPart::Format {
                    operand,
                    target: UnitDisplayTarget::Builtin(BuiltinType::Char),
                } if operand.source_unit() == self.source_unit => {
                    let root = self.ungroup_map_if_expression(operand.expression(), span)?;
                    let node = self
                        .parsed
                        .ast()
                        .expressions()
                        .get(root)
                        .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    if !matches!(node.payload(), Expression::Name)
                        || self.owned.interpolation_operand_completes(operand) != Some(true)
                        || self
                            .typed
                            .expression_type(operand)
                            .and_then(|ty| self.typed.types().get(ty))
                            != Some(&UnitTypeKind::Builtin(BuiltinType::Char))
                    {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                    let LoweredValue::Value(scalar) = self.lower(operand.expression())? else {
                        return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                    };
                    self.append_scalar(Operation::StringFromScalar { scalar }, string, span)?
                }
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            };
            let LoweredValue::Value(right) = value else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            result = Some(if result.is_some() {
                let Some(EntityId::Value(left)) = self.pending_operands.pop() else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                if self.pending_operands.len() != pending {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                }
                let joined = self.append_scalar(
                    Operation::StringConcat {
                        left: EntityId::Value(left),
                        right: EntityId::Value(right),
                    },
                    string,
                    span,
                )?;
                for owner in [right, left] {
                    self.function
                        .append_instruction(
                            self.block,
                            Operation::Drop { owner },
                            Vec::new(),
                            Origin::Source(span),
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                }
                let LoweredValue::Value(joined) = joined else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                joined
            } else {
                right
            });
        }
        result
            .map(LoweredValue::Value)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }
}
