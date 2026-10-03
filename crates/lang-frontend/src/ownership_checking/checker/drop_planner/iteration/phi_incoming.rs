//! 从入边旧状态记录候选环境、完整 selector 写集和实例来源，并转发耗尽入边。
use std::collections::BTreeMap;

use super::super::{ClosureOrigin, DropPlanner, DropPoint, ValueState};
use crate::{
    ast::{ExpressionId, StatementId},
    ownership_checking::{
        CleanupCaptureInput, CleanupCaptureValue, CleanupConditionId, CleanupConditions,
        CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorId, ClosureCaptureEffect,
        ClosureCaptureMode, IterationCaptureGraph, IterationClosurePhiOrigin, IterationPhiBoundary,
        IterationPhiIncoming, IterationPhiIncomingBinding, IterationPhiIncomingEnvironment,
        IterationPhiIncomingKind, IterationPhiIncomingOrigin, IterationPhiIncomingSource,
        IterationPhiIncomingValue, IterationPhiPresenceSource, IterationPhiRootSource,
        IterationPhiSelectorWrite,
    },
};

struct ForwardPhiContext<'a, 'checker> {
    conditions: &'a mut CleanupConditions,
    checker: &'a super::super::Checker<'checker>,
}

#[allow(clippy::too_many_arguments)]
fn record_phi_origin(
    conditions: &mut CleanupConditions,
    root: CleanupOwnerValueId,
    layout: &IterationClosurePhiOrigin,
    candidates: &[&ClosureOrigin],
    available_when: CleanupConditionId,
    instance_path: Option<(CleanupOwnerValueId, Vec<usize>)>,
    origin_layouts: &[IterationClosurePhiOrigin],
    enclosing_capture_phi: &mut Option<ExpressionId>,
) -> IterationPhiIncomingOrigin {
    let mut condition = CleanupConditionId::NEVER;
    let mut environments = Vec::new();
    for candidate in candidates
        .iter()
        .copied()
        .filter(|candidate| candidate.closure == layout.closure())
    {
        let (instance_root, capture_path) = instance_path
            .clone()
            .unwrap_or_else(|| (candidate.owner, Vec::new()));
        let selected = conditions.and(candidate.condition, available_when);
        if selected == CleanupConditionId::NEVER {
            continue;
        }
        condition = conditions.or(condition, selected);
        let mut sources = Vec::new();
        for input in &candidate.inputs {
            let input_condition = conditions.and(selected, input.condition);
            if input_condition == CleanupConditionId::NEVER {
                continue;
            }
            let slot = layout
                .sources()
                .iter()
                .find(|slot| slot.source() == input.source);
            let target = slot.and_then(|slot| {
                (!matches!(input.value, CleanupCaptureValue::Place(_))).then_some(slot.owner())
            });
            let capture_slot = slot.map(|_| {
                conditions
                    .phi_capture_slot(root, layout.closure(), input.source)
                    .expect("preallocated phi capture has a registered layout slot")
            });
            let source_capture_slot = conditions
                .capture_slot(candidate.layout_owner, input.source)
                .or_else(|| {
                    conditions.phi_capture_slot(
                        candidate.layout_owner,
                        candidate.closure,
                        input.source,
                    )
                });
            let read_address = source_capture_slot
                .map(|_| conditions.register_instance_address(instance_root, &capture_path));
            let owned = match input.value {
                CleanupCaptureValue::Owner(owner) => Some(owner),
                _ => None,
            };
            let owned_descendant = slot.is_some_and(|slot| {
                let nested = slot.captured();
                nested
                    .iter()
                    .any(|nested| !origin_layouts[*nested].sources().is_empty())
                    && !(nested.len() == 1 && descendant_expandable(&origin_layouts[nested[0]]))
            });
            let sibling_alternatives = candidate
                .captured
                .iter()
                .filter(|origin| origin.captured_from == Some(input.source))
                .take(2)
                .count()
                > 1;
            let readable_from_parent = sibling_alternatives
                && mutually_exclusive(conditions, &candidate.captured)
                && (conditions
                    .owner_snapshot(candidate.owner)
                    .is_some_and(|snapshot| {
                        snapshot
                            .copies()
                            .iter()
                            .any(|copy| copy.source_value().is_some())
                    })
                    || matches!(
                        conditions.owner_value(candidate.owner),
                        Some(CleanupOwnerValue::IterationPhi { .. })
                    ));
            if matches!(input.value, CleanupCaptureValue::Environment { .. })
                && (owned_descendant || (sibling_alternatives && !readable_from_parent))
            {
                // 内层形成动作若仍读外层形成前的条件，会在外部选择改变后选错子环境。
                enclosing_capture_phi.get_or_insert(layout.closure());
            }
            let nested_path = capture_slot.map(|slot| {
                let mut path = capture_path.clone();
                path.push(
                    conditions
                        .capture_slot_value(slot)
                        .expect("preallocated phi capture has a registered position")
                        .position(),
                );
                (instance_root, path)
            });
            let captured = slot
                .into_iter()
                .flat_map(|slot| {
                    slot.captured()
                        .iter()
                        .map(move |index| &origin_layouts[*index])
                })
                .map(|nested| {
                    let nested_candidates = candidate
                        .captured
                        .iter()
                        .filter(|origin| {
                            origin.captured_from == Some(input.source)
                                && owned.is_none_or(|owner| origin.owner == owner)
                        })
                        .collect::<Vec<_>>();
                    record_phi_origin(
                        conditions,
                        root,
                        nested,
                        &nested_candidates,
                        input_condition,
                        nested_path.clone(),
                        origin_layouts,
                        enclosing_capture_phi,
                    )
                })
                .collect();
            sources.push(IterationPhiIncomingSource {
                target,
                capture_slot,
                source_capture_slot,
                read_address,
                source_environment: candidate.owner,
                nested_instance: !capture_path.is_empty(),
                input: CleanupCaptureInput {
                    condition: input_condition,
                    ..*input
                },
                captured,
            });
        }
        environments.push(IterationPhiIncomingEnvironment {
            owner: candidate.owner,
            instance_root,
            capture_path,
            condition: selected,
            sources,
        });
    }
    IterationPhiIncomingOrigin {
        node: layout.node(),
        target: layout.selector(),
        condition,
        environments,
    }
}

/// selector 写集按完整有限布局补齐 false，再合并各路径的局部条件。
/// 不修改 origin/environment，避免把一条父路径的子环境搬到另一条路径。
pub(super) fn phi_selector_writes(
    conditions: &mut CleanupConditions,
    layout: &[IterationClosurePhiOrigin],
    origins: &[IterationPhiIncomingOrigin],
    root_sources: &[IterationPhiRootSource],
) -> Vec<IterationPhiSelectorWrite> {
    let mut conditions_by_selector = layout
        .iter()
        .map(|node| (node.selector(), CleanupConditionId::NEVER))
        .collect::<BTreeMap<CleanupSelectorId, CleanupConditionId>>();
    let mut pending = origins.iter().collect::<Vec<_>>();
    while let Some(origin) = pending.pop() {
        let condition = conditions_by_selector
            .get_mut(&origin.target())
            .expect("incoming origin belongs to the allocated phi layout");
        *condition = conditions.or(*condition, origin.condition());
        for environment in origin.environments() {
            for source in environment.sources() {
                pending.extend(source.captured());
            }
        }
    }
    if origins.is_empty() {
        for source in root_sources {
            let selector = layout
                .iter()
                .find(|node| node.node() == source.node())
                .expect("root source belongs to the allocated phi layout")
                .selector();
            let condition = conditions_by_selector.get_mut(&selector).unwrap();
            *condition = conditions.or(*condition, source.condition());
        }
    }
    layout
        .iter()
        .map(|node| IterationPhiSelectorWrite {
            node: node.node(),
            target: node.selector(),
            condition: conditions_by_selector[&node.selector()],
        })
        .collect()
}

/// 释放端只能沿静态唯一、直接含 owned move 且无 shared 输入的紧邻子来源展开后代实例。
/// 与此同形的后代才能让 phi 计划发布；其余形状继续 atomic deferred。
fn descendant_expandable(nested: &IterationClosurePhiOrigin) -> bool {
    nested.sources().iter().any(|source| {
        source.mode() == ClosureCaptureMode::Owned && source.effect() == ClosureCaptureEffect::Move
    }) && !nested
        .sources()
        .iter()
        .any(|source| source.mode() == ClosureCaptureMode::Shared)
}

fn mutually_exclusive(conditions: &mut CleanupConditions, origins: &[ClosureOrigin]) -> bool {
    // 同一 capture source 的多个候选是同一槽位的互斥取值：单个 source 在任一时刻只持有其中一个。
    // phi 运输后这些候选的 condition 会变成独立 presence 位的组合，静态条件表无法再证明互补。
    let structural = origins.first().is_some_and(|first| {
        first.captured_from.is_some()
            && origins
                .iter()
                .all(|origin| origin.captured_from == first.captured_from)
    });
    // 无论结构判定如何都执行布尔检查：and() 会注册条件节点，保留该副作用以维持
    // 条件表编号与 combine() 规范化的确定性。
    let mut boolean = true;
    for (index, left) in origins.iter().enumerate() {
        for right in &origins[index + 1..] {
            if conditions.and(left.condition, right.condition) != CleanupConditionId::NEVER {
                boolean = false;
            }
        }
    }
    structural || boolean
}

fn omit_coexisting_capture_writes(origins: &mut [IterationPhiIncomingOrigin]) {
    for origin in origins {
        for environment in &mut origin.environments {
            for source in &mut environment.sources {
                // Keep the formed-instance read edge and nested presence, but never merge
                // two live children into one static phi capture slot/source owner.
                source.target = None;
                source.capture_slot = None;
                omit_coexisting_capture_writes(&mut source.captured);
            }
        }
    }
}

type SeenCaptureInstances =
    BTreeMap<usize, Vec<(CleanupOwnerValueId, Vec<usize>, CleanupConditionId)>>;

/// 同一布局节点的两个可同时成立的环境必须保留各自的实例路径。
pub(super) fn coexisting_capture_node(
    conditions: &mut CleanupConditions,
    graph: &IterationCaptureGraph,
    origins: &[IterationPhiIncomingOrigin],
    seen: &mut SeenCaptureInstances,
) -> Option<usize> {
    for origin in origins {
        for environment in origin.environments() {
            let path = environment.capture_path();
            if !graph.nodes()[origin.node()].sources().is_empty() {
                let prior = seen.entry(origin.node()).or_default();
                if prior.iter().any(|(other_root, other_path, condition)| {
                    (*other_root != environment.instance_root() || other_path.as_slice() != path)
                        && conditions.and(*condition, environment.condition())
                            != CleanupConditionId::NEVER
                }) {
                    return Some(origin.node());
                }
                prior.push((
                    environment.instance_root(),
                    path.to_vec(),
                    environment.condition(),
                ));
            }
            for source in environment.sources() {
                if let Some(node) =
                    coexisting_capture_node(conditions, graph, source.captured(), seen)
                {
                    return Some(node);
                }
            }
        }
    }
    None
}

impl DropPlanner<'_, '_> {
    /// 所有输入都读取保存前的 ValueState；缺席来源写入 false，不能继承上轮位值。
    pub(super) fn record_phi_incoming(
        &mut self,
        statement: StatementId,
        boundary: IterationPhiBoundary,
        kind: IterationPhiIncomingKind,
        point: DropPoint,
        state: &ValueState,
    ) {
        let Some(phis) = self.loop_phis.get(&statement.index()) else {
            return;
        };
        let mut bindings = Vec::new();
        for phi in phis.iter().filter(|phi| phi.boundary() == boundary) {
            let survives_exit = boundary != IterationPhiBoundary::Exit
                || self.exit_binding_required(statement, phi.symbol(), state);
            let value = state
                .values
                .iter()
                .find(|value| survives_exit && value.symbol == phi.symbol());
            let mut available_when = CleanupConditionId::NEVER;
            let values: Vec<IterationPhiIncomingValue> = value
                .map(|value| {
                    value
                        .versions
                        .iter()
                        .filter(|version| version.condition != CleanupConditionId::NEVER)
                        .map(|version| {
                            available_when = self.conditions.or(available_when, version.condition);
                            IterationPhiIncomingValue {
                                source: version.owner,
                                condition: version.condition,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let candidates = state
                .closures
                .get(&phi.symbol())
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let mut root_sources = Vec::new();
            if let Some(graph) = self.loop_capture_graphs.get(&statement.index()) {
                for &node in phi.root_nodes() {
                    for candidate in &candidates {
                        if candidate.closure != graph.nodes()[node].closure() {
                            continue;
                        }
                        let condition = self.conditions.and(candidate.condition, available_when);
                        if condition != CleanupConditionId::NEVER {
                            root_sources.push(IterationPhiRootSource {
                                node,
                                source: candidate.owner,
                                condition,
                            });
                        }
                    }
                }
                for value in &values {
                    // 递归 phi 无树形来源；Snapshot 也须复制其根选择位后再转运。
                    for (closure, source_owner, source_when) in
                        self.phi_root_conditions(value.source())
                    {
                        if candidates.iter().any(|candidate| {
                            candidate.owner == value.source()
                                && candidate.layout_owner == source_owner
                                && candidate.closure == closure
                        }) {
                            continue;
                        }
                        let Some(&node) = phi
                            .root_nodes()
                            .iter()
                            .find(|&&node| graph.nodes()[node].closure() == closure)
                        else {
                            continue;
                        };
                        let condition = self.conditions.and(value.condition(), source_when);
                        if condition != CleanupConditionId::NEVER {
                            root_sources.push(IterationPhiRootSource {
                                node,
                                source: value.source(),
                                condition,
                            });
                        }
                    }
                }
            }
            let mut origins = if self.recursive_phi_bindings.contains(&phi.owner()) {
                Vec::new()
            } else {
                phi.root_origins()
                    .map(|origin| {
                        record_phi_origin(
                            &mut self.conditions,
                            phi.owner(),
                            origin,
                            &candidates,
                            available_when,
                            None,
                            phi.origins(),
                            &mut self.enclosing_capture_phi,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let selector_writes =
                phi_selector_writes(&mut self.conditions, phi.origins(), &origins, &root_sources);
            let coexisting = self
                .loop_capture_graphs
                .get(&statement.index())
                .and_then(|graph| {
                    coexisting_capture_node(
                        &mut self.conditions,
                        graph,
                        &origins,
                        &mut BTreeMap::new(),
                    )
                    .map(|node| graph.nodes()[node].closure())
                });
            if let Some(closure) = coexisting {
                self.coexisting_capture_phi.get_or_insert(closure);
                omit_coexisting_capture_writes(&mut origins);
            }
            bindings.push(IterationPhiIncomingBinding {
                target: phi.owner(),
                capture_slots_to_clear: if self.recursive_phi_bindings.contains(&phi.owner())
                    || phi.origins().is_empty()
                    || coexisting.is_some()
                {
                    // Instance-qualified descendants stay in the formed environment.
                    Vec::new()
                } else {
                    phi.capture_layout()
                        .iter()
                        .map(|slot| slot.slot())
                        .collect()
                },
                availability_selector: phi.availability_selector(),
                available_when,
                values,
                root_sources,
                presence_source: if phi.instance_presence_required
                    || self.recursive_phi_bindings.contains(&phi.owner())
                    || coexisting.is_some()
                {
                    IterationPhiPresenceSource::CapturedInstances
                } else {
                    IterationPhiPresenceSource::StaticConditions
                },
                selector_writes,
                origins,
            });
        }
        self.loop_phi_incomings
            .entry(statement.index())
            .or_default()
            .push(IterationPhiIncoming {
                kind,
                point,
                boundary,
                condition: state.path,
                bindings,
            });
    }

    /// 零轮及后续耗尽均读取已初始化的 header 槽，而非重新读取入口 binding。
    pub(super) fn record_exhaustion_incoming(
        &mut self,
        statement: StatementId,
        state: &ValueState,
    ) {
        let Some(phis) = self.loop_phis.get(&statement.index()) else {
            return;
        };
        let mut bindings = Vec::new();
        for exit in phis
            .iter()
            .filter(|phi| phi.boundary() == IterationPhiBoundary::Exit)
        {
            let header = phis.iter().find(|phi| {
                self.exit_binding_required(statement, exit.symbol(), state)
                    && phi.boundary() == IterationPhiBoundary::Header
                    && phi.symbol() == exit.symbol()
            });
            let available_when = header
                .and_then(|header| {
                    state
                        .values
                        .iter()
                        .find(|value| value.symbol == exit.symbol())
                        .and_then(|value| {
                            value
                                .versions
                                .iter()
                                .find(|version| version.owner == header.owner())
                        })
                        .map(|version| {
                            // 未被出口清理改变时复用原 header 条件；入边自身已保护外层 path。
                            let initial = self
                                .conditions
                                .and(state.path, header.availability_condition());
                            if version.condition == initial {
                                header.availability_condition()
                            } else {
                                version.condition
                            }
                        })
                })
                .unwrap_or(CleanupConditionId::NEVER);
            let values = header
                .filter(|_| available_when != CleanupConditionId::NEVER)
                .map(|header| {
                    vec![IterationPhiIncomingValue {
                        source: header.owner(),
                        condition: available_when,
                    }]
                })
                .unwrap_or_default();
            let root_sources = exit
                .root_nodes()
                .iter()
                .filter_map(|&node| {
                    let header = header?;
                    let source = header.root_origins().find(|origin| origin.node() == node)?;
                    let condition = self.conditions.and(source.condition(), available_when);
                    (condition != CleanupConditionId::NEVER).then_some(IterationPhiRootSource {
                        node,
                        source: header.owner(),
                        condition,
                    })
                })
                .collect::<Vec<_>>();
            let mut origins = if self.recursive_phi_bindings.contains(&exit.owner()) {
                Vec::new()
            } else {
                exit.root_origins()
                    .map(|target| {
                        let source = header.and_then(|header| {
                            header
                                .root_origins()
                                .find(|origin| origin.closure() == target.closure())
                        });
                        Self::forward_phi_origin(
                            &mut ForwardPhiContext {
                                conditions: &mut self.conditions,
                                checker: self.checker,
                            },
                            exit.owner(),
                            target,
                            source,
                            header.map(|header| header.owner()),
                            header.map(|header| header.owner()),
                            None,
                            exit.origins(),
                            header.map(|header| header.origins()).unwrap_or(&[]),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let selector_writes = phi_selector_writes(
                &mut self.conditions,
                exit.origins(),
                &origins,
                &root_sources,
            );
            let coexisting = self
                .loop_capture_graphs
                .get(&statement.index())
                .and_then(|graph| {
                    coexisting_capture_node(
                        &mut self.conditions,
                        graph,
                        &origins,
                        &mut BTreeMap::new(),
                    )
                    .map(|node| graph.nodes()[node].closure())
                });
            if let Some(closure) = coexisting {
                self.coexisting_capture_phi.get_or_insert(closure);
                omit_coexisting_capture_writes(&mut origins);
            }
            bindings.push(IterationPhiIncomingBinding {
                target: exit.owner(),
                capture_slots_to_clear: if self.recursive_phi_bindings.contains(&exit.owner())
                    || exit.origins().is_empty()
                    || coexisting.is_some()
                {
                    Vec::new()
                } else {
                    exit.capture_layout()
                        .iter()
                        .map(|slot| slot.slot())
                        .collect()
                },
                availability_selector: exit.availability_selector(),
                available_when,
                values,
                root_sources,
                presence_source: if exit.instance_presence_required
                    || self.recursive_phi_bindings.contains(&exit.owner())
                    || coexisting.is_some()
                {
                    IterationPhiPresenceSource::CapturedInstances
                } else {
                    IterationPhiPresenceSource::StaticConditions
                },
                selector_writes,
                origins,
            });
        }
        self.loop_phi_incomings
            .entry(statement.index())
            .or_default()
            .push(IterationPhiIncoming {
                kind: IterationPhiIncomingKind::Exhaustion,
                point: DropPoint::LoopExit(statement),
                boundary: IterationPhiBoundary::Exit,
                condition: state.path,
                bindings,
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn forward_phi_origin(
        context: &mut ForwardPhiContext<'_, '_>,
        root: CleanupOwnerValueId,
        target: &IterationClosurePhiOrigin,
        source: Option<&IterationClosurePhiOrigin>,
        source_owner: Option<crate::ownership_checking::CleanupOwnerValueId>,
        source_layout_owner: Option<CleanupOwnerValueId>,
        instance_path: Option<(CleanupOwnerValueId, Vec<usize>)>,
        target_layouts: &[IterationClosurePhiOrigin],
        source_layouts: &[IterationClosurePhiOrigin],
    ) -> IterationPhiIncomingOrigin {
        let condition = source
            .map(|source| source.condition())
            .unwrap_or(CleanupConditionId::NEVER);
        let environments = source
            .zip(source_owner)
            .map(|(source, source_owner)| {
                let (instance_root, capture_path) = instance_path
                    .clone()
                    .unwrap_or_else(|| (source_owner, Vec::new()));
                let sources = context
                    .checker
                    .captures_of(target.closure())
                    .enumerate()
                    .map(|(position, capture)| {
                        let prior = source
                            .sources()
                            .iter()
                            .find(|slot| slot.source() == capture.source());
                        let destination = target
                            .sources()
                            .iter()
                            .find(|slot| slot.source() == capture.source());
                        let value = prior
                            .map(|slot| CleanupCaptureValue::Owner(slot.owner()))
                            .unwrap_or(CleanupCaptureValue::Place(capture.source()));
                        let captured = destination
                            .into_iter()
                            .flat_map(|slot| slot.captured().iter().copied())
                            .map(|target_index| {
                                let nested = &target_layouts[target_index];
                                let previous = prior.and_then(|prior| {
                                    prior
                                        .captured()
                                        .iter()
                                        .copied()
                                        .find(|&source_index| {
                                            source_layouts[source_index].closure()
                                                == nested.closure()
                                        })
                                        .map(|source_index| &source_layouts[source_index])
                                });
                                Self::forward_phi_origin(
                                    context,
                                    root,
                                    nested,
                                    previous,
                                    prior.map(|prior| prior.owner()),
                                    source_layout_owner,
                                    Some((instance_root, {
                                        let mut path = capture_path.clone();
                                        path.push(position);
                                        path
                                    })),
                                    target_layouts,
                                    source_layouts,
                                )
                            })
                            .collect();
                        IterationPhiIncomingSource {
                            target: destination.and_then(|slot| prior.map(|_| slot.owner())),
                            capture_slot: destination.map(|_| {
                                context
                                    .conditions
                                    .phi_capture_slot(root, target.closure(), capture.source())
                                    .expect("preallocated phi capture has a registered layout slot")
                            }),
                            source_capture_slot: prior.map(|_| {
                                context
                                    .conditions
                                    .phi_capture_slot(
                                        source_layout_owner
                                            .expect("forwarded phi has source layout"),
                                        source.closure(),
                                        capture.source(),
                                    )
                                    .expect("preallocated source phi capture has a layout slot")
                            }),
                            read_address: prior.map(|_| {
                                context
                                    .conditions
                                    .register_instance_address(instance_root, &capture_path)
                            }),
                            source_environment: source_owner,
                            nested_instance: !capture_path.is_empty(),
                            input: CleanupCaptureInput {
                                source: capture.source(),
                                value,
                                mode: capture.mode(),
                                effect: capture.effect(),
                                condition,
                                origin: capture.reference_span(),
                            },
                            captured,
                        }
                    })
                    .collect();
                vec![IterationPhiIncomingEnvironment {
                    owner: source_owner,
                    instance_root,
                    capture_path,
                    condition,
                    sources,
                }]
            })
            .unwrap_or_default();
        IterationPhiIncomingOrigin {
            node: target.node(),
            target: target.selector(),
            condition,
            environments,
        }
    }
}
