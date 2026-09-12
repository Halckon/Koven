//! 专用常量入口只消费同一轮物化事实；不重访声明 initializer。
use super::*;
use lang_frontend::{
    name_resolution::{
        DeclarationId, SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
    },
    ownership_checking::{ConstEnabledOwnedUnit, ConstantMaterializationKind},
    source::{SourceMap, Span},
    type_checking::{ConstEnabledTypedUnit, ConstValue, TypeEnvironment, UnitExpressionId},
};

/// 私有交付切片：先验证完整来源，再进入共享 driver；不提供基础 capability 转换。
pub(crate) fn lower_constant_unit_with_entry(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ConstEnabledTypedUnit,
    owned: &ConstEnabledOwnedUnit,
    entry: DeclarationId,
) -> Result<(Program, FunctionId), LoweringError> {
    let rebuilt = index_compilation_unit(sources, inputs).map_err(|_| LoweringError {
        kind: LoweringErrorKind::MismatchedSource,
        span: None,
    })?;
    if &rebuilt != names.names().index()
        || !typed
            .types()
            .is_compatible_with(sources, inputs, names, environment)
        || !owned.is_compatible_with(typed)
    {
        return Err(LoweringError {
            kind: LoweringErrorKind::MismatchedAnalysis,
            span: None,
        });
    }
    lower_unit_from_facts(
        sources,
        inputs,
        names,
        typed.types(),
        owned.ownership(),
        Some(owned),
        entry,
    )
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_constant(
        &mut self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        // 仅拒绝实际 lower 到的控制点，不让不可达函数中的短路阻塞当前 entry。
        if self
            .constant_owned
            .and_then(|owned| owned.short_circuit_at(expression))
            .is_some()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(facts) = self.typed.constants() else {
            return Ok(None);
        };
        let Ok(index) = facts
            .uses()
            .binary_search_by_key(&expression, |use_| use_.expression())
        else {
            return Ok(None);
        };
        let descriptor = &facts.uses()[index];
        let plan = self
            .constant_owned
            .and_then(|owned| owned.materialization_at(expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != descriptor {
            return Err(lowering_error(LoweringErrorKind::MismatchedAnalysis, span));
        }
        let scalar = match (plan.kind(), descriptor.value()) {
            (ConstantMaterializationKind::InlineCopy, ConstValue::Boolean(value)) => {
                ScalarConstant::Boolean(*value)
            }
            (ConstantMaterializationKind::InlineCopy, ConstValue::Integer { value, .. }) => {
                ScalarConstant::Integer(*value)
            }
            (ConstantMaterializationKind::InlineCopy, ConstValue::Char(value)) => {
                ScalarConstant::Char(u32::from(*value))
            }
            (ConstantMaterializationKind::StringTemporary, ConstValue::String(_)) => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            _ => return Err(lowering_error(LoweringErrorKind::MismatchedAnalysis, span)),
        };
        let ty = self.expression_ssa_type(expression.expression(), span)?;
        self.append_scalar(Operation::Constant(scalar), ty, span)
            .map(Some)
    }
}
