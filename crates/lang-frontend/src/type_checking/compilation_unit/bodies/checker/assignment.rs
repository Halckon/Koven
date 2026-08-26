//! SPEC-0197 compilation-unit 一般赋值遍历与 smart-cast fact kill。

use crate::{
    ast::ExpressionId,
    name_resolution::SourceUnitId,
    type_checking::{CompilationUnitTypeError, DeferredReason, UnitTypeId},
};

use super::{BodyChecker, ExpressionCheck};

impl BodyChecker<'_> {
    pub(super) fn check_assignment(
        &mut self,
        source: SourceUnitId,
        target: ExpressionId,
        value: ExpressionId,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        self.check_expression(source, target, None, None, return_type)?;
        self.check_expression(source, value, None, None, return_type)?;
        if let Some(key) = self.stable_flow_key(source, target) {
            self.flow_facts.remove(&key);
        }
        Ok(ExpressionCheck {
            ty: self.deferred_type(DeferredReason::Assignment),
            // 与单文件 Phase 2 契约一致：一般 assignment 的控制效果仍保持 deferred。
            falls_through: true,
        })
    }
}
