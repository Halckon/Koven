//! 从组装产物执行 closure、普通资源与 shared loan 清理；不读取 planner 的分析缓存。
mod calls;
mod chains;
mod coexisting;
mod conditional;
mod file_parent;
mod old_environment;
mod opaque;
mod retained;
mod shared;
mod values;

use std::collections::{BTreeMap, BTreeSet};

use super::super::DropPlan;
use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    ownership_checking::{
        CleanupCaptureSlotId, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
        CleanupConditions, CleanupOwnerValueId, CleanupSelectorId, ClosureCaptureDescriptor,
        ClosureCaptureEffect, ClosureCaptureMode, ClosureReleaseLayout, DropFact, DropPoint,
        DropTarget, IterationCleanupAction as Action, IterationExitKind, IterationOwnershipPlan,
        IterationPhiIncomingKind, IterationPhiPresenceSource,
    },
};

type Choices = BTreeMap<CleanupSelectorId, usize>;

fn selected(table: &CleanupConditions, guard: CleanupConditionId, choices: &Choices) -> bool {
    match table.get(guard).unwrap() {
        CleanupCondition::Always => true,
        CleanupCondition::Never => false,
        CleanupCondition::Choice { selector, branches } => {
            selected(table, branches[choices[selector]], choices)
        }
    }
}

struct Instance {
    /// None 表示源码求值产生的普通 MoveOnly 资源，不是环境。
    closure: Option<ExpressionId>,
    captures: BTreeMap<usize, usize>,
    /// Copyable 槽只记录形成，不建立 owner 或 loan；不解释标量运算。
    copied_captures: BTreeSet<usize>,
    /// 仅显式 Move 或 captured drop 清空的槽可在后续根析构时跳过。
    cleared_captures: BTreeSet<usize>,
    shared: BTreeMap<usize, usize>,
    choices: Choices,
    snapshot_sources: BTreeMap<CleanupOwnerValueId, CleanupOwnerValueId>,
    live: bool,
}

/// 接收 owned/Move 和 shared/Borrow；其余未建模动作仍须失败。
#[derive(Default)]
struct Replay {
    instances: Vec<Instance>,
    owners: BTreeMap<CleanupOwnerValueId, usize>,
    bindings: BTreeMap<SymbolId, CleanupOwnerValueId>,
    result: Option<CleanupOwnerValueId>,
    file_captures: Vec<ClosureCaptureDescriptor>,
    choices: Choices,
    released: Vec<usize>,
    providers: BTreeMap<usize, bool>,
    elements: BTreeSet<usize>,
    /// 展平槽只是根实例内部值的别名，不建立第二份析构义务。
    phi_slots: BTreeMap<CleanupCaptureSlotId, usize>,
    flattened_slots: BTreeSet<CleanupCaptureSlotId>,
    loans: BTreeMap<(usize, usize), usize>,
    retained: BTreeSet<usize>,
    cleanup_roots: BTreeMap<CleanupOwnerValueId, usize>,
    cleanup_edges: BTreeMap<(usize, usize), usize>,
    /// 同点结束凭据：(实际 source，结束该 loan 时是否最后一个)。
    ended_captures: BTreeMap<(usize, usize), (usize, bool)>,
    loan_ends: Vec<(usize, usize)>,
    /// 同清理点内按 root 配对，按实际 loan 结束顺序保存 source/last。
    retained_batches: Vec<(DropFact, Vec<(usize, bool)>)>,
    /// 调用者拥有的参数值只参与 loan，不产生本 callable 的析构义务。
    places: BTreeMap<crate::ownership_checking::ClosureCaptureSource, usize>,
    pending_environment: Option<(ExpressionId, usize)>,
    /// Body 环境是调用期别名，返回后恢复原映射，不新增或消费 owner。
    environments: Vec<(CleanupOwnerValueId, usize, Option<usize>)>,
}

impl Replay {
    fn take_owner(&mut self, owner: CleanupOwnerValueId) -> usize {
        let instance = self.owners.remove(&owner).unwrap();
        self.bindings.retain(|_, current| *current != owner);
        instance
    }

    fn bind(&mut self, symbol: SymbolId, owner: CleanupOwnerValueId) {
        assert!(self.owners.contains_key(&owner));
        assert!(
            self.bindings.insert(symbol, owner).is_none(),
            "binding overwrites an unconsumed owner"
        );
    }

    fn start_loop(&mut self, statement: StatementId) {
        assert!(self.providers.insert(statement.index(), true).is_none());
    }

    fn start_element(&mut self, statement: StatementId) {
        assert!(self.providers[&statement.index()]);
        assert!(self.elements.insert(statement.index()));
    }

    fn take_capture(&mut self, table: &CleanupConditions, value: CleanupCaptureValue) -> usize {
        match value {
            CleanupCaptureValue::Owner(owner) => self.take_owner(owner),
            CleanupCaptureValue::Environment {
                owner,
                slot,
                source,
            } => {
                let parent = self.owners[&owner];
                let slot = table.capture_slot_value(slot).unwrap();
                assert_eq!(slot.source(), source);
                assert_eq!(self.instances[parent].closure, Some(slot.closure()));
                let child = self.instances[parent]
                    .captures
                    .remove(&slot.position())
                    .expect("environment capture must read an initialized owned slot");
                assert!(
                    self.instances[parent]
                        .cleared_captures
                        .insert(slot.position())
                );
                child
            }
            CleanupCaptureValue::Place(_) => panic!("owned closure capture must carry an instance"),
        }
    }

    fn point(&mut self, facts: &DropPlan, point: DropPoint) {
        for (at, action) in &facts.cleanup_steps {
            if *at == point {
                self.action(facts, *action);
            }
        }
        assert!(
            self.instances
                .iter()
                .all(|instance| instance.live || instance.shared.is_empty()),
            "released environment has an unfinished capture loan at {point:?}: released={:?}, loans={:?}",
            self.released,
            self.loans
        );
        assert!(
            self.retained_batches.is_empty(),
            "root release needs retained-source completion"
        );
        self.cleanup_roots.clear();
        self.cleanup_edges.clear();
        self.ended_captures.clear();
    }

    fn exit(&mut self, facts: &DropPlan, plan: &IterationOwnershipPlan, kind: IterationExitKind) {
        let exit = plan
            .exits()
            .iter()
            .find(|exit| exit.kind() == kind)
            .unwrap();
        let actions = facts
            .cleanup_steps
            .iter()
            .filter(|(point, _)| *point == exit.point())
            .map(|(_, action)| *action)
            .collect::<Vec<_>>();
        assert_eq!(exit.actions(), actions);
        self.point(facts, exit.point());
    }

    fn action(&mut self, facts: &DropPlan, action: Action) {
        let table = &facts.cleanup_conditions;
        match action {
            Action::PassClosureEnvironment { callee, closure } => {
                let instance = self.owners[&callee];
                assert!(self.instances[instance].live);
                let actual = self.instances[instance]
                    .closure
                    .expect("callee must be a closure");
                if let Some(closure) = closure {
                    assert_eq!(
                        actual, closure,
                        "passed environment must match its checked lambda"
                    );
                }
                assert!(
                    self.pending_environment
                        .replace((actual, instance))
                        .is_none()
                );
            }
            Action::BindClosureEnvironment { owner, closure } => {
                let (passed, instance) = self
                    .pending_environment
                    .take()
                    .expect("lambda entry needs a passed environment");
                assert_eq!(passed, closure);
                assert!(self.instances[instance].live);
                let previous = self.owners.insert(owner, instance);
                self.environments.push((owner, instance, previous));
            }
            Action::CreateClosureOwner { owner, closure } => {
                let instance = self.instances.len();
                self.instances.push(Instance {
                    closure: Some(closure),
                    captures: BTreeMap::new(),
                    copied_captures: BTreeSet::new(),
                    cleared_captures: BTreeSet::new(),
                    shared: BTreeMap::new(),
                    choices: self.choices.clone(),
                    snapshot_sources: BTreeMap::new(),
                    live: true,
                });
                assert!(self.owners.insert(owner, instance).is_none());
                self.result = Some(owner);
            }
            Action::SaveClosureCapture {
                owner,
                target,
                input,
            } => {
                if !selected(table, input.condition(), &self.choices) {
                    return;
                }
                let parent = self.owners[&owner];
                let layout = table.capture_slot_value(target).unwrap();
                assert_eq!(layout.environment(), owner);
                assert_eq!(Some(layout.closure()), self.instances[parent].closure);
                assert_eq!(layout.source(), input.source());
                if (input.mode(), input.effect())
                    == (ClosureCaptureMode::Owned, ClosureCaptureEffect::Copy)
                {
                    if let CleanupCaptureValue::Environment {
                        owner,
                        slot,
                        source,
                    } = input.value()
                    {
                        let source_slot = table.capture_slot_value(slot).unwrap();
                        let enclosing = self.owners[&owner];
                        assert_eq!(source_slot.source(), source);
                        assert_eq!(
                            self.instances[enclosing].closure,
                            Some(source_slot.closure())
                        );
                        assert!(
                            self.instances[enclosing]
                                .copied_captures
                                .contains(&source_slot.position()),
                            "Copyable source capture must have been initialized"
                        );
                    } else {
                        assert!(
                            matches!(input.value(), CleanupCaptureValue::Place(_)),
                            "Copyable inputs have no owner obligation"
                        );
                    }
                    assert!(
                        self.instances[parent]
                            .copied_captures
                            .insert(layout.position())
                    );
                    return;
                }
                if (input.mode(), input.effect())
                    == (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow)
                {
                    self.save_shared_capture(table, parent, layout.position(), input.value());
                    return;
                }
                assert_eq!(
                    (input.mode(), input.effect()),
                    (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
                );
                let child = self.take_capture(table, input.value());
                assert!(
                    child < parent,
                    "owned capture must retain a pre-existing instance"
                );
                assert!(self.instances[child].live);
                assert!(
                    self.instances[parent]
                        .captures
                        .insert(layout.position(), child)
                        .is_none()
                );
            }
            Action::SaveOwnerSnapshot {
                condition,
                owner,
                value,
            } => {
                if condition.is_some_and(|guard| !selected(table, guard, &self.choices)) {
                    return;
                }
                let snapshot = table.owner_snapshot(owner).unwrap();
                assert_eq!(snapshot.value(), value);
                let inputs = if snapshot.value_inputs().is_empty() {
                    snapshot.capture_inputs()
                } else {
                    snapshot.value_inputs()
                };
                let inputs = inputs
                    .iter()
                    .filter(|input| selected(table, input.condition(), &self.choices))
                    .collect::<Vec<_>>();
                let [input] = inputs.as_slice() else {
                    panic!("snapshot needs one selected value")
                };
                let instance = self.owners[&input.owner()];
                let mut writes = BTreeMap::new();
                for copy in snapshot.copies() {
                    if !selected(table, copy.when(), &self.choices) {
                        continue;
                    }
                    let choice = match copy.source_value() {
                        None => self.choices[&copy.source()],
                        Some(CleanupCaptureValue::Environment {
                            owner,
                            slot,
                            source,
                        }) => {
                            let parent = self.owners[&owner];
                            let slot = table.capture_slot_value(slot).unwrap();
                            assert_eq!(slot.source(), source);
                            let position = slot.position();
                            let child = self.instances[parent].captures[&position];
                            self.instances[child].choices[&copy.source()]
                        }
                        Some(CleanupCaptureValue::Owner(owner)) => {
                            self.instances[self.owners[&owner]].choices[&copy.source()]
                        }
                        Some(CleanupCaptureValue::Place(_)) => {
                            panic!("place has no environment choice")
                        }
                    };
                    assert!(writes.insert(copy.target(), choice).is_none());
                }
                self.instances[instance]
                    .choices
                    .extend(writes.iter().map(|(&key, &value)| (key, value)));
                self.choices.extend(writes);
                self.instances[instance]
                    .snapshot_sources
                    .insert(owner, input.owner());
                assert_eq!(self.take_owner(input.owner()), instance);
                assert!(self.owners.insert(owner, instance).is_none());
                self.result = Some(owner);
            }
            Action::CommitOwnerSnapshot { owner, target } => {
                assert!(table.owner_snapshot(owner).is_some());
                assert!(self.instances[self.owners[&owner]].live);
                self.bind(target, owner);
            }
            Action::Drop(root) => {
                if let Some(instance) = self.take_root(table, root) {
                    assert!(
                        self.instances[instance].captures.is_empty(),
                        "ordinary drop cannot hide owned captures"
                    );
                    self.finish(instance);
                }
            }
            Action::ReleaseClosureInstances { layout, root } => {
                let first_loan = self.loan_ends.len();
                if let Some(instance) = self.take_root(table, root) {
                    self.release(facts, layout, instance);
                }
                let receipts = self.loan_ends[first_loan..]
                    .iter()
                    .map(|loan| self.ended_captures[loan])
                    .collect();
                assert!(
                    !self
                        .retained_batches
                        .iter()
                        .any(|(found, _)| *found == root)
                );
                self.retained_batches.push((root, receipts));
            }
            Action::ReleaseRetainedClosureSources { root } => {
                let at = self
                    .retained_batches
                    .iter()
                    .position(|(found, _)| *found == root)
                    .expect("retained completion needs an unconsumed root batch");
                let (_, receipts) = self.retained_batches.remove(at);
                self.release_retained_sources(facts, receipts);
            }
            Action::EndCaptureLoan {
                instance_address,
                capture_slot,
                condition,
                closure,
                source,
                ..
            } => {
                if condition.is_none_or(|guard| selected(table, guard, &self.choices)) {
                    self.end_capture_loan(table, instance_address, capture_slot, closure, source);
                }
            }
            Action::TestLastCaptureLoan {
                instance_address,
                capture_slot,
                selector,
                condition,
                ..
            } => {
                let last = if condition.is_none_or(|guard| selected(table, guard, &self.choices))
                    && let Some((instance, position, source)) =
                        self.cleanup_source(table, instance_address, capture_slot)
                {
                    let &(ended_source, last) = self
                        .ended_captures
                        .get(&(instance, position))
                        .expect("last borrower test must follow its loan end");
                    assert_eq!(ended_source, source);
                    last
                } else {
                    false
                };
                self.choices.insert(selector, usize::from(last));
            }
            Action::EndBinding { statement, .. } => {
                assert!(self.elements.contains(&statement.index()));
            }
            Action::EndElement(statement) => assert!(self.elements.remove(&statement.index())),
            Action::FinishProvider(statement) => {
                assert!(!self.elements.contains(&statement.index()));
                assert!(*self.providers.get_mut(&statement.index()).unwrap());
                *self.providers.get_mut(&statement.index()).unwrap() = false;
            }
            Action::EndSource(statement) => {
                assert_eq!(self.providers.remove(&statement.index()), Some(false))
            }
            other => panic!("unsupported candidate action: {other:?}"),
        }
    }

    fn take_root(&mut self, table: &CleanupConditions, root: DropFact) -> Option<usize> {
        if root
            .condition()
            .is_some_and(|guard| !selected(table, guard, &self.choices))
        {
            return None;
        }
        if matches!(root.target(), DropTarget::Captured { .. }) {
            return self.take_captured_root(table, root);
        }
        if matches!(root.target(), DropTarget::RetainedSource(_)) {
            let (_, _, source) = self
                .cleanup_source(
                    table,
                    root.instance_address().unwrap(),
                    root.capture_slot().unwrap(),
                )
                .expect("selected retained drop must match its instance layout");
            assert!(
                self.retained.remove(&source),
                "retained source must own its cleanup obligation"
            );
            return Some(source);
        }
        assert!(root.instance_address().is_none());
        let source = root.owner().unwrap();
        let owner = match root.target() {
            DropTarget::Named(symbol) => *self
                .bindings
                .get(&symbol)
                .expect("selected named root has no live binding"),
            DropTarget::Temporary(_) => source,
            other => panic!("unsupported root location: {other:?}"),
        };
        // Named 定位当前 binding；owner 可以是条件快照所保留的原始值定义。
        let mut current = owner;
        let mut visited = BTreeSet::new();
        while source != current {
            assert!(visited.insert(current));
            current = self.instances[self.owners[&owner]].snapshot_sources[&current];
        }
        let instance = self.take_owner(owner);
        assert!(self.cleanup_roots.insert(owner, instance).is_none());
        Some(instance)
    }

    fn leave_environment(&mut self) {
        assert!(self.pending_environment.is_none());
        let (owner, instance, previous) = self.environments.pop().unwrap();
        assert!(
            self.instances[instance].live,
            "callee environment must survive its body"
        );
        assert_eq!(self.owners.remove(&owner), Some(instance));
        if let Some(previous) = previous {
            self.owners.insert(owner, previous);
        }
    }

    fn finish(&mut self, instance: usize) {
        assert!(
            self.pending_environment
                .is_none_or(|(_, passed)| passed != instance)
                && !self
                    .environments
                    .iter()
                    .any(|(_, active, _)| *active == instance),
            "active call environment must not be released"
        );
        assert!(
            !self.places.values().any(|place| *place == instance),
            "callee must not drop its borrowed parameter"
        );
        assert!(
            self.instances[instance].live,
            "instance released more than once"
        );
        assert!(self.instances[instance].captures.is_empty());
        assert!(
            !self.loans.values().any(|source| *source == instance),
            "source released with live capture loans"
        );
        self.instances[instance].live = false;
        self.instances[instance].copied_captures.clear();
        self.phi_slots.retain(|_, value| *value != instance);
        self.released.push(instance);
    }

    fn release(&mut self, facts: &DropPlan, layout: ClosureReleaseLayout, root: usize) {
        assert!(
            self.instances[root].closure.is_some(),
            "instance release root must be a closure"
        );
        let mut pending = vec![(root, false)];
        let mut visited = BTreeSet::new();
        while let Some((instance, finished)) = pending.pop() {
            if finished {
                self.finish(instance);
                let loans = self.instances[instance]
                    .shared
                    .iter()
                    .rev()
                    .map(|(&position, &source)| (position, source))
                    .collect::<Vec<_>>();
                for (position, source) in loans {
                    self.end_instance_loan(instance, position, source);
                }
                continue;
            }
            assert!(visited.insert(instance), "owned instance reached twice");
            assert!(self.instances[instance].live);
            let Some(closure) = self.instances[instance].closure else {
                self.finish(instance);
                continue;
            };
            let captures = match layout {
                ClosureReleaseLayout::Iteration(statement) => facts
                    .iterations
                    .iter()
                    .find(|plan| plan.descriptor().statement() == statement)
                    .unwrap()
                    .capture_graph()
                    .nodes()
                    .iter()
                    .find(|node| node.closure() == closure)
                    .unwrap()
                    .release_captures()
                    .to_vec(),
                ClosureReleaseLayout::File => self
                    .file_captures
                    .iter()
                    .filter(|capture| capture.lambda() == closure)
                    .copied()
                    .collect(),
            };
            assert_eq!(
                captures,
                self.file_captures
                    .iter()
                    .filter(|capture| capture.lambda() == closure)
                    .copied()
                    .collect::<Vec<_>>(),
                "release layout must retain every checked capture"
            );
            pending.push((instance, true));
            for (position, capture) in captures.iter().enumerate() {
                match (capture.mode(), capture.effect()) {
                    (ClosureCaptureMode::Owned, ClosureCaptureEffect::Copy) => {
                        assert!(
                            self.instances[instance].copied_captures.contains(&position),
                            "Copyable capture must have been initialized"
                        );
                    }
                    (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move) => {
                        let Some(child) = self.instances[instance].captures.remove(&position)
                        else {
                            assert!(
                                self.instances[instance]
                                    .cleared_captures
                                    .contains(&position),
                                "missing owned slot must have an explicit consumption"
                            );
                            continue;
                        };
                        assert!(
                            self.cleanup_edges
                                .insert((instance, position), child)
                                .is_none()
                        );
                        pending.push((child, false));
                    }
                    (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow) => {
                        assert!(self.instances[instance].shared.contains_key(&position));
                    }
                    other => panic!("unsupported recursive capture: {other:?}"),
                }
            }
        }
    }

    /// 所有来源与条件先读取旧状态，再一起清空旧句柄并写入目标。
    fn edge(
        &mut self,
        facts: &DropPlan,
        plan: &IterationOwnershipPlan,
        kind: IterationPhiIncomingKind,
    ) {
        let table = &facts.cleanup_conditions;
        let edge = plan
            .closure_phi_incomings()
            .iter()
            .find(|edge| edge.kind() == kind)
            .unwrap();
        assert!(selected(table, edge.condition(), &self.choices));
        let slot_writes = self.read_phi_slots(table, edge);
        let mut values = Vec::new();
        let mut choices = BTreeMap::new();
        for binding in edge.bindings() {
            let available = selected(table, binding.available_when(), &self.choices);
            let sources = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &self.choices))
                .collect::<Vec<_>>();
            assert_eq!(sources.len(), usize::from(available));
            let value = sources.first().map(|source| {
                (
                    source.source(),
                    *self.owners.get(&source.source()).unwrap_or_else(|| {
                        panic!(
                            "{kind:?} missing {:?}, owners={:?}, released={:?}",
                            source.source(),
                            self.owners,
                            self.released
                        )
                    }),
                )
            });
            let layout = plan
                .closure_phis()
                .iter()
                .find(|layout| layout.owner() == binding.target())
                .unwrap();
            assert_eq!(layout.boundary(), edge.boundary());
            let expected = layout
                .origins()
                .iter()
                .map(|origin| (origin.node(), origin.selector()))
                .collect::<BTreeMap<_, _>>();
            let actual = binding
                .selector_writes()
                .iter()
                .map(|write| (write.node(), write.target()))
                .collect::<BTreeMap<_, _>>();
            assert_eq!(expected.len(), layout.origins().len());
            assert_eq!(actual.len(), binding.selector_writes().len());
            assert_eq!(actual, expected);
            let flat = !binding.capture_slots_to_clear().is_empty();
            if flat || binding.presence_source() == IterationPhiPresenceSource::StaticConditions {
                assert_eq!(
                    binding.capture_slots_to_clear(),
                    layout
                        .capture_layout()
                        .iter()
                        .map(|slot| slot.slot())
                        .collect::<Vec<_>>(),
                    "flattened phi must clear its complete layout"
                );
            }
            let mut reached = BTreeSet::new();
            if let Some((source, instance)) =
                value.filter(|(_, instance)| self.instances[*instance].closure.is_some())
            {
                let graph = plan.capture_graph();
                let roots = binding
                    .root_sources()
                    .iter()
                    .filter(|root| {
                        root.source() == source
                            && Some(graph.nodes()[root.node()].closure())
                                == self.instances[instance].closure
                            && selected(table, root.condition(), &self.choices)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    roots.len(),
                    1,
                    "actual closure identity must select one root"
                );
                let mut pending = vec![instance];
                let mut visited = BTreeSet::new();
                while let Some(parent) = pending.pop() {
                    assert!(visited.insert(parent), "owned instance reached twice");
                    assert!(self.instances[parent].live);
                    let node = graph
                        .nodes()
                        .iter()
                        .position(|node| Some(node.closure()) == self.instances[parent].closure)
                        .unwrap();
                    reached.insert(node);
                    for (&position, &child) in self.instances[parent]
                        .captures
                        .iter()
                        .chain(&self.instances[parent].shared)
                    {
                        let source = graph.nodes()[node]
                            .sources()
                            .iter()
                            .find(|source| source.position() == position);
                        let Some(source) = source else {
                            let capture = &graph.nodes()[node].release_captures()[position];
                            assert_eq!(
                                (capture.mode(), capture.effect()),
                                (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow)
                            );
                            assert_eq!(
                                self.places.get(&capture.source()),
                                Some(&child),
                                "untracked capture must retain its actual caller place"
                            );
                            assert!(self.instances[child].live);
                            continue;
                        };
                        if flat {
                            let slot = layout
                                .capture_layout()
                                .iter()
                                .find(|slot| slot.node() == node && slot.position() == position)
                                .unwrap();
                            assert_eq!(
                                slot_writes.get(&slot.slot()),
                                Some(&child),
                                "every live flattened capture needs its phi write"
                            );
                        }
                        if source.capture().mode() == ClosureCaptureMode::Shared
                            || self.instances[child].closure.is_none()
                        {
                            assert!(source.captured().is_empty());
                            assert!(self.instances[child].live);
                            continue;
                        }
                        let child_node = graph
                            .nodes()
                            .iter()
                            .position(|node| Some(node.closure()) == self.instances[child].closure)
                            .filter(|node| source.captured().contains(node));
                        if child_node.is_some() {
                            pending.push(child);
                        } else {
                            assert!(
                                source.may_be_opaque(),
                                "unlisted closure requires an opaque capture edge"
                            );
                            assert!(self.instances[child].live);
                        }
                    }
                }
                assert!(reached.iter().all(|node| expected.contains_key(node)));
            } else if value.is_some() {
                assert!(binding.root_sources().is_empty() && layout.origins().is_empty());
            }
            values.push((binding.target(), value));
            assert!(
                choices
                    .insert(binding.availability_selector(), usize::from(available))
                    .is_none()
            );
            for write in binding.selector_writes() {
                let present = match binding.presence_source() {
                    IterationPhiPresenceSource::StaticConditions => {
                        selected(table, write.condition(), &self.choices)
                    }
                    IterationPhiPresenceSource::CapturedInstances => {
                        reached.contains(&write.node())
                    }
                };
                assert!(available || !present);
                assert!(
                    choices
                        .insert(write.target(), usize::from(present))
                        .is_none()
                );
            }
        }
        for (_, value) in &values {
            if let Some((source, instance)) = value {
                assert_eq!(self.take_owner(*source), *instance);
            }
        }
        for (target, value) in values {
            assert!(
                !self.owners.contains_key(&target),
                "phi overwrites a live root"
            );
            if let Some((_, instance)) = value {
                self.owners.insert(target, instance);
                let symbol = plan
                    .closure_phis()
                    .iter()
                    .find(|layout| layout.owner() == target)
                    .unwrap()
                    .symbol();
                self.bind(symbol, target);
            } else {
                let symbol = plan
                    .closure_phis()
                    .iter()
                    .find(|layout| layout.owner() == target)
                    .unwrap()
                    .symbol();
                if let Some(&old) = self.bindings.get(&symbol) {
                    let instance = self.take_owner(old);
                    assert!(
                        self.loans.values().any(|source| *source == instance),
                        "unavailable value must be retained by a live loan"
                    );
                    assert!(self.retained.insert(instance));
                }
            }
        }
        for slot in edge
            .bindings()
            .iter()
            .flat_map(|binding| binding.capture_slots_to_clear())
        {
            self.flattened_slots.insert(*slot);
            self.phi_slots.remove(slot);
        }
        self.phi_slots.extend(slot_writes);
        self.choices.extend(choices);
    }

    fn done(&self) {
        assert!(self.pending_environment.is_none() && self.environments.is_empty());
        assert!(self.owners.is_empty());
        assert!(self.bindings.is_empty());
        assert!(
            self.phi_slots.is_empty(),
            "slots={:?}, retained={:?}, loans={:?}, released={:?}",
            self.phi_slots,
            self.retained,
            self.loans,
            self.released
        );
        assert!(self.loans.is_empty() && self.retained.is_empty());
        assert!(self.providers.is_empty() && self.elements.is_empty());
        assert!(
            self.instances
                .iter()
                .enumerate()
                .all(|(index, instance)| instance.live
                    == self.places.values().any(|place| *place == index)
                    && instance.captures.is_empty()
                    && instance.shared.is_empty())
        );
        assert_eq!(
            self.released.len(),
            self.instances.len() - self.places.len()
        );
    }
}
