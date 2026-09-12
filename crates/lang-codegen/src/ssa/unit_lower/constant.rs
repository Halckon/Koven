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
    validate_constant_unit_inputs(sources, inputs, names, environment, typed, owned)?;
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

/// 专用 native 与 SSA 入口共享身份门禁，不转换成基础 capability。
pub(crate) fn validate_constant_unit_inputs(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ConstEnabledTypedUnit,
    owned: &ConstEnabledOwnedUnit,
) -> Result<(), LoweringError> {
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
    Ok(())
}

impl UnitExpressionLowerer<'_> {
    /// 与 frontend 一致，只有实际常量物化 use 才穿透透明 Group；不改普通实参来源。
    pub(super) fn constant_materialization_origin(
        &self,
        mut expression: UnitExpressionId,
        span: Span,
    ) -> Result<Option<UnitExpressionId>, LoweringError> {
        let Some(owned) = self.constant_owned else {
            return Ok(None);
        };
        loop {
            if owned.materialization_at(expression).is_some() {
                return Ok(Some(expression));
            }
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression.expression())
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if let Expression::Group { expression: inner } = node.payload() {
                expression = UnitExpressionId::new(expression.source_unit(), *inner);
            } else {
                return Ok(None);
            }
        }
    }

    pub(super) fn lower_constant(
        &mut self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
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
        let ty = self.expression_ssa_type(expression.expression(), span)?;
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
            _ => return Err(lowering_error(LoweringErrorKind::MismatchedAnalysis, span)),
        };
        self.append_scalar(operation, ty, span).map(Some)
    }
}
