//! Re-materialize validated values at each use; never traverse declaration initializers.
use lang_frontend::{ownership_checking::ConstantMaterializationKind, type_checking::ConstValue};

use super::*;

impl ExpressionLowerer<'_> {
    pub(super) fn lower_constant(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let Some(descriptor) = self
            .typed
            .constants()
            .and_then(|facts| facts.use_at(expression))
        else {
            return Ok(None);
        };
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
        let plan = self
            .owned
            .constant_materializations()
            .and_then(|facts| facts.plan_at(expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let planned = plan.descriptor();
        if planned.expression() != descriptor.expression()
            || planned.target() != descriptor.target()
            || planned.ty() != descriptor.ty()
            || planned.value() != descriptor.value()
        {
            return Err(error(LoweringErrorKind::MismatchedAnalysis, span));
        }
        let ty = self.expression_ssa_type(expression, span)?;
        let operation = match (plan.kind(), descriptor.value()) {
            (ConstantMaterializationKind::InlineCopy, ConstValue::Boolean(value)) => {
                Operation::Constant(ScalarConstant::Boolean(*value))
            }
            (ConstantMaterializationKind::InlineCopy, ConstValue::Integer { value, .. }) => {
                Operation::Constant(ScalarConstant::Integer(*value))
            }
            (ConstantMaterializationKind::InlineCopy, ConstValue::Char(value)) => {
                Operation::Constant(ScalarConstant::Char(u32::from(*value)))
            }
            (ConstantMaterializationKind::StringTemporary, ConstValue::String(bytes)) => {
                Operation::StringLiteral {
                    string: ty,
                    bytes: bytes.to_vec(),
                }
            }
            _ => return Err(error(LoweringErrorKind::MismatchedAnalysis, span)),
        };
        let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
        Ok(Some(LoweredValue::Value(value(results[0]))))
    }
}
