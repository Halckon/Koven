//! Source-qualified cleanup execution points shared with Unit consumers.
use crate::type_checking::{UnitExpressionId, UnitItemId, UnitStatementId};

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
