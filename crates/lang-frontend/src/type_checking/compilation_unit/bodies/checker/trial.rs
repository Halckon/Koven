//! SPEC-0197 compilation-unit overload candidate 的事务化试算状态。

use std::collections::BTreeMap;

use crate::{
    diagnostic::Diagnostic,
    type_checking::{CompilationUnitSignatures, UnitTypeId},
};

use super::{BodyChecker, CompilationUnitTypeParts, FlowKey};

/// 一个 unit callable candidate 试算能够改变的完整 typed 状态。
///
/// 名称解析、源码索引与声明图在 body 检查前已经封闭；unit type table、body facts、flow 与
/// diagnostics 必须一起回滚，避免失败候选泄漏后继 ownership 可见的半成品事实。
#[derive(Clone)]
pub(super) struct UnitTrialState {
    signatures: CompilationUnitSignatures,
    flow_facts: BTreeMap<FlowKey, UnitTypeId>,
    parts: CompilationUnitTypeParts,
    diagnostics: Vec<Diagnostic>,
}

impl BodyChecker<'_> {
    pub(super) fn trial_state(&self) -> UnitTrialState {
        UnitTrialState {
            signatures: self.signatures.clone(),
            flow_facts: self.flow_facts.clone(),
            parts: self.parts.clone(),
            diagnostics: self.diagnostics.clone(),
        }
    }

    pub(super) fn restore_trial_state(&mut self, state: UnitTrialState) {
        self.signatures = state.signatures;
        self.flow_facts = state.flow_facts;
        self.parts = state.parts;
        self.diagnostics = state.diagnostics;
    }

    /// 判断当前候选试算是否相对 baseline 新增了未决类型事实。
    ///
    /// Expected function type 会让 lambda 顶层保持确定的 Function 类型，即使 body 内部仍是
    /// Deferred；重载选择必须观察完整事务状态，不能只查看顶层返回值。
    pub(super) fn trial_introduced_deferred(&self, baseline: &UnitTrialState) -> bool {
        let expression_deferred = self.parts.expression_types.iter().any(|(key, ty)| {
            baseline.parts.expression_types.get(key) != Some(ty)
                && self.is_deferred(*ty)
                && !self.is_successful_container_callee(*key)
        });
        let symbol_deferred = self.parts.symbol_types.iter().any(|(key, ty)| {
            baseline.parts.symbol_types.get(key) != Some(ty) && self.is_deferred(*ty)
        });
        let type_ref_deferred = self.parts.type_ref_types.iter().any(|(key, ty)| {
            baseline.parts.type_ref_types.get(key) != Some(ty) && self.is_deferred(*ty)
        });
        expression_deferred || symbol_deferred || type_ref_deferred
    }
}
