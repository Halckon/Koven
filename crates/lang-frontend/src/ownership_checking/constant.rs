//! 单文件运行时常量物化；声明依赖不属于运行时计划。

use std::sync::Arc;

use crate::{
    ast::ExpressionId,
    type_checking::{ConstantUseDescriptor, TypedFile},
};

/// 一次运行时读取产生的值及其 owner 规则。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstantMaterializationKind {
    /// Boolean、整数或 Char 的内联 Copyable 值，没有独立析构义务。
    InlineCopy,
    /// 从编译期 UTF-8 bytes 新建普通 String temporary；owner 为 descriptor.expression。
    StringTemporary,
}

/// 一次读取的物化计划；借用结束和析构由同一 ownership 产物中的通常 facts 表达。
#[derive(Clone, Debug)]
pub struct ConstantMaterializationPlan {
    pub(crate) descriptor: ConstantUseDescriptor,
    pub(crate) kind: ConstantMaterializationKind,
}

impl ConstantMaterializationPlan {
    /// Phase 2 选择的常量、精确类型、编译期值和本次读取身份。
    #[must_use]
    pub fn descriptor(&self) -> &ConstantUseDescriptor {
        &self.descriptor
    }

    /// 物化结果的所有权类别。
    #[must_use]
    pub fn kind(&self) -> ConstantMaterializationKind {
        self.kind
    }
}

/// 仅在 typed constants 有效、所有权无诊断且无 deferred 时发布的单文件能力。
/// 不证明 compilation-unit 或 native 支持；不能与另一轮 typed 分析混用。
#[derive(Clone, Debug)]
pub struct ValidatedConstantMaterializations {
    pub(crate) typed_analysis_owner: Arc<()>,
    pub(crate) plans: Vec<ConstantMaterializationPlan>,
}

impl ValidatedConstantMaterializations {
    /// 检查计划与 typed facts 是否来自同一次分析。
    #[must_use]
    pub fn matches(&self, typed: &TypedFile) -> bool {
        Arc::ptr_eq(&self.typed_analysis_owner, typed.analysis_owner())
    }

    /// 按 AST expression identity 固定排序；分支及循环中的计划保留原控制流位置。
    #[must_use]
    pub fn plans(&self) -> &[ConstantMaterializationPlan] {
        &self.plans
    }

    /// initializer 依赖与未访问的不可达读取没有计划；group 不重复物化。
    #[must_use]
    pub fn plan_at(&self, expression: ExpressionId) -> Option<&ConstantMaterializationPlan> {
        self.plans
            .binary_search_by_key(&expression.index(), |plan| {
                plan.descriptor.expression().index()
            })
            .ok()
            .map(|index| &self.plans[index])
    }
}
