//! 单文件所有权检查产物及构造部件。
use std::sync::Arc;

use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::Diagnostic,
    name_resolution::{NameResolution, SymbolId},
    source::SourceId,
    type_checking::{TypeId, TypedFile},
};

use super::{
    ClosureCaptureDescriptor, ClosureDescriptor, ConstructionOwnershipPlan, DropFact, DropPoint,
    LoanEndFact, LoanEndPoint, LoanFact, LoanTarget, OwnershipBindingDescriptor,
    OwnershipBindingKind, OwnershipDeferredFact, RcOwnershipEffect, StringOwnershipEffect,
    Transferability,
};

/// Phase 3 单文件所有权检查产物。
#[derive(Clone, Debug)]
pub struct OwnershipCheckedFile {
    pub(super) callable_provenance: super::callable_provenance::FileCallableFacts,
    cleanup_steps: Vec<(DropPoint, super::IterationCleanupAction)>,
    cleanup_conditions: super::CleanupConditions,
    iterations: Vec<super::IterationOwnershipPlan>,
    pub(super) map_iterations: Vec<super::MapIterationOwnershipPlan>,
    pub(super) map_iteration_templates: Vec<super::MapIterationOwnershipPlan>,
    pub(super) map_iteration_descriptors: Vec<crate::type_checking::MapIterationDescriptor>,
    pub(super) map_iteration_required_exits: Vec<super::map_iteration::MapIterationRequiredExit>,
    constant_materializations: Option<super::ValidatedConstantMaterializations>,
    non_null_assertions: Vec<super::NonNullAssertionOwnershipPlan>,
    ownership_primitives: Vec<super::OwnershipPrimitiveOwnershipPlan>,
    field_replacements: Vec<super::FieldReplaceOwnershipPlan>,
    pub(super) borrow_results: super::borrow_result::FileBorrowResults,
    pub(super) borrow_return_origins: Vec<super::BorrowReturnOriginFact<ExpressionId, LoanTarget>>,
    loan_ends: Vec<LoanEndFact>,
    nullable_whens: Vec<super::NullableWhenOwnershipPlan>,
    source_id: SourceId,
    environment_owner: Arc<()>,
    typed_analysis_owner: Arc<()>,
    diagnostics: Vec<Diagnostic>,
    bindings: Vec<OwnershipBindingDescriptor>,
    formed_owned_locals: Vec<SymbolId>,
    loans: Vec<LoanFact>,
    drops: Vec<DropFact>,
    construction_plans: Vec<ConstructionOwnershipPlan>,
    rc_effects: Vec<RcOwnershipEffect>,
    string_effects: Vec<StringOwnershipEffect>,
    captures: Vec<ClosureCaptureDescriptor>,
    closures: Vec<ClosureDescriptor>,
    transferabilities: Vec<Transferability>,
    deferred: Vec<OwnershipDeferredFact>,
}

pub(crate) struct OwnershipCheckedParts {
    pub(super) callable_provenance: super::callable_provenance::FileCallableFacts,
    pub(crate) cleanup_steps: Vec<(DropPoint, super::IterationCleanupAction)>,
    pub(crate) cleanup_conditions: super::CleanupConditions,
    pub(crate) iterations: Vec<super::IterationOwnershipPlan>,
    pub(crate) map_iterations: Vec<super::MapIterationOwnershipPlan>,
    pub(crate) map_iteration_templates: Vec<super::MapIterationOwnershipPlan>,
    pub(crate) map_iteration_descriptors: Vec<crate::type_checking::MapIterationDescriptor>,
    pub(crate) map_iteration_required_exits: Vec<super::map_iteration::MapIterationRequiredExit>,
    pub(crate) constant_materializations: Option<super::ValidatedConstantMaterializations>,
    pub(crate) non_null_assertions: Vec<super::NonNullAssertionOwnershipPlan>,
    pub(crate) ownership_primitives: Vec<super::OwnershipPrimitiveOwnershipPlan>,
    pub(crate) field_replacements: Vec<super::FieldReplaceOwnershipPlan>,
    pub(crate) borrow_results: super::borrow_result::FileBorrowResults,
    pub(crate) borrow_return_origins: Vec<super::BorrowReturnOriginFact<ExpressionId, LoanTarget>>,
    pub(crate) loan_ends: Vec<LoanEndFact>,
    pub(crate) nullable_whens: Vec<super::NullableWhenOwnershipPlan>,
    pub(crate) bindings: Vec<OwnershipBindingDescriptor>,
    pub(crate) formed_owned_locals: Vec<SymbolId>,
    pub(crate) loans: Vec<LoanFact>,
    pub(crate) drops: Vec<DropFact>,
    pub(crate) construction_plans: Vec<ConstructionOwnershipPlan>,
    pub(crate) rc_effects: Vec<RcOwnershipEffect>,
    pub(crate) string_effects: Vec<StringOwnershipEffect>,
    pub(crate) captures: Vec<ClosureCaptureDescriptor>,
    pub(crate) closures: Vec<ClosureDescriptor>,
    pub(crate) transferabilities: Vec<Transferability>,
    pub(crate) deferred: Vec<OwnershipDeferredFact>,
}

impl OwnershipCheckedFile {
    /// 已检查的普通 class 一级字段置换；旧字段值由调用结果唯一持有。
    #[must_use]
    pub fn field_replacements(&self) -> &[super::FieldReplaceOwnershipPlan] {
        &self.field_replacements
    }
    /// 按调用 identity 查询独立字段提交能力。
    #[must_use]
    pub fn field_replacement(
        &self,
        expression: ExpressionId,
    ) -> Option<&super::FieldReplaceOwnershipPlan> {
        self.field_replacements
            .iter()
            .find(|plan| plan.descriptor().expression() == expression)
    }

    /// 正常执行前缀后已获准的 owned root 原子 commit。
    #[must_use]
    pub fn ownership_primitives(&self) -> &[super::OwnershipPrimitiveOwnershipPlan] {
        &self.ownership_primitives
    }
    /// 按调用 identity 查询 root commit；无正常后继或超出支持范围时没有计划。
    #[must_use]
    pub fn ownership_primitive(
        &self,
        expression: ExpressionId,
    ) -> Option<&super::OwnershipPrimitiveOwnershipPlan> {
        self.ownership_primitives
            .iter()
            .find(|plan| plan.descriptor().expression() == expression)
    }

    /// 按规划顺序发布快照、drop 与 loan end；同一 point 必须保持此顺序。
    /// 迭代 exit plans 是本序列的关联视图，不得重复执行。
    #[must_use]
    pub fn cleanup_steps(&self) -> &[(DropPoint, super::IterationCleanupAction)] {
        &self.cleanup_steps
    }

    /// Drop 与 capture loan end 共用的条件表；仅引用保存的动态选择。
    #[must_use]
    pub fn cleanup_conditions(&self) -> &super::CleanupConditions {
        &self.cleanup_conditions
    }

    /// 已检查的 statement-keyed iteration plans；错误或不完整分析不发布。
    #[must_use]
    pub fn iterations(&self) -> &[super::IterationOwnershipPlan] {
        &self.iterations
    }
    /// 查询一个实际可达 provider 的 ownership plan。
    #[must_use]
    pub fn iteration(&self, statement: StatementId) -> Option<&super::IterationOwnershipPlan> {
        self.iterations
            .iter()
            .find(|plan| plan.descriptor().statement() == statement)
    }

    /// 实际可达的 Map provider；单独发布，不伪造顺序迭代事实。
    #[must_use]
    pub fn map_iterations(&self) -> &[super::MapIterationOwnershipPlan] {
        &self.map_iterations
    }
    /// 按原始 For identity 查询 Map 生命周期。
    #[must_use]
    pub fn map_iteration(
        &self,
        statement: StatementId,
    ) -> Option<&super::MapIterationOwnershipPlan> {
        self.map_iterations
            .binary_search_by_key(&statement.index(), |plan| {
                plan.descriptor().statement().index()
            })
            .ok()
            .map(|index| &self.map_iterations[index])
    }

    /// 单文件常量物化能力；失败或 deferred 分析不发布半成品计划。
    #[must_use]
    pub fn constant_materializations(&self) -> Option<&super::ValidatedConstantMaterializations> {
        self.constant_materializations.as_ref()
    }

    /// 返回按 assertion 身份排序、仅在无所有权诊断时发布的提取计划。
    #[must_use]
    pub fn non_null_assertions(&self) -> &[super::NonNullAssertionOwnershipPlan] {
        &self.non_null_assertions
    }
    /// 按 assertion 表达式身份查询成功转移与 null Abort 计划。
    #[must_use]
    pub fn non_null_assertion(
        &self,
        expression: ExpressionId,
    ) -> Option<&super::NonNullAssertionOwnershipPlan> {
        self.non_null_assertions
            .iter()
            .find(|plan| plan.descriptor().expression() == expression)
    }

    /// 返回实际控制流上的 loan 终止事实；同边先结束loan再执行drop。
    pub fn loan_ends(&self) -> &[LoanEndFact] {
        &self.loan_ends
    }
    /// 返回仅在无诊断时发布的 nullable ownership plans。
    pub fn nullable_whens(&self) -> &[super::NullableWhenOwnershipPlan] {
        &self.nullable_whens
    }
    /// 按 when 表达式身份查询 ownership plan。
    pub fn nullable_when(
        &self,
        expression: ExpressionId,
    ) -> Option<&super::NullableWhenOwnershipPlan> {
        self.nullable_whens
            .iter()
            .find(|plan| plan.expression() == expression)
    }
    pub(crate) fn new(
        source_id: SourceId,
        environment_owner: Arc<()>,
        typed_analysis_owner: Arc<()>,
        diagnostics: Vec<Diagnostic>,
        parts: OwnershipCheckedParts,
    ) -> Self {
        Self {
            source_id,
            callable_provenance: parts.callable_provenance,
            environment_owner,
            constant_materializations: parts.constant_materializations,
            iterations: parts.iterations,
            map_iterations: parts.map_iterations,
            map_iteration_templates: parts.map_iteration_templates,
            map_iteration_descriptors: parts.map_iteration_descriptors,
            map_iteration_required_exits: parts.map_iteration_required_exits,
            cleanup_conditions: parts.cleanup_conditions,
            cleanup_steps: parts.cleanup_steps,
            nullable_whens: parts.nullable_whens,
            non_null_assertions: parts.non_null_assertions,
            ownership_primitives: parts.ownership_primitives,
            field_replacements: parts.field_replacements,
            borrow_results: parts.borrow_results,
            borrow_return_origins: parts.borrow_return_origins,
            loan_ends: parts.loan_ends,
            typed_analysis_owner,
            diagnostics,
            bindings: parts.bindings,
            formed_owned_locals: parts.formed_owned_locals,
            loans: parts.loans,
            drops: parts.drops,
            construction_plans: parts.construction_plans,
            rc_effects: parts.rc_effects,
            string_effects: parts.string_effects,
            captures: parts.captures,
            closures: parts.closures,
            transferabilities: parts.transferabilities,
            deferred: parts.deferred,
        }
    }

    /// 返回输入源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// 返回本产物是否与名称、类型产物共享源码和显式环境身份。
    #[must_use]
    pub fn is_compatible_with(&self, names: &NameResolution, typed: &TypedFile) -> bool {
        self.source_id == names.source_id()
            && self.source_id == typed.source_id()
            && typed.is_compatible_with_names(names)
            && Arc::ptr_eq(&self.environment_owner, typed.environment_owner())
            && Arc::ptr_eq(&self.typed_analysis_owner, typed.analysis_owner())
    }

    /// 返回稳定源码顺序的所有权诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 返回稳定 symbol 顺序的参数 binding 能力。
    #[must_use]
    pub fn bindings(&self) -> &[OwnershipBindingDescriptor] {
        &self.bindings
    }

    /// 返回某条分析路径正常完成初始化的具名 val/var local，按 symbol 排序去重。
    /// 仅记录形成历史，不证明必执行、当前存活、owner 权限或 nullable proof。
    #[must_use]
    pub fn formed_owned_locals(&self) -> &[SymbolId] {
        &self.formed_owned_locals
    }

    /// 查询参数 binding 能力。
    #[must_use]
    pub fn binding_kind(&self, symbol: SymbolId) -> Option<OwnershipBindingKind> {
        self.bindings
            .binary_search_by_key(&symbol.index(), |binding| binding.symbol().index())
            .ok()
            .map(|index| self.bindings[index].kind())
    }

    /// 返回源码顺序的有效调用期 loans。
    #[must_use]
    pub fn loans(&self) -> &[LoanFact] {
        &self.loans
    }

    /// 查询由指定 argument 建立的 loan。
    #[must_use]
    pub fn loan_begin(&self, argument: ExpressionId) -> Option<&LoanFact> {
        self.loans.iter().find(|loan| loan.argument() == argument)
    }

    /// 返回在指定同步 call 返回时结束的 loans。
    pub fn loans_ending_at(&self, call: ExpressionId) -> impl Iterator<Item = &LoanFact> {
        self.loans.iter().filter(move |loan| {
            loan.call() == call
                && self.loan_ends.iter().any(|end| {
                    end.call == call
                        && end.argument == loan.argument()
                        && end.point == LoanEndPoint::CallReturn(call)
                })
        })
    }

    /// 返回源码 / 控制流顺序的有效 drop facts。
    #[must_use]
    pub fn drops(&self) -> &[DropFact] {
        &self.drops
    }

    /// 返回运行时求值顺序稳定的 construction ownership plans。
    #[must_use]
    pub fn construction_plans(&self) -> &[ConstructionOwnershipPlan] {
        &self.construction_plans
    }

    /// 查询指定 typed construction 的 ordered-delivery/root plan。
    #[must_use]
    pub fn construction_plan(
        &self,
        expression: ExpressionId,
    ) -> Option<&ConstructionOwnershipPlan> {
        self.construction_plans
            .iter()
            .find(|plan| plan.construction() == expression)
    }

    /// 返回已批准的 String 显式复制效果。
    #[must_use]
    pub fn string_effects(&self) -> &[StringOwnershipEffect] {
        &self.string_effects
    }
    /// 查询已批准的 String 显式复制效果。
    #[must_use]
    pub fn string_effect(&self, expression: ExpressionId) -> Option<StringOwnershipEffect> {
        self.string_effects
            .iter()
            .copied()
            .find(|effect| effect.expression() == expression)
    }

    /// 返回源码顺序稳定的 intrinsic `Rc<T>` ownership effects。
    #[must_use]
    pub fn rc_effects(&self) -> &[RcOwnershipEffect] {
        &self.rc_effects
    }

    /// 查询指定表达式的 intrinsic `Rc<T>` ownership effect。
    #[must_use]
    pub fn rc_effect(&self, expression: ExpressionId) -> Option<RcOwnershipEffect> {
        self.rc_effects
            .iter()
            .copied()
            .find(|effect| effect.expression() == expression)
    }

    /// 返回 lambda/source 顺序稳定的 capture facts。
    #[must_use]
    pub fn captures(&self) -> &[ClosureCaptureDescriptor] {
        &self.captures
    }

    /// 返回一个 lambda 的源码顺序 capture facts。
    pub fn captures_of(
        &self,
        lambda: ExpressionId,
    ) -> impl Iterator<Item = &ClosureCaptureDescriptor> {
        self.captures
            .iter()
            .filter(move |capture| capture.lambda() == lambda)
    }

    /// 查询具体 lambda environment 的能力事实。
    #[must_use]
    pub fn closure(&self, expression: ExpressionId) -> Option<ClosureDescriptor> {
        self.closures
            .iter()
            .copied()
            .find(|closure| closure.expression() == expression)
    }

    /// 查询一个规范化类型的结构化跨线程转移能力。
    #[must_use]
    pub fn transferability(&self, ty: TypeId) -> Option<Transferability> {
        self.transferabilities.get(ty.index()).copied()
    }

    /// 返回明确留给后续 Spec 的所有权事实。
    #[must_use]
    pub fn deferred(&self) -> &[OwnershipDeferredFact] {
        &self.deferred
    }
}

#[cfg(test)]
#[path = "iteration_validation_tests.rs"]
mod iteration_validation_tests;
