//! Owner 值保存独立选择；复制读取源快照，不能读取未执行的内层控制。
use std::collections::BTreeMap;

use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    ownership_checking::ClosureCaptureSource,
    source::Span,
};

use super::{
    super::IterationPhiBoundary, CleanupCaptureInput, CleanupCondition, CleanupConditionId,
    CleanupConditions, CleanupSelectorId,
};

/// 静态 owner 值定义身份；不同定义不因来源相同而合并。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CleanupOwnerValueId(usize);

impl CleanupOwnerValueId {
    /// 身份只在所属条件表中有效；循环动态实例须通过 phi 运输。
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// 一个 owned 值的定义；环境快照绑定完整 RHS 并补充已知 capture 来源。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CleanupOwnerValue {
    /// Callable 入口建立的 owned 参数；非 owning 参数不建立此定义。
    Parameter {
        /// 参数身份。
        symbol: SymbolId,
        /// 声明位置。
        origin: Span,
    },
    /// 完整表达式求值得到的新值，Name 移动不建立新定义。
    Expression {
        /// 求值身份。
        expression: ExpressionId,
        /// 求值来源。
        origin: Span,
    },
    /// 消费式解构产生的独立 component owner。
    Component {
        /// 解构操作。
        statement: StatementId,
        /// 接收 component 的 binding。
        symbol: SymbolId,
        /// 声明来源。
        origin: Span,
    },
    /// 本次 lambda 求值建立的环境；循环实例须另经 phi 运输。
    Closure {
        /// 形成环境的表达式。
        expression: ExpressionId,
        /// 定义来源。
        origin: Span,
        /// 形成时的输入；后续快照不能原地改写这些条件。
        inputs: Vec<CleanupCaptureInput>,
    },
    /// 选定来源随条件快照转移的新值。
    Snapshot(CleanupOwnerSnapshot),
    /// 循环边界上的独立 owner 槽；必须由各实际入边初始化后才能执行。
    IterationPhi {
        /// 所属循环。
        statement: StatementId,
        /// 入口或出口。
        boundary: IterationPhiBoundary,
        /// 所持有的 binding 身份。
        symbol: SymbolId,
        /// 源码来源。
        origin: Span,
    },
    /// 环境 phi 捕获所引用的 source owner；Shared 只保存借用关系，动态实例由入边提供。
    IterationPhiSourceOwner {
        /// 包含此捕获槽的环境 phi owner。
        environment: CleanupOwnerValueId,
        /// 捕获槽所在的 lambda 来源。
        closure: ExpressionId,
        /// 解析后的捕获来源。
        source: ClosureCaptureSource,
        /// 捕获引用位置。
        origin: Span,
    },
}

/// 已知捕获来源的关系；不枚举 opaque 函数值，完整环境由 snapshot.value 定位。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupOwnerInput {
    owner: CleanupOwnerValueId,
    condition: CleanupConditionId,
}
impl CleanupOwnerInput {
    /// 输入环境值，不能以 lambda 或 binding 身份代替。
    #[must_use]
    pub const fn owner(self) -> CleanupOwnerValueId {
        self.owner
    }
    /// 保存前状态上的选择条件。
    #[must_use]
    pub const fn condition(self) -> CleanupConditionId {
        self.condition
    }
}

/// 一项选择复制；同一快照的所有 when/source 均读取复制前状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupSelectorCopy {
    source: CleanupSelectorId,
    target: CleanupSelectorId,
    when: CleanupConditionId,
    source_value: Option<super::CleanupCaptureValue>,
}

impl CleanupSelectorCopy {
    /// 原 owner 或刚求值结果中的选择。
    #[must_use]
    pub const fn source(self) -> CleanupSelectorId {
        self.source
    }

    /// 新 owner 独占的选择身份。
    #[must_use]
    pub const fn target(self) -> CleanupSelectorId {
        self.target
    }

    /// 仅在此条件成立时读取 source；否则 target 不初始化，也不得被读取。
    #[must_use]
    pub const fn when(self) -> CleanupConditionId {
        self.when
    }

    /// 快照执行时读取的已形成环境 capture 槽；owned 输入已移入接收槽，不能重读父槽。
    /// `None` 不证明可从当前 owner 读取。
    #[must_use]
    pub const fn source_value(self) -> Option<super::CleanupCaptureValue> {
        self.source_value
    }
}

/// Owner 快照的值关系；执行边界与旧 owner 清理次序由 producer 另行发布。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanupOwnerSnapshot {
    owner: CleanupOwnerValueId,
    origin: Span,
    copies: Vec<CleanupSelectorCopy>,
    capture_inputs: Vec<CleanupOwnerInput>,
    value_inputs: Vec<CleanupOwnerInput>,
    value: ExpressionId,
    conditions: Vec<CleanupConditionId>,
}

impl CleanupOwnerSnapshot {
    /// 新值的静态定义身份。
    #[must_use]
    pub const fn owner(&self) -> CleanupOwnerValueId {
        self.owner
    }

    /// 值定义的源码位置。
    #[must_use]
    pub const fn origin(&self) -> Span {
        self.origin
    }

    /// 按稳定 selector 顺序列出的并行复制，含未执行分支保护。
    #[must_use]
    pub fn copies(&self) -> &[CleanupSelectorCopy] {
        &self.copies
    }

    /// 已知 capture 来源，读取保存前状态；opaque 路径可无来源，环境仍由 value 定位。
    #[must_use]
    pub fn capture_inputs(&self) -> &[CleanupOwnerInput] {
        &self.capture_inputs
    }

    /// 完整 RHS 的已检查 owner 来源；条件读取保存前状态。
    #[must_use]
    pub fn value_inputs(&self) -> &[CleanupOwnerInput] {
        &self.value_inputs
    }

    /// 本次已求值的完整 RHS 环境；不能重新求值或读取已移动的源 binding。
    #[must_use]
    pub const fn value(&self) -> ExpressionId {
        self.value
    }

    /// 与输入条件一一对应，所有选择均重绑定到新快照。
    #[must_use]
    pub fn conditions(&self) -> &[CleanupConditionId] {
        &self.conditions
    }
}

impl CleanupConditions {
    /// 查询 owner 定义及其复制关系，不以 SymbolId 或控制 AST 代替值身份。
    #[must_use]
    pub fn owner_snapshot(&self, owner: CleanupOwnerValueId) -> Option<&CleanupOwnerSnapshot> {
        match self.owner_value(owner)? {
            CleanupOwnerValue::Snapshot(snapshot) => Some(snapshot),
            _ => None,
        }
    }

    /// 形成时的选择属于紧邻环境捕获值，读取必须先定位该当次实例。
    pub(crate) fn set_snapshot_copy_source_value(
        &mut self,
        owner: CleanupOwnerValueId,
        source: CleanupSelectorId,
        value: super::CleanupCaptureValue,
    ) -> bool {
        let Some(CleanupOwnerValue::Snapshot(snapshot)) = self.owners.get_mut(owner.index()) else {
            return false;
        };
        let Some(copy) = snapshot
            .copies
            .iter_mut()
            .find(|copy| copy.source == source)
        else {
            return false;
        };
        copy.source_value = Some(value);
        true
    }

    /// 记录完整 RHS 的来源版本；保存动作必须在复制 selector 前读取这些旧条件。
    pub(crate) fn set_snapshot_value_inputs(
        &mut self,
        owner: CleanupOwnerValueId,
        inputs: &[(CleanupOwnerValueId, CleanupConditionId)],
    ) -> bool {
        if inputs.iter().any(|&(source, condition)| {
            self.owner_value(source).is_none() || self.get(condition).is_none()
        }) {
            return false;
        }
        let Some(CleanupOwnerValue::Snapshot(snapshot)) = self.owners.get_mut(owner.index()) else {
            return false;
        };
        snapshot.value_inputs = inputs
            .iter()
            .map(|&(owner, condition)| CleanupOwnerInput { owner, condition })
            .collect();
        true
    }

    /// 查询值形成或环境快照定义。
    #[must_use]
    pub fn owner_value(&self, owner: CleanupOwnerValueId) -> Option<&CleanupOwnerValue> {
        self.owners.get(owner.index())
    }

    pub(crate) fn create_closure_owner(
        &mut self,
        expression: ExpressionId,
        origin: Span,
        inputs: Vec<CleanupCaptureInput>,
        sources: &[ClosureCaptureSource],
    ) -> CleanupOwnerValueId {
        let owner = self.create_owner(CleanupOwnerValue::Closure {
            expression,
            origin,
            inputs,
        });
        for (position, source) in sources.iter().enumerate() {
            self.register_capture_slot(owner, expression, *source, position);
        }
        owner
    }

    pub(crate) fn create_owner(&mut self, value: CleanupOwnerValue) -> CleanupOwnerValueId {
        let owner = CleanupOwnerValueId(self.owners.len());
        self.owners.push(value);
        owner
    }

    /// 复制整组条件共享的 selector，保留互补关系；无效输入不改变条件表。
    /// 每个根条件必须在执行边界可安全求值；调用方不能传入缺少外围可达保护的内层选择。
    /// 调用方须一次传入所有将重绑定的事实，保持 selector 与其可达保护的相对顺序。
    pub(crate) fn snapshot_conditions(
        &mut self,
        value: ExpressionId,
        origin: Span,
        conditions: &[CleanupConditionId],
        inputs: &[(CleanupOwnerValueId, CleanupConditionId)],
    ) -> Option<CleanupOwnerValueId> {
        if conditions
            .iter()
            .any(|&condition| self.get(condition).is_none())
            || inputs.iter().any(|&(owner, condition)| {
                self.owner_value(owner).is_none() || self.get(condition).is_none()
            })
        {
            return None;
        }
        let mut pending = conditions
            .iter()
            .map(|id| (id.index(), CleanupConditionId::ALWAYS))
            .collect::<BTreeMap<_, _>>();
        let mut reachable = BTreeMap::new();
        // 条件节点按 children-before-parent 分配；降序传播完整可达条件，避免枚举所有路径。
        while let Some((index, path)) = pending.pop_last() {
            let CleanupCondition::Choice { selector, branches } = self.nodes[index].clone() else {
                continue;
            };
            let prior = reachable
                .get(&selector)
                .copied()
                .unwrap_or(CleanupConditionId::NEVER);
            reachable.insert(selector, self.or(prior, path));
            for (arm, child) in branches.into_iter().enumerate() {
                if matches!(
                    self.nodes[child.index()],
                    CleanupCondition::Always | CleanupCondition::Never
                ) {
                    continue;
                }
                let selected = self
                    .selector_branch(selector, arm)
                    .expect("validated selector layout");
                let child_path = self.and(path, selected);
                let prior = pending
                    .get(&child.index())
                    .copied()
                    .unwrap_or(CleanupConditionId::NEVER);
                pending.insert(child.index(), self.or(prior, child_path));
            }
        }
        let mut replacements = BTreeMap::new();
        let mut copies = Vec::with_capacity(reachable.len());
        for (source, when) in reachable {
            let target = self.allocate_selector(self.selectors[source.index()]);
            replacements.insert(source, target);
            copies.push(CleanupSelectorCopy {
                source,
                target,
                when,
                source_value: None,
            });
        }
        let mut rewritten = BTreeMap::new();
        let conditions = conditions
            .iter()
            .map(|&id| self.rebind_snapshot(id, &replacements, &mut rewritten))
            .collect();
        let owner = CleanupOwnerValueId(self.owners.len());
        self.owners
            .push(CleanupOwnerValue::Snapshot(CleanupOwnerSnapshot {
                owner,
                origin,
                copies,
                value,
                capture_inputs: inputs
                    .iter()
                    .map(|&(owner, condition)| CleanupOwnerInput { owner, condition })
                    .collect(),
                value_inputs: Vec::new(),
                conditions,
            }));
        Some(owner)
    }

    fn rebind_snapshot(
        &mut self,
        id: CleanupConditionId,
        replacements: &BTreeMap<CleanupSelectorId, CleanupSelectorId>,
        rewritten: &mut BTreeMap<CleanupConditionId, CleanupConditionId>,
    ) -> CleanupConditionId {
        if let Some(&result) = rewritten.get(&id) {
            return result;
        }
        let result = match self.nodes[id.index()].clone() {
            CleanupCondition::Always | CleanupCondition::Never => id,
            CleanupCondition::Choice { selector, branches } => {
                let branches = branches
                    .into_iter()
                    .map(|child| self.rebind_snapshot(child, replacements, rewritten))
                    .collect();
                // 全量复制且按源 ID 顺序分配目标，保留 MDD 决策顺序与可达保护依赖。
                self.choice(replacements[&selector], branches)
            }
        };
        rewritten.insert(id, result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (
        CleanupConditions,
        CleanupConditionId,
        CleanupConditionId,
        Span,
    ) {
        let mut sources = crate::source::SourceMap::new();
        let source = sources
            .add_source(
                "snapshots.ko",
                "fun f() { if (a) { if (b) {} else {} } else {} }",
            )
            .unwrap();
        let parsed =
            crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap())
                .unwrap();
        assert!(parsed.diagnostics().is_empty());
        let mut controls = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), crate::parser::Expression::If { .. })
                    .then_some((id, node.span()))
            })
            .collect::<Vec<_>>();
        controls.sort_by_key(|(_, span)| span.start());
        let mut table = CleanupConditions::default();
        let outer = table.branch(controls[0].0, controls[0].1, 2, 0).unwrap();
        let inner = table.branch(controls[1].0, controls[1].1, 2, 0).unwrap();
        (table, outer, inner, controls[0].1)
    }

    fn selector(table: &CleanupConditions, id: CleanupConditionId) -> CleanupSelectorId {
        let CleanupCondition::Choice { selector, .. } = table.get(id).unwrap() else {
            panic!("choice")
        };
        *selector
    }

    fn evaluate(
        table: &CleanupConditions,
        id: CleanupConditionId,
        values: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(id).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                evaluate(table, branches[values[selector]], values)
            }
        }
    }

    fn copy(
        table: &CleanupConditions,
        owner: CleanupOwnerValueId,
        before: &BTreeMap<CleanupSelectorId, usize>,
    ) -> BTreeMap<CleanupSelectorId, usize> {
        let mut after = before.clone();
        for copy in table.owner_snapshot(owner).unwrap().copies() {
            if evaluate(table, copy.when(), before) {
                after.insert(copy.target(), before[&copy.source()]);
            }
        }
        after
    }

    #[test]
    fn moving_a_snapshot_then_replacing_the_source_keeps_the_old_selection() {
        let (mut table, condition, _, origin) = fixture();
        let direct = selector(&table, condition);
        let opposite = table.not(condition);
        let f = table
            .snapshot_conditions(
                table.selectors[0].control().unwrap(),
                origin,
                &[condition, opposite],
                &[],
            )
            .unwrap();
        let f_conditions = table.owner_snapshot(f).unwrap().conditions().to_vec();
        let g = table
            .snapshot_conditions(
                table.selectors[0].control().unwrap(),
                origin,
                &f_conditions,
                &[],
            )
            .unwrap();
        let next_f = table
            .snapshot_conditions(
                table.selectors[0].control().unwrap(),
                origin,
                &[condition, opposite],
                &[],
            )
            .unwrap();
        assert_ne!(f, g);
        assert_ne!(g, next_f);
        let old = BTreeMap::from([(direct, 0)]);
        let old_f = copy(&table, f, &old);
        let mut moved = copy(&table, g, &old_f);
        moved.insert(direct, 1);
        let replaced = copy(&table, next_f, &moved);
        assert!(evaluate(
            &table,
            table.owner_snapshot(g).unwrap().conditions()[0],
            &replaced
        ));
        assert!(!evaluate(
            &table,
            table.owner_snapshot(next_f).unwrap().conditions()[0],
            &replaced
        ));
        for owner in [f, g, next_f] {
            let snapshot = table.owner_snapshot(owner).unwrap();
            assert_eq!(
                snapshot.copies().len(),
                1,
                "related roots share one copied selector"
            );
            let conditions = snapshot.conditions().to_vec();
            assert_eq!(
                table.or(conditions[0], conditions[1]),
                CleanupConditionId::ALWAYS
            );
            assert_eq!(
                table.and(conditions[0], conditions[1]),
                CleanupConditionId::NEVER
            );
        }
    }

    #[test]
    fn snapshots_do_not_read_an_inner_selector_on_the_outer_else_path() {
        let (mut table, outer, inner, origin) = fixture();
        let selected = table.and(outer, inner);
        let opposite = table.not(selected);
        let owner = table
            .snapshot_conditions(
                table.selectors[0].control().unwrap(),
                origin,
                &[selected, opposite],
                &[],
            )
            .unwrap();
        let before = BTreeMap::from([(selector(&table, outer), 1)]);
        let after = copy(&table, owner, &before);
        let snapshot = table.owner_snapshot(owner).unwrap();
        assert_eq!(snapshot.copies().len(), 2);
        assert_eq!(
            after.len(),
            2,
            "only the outer selector is initialized in the snapshot"
        );
        assert!(!evaluate(&table, snapshot.conditions()[0], &after));
        assert!(evaluate(&table, snapshot.conditions()[1], &after));
        for outer_arm in 0..2 {
            for inner_arm in 0..2 {
                let before = BTreeMap::from([
                    (selector(&table, outer), outer_arm),
                    (selector(&table, inner), inner_arm),
                ]);
                let after = copy(&table, owner, &before);
                assert_eq!(
                    evaluate(&table, snapshot.conditions()[0], &after),
                    outer_arm == 0 && inner_arm == 0
                );
            }
        }
    }

    #[test]
    fn invalid_snapshot_roots_are_atomic_and_identical_inputs_are_deterministic() {
        let (mut table, outer, inner, origin) = fixture();
        let before = table.clone();
        assert!(
            table
                .snapshot_conditions(
                    table.selectors[0].control().unwrap(),
                    origin,
                    &[outer, CleanupConditionId(usize::MAX)],
                    &[]
                )
                .is_none()
        );
        assert_eq!(table, before);
        let mut repeated = table.clone();
        assert_eq!(
            table.snapshot_conditions(
                table.selectors[0].control().unwrap(),
                origin,
                &[outer, inner],
                &[]
            ),
            repeated.snapshot_conditions(
                repeated.selectors[0].control().unwrap(),
                origin,
                &[outer, inner],
                &[]
            )
        );
        assert_eq!(table, repeated);
    }

    #[test]
    fn shared_child_selector_combines_all_parent_paths_before_copying() {
        let (mut table, a, b, origin) = fixture();
        let c_selector = table.allocate_selector(*table.selector(selector(&table, b)).unwrap());
        let c = table.selector_branch(c_selector, 0).unwrap();
        let not_a = table.not(a);
        let bc = table.and(b, c);
        let otherwise = table.and(not_a, bc);
        let ac = table.and(a, c);
        let condition = table.or(ac, otherwise);
        let owner = table
            .snapshot_conditions(
                table.selectors[0].control().unwrap(),
                origin,
                &[condition],
                &[],
            )
            .unwrap();
        let cases = [
            (
                BTreeMap::from([(selector(&table, a), 0), (c_selector, 0)]),
                true,
            ),
            (
                BTreeMap::from([(selector(&table, a), 1), (selector(&table, b), 1)]),
                false,
            ),
            (
                BTreeMap::from([
                    (selector(&table, a), 1),
                    (selector(&table, b), 0),
                    (c_selector, 0),
                ]),
                true,
            ),
        ];
        for (before, expected) in cases {
            let after = copy(&table, owner, &before);
            assert_eq!(
                evaluate(
                    &table,
                    table.owner_snapshot(owner).unwrap().conditions()[0],
                    &after
                ),
                expected
            );
        }
    }

    #[test]
    fn rebound_state_can_read_a_selector_on_a_broader_path_than_the_result() {
        let (mut table, a, b, origin) = fixture();
        let result = table.and(a, b);
        let owner = table
            .snapshot_conditions(
                table.selectors[0].control().unwrap(),
                origin,
                &[result, b],
                &[],
            )
            .unwrap();
        let rebound = table.owner_snapshot(owner).unwrap().conditions()[1];
        let before = BTreeMap::from([(selector(&table, a), 1), (selector(&table, b), 0)]);
        let after = copy(&table, owner, &before);
        assert!(
            evaluate(&table, rebound, &after),
            "the broader state's selector must be initialized"
        );
    }

    #[test]
    fn rebinding_keeps_branch_local_snapshots_behind_their_reachability_guard() {
        let (mut table, outer, _, origin) = fixture();
        let f = table
            .snapshot_conditions(table.selectors[0].control().unwrap(), origin, &[outer], &[])
            .unwrap();
        let g = table
            .snapshot_conditions(table.selectors[0].control().unwrap(), origin, &[outer], &[])
            .unwrap();
        let f_condition = table.owner_snapshot(f).unwrap().conditions()[0];
        let g_condition = table.owner_snapshot(g).unwrap().conditions()[0];
        let result = table.and(outer, g_condition);
        let held_source = table.and(result, f_condition);
        let h = table
            .snapshot_conditions(
                table.selectors[0].control().unwrap(),
                origin,
                &[result, held_source],
                &[],
            )
            .unwrap();
        let rebound = table.owner_snapshot(h).unwrap().conditions()[1];
        // The outer else never executed either branch-local save.
        let before = BTreeMap::from([(selector(&table, outer), 1)]);
        let after = copy(&table, h, &before);
        assert!(!evaluate(&table, rebound, &after));
    }
}
