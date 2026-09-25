//! 捕获槽关联实际来源值；非 owning place 与紧邻 callable 的环境槽不得伪装为 owned local。
use super::{CleanupConditionId, CleanupOwnerValueId};
use crate::{
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource},
    source::Span,
};

/// Closure 定义上的捕获槽；执行时每个环境实例分别持有该槽的值。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CleanupCaptureSlotId(pub(super) usize);

impl CleanupCaptureSlotId {
    /// 身份只在所属清理条件表中有效，不是动态环境实例句柄。
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// 静态事实引用的实例地址；执行时从根值沿已保存的 owned capture 槽读取。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CleanupInstanceAddressId(pub(super) usize);

impl CleanupInstanceAddressId {
    /// 身份只在所属清理条件表中有效。
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// `capture_path` 是每层已检查捕获的原始位置，不是静态 lambda 节点路径。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanupInstanceAddress {
    pub(super) root: CleanupOwnerValueId,
    pub(super) capture_path: Vec<usize>,
}

impl CleanupInstanceAddress {
    /// 保存当前动态根句柄的值槽。
    #[must_use]
    pub const fn root(&self) -> CleanupOwnerValueId {
        self.root
    }

    /// 从实际父实例逐级读取子环境句柄的位置。
    #[must_use]
    pub fn capture_path(&self) -> &[usize] {
        &self.capture_path
    }
}

/// 一个静态环境定义中按已检查捕获首次引用顺序分配的槽。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupCaptureSlot {
    pub(super) environment: CleanupOwnerValueId,
    pub(super) closure: crate::ast::ExpressionId,
    pub(super) source: ClosureCaptureSource,
    pub(super) position: usize,
}

impl CleanupCaptureSlot {
    /// 定义此槽的 closure owner。
    #[must_use]
    pub const fn environment(self) -> CleanupOwnerValueId {
        self.environment
    }

    /// 槽布局所属的 lambda 来源；phi owner 可同时容纳多个来源。
    #[must_use]
    pub const fn closure(self) -> crate::ast::ExpressionId {
        self.closure
    }

    /// 在该环境内定位槽的已检查捕获来源。
    #[must_use]
    pub const fn source(self) -> ClosureCaptureSource {
        self.source
    }

    /// 该环境实例内的捕获首次引用序号；同源的条件输入不另占槽。
    #[must_use]
    pub const fn position(self) -> usize {
        self.position
    }
}

/// 捕获形成时读取的实际来源，独立于接收环境和词法 binding。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CleanupCaptureValue {
    /// 当前 owned 值槽的静态定义；形成环境时须读取并保存该槽当次的实例句柄。
    /// 后续 binding/phi 覆写不得改变已捕获的句柄。
    Owner(CleanupOwnerValueId),
    /// Copyable 或非 owning binding 的 place；不建立独立析构义务。
    Place(ClosureCaptureSource),
    /// Nested lambda 只能从紧邻 callable 环境的槽取得捕获。
    Environment {
        /// 紧邻环境值。
        owner: CleanupOwnerValueId,
        /// 紧邻环境内的槽。
        source: ClosureCaptureSource,
        /// 该环境定义中稳定的捕获槽；动态实例仍由 owner 值运输。
        slot: CleanupCaptureSlotId,
    },
}

/// 创建输入保持不变；planner 为当前清理另存副本并运输其中的条件。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupCaptureInput {
    pub(crate) source: ClosureCaptureSource,
    pub(crate) value: CleanupCaptureValue,
    pub(crate) mode: ClosureCaptureMode,
    pub(crate) effect: ClosureCaptureEffect,
    pub(crate) condition: CleanupConditionId,
    pub(crate) origin: Span,
}

impl CleanupCaptureInput {
    /// 接收环境的捕获槽。
    #[must_use]
    pub const fn source(self) -> ClosureCaptureSource {
        self.source
    }
    /// 本次捕获的来源值或环境槽。
    #[must_use]
    pub const fn value(self) -> CleanupCaptureValue {
        self.value
    }
    /// 对来源持有的能力。
    #[must_use]
    pub const fn mode(self) -> ClosureCaptureMode {
        self.mode
    }
    /// 形成捕获时的效果。
    #[must_use]
    pub const fn effect(self) -> ClosureCaptureEffect {
        self.effect
    }
    /// 选中此输入的路径条件。
    #[must_use]
    pub const fn condition(self) -> CleanupConditionId {
        self.condition
    }
    /// 捕获引用位置。
    #[must_use]
    pub const fn origin(self) -> Span {
        self.origin
    }
}

/// 一次环境形成的目标槽与来源输入；动态实例在执行形成动作时读取该输入。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupCaptureEdge {
    pub(super) target: CleanupCaptureSlotId,
    pub(super) input: CleanupCaptureInput,
}

impl CleanupCaptureEdge {
    /// 接收本次捕获值的环境槽。
    #[must_use]
    pub const fn target(self) -> CleanupCaptureSlotId {
        self.target
    }

    /// 形成时读取的来源值、能力和路径条件。
    #[must_use]
    pub const fn input(self) -> CleanupCaptureInput {
        self.input
    }
}
