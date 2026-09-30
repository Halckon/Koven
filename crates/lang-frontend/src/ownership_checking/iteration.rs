//! 已检查的迭代借用与退出序列；所有字段仅由 Phase 3 构造。
use super::{
    CleanupCaptureInput, CleanupCaptureValue, CleanupConditionId, CleanupInstanceAddressId,
    CleanupOwnerValueId, CleanupSelectorId, ClosureCaptureDescriptor, ClosureCaptureEffect,
    ClosureCaptureMode, ClosureCaptureSource, DropFact, DropPoint, LoanEndFact, LoanTarget,
    OwnershipBindingDescriptor,
};
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    type_checking::SequentialIterationDescriptor,
};

/// 动态 owner 来源的循环边界；入口与出口拥有不同的 phi 身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IterationPhiBoundary {
    /// 入口与每轮回边汇合。
    Header,
    /// 耗尽与 break 汇合。
    Exit,
}

/// 向循环 phi 写入的一条边；记录输入不表示动态复制已实现。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationPhiIncomingKind {
    /// source 求值完成后、首次查询 provider 之前。
    Entry,
    /// 本轮 body 正常完成。
    Fallthrough,
    /// 本轮 continue。
    Continue(ExpressionId),
    /// provider 耗尽。
    Exhaustion,
    /// 本轮 break。
    Break(ExpressionId),
}

/// 一条迭代退出边。Abort 没有 cleanup edge。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationExitKind {
    /// 本轮 body 正常完成，provider 保留。
    Fallthrough,
    /// 结束本轮并保留 provider。
    Continue(ExpressionId),
    /// 结束最近循环的 provider。
    Break(ExpressionId),
    /// operand 已交付后结束所有离开的 provider。
    Return(ExpressionId),
    /// HasNext 为 false 的独立出口，不与 break 清理串联。
    Exhaustion,
}

/// 逐实例释放读取静态捕获布局的位置；动态子实例始终从形成时保存的槽读取。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosureReleaseLayout {
    /// 循环 phi 的有限 capture 图；节点索引只在该循环内有效。
    Iteration(StatementId),
    /// 从当次实例的 closure 身份读取单文件已检查的完整捕获描述符。
    File,
}

/// 同一退出点严格按此列表执行；Drop 不得再从普通 drop 列表重复执行。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationCleanupAction {
    /// 调用点从已求值的 callee owner 读取当次函数值的入口与环境。
    /// 稳定 callee 的唯一 owner 在参数求值后仍可用才发布；调用不消费该实例。
    PassClosureEnvironment {
        /// 保存 callee 当次值的静态 owner，可为快照身份。
        callee: super::CleanupOwnerValueId,
        /// 能静态证明时的唯一 lambda；None 从实际函数值选择入口，不能猜选来源，
        /// 也不保证该入口在本文件有可查询的 lambda ExpressionId。
        closure: Option<ExpressionId>,
    },
    /// 调用入口把传入 closure 的当次环境实例绑定到 body 的静态环境 owner。
    /// 调用不消费该实例；后续环境槽读取使用这个入口绑定。
    BindClosureEnvironment {
        /// Body 中引用的环境 owner 定义。
        owner: super::CleanupOwnerValueId,
        /// 当前进入的 lambda。
        closure: ExpressionId,
    },
    /// 已完成的 lambda 求值建立新的环境实例；随后逐条 SaveClosureCapture
    /// 从形成前状态读取并保存捕获，不能留下可被后续覆写的来源槽别名。
    CreateClosureOwner {
        /// 本次形成的静态值身份，重复执行由后续 phi 运输。
        owner: super::CleanupOwnerValueId,
        /// 环境布局对应的 lambda。
        closure: ExpressionId,
    },
    /// 形成后从当次来源实例读取并保存一个 capture；同槽条件版本只执行选中者。
    SaveClosureCapture {
        /// 接收捕获的环境定义，执行时指向本次新实例。
        owner: super::CleanupOwnerValueId,
        /// 新环境中的目标槽。
        target: super::CleanupCaptureSlotId,
        /// 形成前状态的来源值或紧邻环境槽及其选择条件。
        input: super::CleanupCaptureInput,
    },
    /// RHS 完成后保存其选择；先于同点旧 owner 的清理。
    SaveOwnerSnapshot {
        /// 此执行点的可达条件；None 表示无条件保存。
        condition: Option<super::CleanupConditionId>,
        /// 本次新值的静态身份。
        owner: super::CleanupOwnerValueId,
        /// 已求值的 RHS/initializer。
        value: ExpressionId,
    },
    /// 旧值清理完成后，把新值和快照一起提交给 binding。
    CommitOwnerSnapshot {
        /// 已保存的新值身份。
        owner: super::CleanupOwnerValueId,
        /// 接收 binding。
        target: SymbolId,
    },
    /// 复用既有 owner drop 身份。
    Drop(DropFact),
    /// 从根实例沿形成时保存的 owned capture 边释放环境；每个实际实例的 shared
    /// capture 也须从该实例保存的 source 结束 loan，不能另按静态图节点释放一次。
    /// 静态图只提供槽布局，不决定运行时深度。递归计划解除 deferred 前，此动作
    /// 仅留在 planner 内部。
    /// 同点保存本次根的有序 loan-end 批次，未选中根留空批次，供配对动作消费。
    ReleaseClosureInstances {
        /// 取得当次实例完整已检查 capture 布局的来源。
        layout: ClosureReleaseLayout,
        /// 待释放的根值及其边界条件；不与普通 Drop 重复执行。
        root: DropFact,
    },
    /// 消费同点、同 root 的实例释放产生的有序 loan-end 凭据，恰好一次。
    /// 仅 last=true 且仍承担 retained 析构义务的实际 source 可以释放；具名或外部
    /// source 不在此取得析构义务。源若也是 closure，按 File 完整布局释放并继续
    /// 消费其新产生的凭据。须在整次外层 owner 收尾后执行，不与有限 retained
    /// TestLastCaptureLoan/Drop 重复；条件未选中的根仍对应一个空批次。
    ReleaseRetainedClosureSources {
        /// 配对 ReleaseClosureInstances 的完整根事实；不是新的 owner drop。
        root: DropFact,
    },
    /// 结束本轮未完成调用的派生 loan。
    EndCallLoan(LoanEndFact),
    /// closure owner 释放后结束其 shared capture。
    EndCaptureLoan {
        /// 静态环境或捕获来源关系身份；phi 路径可为 `IterationPhiSourceOwner`。
        /// 实际环境由 `instance_address` 定位，不能从本字段推断当次实例。
        owner: super::CleanupOwnerValueId,
        /// 从当次根值沿已保存捕获边定位这个环境实例。
        instance_address: super::CleanupInstanceAddressId,
        /// 已跟踪 source 在该实例中的捕获槽；非 owning place 可以没有 phi 槽。
        capture_slot: Option<super::CleanupCaptureSlotId>,
        /// 当前边界额外要求的保存路径条件；None 表示无条件。
        condition: Option<super::CleanupConditionId>,
        /// 候选 closure 布局身份；地址的实际 closure 不匹配时不结束 loan。
        /// 路径的 owned 槽已由显式 Move/captured drop 清空，且无同点释放边凭据可继续解析时，
        /// 跳过候选；有同点释放边时仍须沿该边结束 loan。
        /// 其余路径缺失或匹配布局的槽缺失仍是无效事实，不能作为未选中忽略。
        closure: ExpressionId,
        /// 原 shared capture 的来源。
        source: ClosureCaptureSource,
        /// 此 loan 实际保护的来源值或紧邻环境槽。
        value: super::CleanupCaptureValue,
    },
    /// 读取同点该 capture loan 结束时保存的最后借用者结果。
    /// 根动作可先结束多条 loan；不能在延后查询时用当前计数把它们都判为最后一个。
    TestLastCaptureLoan {
        /// 来源的静态 owner 身份；实际值须从下方环境地址与槽读取。
        owner: CleanupOwnerValueId,
        /// 持有该 source loan 的环境实例。
        instance_address: super::CleanupInstanceAddressId,
        /// 从该环境实例读取当次 source 值的 capture 槽。
        capture_slot: super::CleanupCaptureSlotId,
        /// 写入的独立选择；1 表示结束该 loan 时，其 source 从一个有效 capture loan 变为零。
        /// 消费端按实际环境实例/槽保存结果和 source 身份，清理点结束后清空凭据。
        /// condition 不成立、路径槽已被显式消费且无同点释放边凭据、或实际 closure 与
        /// capture_slot 布局不匹配时写 0，避免沿用旧轮结果。
        selector: CleanupSelectorId,
        /// 此 source 释放路径的额外保护。
        condition: Option<CleanupConditionId>,
    },
    /// 结束具名 element/component binding，按声明逆序。
    EndBinding {
        /// 所属 for。
        statement: StatementId,
        /// 具名 element/component。
        symbol: SymbolId,
    },
    /// 结束当前动态 element access（包括 discard）。
    EndElement(StatementId),
    /// 结束线性 provider。
    FinishProvider(StatementId),
    /// provider 结束后释放 source shared loan。
    EndSource(StatementId),
}

/// 一个控制流出口的完整清理；嵌套 return 的各计划引用等值的完整序列，只执行一次。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationExitPlan {
    pub(crate) kind: IterationExitKind,
    pub(crate) point: DropPoint,
    pub(crate) actions: Vec<IterationCleanupAction>,
}
impl IterationExitPlan {
    /// 退出种类与 jump identity。
    #[must_use]
    pub fn kind(&self) -> IterationExitKind {
        self.kind
    }
    /// 普通 drop facts 的对应点。
    #[must_use]
    pub fn point(&self) -> DropPoint {
        self.point
    }
    /// 已排序的完整退出动作。
    #[must_use]
    pub fn actions(&self) -> &[IterationCleanupAction] {
        &self.actions
    }
}

/// 仅在无诊断、无 deferred 且所有可达 provider 都有退出事实时整体发布。
/// 静态来源摘要不代表动态环境实例或可执行 phi 运输。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationOwnershipPlan {
    pub(crate) descriptor: SequentialIterationDescriptor,
    pub(crate) source: LoanTarget,
    pub(crate) bindings: Vec<OwnershipBindingDescriptor>,
    pub(crate) exits: Vec<IterationExitPlan>,
    pub(crate) closure_flow: IterationClosureFlow,
    pub(crate) capture_graph: IterationCaptureGraph,
    pub(crate) closure_phis: Vec<IterationClosurePhiBinding>,
    pub(crate) closure_phi_incomings: Vec<IterationPhiIncoming>,
}
impl IterationOwnershipPlan {
    /// 去重的静态 lambda/capture 来源图；节点索引不是运行时环境实例句柄。
    #[must_use]
    pub fn capture_graph(&self) -> &IterationCaptureGraph {
        &self.capture_graph
    }
    /// 用于预分配 phi owner 与已知来源字段的有限摘要；不能据此生成 drop 或结束 loan。
    #[must_use]
    pub fn closure_flow(&self) -> &IterationClosureFlow {
        &self.closure_flow
    }
    /// 预分配的来源/owner 身份；在入边赋值与 live owner 运输发布前不可执行。
    #[must_use]
    pub fn closure_phis(&self) -> &[IterationClosurePhiBinding] {
        &self.closure_phis
    }
    /// 已规划的实际入边；缺少的边仍使 phi 不可执行。
    #[must_use]
    pub fn closure_phi_incomings(&self) -> &[IterationPhiIncoming] {
        &self.closure_phi_incomings
    }
    /// Phase 2 的唯一 source 求值与 provider 身份。
    #[must_use]
    pub fn descriptor(&self) -> &SequentialIterationDescriptor {
        &self.descriptor
    }
    /// 覆盖 provider 全生命周期的 shared loan 目标；temporary 指实际 backing owner。
    /// 若 source 是 temporary container 的 element，其访问仍由 descriptor.source 表示。
    #[must_use]
    pub fn source(&self) -> &LoanTarget {
        &self.source
    }
    /// 本轮具名 shared bindings；discard 不建立 symbol。
    #[must_use]
    pub fn bindings(&self) -> &[OwnershipBindingDescriptor] {
        &self.bindings
    }
    /// 各独立出口；同一 point 只执行一次完整清理。
    #[must_use]
    pub fn exits(&self) -> &[IterationExitPlan] {
        &self.exits
    }
}

/// 一个循环的有限静态 capture 来源图；动态 owner 运输仍由后续事实定义。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IterationCaptureGraph {
    pub(crate) nodes: Vec<IterationCaptureNode>,
}
impl IterationCaptureGraph {
    /// 节点索引是 `IterationCaptureSource::captured` 的目标。
    #[must_use]
    pub fn nodes(&self) -> &[IterationCaptureNode] {
        &self.nodes
    }
}

/// 同一个 lambda identity 在图中只占一个静态节点。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationCaptureNode {
    pub(crate) closure: ExpressionId,
    pub(crate) release_captures: Vec<ClosureCaptureDescriptor>,
    pub(crate) sources: Vec<IterationCaptureSource>,
}
impl IterationCaptureNode {
    /// 此节点对应的唯一 lambda AST identity。
    #[must_use]
    pub const fn closure(&self) -> ExpressionId {
        self.closure
    }
    /// 当次环境的完整已检查捕获布局，顺序即原始捕获位置；包括未进入 phi 来源图的
    /// Copyable/shared 捕获。逐实例释放必须按此布局处理每个槽和 loan。
    #[must_use]
    pub fn release_captures(&self) -> &[ClosureCaptureDescriptor] {
        &self.release_captures
    }
    /// 按捕获声明顺序排列的已跟踪来源。
    #[must_use]
    pub fn sources(&self) -> &[IterationCaptureSource] {
        &self.sources
    }
}

/// 捕获的候选内层 lambda 节点，不沿捕获路径复制子图。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationCaptureSource {
    pub(crate) capture: ClosureCaptureDescriptor,
    pub(crate) position: usize,
    pub(crate) captured: Vec<usize>,
    pub(crate) may_be_opaque: bool,
}
impl IterationCaptureSource {
    /// 此边的已检查捕获能力及来源。
    #[must_use]
    pub const fn capture(&self) -> ClosureCaptureDescriptor {
        self.capture
    }
    /// 完整已检查捕获集的首次引用序号；未跟踪的 capture 仍占位。
    #[must_use]
    pub const fn position(&self) -> usize {
        self.position
    }
    /// owned/Move 捕获的候选内层环境节点；实际实例由形成时句柄决定。
    /// Shared 来源即使自身是 closure 也不沿此边运输其 owned 子图。
    #[must_use]
    pub fn captured(&self) -> &[usize] {
        &self.captured
    }
    /// owned 来源还可能是来源分析未枚举的值（例如函数参数或调用结果）。
    /// 实际子 closure 不属于 captured 候选时，只在此标记为 true 时按 opaque 边运输；
    /// 不把它猜成任一已知节点，也不沿其内部值写本图存在位。此标记不提供析构布局。
    #[must_use]
    pub const fn may_be_opaque(&self) -> bool {
        self.may_be_opaque
    }
}

/// 有限来源图中的 capture 边与 phi 静态布局槽的对应关系；不承载子环境实例。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IterationPhiCaptureSlot {
    pub(crate) node: usize,
    pub(crate) position: usize,
    pub(crate) slot: super::CleanupCaptureSlotId,
}
impl IterationPhiCaptureSlot {
    /// `IterationCaptureGraph::nodes()` 中的静态节点；不是当次环境实例。
    #[must_use]
    pub const fn node(self) -> usize {
        self.node
    }
    /// 该 lambda 完整 capture 集的首次引用序号。
    #[must_use]
    pub const fn position(self) -> usize {
        self.position
    }
    /// 此 phi 根的静态布局槽；同一节点的不同捕获路径不得共用它保存动态子实例。
    #[must_use]
    pub const fn slot(self) -> super::CleanupCaptureSlotId {
        self.slot
    }
}

/// 一个循环边界上的 binding owner 槽；入口与出口分别定义身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationClosurePhiBinding {
    pub(crate) boundary: IterationPhiBoundary,
    pub(crate) symbol: SymbolId,
    pub(crate) owner: CleanupOwnerValueId,
    pub(crate) root_nodes: Vec<usize>,
    pub(crate) capture_layout: Vec<IterationPhiCaptureSlot>,
    pub(crate) availability_selector: CleanupSelectorId,
    pub(crate) availability_condition: CleanupConditionId,
    pub(crate) instance_presence_required: bool,
    pub(crate) origins: Vec<IterationClosurePhiOrigin>,
}
impl IterationClosurePhiBinding {
    /// 所属边界。
    #[must_use]
    pub const fn boundary(&self) -> IterationPhiBoundary {
        self.boundary
    }
    /// 接收 binding。
    #[must_use]
    pub const fn symbol(&self) -> SymbolId {
        self.symbol
    }
    /// 与其他边界和 binding 不共享的 owner 身份。
    #[must_use]
    pub const fn owner(&self) -> CleanupOwnerValueId {
        self.owner
    }
    /// 此 binding 可持有的有限图根节点；节点不是当次环境实例。
    #[must_use]
    pub fn root_nodes(&self) -> &[usize] {
        &self.root_nodes
    }
    /// 每个可达图节点的已跟踪 capture 只映射一次；动态值仍须按当次实例读取。
    #[must_use]
    pub fn capture_layout(&self) -> &[IterationPhiCaptureSlot] {
        &self.capture_layout
    }
    /// 当前动态实例有 owner 时为真的独立存在位。
    #[must_use]
    pub const fn availability_selector(&self) -> CleanupSelectorId {
        self.availability_selector
    }
    /// 当前动态实例有 owner 时为真的查询条件。
    #[must_use]
    pub const fn availability_condition(&self) -> CleanupConditionId {
        self.availability_condition
    }
    /// 每个可达图节点一份存在位布局；根节点由 `root_nodes` 标识。
    #[must_use]
    pub fn origins(&self) -> &[IterationClosurePhiOrigin] {
        &self.origins
    }
    /// 根节点的存在位布局，按有限布局顺序。
    pub fn root_origins(&self) -> impl Iterator<Item = &IterationClosurePhiOrigin> {
        self.origins
            .iter()
            .filter(|origin| self.root_nodes.contains(&origin.node()))
    }
}

/// 入边上的并行复制：所有条件及来源均读取写入前的状态。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationPhiIncoming {
    pub(crate) kind: IterationPhiIncomingKind,
    pub(crate) point: DropPoint,
    pub(crate) boundary: IterationPhiBoundary,
    pub(crate) condition: CleanupConditionId,
    pub(crate) bindings: Vec<IterationPhiIncomingBinding>,
}
impl IterationPhiIncoming {
    /// 当前执行的是哪一条循环边。
    #[must_use]
    pub const fn kind(&self) -> IterationPhiIncomingKind {
        self.kind
    }
    /// 入边复制所在的执行边界。
    #[must_use]
    pub const fn point(&self) -> DropPoint {
        self.point
    }
    /// 本次写入 header 或 exit。
    #[must_use]
    pub const fn boundary(&self) -> IterationPhiBoundary {
        self.boundary
    }
    /// 到达当前入边后的路径条件；恒真表示相对于此边无额外选择。
    #[must_use]
    pub const fn condition(&self) -> CleanupConditionId {
        self.condition
    }
    /// 按目标 phi 顺序排列的并行输入。
    #[must_use]
    pub fn bindings(&self) -> &[IterationPhiIncomingBinding] {
        &self.bindings
    }
}

/// 一个 binding 的存在位、具体 owner 值和已知 lambda 来源同时写入。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationPhiIncomingBinding {
    pub(crate) target: CleanupOwnerValueId,
    pub(crate) capture_slots_to_clear: Vec<super::CleanupCaptureSlotId>,
    pub(crate) availability_selector: CleanupSelectorId,
    pub(crate) available_when: CleanupConditionId,
    pub(crate) values: Vec<IterationPhiIncomingValue>,
    pub(crate) root_sources: Vec<IterationPhiRootSource>,
    pub(crate) presence_source: IterationPhiPresenceSource,
    pub(crate) selector_writes: Vec<IterationPhiSelectorWrite>,
    pub(crate) origins: Vec<IterationPhiIncomingOrigin>,
}
impl IterationPhiIncomingBinding {
    /// 接收本次值的 phi owner。
    #[must_use]
    pub const fn target(&self) -> CleanupOwnerValueId {
        self.target
    }
    /// 提交此入边前必须清空的目标 capture 槽；先读完整旧状态再清空并写入选中来源。
    /// 这只清理 phi 布局的引用槽，不修改已形成环境实例内部的 capture。
    #[must_use]
    pub fn capture_slots_to_clear(&self) -> &[super::CleanupCaptureSlotId] {
        &self.capture_slots_to_clear
    }
    /// 接收可用性的独立 selector。
    #[must_use]
    pub const fn availability_selector(&self) -> CleanupSelectorId {
        self.availability_selector
    }
    /// 当前边有 owner 的条件；Never 表示明确写入 false。
    /// 入边须先读取完整旧状态。旧具名 owner 未进入新 binding、但仍有有效 capture
    /// loan 时，移除其具名映射并把唯一析构义务转为 retained；不复制或立即 drop。
    /// 后续仅最后 loan 的结束凭据可交付该义务；无有效 loan 的缺席 owner 不能留存。
    #[must_use]
    pub const fn available_when(&self) -> CleanupConditionId {
        self.available_when
    }
    /// 各实际 owner 值及其入边条件。
    #[must_use]
    pub fn values(&self) -> &[IterationPhiIncomingValue] {
        &self.values
    }
    /// 已知 lambda 根节点与来源 owner 的候选映射；动态实例须再以来源句柄的
    /// closure 身份选根，后代存在位不能代替根身份。
    #[must_use]
    pub fn root_sources(&self) -> &[IterationPhiRootSource] {
        &self.root_sources
    }
    /// 存在位是否可由静态条件直接写入，或必须读取当次根实例的捕获边。
    #[must_use]
    pub const fn presence_source(&self) -> IterationPhiPresenceSource {
        self.presence_source
    }
    /// 每个预分配图节点的静态条件候选，包括缺席时的 false；当
    /// `presence_source()` 为 `CapturedInstances` 时，不能将候选直接作为实际存在位。
    #[must_use]
    pub fn selector_writes(&self) -> &[IterationPhiSelectorWrite] {
        &self.selector_writes
    }
    /// 路径局部的候选环境；不作为 selector 写集使用。
    #[must_use]
    pub fn origins(&self) -> &[IterationPhiIncomingOrigin] {
        &self.origins
    }
}

/// 循环入边的节点存在位必须从哪种来源确定。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationPhiPresenceSource {
    /// 静态条件写集精确描述每个节点。
    StaticConditions,
    /// 先按 `root_sources` 选择当次根实例，再沿其形成时保存的捕获边遍历有限图；
    /// 只把实际到达的节点写 true，其余布局节点写 false。
    CapturedInstances,
}

/// 根句柄的有限图候选；同一节点可对应不同轮次形成的实例。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IterationPhiRootSource {
    pub(crate) node: usize,
    pub(crate) source: CleanupOwnerValueId,
    pub(crate) condition: CleanupConditionId,
}
impl IterationPhiRootSource {
    /// 目标 phi 的有限捕获图节点。
    #[must_use]
    pub const fn node(self) -> usize {
        self.node
    }
    /// 复制前持有根实例句柄的 owner。
    #[must_use]
    pub const fn source(self) -> CleanupOwnerValueId {
        self.source
    }
    /// 复制前的候选条件；实例模式下，后代存在可能也使此条件为真，
    /// 仍须用来源句柄的 closure 身份确认它是当前根。
    #[must_use]
    pub const fn condition(self) -> CleanupConditionId {
        self.condition
    }
}

/// 入边在有限图节点上写入的存在位；全部条件从写入前状态读取。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IterationPhiSelectorWrite {
    pub(crate) node: usize,
    pub(crate) target: CleanupSelectorId,
    pub(crate) condition: CleanupConditionId,
}
impl IterationPhiSelectorWrite {
    /// 对应的有限捕获图节点。
    #[must_use]
    pub const fn node(self) -> usize {
        self.node
    }
    /// 接收此存在位的选择器。
    #[must_use]
    pub const fn target(self) -> CleanupSelectorId {
        self.target
    }
    /// 写入前状态下该图节点存在的条件。
    #[must_use]
    pub const fn condition(self) -> CleanupConditionId {
        self.condition
    }
}

/// owner 值本身的条件复制，不能用 binding 身份代替来源值。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IterationPhiIncomingValue {
    pub(crate) source: CleanupOwnerValueId,
    pub(crate) condition: CleanupConditionId,
}
impl IterationPhiIncomingValue {
    /// 复制前的具体 owner 值。
    #[must_use]
    pub const fn source(self) -> CleanupOwnerValueId {
        self.source
    }
    /// 该值存在于当前入边的条件。
    #[must_use]
    pub const fn condition(self) -> CleanupConditionId {
        self.condition
    }
}

/// 一条捕获路径的已知来源；缺席时其局部条件为 false，全局位由 selector_writes 合并。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationPhiIncomingOrigin {
    pub(crate) node: usize,
    pub(crate) target: CleanupSelectorId,
    pub(crate) condition: CleanupConditionId,
    pub(crate) environments: Vec<IterationPhiIncomingEnvironment>,
}
impl IterationPhiIncomingOrigin {
    /// 与目标 phi 布局一致的有限 capture 图节点索引。
    #[must_use]
    pub const fn node(&self) -> usize {
        self.node
    }
    /// 接收来源存在位的 selector。
    #[must_use]
    pub const fn target(&self) -> CleanupSelectorId {
        self.target
    }
    /// 当前捕获路径的任一实际环境存在时为真。
    #[must_use]
    pub const fn condition(&self) -> CleanupConditionId {
        self.condition
    }
    /// 同一 lambda 在不同值定义下的候选实际环境。
    #[must_use]
    pub fn environments(&self) -> &[IterationPhiIncomingEnvironment] {
        &self.environments
    }
}

/// 同一 lambda 的不同实际环境值不能按 AST identity 合并。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationPhiIncomingEnvironment {
    pub(crate) owner: CleanupOwnerValueId,
    pub(crate) instance_root: CleanupOwnerValueId,
    pub(crate) capture_path: Vec<usize>,
    pub(crate) condition: CleanupConditionId,
    pub(crate) sources: Vec<IterationPhiIncomingSource>,
}
impl IterationPhiIncomingEnvironment {
    /// 复制前的实际环境 owner。
    #[must_use]
    pub const fn owner(&self) -> CleanupOwnerValueId {
        self.owner
    }
    /// 从该入边的根 owner 值取得当次环境句柄；不能用 `owner()` 推断嵌套实例。
    #[must_use]
    pub const fn instance_root(&self) -> CleanupOwnerValueId {
        self.instance_root
    }
    /// 从根实例沿各级已形成环境的原始 capture 位置读取子实例句柄。
    #[must_use]
    pub fn capture_path(&self) -> &[usize] {
        &self.capture_path
    }
    /// 该环境存在于当前边的条件。
    #[must_use]
    pub const fn condition(&self) -> CleanupConditionId {
        self.condition
    }
    /// 按捕获声明顺序排列的来源关系。
    #[must_use]
    pub fn sources(&self) -> &[IterationPhiIncomingSource] {
        &self.sources
    }
}

/// 捕获来源关系读取当前 owner/紧邻环境；非 owning Place 没有目标 owner 槽。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationPhiIncomingSource {
    pub(crate) target: Option<CleanupOwnerValueId>,
    pub(crate) capture_slot: Option<super::CleanupCaptureSlotId>,
    pub(crate) source_capture_slot: Option<super::CleanupCaptureSlotId>,
    pub(crate) read_address: Option<CleanupInstanceAddressId>,
    pub(crate) source_environment: CleanupOwnerValueId,
    pub(crate) nested_instance: bool,
    pub(crate) input: CleanupCaptureInput,
    pub(crate) captured: Vec<IterationPhiIncomingOrigin>,
}
impl IterationPhiIncomingSource {
    /// 接收环境的静态捕获槽；运行时是否持值仍由本入边条件与来源值决定。
    #[must_use]
    pub const fn capture_slot(&self) -> Option<super::CleanupCaptureSlotId> {
        self.capture_slot
    }
    /// 复制前的来源布局槽。根层可配合 `transport_value()` 读取；嵌套层只取其原始
    /// capture 位置，实际实例由所属环境的根句柄与捕获路径定位。
    #[must_use]
    pub const fn source_capture_slot(&self) -> Option<super::CleanupCaptureSlotId> {
        self.source_capture_slot
    }
    /// 从当次根实例沿已保存的捕获边定位来源环境，再读其原始捕获槽。
    /// 地址 ID 不是实例句柄；执行入边时须以复制前状态解析，并与其他读取一起并行提交。
    #[must_use]
    pub const fn transport_read(
        &self,
    ) -> Option<(CleanupInstanceAddressId, super::CleanupCaptureSlotId)> {
        match (self.read_address, self.source_capture_slot) {
            (Some(address), Some(slot)) => Some((address, slot)),
            _ => None,
        }
    }
    /// 根层入边复制读取已形成环境的当次捕获槽；形成输入可能指向已被 move 清空的外层槽。
    /// 嵌套来源返回 `None`，须从所属环境的 `instance_root()` 沿 `capture_path()`
    /// 找到实际子实例，再以 `source_capture_slot()` 的捕获位置读取该实例。
    /// 没有已发布目标槽或来源槽时同样返回 `None`，不得用形成输入或仅供定位的
    /// 原始来源槽猜测静态 phi 运输值。
    #[must_use]
    pub const fn transport_value(&self) -> Option<CleanupCaptureValue> {
        match (
            self.nested_instance,
            self.capture_slot,
            self.source_capture_slot,
        ) {
            (false, Some(_), Some(slot)) => Some(CleanupCaptureValue::Environment {
                owner: self.source_environment,
                source: self.input.source,
                slot,
            }),
            _ => None,
        }
    }
    /// 拥有值的来源关系槽；非 owning Place 没有目标槽。
    #[must_use]
    pub const fn target(&self) -> Option<CleanupOwnerValueId> {
        self.target
    }
    /// 候选的已检查来源或 header 转发来源；不表示 phi 复制时仍从该位置读取。
    #[must_use]
    pub const fn value(&self) -> CleanupCaptureValue {
        self.input.value()
    }
    /// 保留捕获 mode、effect、来源范围与已检查条件。
    #[must_use]
    pub const fn input(&self) -> CleanupCaptureInput {
        self.input
    }
    /// 被这个 owned capture 持有的内层环境来源及动态存在位。
    #[must_use]
    pub fn captured(&self) -> &[IterationPhiIncomingOrigin] {
        &self.captured
    }
}

/// 某个已知 lambda 来源的存在位，独立于 owner value 身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationClosurePhiOrigin {
    pub(crate) node: usize,
    pub(crate) closure: ExpressionId,
    pub(crate) selector: CleanupSelectorId,
    pub(crate) condition: CleanupConditionId,
    pub(crate) sources: Vec<IterationClosurePhiSource>,
}
impl IterationClosurePhiOrigin {
    /// 有限 capture 图中的节点索引，不是动态环境实例身份。
    #[must_use]
    pub const fn node(&self) -> usize {
        self.node
    }
    /// 可能的 lambda 来源。
    #[must_use]
    pub const fn closure(&self) -> ExpressionId {
        self.closure
    }
    /// 此边界的独立 bool selector；不表示它已经初始化。
    #[must_use]
    pub const fn selector(&self) -> CleanupSelectorId {
        self.selector
    }
    /// 此来源实际存在时为真的清理条件。
    #[must_use]
    pub const fn condition(&self) -> CleanupConditionId {
        self.condition
    }
    /// 捕获所引用的实际 source owner 槽；Shared 仍只是 loan，不取得所有权。
    #[must_use]
    pub fn sources(&self) -> &[IterationClosurePhiSource] {
        &self.sources
    }
}

/// 已知 lambda 捕获所关联的 owned source；Shared 只引用，Owned/Move 才转移所有权。
/// 此槽尚未由实际入边填入动态实例。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationClosurePhiSource {
    pub(crate) source: ClosureCaptureSource,
    pub(crate) mode: ClosureCaptureMode,
    pub(crate) effect: ClosureCaptureEffect,
    pub(crate) owner: CleanupOwnerValueId,
    pub(crate) captured: Vec<usize>,
}
impl IterationClosurePhiSource {
    /// 捕获来源的解析后身份。
    #[must_use]
    pub const fn source(&self) -> ClosureCaptureSource {
        self.source
    }
    /// 共享或独占持有方式。
    #[must_use]
    pub const fn mode(&self) -> ClosureCaptureMode {
        self.mode
    }
    /// 捕获形成时的值效果。
    #[must_use]
    pub const fn effect(&self) -> ClosureCaptureEffect {
        self.effect
    }
    /// 与环境 phi 绑定的实际 source owner 身份，不表示 closure 必然拥有它。
    #[must_use]
    pub const fn owner(&self) -> CleanupOwnerValueId {
        self.owner
    }
    /// 此 source 可持有的已知 closure 节点；索引进所属 binding 的有限布局。
    #[must_use]
    pub fn captured(&self) -> &[usize] {
        &self.captured
    }
}

/// 可能持有 MoveOnly owner 的 binding；空来源表示 opaque 或无已知 lambda 来源。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IterationClosureBinding {
    pub(crate) symbol: SymbolId,
    pub(crate) origins: Vec<ExpressionId>,
}
impl IterationClosureBinding {
    /// 词法 binding，也是独立于已知 lambda 来源的 owner 可用性 key。
    #[must_use]
    pub fn symbol(&self) -> SymbolId {
        self.symbol
    }
    /// 按 AST identity 排序的可能来源，不表示当次动态实例。
    #[must_use]
    pub fn origins(&self) -> &[ExpressionId] {
        &self.origins
    }
}

/// 循环入口及回边的最小固定点，与正常出口的可用 owner 和已知来源。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IterationClosureFlow {
    pub(crate) header: Vec<IterationClosureBinding>,
    pub(crate) exit: Vec<IterationClosureBinding>,
}
impl IterationClosureFlow {
    /// 入口、fallthrough 和 continue 合流；不含 break/return/abort。
    #[must_use]
    pub fn header(&self) -> &[IterationClosureBinding] {
        &self.header
    }
    /// 零轮/耗尽与 break 的合流。
    #[must_use]
    pub fn exit(&self) -> &[IterationClosureBinding] {
        &self.exit
    }
}
