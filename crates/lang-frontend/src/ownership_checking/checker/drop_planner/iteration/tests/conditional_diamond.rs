use super::*;

#[test]
fn conditional_diamond_keeps_distinct_saved_paths_at_exhaustion() {
    fn paths_to(
        table: &CleanupConditions,
        origin: &IterationPhiIncomingOrigin,
        node: usize,
        capture_position: usize,
        paths: &mut BTreeSet<Vec<usize>>,
        conditions: &mut Vec<(
            Vec<usize>,
            CleanupConditionId,
            CleanupOwnerValueId,
            CleanupCaptureValue,
            CleanupOwnerValueId,
        )>,
        reads: &mut usize,
    ) {
        for environment in origin.environments() {
            for source in environment.sources() {
                if let Some((address, _)) = source.transport_read() {
                    let address = table.instance_address(address).unwrap();
                    assert_eq!(address.root(), environment.instance_root());
                    assert_eq!(address.capture_path(), environment.capture_path());
                    *reads += 1;
                }
            }
        }
        if origin.node() == node {
            for environment in origin.environments() {
                let path = environment.capture_path().to_vec();
                paths.insert(path.clone());
                let leaf_reads = environment
                    .sources()
                    .iter()
                    .filter_map(IterationPhiIncomingSource::transport_read)
                    .filter(|(_, slot)| {
                        table.capture_slot_value(*slot).unwrap().position() == capture_position
                    })
                    .count();
                assert_eq!(leaf_reads, 1, "base must read its own captured source");
                assert_eq!(environment.sources().len(), 1);
                conditions.push((
                    path,
                    environment.condition(),
                    environment.instance_root(),
                    environment.sources()[0].input().value(),
                    environment.owner(),
                ));
            }
        }
        for nested in origin
            .environments()
            .iter()
            .flat_map(|environment| environment.sources())
            .flat_map(|source| source.captured())
        {
            paths_to(
                table,
                nested,
                node,
                capture_position,
                paths,
                conditions,
                reads,
            );
        }
    }

    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "conditional-diamond.ko",
            r#"fun read(xs: List<Int>) {}
fun run(own xs: List<Int>, flag: Boolean, flags: List<Int>) {
    var base: move () -> Unit = move { read(xs) }
    var f: move () -> Unit = move {}
    var g: move () -> Unit = move {}
    if (flag) { f = move { base() } } else { g = move { base() } }
    var outer: move () -> Unit = move { val first = f()
val second = g() }
    for (_ in flags) {}
    val used = outer()
}"#,
        )
        .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (names, types) = crate::type_checking::standard_environments();
    let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
    let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let mut checker =
        super::super::super::super::Checker::new(&sources, &parsed, &names, &typed).unwrap();
    let capture_liveness = super::super::super::capture_liveness(&checker).unwrap();
    checker.expression_live_after = capture_liveness.expression_after;
    checker.statement_live_after = capture_liveness.statement_after;
    let mut state = super::super::super::super::State::default();
    for &root in parsed.roots() {
        checker.check_item(root, &mut state).unwrap();
    }
    assert!(checker.diagnostics.is_empty());
    let statement = checker
        .iterations
        .values()
        .next()
        .unwrap()
        .descriptor()
        .statement();
    let liveness = super::super::super::liveness::Liveness::build(&checker).unwrap();
    let (origins, captures) = super::super::super::origins::analyze(&checker).unwrap();
    let mut planner = super::super::super::DropPlanner::new(&checker, liveness, origins, captures);
    for &root in parsed.roots() {
        planner.item(root).unwrap();
    }
    assert!(planner.coexisting_capture_phi.is_some());
    let candidate = planner.into_candidate_facts();
    let plan = candidate
        .iterations
        .iter()
        .find(|plan| plan.descriptor().statement() == statement)
        .unwrap();
    let table = &candidate.cleanup_conditions;
    let graph = plan.capture_graph();
    let incomings = plan.closure_phi_incomings();
    let base = graph
        .nodes()
        .iter()
        .position(|node| {
            sources.slice(
                parsed
                    .ast()
                    .expressions()
                    .get(node.closure())
                    .unwrap()
                    .span(),
            ) == Ok("move { read(xs) }")
        })
        .unwrap();
    assert_eq!(graph.nodes()[base].sources().len(), 1);
    let base_capture_position = graph.nodes()[base].sources()[0].position();
    let outer = graph
        .nodes()
        .iter()
        .position(|node| {
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                .starts_with("move { val first = f()")
        })
        .unwrap();
    let phis = plan.closure_phis();
    let mut path_conditions_by_edge = [Vec::new(), Vec::new()];
    for (index, kind) in [
        IterationPhiIncomingKind::Entry,
        IterationPhiIncomingKind::Exhaustion,
    ]
    .into_iter()
    .enumerate()
    {
        let edge = incomings.iter().find(|edge| edge.kind() == kind).unwrap();
        let boundary = if kind == IterationPhiIncomingKind::Entry {
            IterationPhiBoundary::Header
        } else {
            IterationPhiBoundary::Exit
        };
        let phi = phis
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.root_nodes().contains(&outer))
            .unwrap();
        let binding = edge
            .bindings()
            .iter()
            .find(|binding| binding.target() == phi.owner())
            .unwrap();
        assert_eq!(
            binding.presence_source(),
            IterationPhiPresenceSource::CapturedInstances,
            "this graph can carry a conditional child instance across the loop"
        );
        let mut paths = BTreeSet::new();
        let mut path_conditions = Vec::new();
        let mut reads = 0;
        for origin in binding.origins() {
            paths_to(
                table,
                origin,
                base,
                base_capture_position,
                &mut paths,
                &mut path_conditions,
                &mut reads,
            );
        }
        assert_eq!(paths, BTreeSet::from([vec![0, 0], vec![1, 0]]));
        assert!(reads > 0);
        path_conditions_by_edge[index] = path_conditions;
        if kind == IterationPhiIncomingKind::Exhaustion {
            assert!(binding.capture_slots_to_clear().is_empty());
        }
    }
    let outer_closure = graph.nodes()[outer].closure();
    let (outer_snapshot_index, outer_owner) = candidate
        .cleanup_steps
        .iter()
        .enumerate()
        .find_map(|(index, (_, action))| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                if *value == outer_closure =>
            {
                Some((index, *owner))
            }
            _ => None,
        })
        .unwrap();
    let snapshot = table.owner_snapshot(outer_owner).unwrap();
    assert_eq!(snapshot.copies().len(), 3);
    let control = snapshot.copies()[0].source();
    assert!(matches!(
        table.selector(control).unwrap().source(),
        CleanupSelectorSource::Control(_)
    ));
    let branch_sources = snapshot.copies()[1..]
        .iter()
        .map(|copy| copy.source())
        .collect::<BTreeSet<_>>();
    let header = phis
        .iter()
        .find(|phi| {
            phi.boundary() == IterationPhiBoundary::Header && phi.root_nodes().contains(&outer)
        })
        .unwrap()
        .owner();
    let exit = phis
        .iter()
        .find(|phi| {
            phi.boundary() == IterationPhiBoundary::Exit && phi.root_nodes().contains(&outer)
        })
        .unwrap()
        .owner();
    for (branch, expected) in [(0, vec![0, 0]), (1, vec![1, 0])] {
        let mut choices = BTreeMap::from([(control, branch)]);
        let mut choices_before_cleanup = BTreeMap::new();
        for (index, (_, action)) in candidate
            .cleanup_steps
            .iter()
            .take(outer_snapshot_index + 1)
            .enumerate()
        {
            choices_before_cleanup.insert(index, choices.clone());
            if let IterationCleanupAction::SaveOwnerSnapshot {
                condition, owner, ..
            } = action
            {
                let snapshot = table.owner_snapshot(*owner).unwrap();
                if (*owner == outer_owner
                    || snapshot
                        .copies()
                        .iter()
                        .any(|copy| branch_sources.contains(&copy.target())))
                    && condition.is_none_or(|guard| selected(table, guard, &choices))
                {
                    replay_snapshot_choices(table, *owner, &mut choices);
                }
            }
        }
        let branch_snapshots = candidate
            .cleanup_steps
            .iter()
            .enumerate()
            .filter_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveOwnerSnapshot {
                    condition: Some(guard),
                    owner,
                    ..
                } if index < outer_snapshot_index
                    && selected(table, *guard, &choices_before_cleanup[&index]) =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [branch_snapshot] = branch_snapshots.as_slice() else {
            panic!("one branch must form the captured inner environment")
        };
        let [branch_input] = table
            .owner_snapshot(*branch_snapshot)
            .unwrap()
            .capture_inputs()
        else {
            panic!("branch snapshot must retain one formed wrapper")
        };
        let wrapper_owner = branch_input.owner();
        let wrapper_saves = candidate
            .cleanup_steps
            .iter()
            .enumerate()
            .filter_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == wrapper_owner
                    && index < outer_snapshot_index
                    && selected(table, input.condition(), &choices_before_cleanup[&index]) =>
                {
                    Some((*target, *input))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(wrapper_target, wrapper_input)] = wrapper_saves.as_slice() else {
            panic!("the selected wrapper must capture the formed base")
        };
        let CleanupCaptureValue::Owner(base_owner) = wrapper_input.value() else {
            panic!("the base is an owned closure value")
        };
        let [outer_input] = snapshot.capture_inputs() else {
            panic!("outer snapshot must retain one formed parent")
        };
        let outer_formed = outer_input.owner();
        let outer_saves = candidate
            .cleanup_steps
            .iter()
            .enumerate()
            .filter_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == outer_formed
                    && index < outer_snapshot_index
                    && selected(table, input.condition(), &choices_before_cleanup[&index]) =>
                {
                    Some((*target, *input))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(outer_saves.len(), 2);
        let other_owner = outer_saves
            .iter()
            .find_map(|(_, input)| match input.value() {
                CleanupCaptureValue::Owner(owner) if owner != *branch_snapshot => Some(owner),
                _ => None,
            })
            .unwrap();
        let formed_closure = |owner| {
            candidate
                .cleanup_steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::CreateClosureOwner {
                        owner: formed,
                        closure,
                    } if *formed == owner => Some(*closure),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(formed_closure(base_owner), graph.nodes()[base].closure());
        assert_eq!(formed_closure(outer_formed), outer_closure);
        let assert_formed_before_snapshot = |formed_owner, snapshot_owner, saves| {
            let closure = formed_closure(formed_owner);
            let creates = candidate
                .cleanup_steps
                .iter()
                .enumerate()
                .filter_map(|(index, (point, action))| match action {
                    IterationCleanupAction::CreateClosureOwner {
                        owner,
                        closure: value,
                    } if *owner == formed_owner => Some((index, *point, *value)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(create_index, create_point, created_closure)] = creates.as_slice() else {
                panic!("one CreateClosureOwner must form the selected environment")
            };
            assert_eq!(*created_closure, closure);
            assert_eq!(*create_point, DropPoint::AfterExpression(closure));
            let snapshots = candidate
                .cleanup_steps
                .iter()
                .enumerate()
                .filter_map(|(index, (point, action))| match action {
                    IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                        if *owner == snapshot_owner =>
                    {
                        Some((index, *point, *value))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(snapshot_index, snapshot_point, snapshot_value)] = snapshots.as_slice() else {
                panic!("one snapshot must retain the selected environment")
            };
            assert_eq!(*snapshot_value, closure);
            assert_eq!(*snapshot_point, DropPoint::AfterExpression(closure));
            assert!(create_index < snapshot_index);
            let save_steps = candidate
                .cleanup_steps
                .iter()
                .enumerate()
                .filter_map(|(index, (point, action))| {
                    matches!(action, IterationCleanupAction::SaveClosureCapture { owner, .. }
                            if *owner == formed_owner)
                    .then_some((index, *point))
                })
                .collect::<Vec<_>>();
            assert_eq!(save_steps.len(), saves);
            assert!(save_steps.iter().all(|(index, point)| {
                *point == DropPoint::AfterExpression(closure)
                    && create_index < index
                    && index < snapshot_index
            }));
            *snapshot_index
        };
        assert!(
            assert_formed_before_snapshot(wrapper_owner, *branch_snapshot, 1)
                < outer_snapshot_index
        );
        assert_eq!(
            assert_formed_before_snapshot(outer_formed, outer_owner, 4),
            outer_snapshot_index
        );
        let instance_closures = BTreeMap::from([
            (1, formed_closure(base_owner)),
            (2, formed_closure(wrapper_owner)),
            (3, formed_closure(other_owner)),
            (4, formed_closure(outer_formed)),
        ]);
        let mut formed_values = BTreeMap::from([
            (base_owner, 1),
            (wrapper_owner, 2),
            (other_owner, 3),
            (outer_formed, 4),
        ]);
        assert_eq!(formed_values.len(), 4);
        let mut saved_children = BTreeMap::new();
        let wrapper_slot = table.capture_slot_value(*wrapper_target).unwrap();
        assert_eq!(wrapper_slot.environment(), wrapper_owner);
        assert_eq!(wrapper_slot.closure(), instance_closures[&2]);
        assert_eq!(wrapper_slot.source(), wrapper_input.source());
        let base_instance = formed_values.remove(&base_owner).unwrap();
        assert!(
            saved_children
                .insert((2, wrapper_slot.position()), base_instance)
                .is_none()
        );
        let wrapper_instance = formed_values.remove(&wrapper_owner).unwrap();
        assert!(
            formed_values
                .insert(*branch_snapshot, wrapper_instance)
                .is_none()
        );
        for (target, input) in outer_saves {
            let slot = table.capture_slot_value(target).unwrap();
            assert_eq!(slot.environment(), outer_formed);
            assert_eq!(slot.closure(), outer_closure);
            assert_eq!(slot.source(), input.source());
            let CleanupCaptureValue::Owner(source) = input.value() else {
                panic!("outer captures formed closure values")
            };
            let child = formed_values.remove(&source).unwrap();
            assert!(saved_children.insert((4, slot.position()), child).is_none());
        }
        let parent_instance = formed_values.remove(&outer_formed).unwrap();
        assert!(formed_values.insert(outer_owner, parent_instance).is_none());
        assert_eq!(formed_values.len(), 1);
        let path_to_base = |root| {
            let paths = graph.nodes()[outer]
                .sources()
                .iter()
                .flat_map(|source| {
                    let child = saved_children[&(root, source.position())];
                    let child_node = graph
                        .nodes()
                        .iter()
                        .position(|node| node.closure() == instance_closures[&child])
                        .unwrap();
                    assert!(source.captured().contains(&child_node));
                    graph.nodes()[child_node]
                        .sources()
                        .iter()
                        .filter_map(|nested| {
                            let leaf = saved_children.get(&(child, nested.position()))?;
                            assert_eq!(*leaf, base_instance);
                            assert_eq!(instance_closures[leaf], graph.nodes()[base].closure());
                            assert!(nested.captured().contains(&base));
                            Some(vec![source.position(), nested.position()])
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let [path] = paths.as_slice() else {
                panic!("only one formed parent capture contains the base instance")
            };
            path.clone()
        };
        let formed_path = path_to_base(parent_instance);
        assert_eq!(formed_path, expected);
        let replay_instance_edge = |edge: &crate::ownership_checking::IterationPhiIncoming,
                                    target: CleanupOwnerValueId,
                                    values: &BTreeMap<CleanupOwnerValueId, usize>,
                                    choices: &mut BTreeMap<
            crate::ownership_checking::CleanupSelectorId,
            usize,
        >| {
            let before = choices.clone();
            assert!(selected(table, edge.condition(), &before));
            let binding = edge
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap();
            assert_eq!(
                binding.presence_source(),
                IterationPhiPresenceSource::CapturedInstances
            );
            let roots = binding
                .root_sources()
                .iter()
                .filter(|root| selected(table, root.condition(), &before))
                .collect::<Vec<_>>();
            let [root] = roots.as_slice() else {
                panic!("one formed parent reaches this phi edge")
            };
            let selected_values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &before))
                .collect::<Vec<_>>();
            let [value] = selected_values.as_slice() else {
                panic!("one available owner value reaches this phi edge")
            };
            assert_eq!(value.source(), root.source());
            assert!(selected(table, binding.available_when(), &before));
            let root_instance = values[&root.source()];
            assert_eq!(root_instance, parent_instance);
            let mut pending = vec![root_instance];
            let mut visited = BTreeSet::new();
            let mut reached = BTreeSet::new();
            while let Some(instance) = pending.pop() {
                assert!(
                    visited.insert(instance),
                    "formed owned instance visited twice"
                );
                let node = graph
                    .nodes()
                    .iter()
                    .position(|node| node.closure() == instance_closures[&instance])
                    .unwrap();
                reached.insert(node);
                for source in graph.nodes()[node].sources() {
                    if let Some(&child) = saved_children.get(&(instance, source.position())) {
                        let child_node = graph
                            .nodes()
                            .iter()
                            .position(|node| node.closure() == instance_closures[&child])
                            .unwrap();
                        assert!(source.captured().contains(&child_node));
                        pending.push(child);
                    }
                }
            }
            assert_eq!(visited.len(), 4);
            assert!(reached.contains(&base));
            assert!(reached.contains(&outer));
            let layout = phis.iter().find(|phi| phi.owner() == target).unwrap();
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
            assert_eq!(binding.selector_writes().len(), expected_writes.len());
            assert_eq!(actual_writes, expected_writes);
            let mut writes = vec![(
                binding.availability_selector(),
                usize::from(selected(table, binding.available_when(), &before)),
            )];
            writes.extend(
                binding
                    .selector_writes()
                    .iter()
                    .map(|write| (write.target(), usize::from(reached.contains(&write.node())))),
            );
            for (selector, value) in writes {
                choices.insert(selector, value);
            }
            for write in binding.selector_writes() {
                assert_eq!(
                    choices[&write.target()],
                    usize::from(reached.contains(&write.node()))
                );
            }
            (value.source(), root_instance)
        };
        let root_drop = candidate
            .drops
            .iter()
            .find(|fact| {
                fact.owner() == Some(exit) && matches!(fact.target(), DropTarget::Named(_))
            })
            .unwrap();
        let parent_aliases = [outer_formed, outer_owner, header, exit];
        let assert_no_early_parent_cleanup = |choices: &BTreeMap<
            crate::ownership_checking::CleanupSelectorId,
            usize,
        >| {
            for (point, action) in &candidate.cleanup_steps {
                if *point == root_drop.point() {
                    continue;
                }
                let fact = match action {
                    IterationCleanupAction::Drop(fact)
                    | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => fact,
                    _ => continue,
                };
                let reaches_parent = fact.target() == root_drop.target()
                    || fact
                        .owner()
                        .is_some_and(|owner| parent_aliases.contains(&owner))
                    || fact.instance_address().is_some_and(|address| {
                        parent_aliases.contains(&table.instance_address(address).unwrap().root())
                    });
                assert!(
                    !reaches_parent
                        || fact
                            .condition()
                            .is_some_and(|guard| !selected(table, guard, choices)),
                    "parent instance released before its call: {point:?} {action:?}"
                );
            }
        };
        for rounds in [0, 2] {
            let mut choices = choices.clone();
            assert_no_early_parent_cleanup(&choices);
            let mut values = BTreeMap::new();
            let entry = incomings
                .iter()
                .find(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
                .unwrap();
            let entry_binding = entry
                .bindings()
                .iter()
                .find(|binding| binding.target() == header)
                .unwrap();
            assert_eq!(entry_binding.values().len(), 1);
            assert_eq!(entry_binding.values()[0].source(), outer_owner);
            assert!(
                values
                    .insert(entry_binding.values()[0].source(), parent_instance)
                    .is_none()
            );
            for (index, kind) in [
                IterationPhiIncomingKind::Entry,
                IterationPhiIncomingKind::Exhaustion,
            ]
            .into_iter()
            .enumerate()
            {
                if rounds > 0 && kind == IterationPhiIncomingKind::Exhaustion {
                    let fallthrough = incomings
                        .iter()
                        .find(|edge| edge.kind() == IterationPhiIncomingKind::Fallthrough)
                        .unwrap();
                    for _ in 0..rounds {
                        let (source, incoming_instance) =
                            replay_instance_edge(fallthrough, header, &values, &mut choices);
                        let binding = fallthrough
                            .bindings()
                            .iter()
                            .find(|binding| binding.target() == header)
                            .unwrap();
                        assert_eq!(binding.values().len(), 1);
                        assert_eq!(binding.values()[0].source(), source);
                        assert_eq!(source, header);
                        assert_eq!(values.remove(&source), Some(incoming_instance));
                        assert!(values.insert(header, incoming_instance).is_none());
                        assert_no_early_parent_cleanup(&choices);
                    }
                }
                let edge = incomings.iter().find(|edge| edge.kind() == kind).unwrap();
                let target = if kind == IterationPhiIncomingKind::Entry {
                    header
                } else {
                    exit
                };
                let (source, incoming_instance) =
                    replay_instance_edge(edge, target, &values, &mut choices);
                let binding = edge
                    .bindings()
                    .iter()
                    .find(|binding| binding.target() == target)
                    .unwrap();
                assert_eq!(binding.values().len(), 1);
                assert_eq!(binding.values()[0].source(), source);
                if kind == IterationPhiIncomingKind::Exhaustion {
                    assert_eq!(source, header);
                }
                assert_eq!(values.remove(&source), Some(incoming_instance));
                assert!(values.insert(target, incoming_instance).is_none());
                assert_eq!(path_to_base(values[&target]), formed_path);
                assert_no_early_parent_cleanup(&choices);
                let selected_paths = path_conditions_by_edge[index]
                    .iter()
                    .filter(|(_, condition, _, _, _)| selected(table, *condition, &choices))
                    .map(|(path, _, root, _, _)| {
                        assert_eq!(*root, source);
                        assert_eq!(values[&target], parent_instance);
                        path.clone()
                    })
                    .collect::<BTreeSet<_>>();
                if kind == IterationPhiIncomingKind::Entry {
                    assert_eq!(
                        selected_paths,
                        BTreeSet::from([formed_path.clone()]),
                        "{kind:?}, branch {branch}, rounds {rounds}"
                    );
                } else {
                    // 静态 header presence 无法判定实际父实例的哪条捕获边存在；
                    // Exhaustion 的候选可以过近似，公开计划仍须 deferred。
                    assert!(
                        selected_paths.contains(&formed_path),
                        "{kind:?}, branch {branch}, rounds {rounds}"
                    );
                }
            }
            let (base_value, _base_owner) = path_conditions_by_edge[1]
                .iter()
                .find(|(path, condition, _, _, _)| {
                    path == &formed_path && selected(table, *condition, &choices)
                })
                .map(|(_, _, _, value, owner)| (*value, *owner))
                .unwrap();
            let selected_drops = candidate
                .cleanup_steps
                .iter()
                .filter(|(point, _)| *point == root_drop.point())
                .filter_map(|(_, action)| match action {
                    IterationCleanupAction::Drop(fact)
                        if fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, &choices)) =>
                    {
                        Some(*fact)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let selected_root_cleanup = candidate
                .cleanup_steps
                .iter()
                .filter_map(|(point, action)| {
                    let fact = match action {
                        IterationCleanupAction::Drop(fact)
                        | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => {
                            fact
                        }
                        _ => return None,
                    };
                    let from_exit = fact.owner() == Some(exit)
                        || fact.instance_address().is_some_and(|address| {
                            table.instance_address(address).unwrap().root() == exit
                        });
                    (from_exit
                        && fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, &choices)))
                    .then_some((*point, *action))
                })
                .collect::<Vec<_>>();
            assert_eq!(
                selected_root_cleanup,
                selected_drops
                    .iter()
                    .map(|fact| (root_drop.point(), IterationCleanupAction::Drop(*fact)))
                    .collect::<Vec<_>>()
            );
            assert_eq!(selected_drops.last(), Some(root_drop));
            let captured = selected_drops[..selected_drops.len() - 1]
                .iter()
                .map(|fact| {
                    let DropTarget::Captured {
                        owner: _,
                        closure,
                        source,
                        value,
                        ..
                    } = fact.target()
                    else {
                        panic!("only the root drop may be named")
                    };
                    let address = table
                        .instance_address(fact.instance_address().unwrap())
                        .unwrap();
                    assert_eq!(address.root(), exit);
                    let slot = table
                        .capture_slot_value(fact.capture_slot().unwrap())
                        .unwrap();
                    assert_eq!(slot.closure(), closure);
                    assert_eq!(slot.source(), source);
                    let mut parent_nodes = vec![outer];
                    for &position in address.capture_path() {
                        parent_nodes = parent_nodes
                            .iter()
                            .flat_map(|&node| graph.nodes()[node].sources())
                            .filter(|candidate| candidate.position() == position)
                            .flat_map(|candidate| candidate.captured().iter().copied())
                            .collect();
                        assert!(!parent_nodes.is_empty());
                    }
                    assert!(parent_nodes.iter().any(|&node| {
                        graph.nodes()[node].closure() == closure
                            && graph.nodes()[node].sources().iter().any(|candidate| {
                                candidate.position() == slot.position()
                                    && candidate.capture().source() == source
                            })
                    }));
                    if closure == graph.nodes()[base].closure() {
                        assert_eq!(source, graph.nodes()[base].sources()[0].capture().source());
                        let (
                            CleanupCaptureValue::Owner(drop_owner),
                            CleanupCaptureValue::Owner(input_owner),
                        ) = (value, base_value)
                        else {
                            panic!("base capture must refer to owned phi source values")
                        };
                        assert_eq!(fact.owner(), Some(drop_owner));
                        assert!(matches!(
                            table.owner_value(drop_owner),
                            Some(CleanupOwnerValue::IterationPhiSourceOwner {
                                environment,
                                closure: defined_closure,
                                source: defined_source,
                                ..
                            }) if *environment == exit
                                && *defined_closure == closure
                                && *defined_source == source
                        ));
                        assert!(matches!(
                            table.owner_value(input_owner),
                            Some(CleanupOwnerValue::IterationPhiSourceOwner {
                                environment,
                                closure: defined_closure,
                                source: defined_source,
                                ..
                            }) if *environment == header
                                && *defined_closure == closure
                                && *defined_source == source
                        ));
                        assert_eq!(slot.position(), base_capture_position);
                        assert_eq!(address.capture_path(), formed_path);
                    }
                    (address.capture_path().to_vec(), slot.position())
                })
                .collect::<Vec<_>>();
            let expected_captures = if branch == 0 {
                vec![(vec![], 1), (vec![0, 0], 0), (vec![0], 0), (vec![], 0)]
            } else {
                vec![(vec![1, 0], 0), (vec![1], 0), (vec![], 1), (vec![], 0)]
            };
            assert_eq!(captured, expected_captures);
        }
    }
}
