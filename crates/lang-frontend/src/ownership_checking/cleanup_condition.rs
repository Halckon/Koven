//! 清理条件引用保存的控制选择，不重新执行用户条件。
use std::collections::BTreeMap;

use super::{ClosureCaptureSource, IterationPhiBoundary};
use crate::{
    ast::{ExpressionId, StatementId},
    source::Span,
};

mod capture;
mod snapshot;
pub use capture::{
    CleanupCaptureEdge, CleanupCaptureInput, CleanupCaptureSlot, CleanupCaptureSlotId,
    CleanupCaptureValue, CleanupInstanceAddress, CleanupInstanceAddressId,
};
pub use snapshot::{
    CleanupOwnerInput, CleanupOwnerSnapshot, CleanupOwnerValue, CleanupOwnerValueId,
    CleanupSelectorCopy,
};

/// 同一个所有权产物内的稳定条件身份。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CleanupConditionId(usize);

impl CleanupConditionId {
    pub(crate) const ALWAYS: Self = Self(0);
    pub(crate) const NEVER: Self = Self(1);

    /// 返回条件表中的索引；身份只在所属产物中有效。
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// 一个保存的分支选择身份；相同 control 的不同 owner 实例不能共用身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CleanupSelectorId(usize);

impl CleanupSelectorId {
    /// 身份只在所属所有权产物中有效。
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// 一次控制求值保存哪种选择；同一 when 的各 alternative 有独立身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CleanupSelection {
    /// 普通控制表达式的分支下标；Elvis 的 0 为非空交付，1 为 null RHS。
    Branch,
    /// 当次 alternative 比较结果：0 为匹配，1 为继续匹配后继。
    WhenAlternative {
        /// 源码 entry 下标。
        entry: usize,
        /// entry 内 alternative 下标。
        alternative: usize,
    },
    /// 循环 phi 来源是否存在；0 不存在，1 存在。
    IterationPresence,
    /// 当前 source 实例的 capture loan 已全部结束；0 否，1 是。
    LastCaptureLoan,
}

/// selector 的来源身份；循环 phi 不借用任何表达式 AST 身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupSelectorSource {
    /// 一次已求值的源码控制表达式。
    Control(ExpressionId),
    /// 一个循环边界上预分配的有限来源槽。
    IterationPhi {
        /// 所属循环。
        statement: StatementId,
        /// 入口或出口。
        boundary: IterationPhiBoundary,
        /// 该边界按 binding/lambda 顺序排列的槽下标。
        slot: usize,
    },
    /// 一次 shared capture 结束后，查询实际 source 实例的剩余 loan。
    CaptureLoan {
        /// 已运输的 source owner 槽；运行时值决定实例身份。
        owner: CleanupOwnerValueId,
    },
}

/// 选择的静态来源与布局；保存/转移位置由 producer 的生命周期契约决定。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupSelector {
    source: CleanupSelectorSource,
    selection: CleanupSelection,
    origin: Span,
    branch_count: usize,
}

impl CleanupSelector {
    /// 来源类型与身份；phi 来源绝不伪装成表达式。
    #[must_use]
    pub const fn source(self) -> CleanupSelectorSource {
        self.source
    }

    /// 产生普通控制选择的源码表达式；循环 phi 没有控制表达式。
    #[must_use]
    pub const fn control(self) -> Option<ExpressionId> {
        match self.source {
            CleanupSelectorSource::Control(control) => Some(control),
            CleanupSelectorSource::IterationPhi { .. }
            | CleanupSelectorSource::CaptureLoan { .. } => None,
        }
    }

    /// 所保存的选择，不能仅按 control AST 区分 when alternatives。
    #[must_use]
    pub const fn selection(self) -> CleanupSelection {
        self.selection
    }

    /// 原控制范围，供诊断追溯。
    #[must_use]
    pub const fn origin(self) -> Span {
        self.origin
    }

    /// 保存的选择允许的分支数量。
    #[must_use]
    pub const fn branch_count(self) -> usize {
        self.branch_count
    }
}

/// 按 selector 分配顺序排序的多路决策节点；保护选择须先于受保护选择分配。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CleanupCondition {
    /// 当前事实所在边界无条件执行。
    Always,
    /// 不可执行的清理路径；不得发布对应 drop。
    Never,
    /// 查询当次控制求值保存的选择，再查询对应子条件。
    /// 选择必须随 owner 动态实例传递，不能读取源码变量或全局最近选择。
    Choice {
        /// 查询独立选择身份，不能以其 control AST 身份替代。
        selector: CleanupSelectorId,
        /// 源码顺序的分支；包含存在的隐式 fallthrough 分支。
        branches: Vec<CleanupConditionId>,
    },
}

/// 单文件清理条件表；只读消费者与 facts 共享同一分析身份。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanupConditions {
    nodes: Vec<CleanupCondition>,
    choices: BTreeMap<(CleanupSelectorId, Vec<CleanupConditionId>), CleanupConditionId>,
    selectors: Vec<CleanupSelector>,
    controls: BTreeMap<(usize, CleanupSelection), CleanupSelectorId>,
    iteration_phis: BTreeMap<(usize, IterationPhiBoundary, usize), CleanupSelectorId>,
    owners: Vec<CleanupOwnerValue>,
    capture_slots: Vec<CleanupCaptureSlot>,
    instance_addresses: Vec<CleanupInstanceAddress>,
    instance_address_keys: BTreeMap<(CleanupOwnerValueId, Vec<usize>), CleanupInstanceAddressId>,
    capture_slot_keys: BTreeMap<(CleanupOwnerValueId, ClosureCaptureSource), CleanupCaptureSlotId>,
    phi_capture_slot_keys:
        BTreeMap<(CleanupOwnerValueId, usize, ClosureCaptureSource), CleanupCaptureSlotId>,
}

impl Default for CleanupConditions {
    fn default() -> Self {
        Self {
            nodes: vec![CleanupCondition::Always, CleanupCondition::Never],
            choices: BTreeMap::new(),
            selectors: Vec::new(),
            controls: BTreeMap::new(),
            iteration_phis: BTreeMap::new(),
            owners: Vec::new(),
            capture_slots: Vec::new(),
            instance_addresses: Vec::new(),
            instance_address_keys: BTreeMap::new(),
            capture_slot_keys: BTreeMap::new(),
            phi_capture_slot_keys: BTreeMap::new(),
        }
    }
}

impl CleanupConditions {
    /// 相同根与捕获位置路径复用一个稳定地址身份。
    pub(crate) fn register_instance_address(
        &mut self,
        root: CleanupOwnerValueId,
        capture_path: &[usize],
    ) -> CleanupInstanceAddressId {
        let key = (root, capture_path.to_vec());
        if let Some(&id) = self.instance_address_keys.get(&key) {
            return id;
        }
        let id = CleanupInstanceAddressId(self.instance_addresses.len());
        self.instance_addresses.push(CleanupInstanceAddress {
            root,
            capture_path: key.1.clone(),
        });
        self.instance_address_keys.insert(key, id);
        id
    }

    /// 已形成环境的实例地址；静态路径按当次捕获槽内容读取。
    #[must_use]
    pub fn instance_address(
        &self,
        id: CleanupInstanceAddressId,
    ) -> Option<&CleanupInstanceAddress> {
        self.instance_addresses.get(id.index())
    }

    /// 紧邻 closure 环境的静态捕获槽；重复执行时仍须用当次环境实例定位槽值。
    #[must_use]
    pub fn capture_slot(
        &self,
        environment: CleanupOwnerValueId,
        source: ClosureCaptureSource,
    ) -> Option<CleanupCaptureSlotId> {
        self.capture_slot_keys.get(&(environment, source)).copied()
    }

    /// 一个 phi 环境的候选 lambda 槽；同 source 的不同 lambda 不共用槽身份。
    #[must_use]
    pub fn phi_capture_slot(
        &self,
        environment: CleanupOwnerValueId,
        closure: ExpressionId,
        source: ClosureCaptureSource,
    ) -> Option<CleanupCaptureSlotId> {
        self.phi_capture_slot_keys
            .get(&(environment, closure.index(), source))
            .copied()
    }

    /// 按稳定身份查询槽布局，不把静态槽当作运行时 owner 实例。
    #[must_use]
    pub fn capture_slot_value(&self, slot: CleanupCaptureSlotId) -> Option<CleanupCaptureSlot> {
        self.capture_slots.get(slot.index()).copied()
    }

    /// 已形成 closure 的捕获写入关系；同槽的条件输入保留为独立边。
    /// 非 closure owner 或缺少任一已注册槽时返回 None；不发布截断关系。
    /// 静态 owner/slot 只定位读写位置，不代表本次执行的环境实例。
    #[must_use]
    pub fn closure_capture_edges(
        &self,
        owner: CleanupOwnerValueId,
    ) -> Option<Vec<CleanupCaptureEdge>> {
        let CleanupOwnerValue::Closure { inputs, .. } = self.owner_value(owner)? else {
            return None;
        };
        inputs
            .iter()
            .copied()
            .map(|input| {
                Some(CleanupCaptureEdge {
                    target: self.capture_slot(owner, input.source())?,
                    input,
                })
            })
            .collect()
    }

    pub(crate) fn register_capture_slot(
        &mut self,
        environment: CleanupOwnerValueId,
        closure: ExpressionId,
        source: ClosureCaptureSource,
        position: usize,
    ) -> CleanupCaptureSlotId {
        if let Some(slot) = self.capture_slot(environment, source) {
            return slot;
        }
        let slot = CleanupCaptureSlotId(self.capture_slots.len());
        self.capture_slots.push(CleanupCaptureSlot {
            environment,
            closure,
            source,
            position,
        });
        self.capture_slot_keys.insert((environment, source), slot);
        slot
    }

    pub(crate) fn register_phi_capture_slot(
        &mut self,
        environment: CleanupOwnerValueId,
        closure: ExpressionId,
        source: ClosureCaptureSource,
        position: usize,
    ) -> CleanupCaptureSlotId {
        if let Some(slot) = self.phi_capture_slot(environment, closure, source) {
            return slot;
        }
        let slot = CleanupCaptureSlotId(self.capture_slots.len());
        self.capture_slots.push(CleanupCaptureSlot {
            environment,
            closure,
            source,
            position,
        });
        self.phi_capture_slot_keys
            .insert((environment, closure.index(), source), slot);
        slot
    }

    /// 查询本产物中的条件；无效身份不产生节点。
    #[must_use]
    pub fn get(&self, condition: CleanupConditionId) -> Option<&CleanupCondition> {
        self.nodes.get(condition.index())
    }

    /// 返回按稳定构造顺序保存的条件节点。
    #[must_use]
    pub fn nodes(&self) -> &[CleanupCondition] {
        &self.nodes
    }

    /// 按身份查询选择来源；不同身份允许来自同一控制表达式。
    #[must_use]
    pub fn selector(&self, selector: CleanupSelectorId) -> Option<&CleanupSelector> {
        self.selectors.get(selector.index())
    }

    /// 返回按稳定分配顺序保存的选择布局。
    #[must_use]
    pub fn selectors(&self) -> &[CleanupSelector] {
        &self.selectors
    }

    /// 分配独立选择，不能按 AST 去重，否则旧 owner 与新 RHS 会互相覆盖。
    fn allocate_selector(&mut self, layout: CleanupSelector) -> CleanupSelectorId {
        let selector = CleanupSelectorId(self.selectors.len());
        self.selectors.push(layout);
        selector
    }

    /// Typed control 提供分支布局；公式须带可达前提，不能单独查询未执行子分支。
    /// 相同身份的源码范围和分支数量必须一致。
    pub(crate) fn branch(
        &mut self,
        control: ExpressionId,
        origin: Span,
        count: usize,
        selected: usize,
    ) -> Option<CleanupConditionId> {
        self.control_selection(control, origin, count, selected, CleanupSelection::Branch)
    }

    pub(crate) fn when_alternative(
        &mut self,
        control: ExpressionId,
        origin: Span,
        entry: usize,
        alternative: usize,
        matched: bool,
    ) -> Option<CleanupConditionId> {
        self.control_selection(
            control,
            origin,
            2,
            usize::from(!matched),
            CleanupSelection::WhenAlternative { entry, alternative },
        )
    }

    /// 对静态来源槽分配独立的运行时存在标记；分配不代表任何入边已初始化。
    pub(crate) fn iteration_presence(
        &mut self,
        statement: StatementId,
        boundary: IterationPhiBoundary,
        slot: usize,
        origin: Span,
    ) -> CleanupConditionId {
        let key = (statement.index(), boundary, slot);
        let selector = if let Some(&selector) = self.iteration_phis.get(&key) {
            selector
        } else {
            let selector = self.allocate_selector(CleanupSelector {
                source: CleanupSelectorSource::IterationPhi {
                    statement,
                    boundary,
                    slot,
                },
                selection: CleanupSelection::IterationPresence,
                origin,
                branch_count: 2,
            });
            self.iteration_phis.insert(key, selector);
            selector
        };
        self.selector_branch(selector, 1)
            .expect("preallocated iteration presence has two branches")
    }

    /// 每次释放独立分配查询位；由同点 TestLastCaptureLoan 动作在所有 loan end 后写入。
    pub(crate) fn last_capture_loan(
        &mut self,
        owner: CleanupOwnerValueId,
        origin: Span,
    ) -> (CleanupSelectorId, CleanupConditionId) {
        let selector = self.allocate_selector(CleanupSelector {
            source: CleanupSelectorSource::CaptureLoan { owner },
            selection: CleanupSelection::LastCaptureLoan,
            origin,
            branch_count: 2,
        });
        let condition = self
            .selector_branch(selector, 1)
            .expect("last capture loan has two branches");
        (selector, condition)
    }

    fn control_selection(
        &mut self,
        control: ExpressionId,
        origin: Span,
        count: usize,
        selected: usize,
        selection: CleanupSelection,
    ) -> Option<CleanupConditionId> {
        let layout = CleanupSelector {
            source: CleanupSelectorSource::Control(control),
            selection,
            origin,
            branch_count: count,
        };
        if selected >= count {
            return None;
        }
        let selector = if let Some(&selector) = self.controls.get(&(control.index(), selection)) {
            if self.selector(selector) != Some(&layout) {
                return None;
            }
            selector
        } else {
            let selector = self.allocate_selector(layout);
            self.controls.insert((control.index(), selection), selector);
            selector
        };
        self.selector_branch(selector, selected)
    }

    fn selector_branch(
        &mut self,
        selector: CleanupSelectorId,
        selected: usize,
    ) -> Option<CleanupConditionId> {
        let count = self.selector(selector)?.branch_count;
        if selected >= count {
            return None;
        }
        let mut branches = vec![CleanupConditionId::NEVER; count];
        branches[selected] = CleanupConditionId::ALWAYS;
        Some(self.choice(selector, branches))
    }

    pub(crate) fn and(
        &mut self,
        left: CleanupConditionId,
        right: CleanupConditionId,
    ) -> CleanupConditionId {
        self.combine(left, right, true)
    }

    pub(crate) fn or(
        &mut self,
        left: CleanupConditionId,
        right: CleanupConditionId,
    ) -> CleanupConditionId {
        self.combine(left, right, false)
    }

    pub(crate) fn not(&mut self, condition: CleanupConditionId) -> CleanupConditionId {
        match self.nodes[condition.index()].clone() {
            CleanupCondition::Always => CleanupConditionId::NEVER,
            CleanupCondition::Never => CleanupConditionId::ALWAYS,
            CleanupCondition::Choice { selector, branches } => {
                let branches = branches
                    .into_iter()
                    .map(|branch| self.not(branch))
                    .collect();
                self.choice(selector, branches)
            }
        }
    }

    fn choice(
        &mut self,
        selector: CleanupSelectorId,
        branches: Vec<CleanupConditionId>,
    ) -> CleanupConditionId {
        // Every caller supplies the nonempty layout established by branch().
        if branches.iter().all(|&branch| branch == branches[0]) {
            return branches[0];
        }
        let key = (selector, branches.clone());
        if let Some(&id) = self.choices.get(&key) {
            return id;
        }
        let id = CleanupConditionId(self.nodes.len());
        self.nodes
            .push(CleanupCondition::Choice { selector, branches });
        self.choices.insert(key, id);
        id
    }

    fn combine(
        &mut self,
        left: CleanupConditionId,
        right: CleanupConditionId,
        conjunction: bool,
    ) -> CleanupConditionId {
        let (identity, absorbing) = if conjunction {
            (CleanupConditionId::ALWAYS, CleanupConditionId::NEVER)
        } else {
            (CleanupConditionId::NEVER, CleanupConditionId::ALWAYS)
        };
        if left == absorbing || right == absorbing {
            return absorbing;
        }
        if left == identity || left == right {
            return right;
        }
        if right == identity {
            return left;
        }
        let CleanupCondition::Choice {
            selector: left_selector,
            branches: left_branches,
        } = self.nodes[left.index()].clone()
        else {
            return left;
        };
        let CleanupCondition::Choice {
            selector: right_selector,
            branches: right_branches,
        } = self.nodes[right.index()].clone()
        else {
            return right;
        };
        // 保存的选择必须排在其执行路径保护之后；原控制的 Span 只用于溯源。
        let selector = left_selector.min(right_selector);
        let count = if selector == left_selector {
            left_branches.len()
        } else {
            right_branches.len()
        };
        let branches = (0..count)
            .map(|index| {
                let left = if selector == left_selector {
                    left_branches[index]
                } else {
                    left
                };
                let right = if selector == right_selector {
                    right_branches[index]
                } else {
                    right
                };
                self.combine(left, right, conjunction)
            })
            .collect();
        self.choice(selector, branches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controls() -> Vec<(ExpressionId, Span)> {
        let mut sources = crate::source::SourceMap::new();
        let source = sources
            .add_source(
                "guards.ko",
                "fun f() { if (a) {} else {}\nwhen { a -> 1\nb -> 2\nelse -> 3 } }",
            )
            .unwrap();
        let lexed = crate::lexer::lex(&sources, source).unwrap();
        let parsed = crate::parser::parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(
                    node.payload(),
                    crate::parser::Expression::If { .. } | crate::parser::Expression::When { .. }
                )
                .then_some((id, node.span()))
            })
            .collect()
    }

    fn evaluate(
        table: &CleanupConditions,
        id: CleanupConditionId,
        selections: &[(ExpressionId, usize)],
    ) -> bool {
        match table.get(id).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                let control = table.selector(*selector).unwrap().control().unwrap();
                let choice = selections.iter().find(|(id, _)| *id == control).unwrap().1;
                evaluate(table, branches[choice], selections)
            }
        }
    }

    #[test]
    fn repeated_control_choices_do_not_collapse_across_owner_instances() {
        let (control, origin) = controls()[0];
        let mut table = CleanupConditions::default();
        let layout = CleanupSelector {
            source: CleanupSelectorSource::Control(control),
            selection: CleanupSelection::Branch,
            origin,
            branch_count: 2,
        };
        let old = table.allocate_selector(layout);
        let rhs = table.allocate_selector(layout);
        let old_then = table.selector_branch(old, 0).unwrap();
        let rhs_else = table.selector_branch(rhs, 1).unwrap();
        let both = table.and(old_then, rhs_else);
        assert_ne!(
            both,
            CleanupConditionId::NEVER,
            "old then and new else can coexist"
        );
        assert_ne!(old_then, table.selector_branch(rhs, 0).unwrap());
        assert_eq!(both, table.and(rhs_else, old_then));
        let either = table.or(old_then, rhs_else);
        assert_ne!(
            either,
            CleanupConditionId::ALWAYS,
            "independent instances are not complements"
        );
        let opposite = table.not(both);
        assert_eq!(table.and(both, opposite), CleanupConditionId::NEVER);

        fn selected(table: &CleanupConditions, id: CleanupConditionId, arms: &[usize]) -> bool {
            match table.get(id).unwrap() {
                CleanupCondition::Always => true,
                CleanupCondition::Never => false,
                CleanupCondition::Choice { selector, branches } => {
                    selected(table, branches[arms[selector.index()]], arms)
                }
            }
        }
        for old_arm in 0..2 {
            for rhs_arm in 0..2 {
                let arms = [old_arm, rhs_arm];
                assert_eq!(selected(&table, both, &arms), old_arm == 0 && rhs_arm == 1);
                assert_eq!(
                    selected(&table, either, &arms),
                    old_arm == 0 || rhs_arm == 1
                );
            }
        }
        assert_eq!(table.selector(old), table.selector(rhs));
        assert_ne!(old, rhs);
        let before = table.clone();
        assert!(table.selector_branch(rhs, 2).is_none());
        assert!(table.selector_branch(CleanupSelectorId(99), 0).is_none());
        assert_eq!(before, table);
    }

    #[test]
    fn nested_choices_preserve_each_path_and_canonical_boolean_operations() {
        let ids = controls();
        let mut table = CleanupConditions::default();
        let a = table.branch(ids[0].0, ids[0].1, 2, 0).unwrap();
        let b = table.branch(ids[1].0, ids[1].1, 3, 1).unwrap();
        let both = table.and(a, b);
        assert_eq!(both, table.and(b, a));
        let either = table.or(a, b);
        assert_eq!(either, table.or(b, a));
        let inverse = table.not(both);
        assert_eq!(table.and(both, inverse), CleanupConditionId::NEVER);
        assert_eq!(table.or(both, inverse), CleanupConditionId::ALWAYS);
        for x in 0..2 {
            for y in 0..3 {
                let selections = [(ids[0].0, x), (ids[1].0, y)];
                assert_eq!(evaluate(&table, both, &selections), x == 0 && y == 1);
                assert_eq!(evaluate(&table, either, &selections), x == 0 || y == 1);
                assert_eq!(evaluate(&table, inverse, &selections), !(x == 0 && y == 1));
            }
        }
    }

    #[test]
    fn all_branches_cover_the_control_without_repeating_or_invalid_layouts() {
        let id = controls()[1];
        let mut table = CleanupConditions::default();
        let a = table.branch(id.0, id.1, 3, 0).unwrap();
        let b = table.branch(id.0, id.1, 3, 1).unwrap();
        let c = table.branch(id.0, id.1, 3, 2).unwrap();
        assert_eq!(a, table.branch(id.0, id.1, 3, 0).unwrap());
        assert_eq!(table.and(a, b), CleanupConditionId::NEVER);
        let ab = table.or(a, b);
        assert_eq!(table.or(ab, c), CleanupConditionId::ALWAYS);
        let before = table.clone();
        assert!(table.branch(id.0, id.1, 2, 0).is_none());
        assert!(table.branch(id.0, id.1, 3, 3).is_none());
        assert!(table.branch(id.0, controls()[0].1, 3, 0).is_none());
        assert_eq!(before, table);
    }
    #[test]
    fn unreachable_inner_choice_is_not_read_before_its_outer_guard() {
        let mut sources = crate::source::SourceMap::new();
        let source = sources
            .add_source(
                "nested.ko",
                "fun f() { if (a) { if (b) {} else {} } else {} }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let ids = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), crate::parser::Expression::If { .. })
                    .then_some((id, node.span()))
            })
            .collect::<Vec<_>>();
        let mut table = CleanupConditions::default();
        let outer = table.branch(ids[1].0, ids[1].1, 2, 0).unwrap();
        let inner = table.branch(ids[0].0, ids[0].1, 2, 0).unwrap();
        let selected = table.and(outer, inner);
        assert!(!evaluate(&table, selected, &[(ids[1].0, 1)]));
        let not_selected = table.not(selected);
        assert!(evaluate(&table, not_selected, &[(ids[1].0, 1)]));
    }
}
