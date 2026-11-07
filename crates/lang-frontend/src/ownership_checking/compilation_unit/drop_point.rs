//! Source-qualified cleanup execution points shared with Unit consumers.
use super::UnitClosureCaptureSource;
use crate::{
    name_resolution::{DeclarationId, UnitSymbolId},
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId, UnitTypeId},
};

/// Phase 4 可消费的 source-qualified 析构边界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitDropPoint {
    /// 最后一次读取或 replacement RHS 完成后。
    AfterExpression(UnitExpressionId),
    /// 非消费式 binary 已读完全部 operand、但结果仍存活时。
    AfterBinaryOperands(UnitExpressionId),
    /// String when 的隐式比较已产出 Boolean，尚未分流；只清理本次候选。
    WhenComparisonCompleted {
        /// 同轮 source-qualified when identity。
        control: UnitExpressionId,
        /// 源码 entry 下标。
        entry: usize,
        /// entry 内候选下标。
        alternative: usize,
    },
    /// String when 的比较失败后，进入下一候选、else 或落空之前。
    WhenAlternativeNoMatch {
        /// 同轮 source-qualified when identity。
        control: UnitExpressionId,
        /// 源码 entry 下标。
        entry: usize,
        /// entry 内候选下标。
        alternative: usize,
    },
    /// 未使用 binding 建立或完整 statement 完成后。
    AfterStatement(UnitStatementId),
    /// 借用 temporary 在同步 call 返回后。
    CallReturn(UnitExpressionId),
    /// return / break / continue 控制转移边。
    ControlTransfer(UnitExpressionId),
    /// if / when 的选中分支开始执行 body 或空分支之前。
    BranchEntry {
        /// 控制表达式。
        control: UnitExpressionId,
        /// 源码顺序的 branch 下标。
        branch: usize,
    },
    /// if / when 的特定 branch 正常离开边。
    BranchExit {
        /// 控制表达式。
        control: UnitExpressionId,
        /// 源码顺序的 branch 下标。
        branch: usize,
    },
    /// when 某个 alternative 成功、进入共享 entry body 之前的边。
    WhenAlternativeMatch {
        /// 控制表达式。
        control: UnitExpressionId,
        /// 源码顺序的 entry 下标。
        entry: usize,
        /// 当前 entry 内的 alternative 下标。
        alternative: usize,
    },
    /// while / for 的零次或正常退出边。
    LoopExit(UnitStatementId),
    /// callable body 开始、参数 binding 建立之后。
    FunctionEntry(UnitItemId),
    /// lambda callable body 开始、参数 binding 建立之后。
    LambdaEntry(UnitExpressionId),
    /// 普通字段 replacement 的 RHS 正常完成、旧字段析构开始之前。
    BeforeReplacement(UnitExpressionId),
    /// element replacement 已提交新值之后。
    AfterReplacement(UnitExpressionId),
}

/// 一个需要唯一析构的 source-qualified 运行时值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitDropTarget {
    /// Value receiver 尚未被整体移动时由当前 callable 负责的根值。
    This(DeclarationId),
    /// 当前 callable 拥有的 named binding 值。
    Named(UnitSymbolId),
    /// 完整表达式产生的 anonymous temporary。
    Temporary(UnitExpressionId),
    /// replacement 前原 element value；payload 是 assignment expression。
    ReplacedElement(UnitExpressionId),
    /// replacement 前原 field value；字段身份必须与 assignment target 的 typed projection 一致。
    ReplacedField {
        /// assignment expression。
        assignment: UnitExpressionId,
        /// 被替换字段的 source-qualified symbol。
        field: UnitSymbolId,
    },
    /// 独占借入的 MoveOnly 槽在正常替换时持有的旧值。
    ReplacedCapture {
        /// 持有独占借用的 lambda。
        closure: UnitExpressionId,
        /// 完成 RHS 求值后执行替换的赋值。
        assignment: UnitExpressionId,
        /// 调用者仍拥有的捕获根。
        symbol: UnitSymbolId,
    },
    /// `move` closure environment 中一个 owned MoveOnly capture。
    Captured {
        /// 拥有 environment 的 lambda。
        closure: UnitExpressionId,
        /// 被析构的捕获来源。
        source: UnitClosureCaptureSource,
    },
}

/// 一个确定的 source-qualified ASAP 析构事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitDropFact {
    point: UnitDropPoint,
    target: UnitDropTarget,
    value_origin: Span,
}

/// interface default 的 `StaticSelf` Value receiver 析构义务。
///
/// Phase 4 必须先把 [`Self::receiver_type`] 实例化为具体 receiver；仅当具体类型为
/// MoveOnly 时才消费该事实并生成析构。该事实与无条件 [`UnitDropFact`] 分开发布，避免在
/// Copyable specialization 上误析构。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitConditionalReceiverDropFact {
    point: UnitDropPoint,
    owner: DeclarationId,
    receiver_type: UnitTypeId,
    value_origin: Span,
    preceding_drops: usize,
}

impl UnitConditionalReceiverDropFact {
    pub(super) const fn new(
        point: UnitDropPoint,
        owner: DeclarationId,
        receiver_type: UnitTypeId,
        value_origin: Span,
        preceding_drops: usize,
    ) -> Self {
        Self {
            point,
            owner,
            receiver_type,
            value_origin,
            preceding_drops,
        }
    }

    /// 返回同一 drop point 中必须先消费的无条件 drop fact 数量。
    /// 若存在iteration cleanup，应按显式动作执行；此数量不能定位provider动作之间的位置。
    #[must_use]
    pub const fn preceding_drops(self) -> usize {
        self.preceding_drops
    }

    /// 返回与普通 drop fact 相同的 source-qualified 控制流边界。
    #[must_use]
    pub const fn point(self) -> UnitDropPoint {
        self.point
    }

    /// 返回声明 `StaticSelf` receiver 的 interface owner。
    #[must_use]
    pub const fn owner(self) -> DeclarationId {
        self.owner
    }

    /// 返回必须由下游实例化的 `StaticSelf` 类型模板。
    #[must_use]
    pub const fn receiver_type(self) -> UnitTypeId {
        self.receiver_type
    }

    /// 返回当前 receiver 值实例的建立位置。
    #[must_use]
    pub const fn value_origin(self) -> Span {
        self.value_origin
    }
}

impl UnitDropFact {
    pub(super) const fn new(
        point: UnitDropPoint,
        target: UnitDropTarget,
        value_origin: Span,
    ) -> Self {
        Self {
            point,
            target,
            value_origin,
        }
    }

    /// 返回析构发生的 source-qualified 控制流边界。
    #[must_use]
    pub const fn point(self) -> UnitDropPoint {
        self.point
    }

    /// 返回析构目标或提交时被替换的旧值。
    #[must_use]
    pub const fn target(self) -> UnitDropTarget {
        self.target
    }

    /// 返回诊断位置。对于 capture 替换，这是目标 place 的引用位置。
    #[must_use]
    pub const fn value_origin(self) -> Span {
        self.value_origin
    }
}
