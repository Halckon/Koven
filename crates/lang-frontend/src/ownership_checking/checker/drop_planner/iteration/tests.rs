use std::collections::{BTreeMap, BTreeSet};

use super::{
    DropPoint, DropTarget, PhiCaptureGraph, coexisting_capture_node, cyclic_node,
    finite_layout_order, phi_selector_writes,
};
use crate::{
    ast::ExpressionId,
    name_resolution::SymbolId,
    ownership_checking::{
        CleanupCaptureInput, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
        CleanupConditions, CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorSource,
        ClosureCaptureDescriptor, ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource,
        ClosureReleaseLayout, IterationCaptureGraph, IterationCaptureNode, IterationCaptureSource,
        IterationCleanupAction, IterationClosurePhiBinding, IterationClosurePhiOrigin,
        IterationPhiBoundary, IterationPhiIncomingBinding, IterationPhiIncomingEnvironment,
        IterationPhiIncomingKind, IterationPhiIncomingOrigin, IterationPhiIncomingSource,
        IterationPhiPresenceSource,
    },
    parser::Expression,
    source::SourceMap,
    type_checking::TypeId,
};

/// 测试回放只处理夹具中的已知 owned closure 边；实例/槽来自形成动作。
fn replay_owned_closure_release(
    graph: &IterationCaptureGraph,
    instance_nodes: &BTreeMap<usize, usize>,
    captured: &mut BTreeMap<(usize, usize), usize>,
    root: usize,
) -> Vec<usize> {
    let mut pending = vec![(root, false)];
    let mut visited = BTreeSet::new();
    let mut released = Vec::new();
    while let Some((instance, finished)) = pending.pop() {
        if finished {
            released.push(instance);
            continue;
        }
        assert!(
            visited.insert(instance),
            "an owned environment was reached twice"
        );
        pending.push((instance, true));
        for source in graph.nodes()[instance_nodes[&instance]].sources() {
            if source.capture().mode() != ClosureCaptureMode::Owned
                || source.capture().effect() != ClosureCaptureEffect::Move
            {
                continue;
            }
            assert!(!source.captured().is_empty(), "fixture needs a known child");
            let child = captured.remove(&(instance, source.position())).unwrap();
            assert!(source.captured().contains(&instance_nodes[&child]));
            pending.push((child, false));
        }
    }
    released
}

fn selected(
    table: &CleanupConditions,
    condition: CleanupConditionId,
    choices: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
) -> bool {
    match table.get(condition).unwrap() {
        CleanupCondition::Always => true,
        CleanupCondition::Never => false,
        CleanupCondition::Choice { selector, branches } => {
            let arm = choices.get(selector).unwrap_or_else(|| {
                panic!(
                    "missing selector {selector:?} {:?} for {condition:?}",
                    table.selector(*selector)
                )
            });
            selected(table, branches[*arm], choices)
        }
    }
}

/// 静态条件入边的完整回放；实例入边必须读取当次捕获槽。
fn replay_edge_presence(
    table: &CleanupConditions,
    edge: &crate::ownership_checking::IterationPhiIncoming,
    choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
) {
    assert!(
        edge.bindings().iter().all(|binding| {
            binding.presence_source() == IterationPhiPresenceSource::StaticConditions
        }),
        "dynamic presence needs the selected root instance and saved capture slots"
    );
    replay_edge_candidates(table, edge, choices);
}

/// 仅供未完成动态实例验收的局部夹具读取静态候选，不能证明实际后代存在。
fn replay_edge_candidates(
    table: &CleanupConditions,
    edge: &crate::ownership_checking::IterationPhiIncoming,
    choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
) {
    let before = choices.clone();
    assert!(selected(table, edge.condition(), &before));
    let mut writes = Vec::new();
    for binding in edge.bindings() {
        writes.push((
            binding.availability_selector(),
            usize::from(selected(table, binding.available_when(), &before)),
        ));
        for write in binding.selector_writes() {
            writes.push((
                write.target(),
                usize::from(selected(table, write.condition(), &before)),
            ));
        }
    }
    for (target, value) in writes {
        choices.insert(target, value);
    }
}

/// 同一次入边回放读取的 owner 句柄、实例节点和已保存捕获槽。
#[derive(Clone, Copy)]
struct ReplayInstances<'a> {
    values: &'a BTreeMap<CleanupOwnerValueId, usize>,
    nodes: &'a BTreeMap<usize, usize>,
    captured: &'a BTreeMap<(usize, usize), usize>,
}

/// 从旧状态选根句柄，再按实例保存的捕获槽写入实际可达节点。
fn replay_captured_edge_presence(
    table: &CleanupConditions,
    graph: &IterationCaptureGraph,
    layout: &IterationClosurePhiBinding,
    edge: &crate::ownership_checking::IterationPhiIncoming,
    target: CleanupOwnerValueId,
    instances: ReplayInstances<'_>,
    choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
) -> (CleanupOwnerValueId, usize) {
    let ReplayInstances {
        values,
        nodes: instance_nodes,
        captured,
    } = instances;
    let before = choices.clone();
    assert!(selected(table, edge.condition(), &before));
    let binding = edge
        .bindings()
        .iter()
        .find(|binding| binding.target() == target)
        .unwrap();
    assert_eq!(layout.owner(), target);
    assert_eq!(
        binding.presence_source(),
        IterationPhiPresenceSource::CapturedInstances
    );
    assert!(selected(table, binding.available_when(), &before));
    let selected_values = binding
        .values()
        .iter()
        .filter(|value| selected(table, value.condition(), &before))
        .collect::<Vec<_>>();
    let [value] = selected_values.as_slice() else {
        panic!("one owner value must reach this edge")
    };
    let instance = values[&value.source()];
    let roots = binding
        .root_sources()
        .iter()
        .filter(|root| {
            root.source() == value.source()
                && root.node() == instance_nodes[&instance]
                && selected(table, root.condition(), &before)
        })
        .collect::<Vec<_>>();
    let [root] = roots.as_slice() else {
        panic!("the selected instance must match exactly one root candidate")
    };
    assert_eq!(instance_nodes[&instance], root.node());
    let mut pending = vec![instance];
    let mut visited = BTreeSet::new();
    let mut reached = BTreeSet::new();
    while let Some(parent) = pending.pop() {
        assert!(visited.insert(parent), "an instance was reached twice");
        let node = instance_nodes[&parent];
        reached.insert(node);
        for source in graph.nodes()[node].sources() {
            if let Some(&child) = captured.get(&(parent, source.position())) {
                assert!(source.captured().contains(&instance_nodes[&child]));
                pending.push(child);
            }
        }
    }
    let expected_writes = layout
        .origins()
        .iter()
        .map(|origin| (origin.node(), origin.selector()))
        .collect::<BTreeMap<_, _>>();
    let actual_writes = binding
        .selector_writes()
        .iter()
        .map(|write| (write.node(), write.target()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(layout.origins().len(), expected_writes.len());
    assert_eq!(binding.selector_writes().len(), actual_writes.len());
    assert_eq!(actual_writes, expected_writes);
    assert!(
        reached
            .iter()
            .all(|node| expected_writes.contains_key(node))
    );
    let writes = binding
        .selector_writes()
        .iter()
        .map(|write| (write.target(), usize::from(reached.contains(&write.node()))))
        .collect::<Vec<_>>();
    choices.insert(binding.availability_selector(), 1);
    for (selector, present) in writes {
        choices.insert(selector, present);
    }
    (value.source(), instance)
}

/// 同一入边的全部 phi 从同一旧状态读取，缺席 binding 也必须清空完整布局。
fn replay_captured_edge(
    table: &CleanupConditions,
    graph: &IterationCaptureGraph,
    layouts: &[IterationClosurePhiBinding],
    edge: &crate::ownership_checking::IterationPhiIncoming,
    instances: ReplayInstances<'_>,
    choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
) -> BTreeMap<CleanupOwnerValueId, (CleanupOwnerValueId, usize)> {
    let values = instances.values;
    let before = choices.clone();
    assert!(selected(table, edge.condition(), &before));
    let mut writes = BTreeMap::new();
    let mut transported = BTreeMap::new();
    for binding in edge.bindings() {
        let layout = layouts
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
        let selected_values = binding
            .values()
            .iter()
            .filter(|value| selected(table, value.condition(), &before))
            .count();
        let available = selected(table, binding.available_when(), &before);
        assert_eq!(selected_values, usize::from(available));
        let mut after = before.clone();
        match binding.presence_source() {
            IterationPhiPresenceSource::StaticConditions => {
                after.insert(binding.availability_selector(), usize::from(available));
                for write in binding.selector_writes() {
                    let present = selected(table, write.condition(), &before);
                    assert!(available || !present);
                    after.insert(write.target(), usize::from(present));
                }
                if available {
                    let value = binding
                        .values()
                        .iter()
                        .find(|value| selected(table, value.condition(), &before))
                        .unwrap();
                    if let Some(&instance) = values.get(&value.source()) {
                        assert!(
                            transported
                                .insert(binding.target(), (value.source(), instance))
                                .is_none()
                        );
                    } else {
                        // 本夹具只给 closure 分配实例；无根布局的普通 source 仍写选择位。
                        assert!(layout.origins().is_empty());
                        assert!(binding.root_sources().is_empty());
                    }
                }
            }
            IterationPhiPresenceSource::CapturedInstances if available => {
                let source = replay_captured_edge_presence(
                    table,
                    graph,
                    layout,
                    edge,
                    binding.target(),
                    instances,
                    &mut after,
                );
                assert!(transported.insert(binding.target(), source).is_none());
            }
            IterationPhiPresenceSource::CapturedInstances => {
                after.insert(binding.availability_selector(), 0);
                for write in binding.selector_writes() {
                    after.insert(write.target(), 0);
                }
            }
        }
        assert!(
            writes
                .insert(
                    binding.availability_selector(),
                    after[&binding.availability_selector()],
                )
                .is_none()
        );
        for write in binding.selector_writes() {
            assert!(
                writes
                    .insert(write.target(), after[&write.target()])
                    .is_none()
            );
        }
    }
    choices.extend(writes);
    transported
}

fn replay_snapshot_choices(
    table: &CleanupConditions,
    owner: CleanupOwnerValueId,
    choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
) {
    let snapshot = table.owner_snapshot(owner).unwrap();
    let before = choices.clone();
    let writes = snapshot
        .copies()
        .iter()
        .filter(|copy| selected(table, copy.when(), &before))
        .map(|copy| {
            assert!(
                copy.source_value().is_none(),
                "this flat replay needs a direct selector source"
            );
            (copy.target(), before[&copy.source()])
        })
        .collect::<Vec<_>>();
    for (target, value) in writes {
        choices.insert(target, value);
    }
}

mod capture_graph;
mod coexisting_paths;
mod conditional_diamond;
mod conditional_leaf;
mod conditional_rebinding;
mod environment_drop;
mod file_parent;
mod independent_roots;
mod loop_handoff;
mod recursive_body;
mod recursive_chain_jumps;
mod recursive_chains;
mod recursive_jump_roots;
mod recursive_shared_layout;
mod sibling_shared_loans;
mod snapshot_cleanup;
mod snapshot_mixed_roots;
mod snapshot_recursive_roots;
