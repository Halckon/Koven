use super::*;

#[test]
fn coexisting_same_lambda_paths_replay_two_rounds_and_release() {
    fn no_flat_capture_writes(origin: &IterationPhiIncomingOrigin) -> bool {
        origin
            .environments()
            .iter()
            .flat_map(|environment| environment.sources())
            .all(|source| {
                source.target().is_none()
                    && source.capture_slot().is_none()
                    && source.captured().iter().all(no_flat_capture_writes)
            })
    }

    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "same-lambda-children.ko",
            "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>, next: List<Int>) {
                    var first: move () -> Unit = move {}
                    var second: move () -> Unit = move {}
                    for (_ in flags) {
                        second = first
                        val xs = listOf(1)
                        { first = move { read(xs) } }
                    }
                    var outer: move () -> Unit = move { val a = first()\nval b = second() }
                    for (_ in next) {}
                    val used = outer()
                }",
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
    let liveness = super::super::super::liveness::Liveness::build(&checker).unwrap();
    let (origins, captures) = super::super::super::origins::analyze(&checker).unwrap();
    let mut planner = super::super::super::DropPlanner::new(&checker, liveness, origins, captures);
    for &root in parsed.roots() {
        planner.item(root).unwrap();
    }
    assert!(planner.coexisting_capture_phi.is_some());
    let candidate = planner.into_candidate_facts();
    let plans = candidate
        .iterations
        .iter()
        .map(|plan| (plan.descriptor().statement().index(), plan))
        .collect::<BTreeMap<_, _>>();
    let steps = &candidate.cleanup_steps;
    let table = &candidate.cleanup_conditions;
    let (statement, graph, parent) = plans
        .iter()
        .find_map(|(&statement, plan)| {
            let graph = plan.capture_graph();
            graph.nodes().iter().enumerate().find_map(|(index, node)| {
                (sources.slice(parsed.ast().expressions().get(node.closure()).ok()?.span())
                    == Ok("move { val a = first()\nval b = second() }"))
                .then_some((statement, graph, index))
            })
        })
        .unwrap();
    let parent = &graph.nodes()[parent];
    assert_eq!(parent.sources().len(), 2);
    let repeated = graph
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
    assert!(
        parent
            .sources()
            .iter()
            .all(|source| source.captured().contains(&repeated))
    );
    assert_eq!(parent.sources()[0].position(), 0);
    assert_eq!(parent.sources()[1].position(), 1);
    assert_eq!(statement, *plans.keys().max().unwrap());
    let formed = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *closure == parent.closure() =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    let saves = steps
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } if *owner == formed => Some((target, input)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(saves.len(), 2);
    for (index, (target, input)) in saves.iter().enumerate() {
        let slot = table.capture_slot_value(**target).unwrap();
        assert_eq!(slot.environment(), formed);
        assert_eq!(slot.closure(), parent.closure());
        assert_eq!(slot.source(), parent.sources()[index].capture().source());
        assert_eq!(slot.position(), index);
        assert_eq!(input.mode(), ClosureCaptureMode::Owned);
        assert_eq!(input.effect(), ClosureCaptureEffect::Move);
    }
    let first = match saves[0].1.value() {
        CleanupCaptureValue::Owner(owner) => owner,
        other => panic!("first source must retain a phi owner: {other:?}"),
    };
    let second = match saves[1].1.value() {
        CleanupCaptureValue::Owner(owner) => owner,
        other => panic!("second source must retain a phi owner: {other:?}"),
    };
    assert_ne!(first, second);
    let incomings = &plans[&statement].closure_phi_incomings();
    assert!(
        incomings
            .iter()
            .any(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
    );
    assert!(
        incomings
            .iter()
            .any(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
    );
    assert!(
        incomings
            .iter()
            .any(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
    );
    assert!(
        incomings.iter().all(|incoming| incoming
            .bindings()
            .iter()
            .all(|binding| binding.capture_slots_to_clear().is_empty()
                && binding.origins().iter().all(no_flat_capture_writes))),
        "coexisting child instances must retain presence without static capture writes"
    );

    // Execute the first loop's published formation and phi root facts twice.
    let first_loop = *plans.keys().min().unwrap();
    let phis = &plans[&first_loop].closure_phis();
    let source_symbol = |index: usize| match parent.sources()[index].capture().source() {
        ClosureCaptureSource::Symbol(symbol) => symbol,
        other => panic!("parent capture must read a named binding: {other:?}"),
    };
    let first_symbol = source_symbol(0);
    let second_symbol = source_symbol(1);
    let phi = |symbol, boundary| {
        phis.iter()
            .find(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
            .unwrap()
    };
    let first_header = phi(first_symbol, IterationPhiBoundary::Header);
    let second_header = phi(second_symbol, IterationPhiBoundary::Header);
    let first_exit = phi(first_symbol, IterationPhiBoundary::Exit);
    let second_exit = phi(second_symbol, IterationPhiBoundary::Exit);
    let first_incomings = &plans[&first_loop].closure_phi_incomings();
    let incoming = |kind| {
        first_incomings
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    let first_binding = |kind, target| {
        incoming(kind)
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap()
    };
    let first_entry = first_binding(IterationPhiIncomingKind::Entry, first_header.owner());
    let second_entry = first_binding(IterationPhiIncomingKind::Entry, second_header.owner());
    let first_backedge = first_binding(IterationPhiIncomingKind::Fallthrough, first_header.owner());
    let second_backedge =
        first_binding(IterationPhiIncomingKind::Fallthrough, second_header.owner());
    let first_exhausted = first_binding(IterationPhiIncomingKind::Exhaustion, first_exit.owner());
    let second_exhausted = first_binding(IterationPhiIncomingKind::Exhaustion, second_exit.owner());
    for binding in [
        first_entry,
        second_entry,
        first_backedge,
        second_backedge,
        first_exhausted,
        second_exhausted,
    ] {
        assert_eq!(binding.values().len(), 1);
    }
    let create_for = |wanted| {
        steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == wanted => {
                    Some(*action)
                }
                _ => None,
            })
            .unwrap()
    };
    let mut next_instance = 0;
    let mut instance_nodes = BTreeMap::new();
    let mut values = BTreeMap::new();
    let mut create = |action,
                      values: &mut BTreeMap<CleanupOwnerValueId, usize>,
                      instance_nodes: &mut BTreeMap<usize, usize>| {
        let IterationCleanupAction::CreateClosureOwner { owner, closure } = action else {
            panic!("an environment must be formed by its Create action")
        };
        next_instance += 1;
        let node = graph
            .nodes()
            .iter()
            .position(|node| node.closure() == closure)
            .unwrap();
        assert!(instance_nodes.insert(next_instance, node).is_none());
        assert!(values.insert(owner, next_instance).is_none());
        next_instance
    };
    let initial_first = create(
        create_for(first_entry.values()[0].source()),
        &mut values,
        &mut instance_nodes,
    );
    let initial_second = create(
        create_for(second_entry.values()[0].source()),
        &mut values,
        &mut instance_nodes,
    );
    let mut choices = BTreeMap::new();
    let selected_root = |binding: &IterationPhiIncomingBinding, choices: &BTreeMap<_, _>| {
        let roots = binding
            .root_sources()
            .iter()
            .filter(|root| selected(table, root.condition(), choices))
            .collect::<Vec<_>>();
        let values = binding
            .values()
            .iter()
            .filter(|value| selected(table, value.condition(), choices))
            .collect::<Vec<_>>();
        let ([root], [value]) = (roots.as_slice(), values.as_slice()) else {
            panic!("a present phi binding must select one root and one value")
        };
        assert_eq!(root.source(), value.source());
        **root
    };
    for binding in [first_entry, second_entry] {
        assert_eq!(
            binding.presence_source(),
            IterationPhiPresenceSource::StaticConditions
        );
        assert_eq!(
            selected_root(binding, &choices).source(),
            binding.values()[0].source()
        );
    }
    replay_edge_presence(
        table,
        incoming(IterationPhiIncomingKind::Entry),
        &mut choices,
    );
    for binding in [first_entry, second_entry] {
        let moved = values.remove(&binding.values()[0].source()).unwrap();
        assert!(values.insert(binding.target(), moved).is_none());
    }
    let snapshot_for = |symbol| {
        steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *target == symbol =>
                {
                    Some(*owner)
                }
                _ => None,
            })
            .unwrap()
    };
    let first_snapshot = snapshot_for(first_symbol);
    let second_snapshot = snapshot_for(second_symbol);
    let inner_create = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { closure, .. }
                if *closure == graph.nodes()[repeated].closure() =>
            {
                Some(*action)
            }
            _ => None,
        })
        .unwrap();
    let IterationCleanupAction::CreateClosureOwner {
        owner: inner_owner, ..
    } = inner_create
    else {
        unreachable!()
    };
    let inner_capture = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } if *owner == inner_owner => Some((*target, *input)),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        table
            .capture_slot_value(inner_capture.0)
            .unwrap()
            .position(),
        0
    );
    assert_eq!(inner_capture.1.mode(), ClosureCaptureMode::Owned);
    assert_eq!(inner_capture.1.effect(), ClosureCaptureEffect::Move);
    let CleanupCaptureValue::Owner(xs_owner) = inner_capture.1.value() else {
        panic!("the repeated inner closure must move its own source")
    };
    let Some(CleanupOwnerValue::Expression { expression: xs, .. }) = table.owner_value(xs_owner)
    else {
        panic!("the owned source must come from this round's expression")
    };
    assert_eq!(
        sources.slice(parsed.ast().expressions().get(*xs).unwrap().span()),
        Ok("listOf(1)")
    );
    assert_eq!(
        table
            .owner_snapshot(second_snapshot)
            .unwrap()
            .capture_inputs()[0]
            .owner(),
        first_header.owner()
    );
    assert_eq!(
        table
            .owner_snapshot(first_snapshot)
            .unwrap()
            .capture_inputs()[0]
            .owner(),
        inner_owner
    );
    assert_eq!(first_backedge.values()[0].source(), first_snapshot);
    assert_eq!(second_backedge.values()[0].source(), second_snapshot);
    let step = |wanted| {
        steps
            .iter()
            .position(|(_, action)| *action == wanted)
            .unwrap()
    };
    let snapshot_action = |wanted| {
        steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, .. } if *owner == wanted => {
                    Some(*action)
                }
                _ => None,
            })
            .unwrap()
    };
    let second_drop = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::Drop(fact)
                if fact.target() == DropTarget::Named(second_symbol)
                    && fact.owner() == Some(second_header.owner()) =>
            {
                Some(*action)
            }
            _ => None,
        })
        .unwrap();
    let second_replacement_value = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(_, node)| {
            (sources.slice(node.span()).ok()? == "second = first").then(|| {
                let Expression::Assignment { value, .. } = node.payload() else {
                    panic!("the old second drop belongs to an assignment")
                };
                *value
            })
        })
        .unwrap();
    let IterationCleanupAction::Drop(old_second_fact) = second_drop else {
        unreachable!()
    };
    assert_eq!(
        old_second_fact.point(),
        DropPoint::AfterExpression(second_replacement_value)
    );
    assert_eq!(steps[step(second_drop)].0, old_second_fact.point());
    let inner_save = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture { owner, .. } if *owner == inner_owner => {
                Some(*action)
            }
            _ => None,
        })
        .unwrap();
    assert!(
        step(snapshot_action(second_snapshot)) < step(second_drop)
            && step(second_drop)
                < step(IterationCleanupAction::CommitOwnerSnapshot {
                    owner: second_snapshot,
                    target: second_symbol,
                })
            && step(IterationCleanupAction::CommitOwnerSnapshot {
                owner: second_snapshot,
                target: second_symbol,
            }) < step(inner_create)
            && step(inner_create) < step(inner_save)
            && step(inner_save) < step(snapshot_action(first_snapshot))
            && step(snapshot_action(first_snapshot))
                < step(IterationCleanupAction::CommitOwnerSnapshot {
                    owner: first_snapshot,
                    target: first_symbol,
                })
    );
    let mut leaf_resources = BTreeMap::new();
    let mut dropped_old = Vec::new();
    for round in 1..=2 {
        let IterationCleanupAction::SaveOwnerSnapshot { condition, .. } =
            snapshot_action(second_snapshot)
        else {
            unreachable!()
        };
        assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
        let second_inputs = table
            .owner_snapshot(second_snapshot)
            .unwrap()
            .capture_inputs()
            .iter()
            .filter(|input| selected(table, input.condition(), &choices))
            .collect::<Vec<_>>();
        let [second_input] = second_inputs.as_slice() else {
            panic!("round {round} must select one old second input")
        };
        let second_input = second_input.owner();
        replay_snapshot_choices(table, second_snapshot, &mut choices);
        let moved = values.remove(&second_input).unwrap();
        assert!(values.insert(second_snapshot, moved).is_none());
        let IterationCleanupAction::Drop(old_second) = second_drop else {
            unreachable!()
        };
        assert!(
            old_second
                .condition()
                .is_none_or(|guard| selected(table, guard, &choices))
        );
        let old = values.remove(&old_second.owner().unwrap()).unwrap();
        assert!(graph.nodes()[instance_nodes[&old]].sources().is_empty());
        dropped_old.push(old);
        let inner = create(inner_create, &mut values, &mut instance_nodes);
        assert_eq!(instance_nodes[&inner], repeated);
        let IterationCleanupAction::SaveClosureCapture {
            owner,
            target,
            input,
        } = inner_save
        else {
            unreachable!()
        };
        assert_eq!(owner, inner_owner);
        assert_eq!((target, input), inner_capture);
        assert!(selected(table, input.condition(), &choices));
        let CleanupCaptureValue::Owner(source) = input.value() else {
            unreachable!()
        };
        let mut evaluated_source = BTreeMap::from([(xs_owner, 100 + round)]);
        let captured_source = evaluated_source.remove(&source).unwrap();
        assert!(evaluated_source.is_empty());
        let position = table.capture_slot_value(target).unwrap().position();
        assert!(
            leaf_resources
                .insert((inner, position), captured_source)
                .is_none()
        );
        let IterationCleanupAction::SaveOwnerSnapshot { condition, .. } =
            snapshot_action(first_snapshot)
        else {
            unreachable!()
        };
        assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
        let first_inputs = table
            .owner_snapshot(first_snapshot)
            .unwrap()
            .capture_inputs()
            .iter()
            .filter(|input| selected(table, input.condition(), &choices))
            .collect::<Vec<_>>();
        let [first_input] = first_inputs.as_slice() else {
            panic!("round {round} must select one new first input")
        };
        let first_input = first_input.owner();
        replay_snapshot_choices(table, first_snapshot, &mut choices);
        let formed = values.remove(&first_input).unwrap();
        assert_eq!(formed, inner);
        assert!(values.insert(first_snapshot, formed).is_none());
        for binding in [first_backedge, second_backedge] {
            assert_eq!(
                selected_root(binding, &choices).source(),
                binding.values()[0].source()
            );
        }
        replay_edge_presence(
            table,
            incoming(IterationPhiIncomingKind::Fallthrough),
            &mut choices,
        );
        let writes = [first_backedge, second_backedge]
            .into_iter()
            .map(|binding| {
                (
                    binding.target(),
                    values.remove(&binding.values()[0].source()).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        for (target, instance) in writes {
            assert!(values.insert(target, instance).is_none());
        }
    }
    assert_eq!(dropped_old, [initial_second, initial_first]);
    let first_child = values[&first_header.owner()];
    let second_child = values[&second_header.owner()];
    assert_ne!(first_child, second_child);
    assert_eq!(instance_nodes[&first_child], repeated);
    assert_eq!(instance_nodes[&second_child], repeated);
    for binding in [first_exhausted, second_exhausted] {
        assert_eq!(
            selected_root(binding, &choices).source(),
            binding.values()[0].source()
        );
    }
    replay_edge_presence(
        table,
        incoming(IterationPhiIncomingKind::Exhaustion),
        &mut choices,
    );
    for binding in [first_exhausted, second_exhausted] {
        let moved = values.remove(&binding.values()[0].source()).unwrap();
        assert!(values.insert(binding.target(), moved).is_none());
    }
    assert_eq!(values[&first_exit.owner()], first_child);
    assert_eq!(values[&second_exit.owner()], second_child);

    // The second loop must forward the parent handle, never rewrite either saved child edge.
    let parent_create = create_for(formed);
    let parent_instance = create(parent_create, &mut values, &mut instance_nodes);
    let mut saved_children = BTreeMap::new();
    for (target, input) in &saves {
        assert!(selected(table, input.condition(), &choices));
        let slot = table.capture_slot_value(**target).unwrap();
        let CleanupCaptureValue::Owner(source) = input.value() else {
            unreachable!()
        };
        let child = values.remove(&source).unwrap();
        assert!(
            saved_children
                .insert((parent_instance, slot.position()), child)
                .is_none()
        );
    }
    assert_eq!(saved_children[&(parent_instance, 0)], first_child);
    assert_eq!(saved_children[&(parent_instance, 1)], second_child);
    let replay_instance_presence =
        |edge: &crate::ownership_checking::IterationPhiIncoming,
         values: &BTreeMap<CleanupOwnerValueId, usize>,
         choices: &mut BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>| {
            let before = choices.clone();
            assert!(selected(table, edge.condition(), &before));
            let [binding] = edge.bindings() else {
                panic!("the second loop carries one parent binding")
            };
            assert!(selected(table, binding.available_when(), &before));
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
                panic!("the parent must have one selected root instance")
            };
            let selected_values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &before))
                .collect::<Vec<_>>();
            let [value] = selected_values.as_slice() else {
                panic!("the parent must have one selected value input")
            };
            assert_eq!(root.source(), value.source());
            let instance = values[&value.source()];
            assert_eq!(instance_nodes[&instance], root.node());
            let mut pending = vec![instance];
            let mut visited = BTreeSet::new();
            let mut reached = BTreeSet::new();
            while let Some(instance) = pending.pop() {
                assert!(visited.insert(instance), "a formed child is visited once");
                let node = instance_nodes[&instance];
                reached.insert(node);
                for source in graph.nodes()[node].sources() {
                    if source.captured().is_empty() {
                        continue;
                    }
                    let child = saved_children[&(instance, source.position())];
                    assert!(source.captured().contains(&instance_nodes[&child]));
                    pending.push(child);
                }
            }
            assert_eq!(
                visited,
                BTreeSet::from([parent_instance, first_child, second_child])
            );
            assert_eq!(
                reached,
                BTreeSet::from([instance_nodes[&parent_instance], repeated])
            );
            let layout = plans[&statement]
                .closure_phis()
                .iter()
                .find(|phi| phi.owner() == binding.target())
                .unwrap();
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
            for (target, value) in writes {
                choices.insert(target, value);
            }
            for write in binding.selector_writes() {
                assert_eq!(
                    choices[&write.target()],
                    usize::from(reached.contains(&write.node()))
                );
            }
            (binding.target(), value.source(), instance)
        };
    let parent_snapshot = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                if *value == parent.closure() =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        table
            .owner_snapshot(parent_snapshot)
            .unwrap()
            .capture_inputs()[0]
            .owner(),
        formed
    );
    let parent_saves = saves
        .iter()
        .map(
            |(target, input)| IterationCleanupAction::SaveClosureCapture {
                owner: formed,
                target: **target,
                input: **input,
            },
        )
        .collect::<Vec<_>>();
    let parent_snapshot_action = snapshot_action(parent_snapshot);
    let header = plans[&statement]
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
        .unwrap();
    let outer_symbol = header.symbol();
    assert_eq!(
        sources.slice(names.symbols()[outer_symbol.index()].span()),
        Ok("outer")
    );
    let parent_commit = IterationCleanupAction::CommitOwnerSnapshot {
        owner: parent_snapshot,
        target: outer_symbol,
    };
    let parent_actions = [
        parent_create,
        parent_saves[0],
        parent_saves[1],
        parent_snapshot_action,
        parent_commit,
    ];
    assert!(
        parent_actions
            .windows(2)
            .all(|pair| step(pair[0]) < step(pair[1]))
    );
    assert!(
        parent_actions.iter().all(|action| {
            steps[step(*action)].0 == DropPoint::AfterExpression(parent.closure())
        })
    );
    let IterationCleanupAction::SaveOwnerSnapshot { condition, .. } = parent_snapshot_action else {
        unreachable!()
    };
    assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
    let parent_inputs = table
        .owner_snapshot(parent_snapshot)
        .unwrap()
        .capture_inputs()
        .iter()
        .filter(|input| selected(table, input.condition(), &choices))
        .collect::<Vec<_>>();
    let [parent_input] = parent_inputs.as_slice() else {
        panic!("parent snapshot must select its formed instance")
    };
    assert_eq!(parent_input.owner(), formed);
    replay_snapshot_choices(table, parent_snapshot, &mut choices);
    let formed_instance = values.remove(&formed).unwrap();
    assert_eq!(formed_instance, parent_instance);
    assert!(values.insert(parent_snapshot, formed_instance).is_none());
    let second_entry = incomings
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let second_backedge = incomings
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
        .unwrap();
    let second_exhausted = incomings
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let [entry] = second_entry.bindings() else {
        panic!("outer has one phi root")
    };
    let [backedge] = second_backedge.bindings() else {
        panic!("outer has one phi root")
    };
    let [exhausted] = second_exhausted.bindings() else {
        panic!("outer has one phi root")
    };
    let parent_instance_node = graph
        .nodes()
        .iter()
        .position(|node| node.closure() == parent.closure())
        .unwrap();
    assert_eq!(entry.values()[0].source(), parent_snapshot);
    for binding in [entry, backedge, exhausted] {
        let [root] = binding.root_sources() else {
            panic!("the parent instance needs one root handle source")
        };
        assert_eq!(root.source(), binding.values()[0].source());
        assert_eq!(root.node(), parent_instance_node);
    }
    assert_eq!(selected_root(entry, &choices).source(), parent_snapshot);
    let (target, source, moved) = replay_instance_presence(second_entry, &values, &mut choices);
    assert_eq!(target, entry.target());
    assert_eq!(values.remove(&source), Some(moved));
    assert!(values.insert(entry.target(), moved).is_none());
    assert_eq!(backedge.target(), entry.target());
    assert_eq!(backedge.values()[0].source(), entry.target());
    assert_eq!(
        backedge.availability_selector(),
        header.availability_selector()
    );
    assert_eq!(backedge.available_when(), header.availability_condition());
    assert_eq!(
        backedge.values()[0].condition(),
        header.availability_condition()
    );
    for _ in 0..2 {
        let saved_before = saved_children.clone();
        assert_eq!(
            selected_root(backedge, &choices).source(),
            backedge.target()
        );
        let (target, source, old) =
            replay_instance_presence(second_backedge, &values, &mut choices);
        assert_eq!((target, source), (backedge.target(), backedge.target()));
        assert_eq!(values.insert(target, old), Some(old));
        assert_eq!(values[&entry.target()], parent_instance);
        assert_eq!(saved_children, saved_before);
        assert_eq!(saved_children[&(parent_instance, 0)], first_child);
        assert_eq!(saved_children[&(parent_instance, 1)], second_child);
    }
    assert_eq!(
        selected_root(exhausted, &choices).source(),
        backedge.target()
    );
    let (target, source, moved) = replay_instance_presence(second_exhausted, &values, &mut choices);
    assert_eq!(target, exhausted.target());
    assert_eq!(values.remove(&source), Some(moved));
    assert!(values.insert(exhausted.target(), moved).is_none());
    // Two live children have the same static lambda; release must start from the root
    // instance so neither child is conflated with the other one's capture layout.
    let root_drop = candidate
        .drops
        .iter()
        .find(|fact| {
            fact.owner() == Some(exhausted.target())
                && matches!(fact.target(), DropTarget::Named(_))
        })
        .unwrap();
    assert_eq!(root_drop.target(), DropTarget::Named(outer_symbol));
    let outer_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "outer()").then_some(id))
        .unwrap();
    assert_eq!(root_drop.point(), DropPoint::CallReturn(outer_call));
    assert_ne!(
        root_drop.condition(),
        Some(CleanupConditionId::NEVER),
        "the formed root must have a reachable release guard"
    );
    assert!(
        root_drop
            .condition()
            .is_none_or(|guard| selected(table, guard, &choices))
    );
    let releases = steps
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::ReleaseClosureInstances {
                layout: ClosureReleaseLayout::Iteration(released_statement),
                root,
            } if *point == root_drop.point() && released_statement.index() == statement => {
                Some((*released_statement, *root))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(release_statement, release_root)] = releases.as_slice() else {
        panic!("the exit must release one formed root instance")
    };
    assert_eq!(*release_root, *root_drop);
    let release_graph = &plans[&release_statement.index()].capture_graph();
    assert_eq!(
        steps
            .iter()
            .filter(|(_, action)| match action {
                IterationCleanupAction::Drop(fact)
                | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => {
                    fact.owner() == Some(exhausted.target())
                }
                _ => false,
            })
            .count(),
        1,
        "the exit root must have one release action across all points"
    );
    assert!(!steps.iter().any(|(_, action)| {
        matches!(action, IterationCleanupAction::Drop(fact)
        if matches!(fact.target(), DropTarget::Captured { .. })
            && fact.instance_address().is_some_and(|address| {
                table.instance_address(address)
                    .is_some_and(|address| address.root() == exhausted.target())
            }))
    }));
    let root_instance = values.remove(&release_root.owner().unwrap()).unwrap();
    assert_eq!(root_instance, parent_instance);
    enum ReleaseStep {
        Enter(usize),
        DropValue(usize, usize),
        Finish(usize),
    }
    #[derive(Debug, PartialEq, Eq)]
    enum Released {
        Value(usize),
        Environment(usize),
    }
    let mut pending = vec![ReleaseStep::Enter(root_instance)];
    let mut entered = BTreeSet::new();
    let mut released = Vec::new();
    while let Some(step) = pending.pop() {
        match step {
            ReleaseStep::Enter(instance) => {
                assert!(
                    entered.insert(instance),
                    "an instance must be released once"
                );
                pending.push(ReleaseStep::Finish(instance));
                for source in release_graph.nodes()[instance_nodes[&instance]].sources() {
                    assert_eq!(source.capture().mode(), ClosureCaptureMode::Owned);
                    assert_eq!(source.capture().effect(), ClosureCaptureEffect::Move);
                    if source.captured().is_empty() {
                        pending.push(ReleaseStep::DropValue(instance, source.position()));
                    } else {
                        let child = saved_children
                            .remove(&(instance, source.position()))
                            .unwrap();
                        assert!(source.captured().contains(&instance_nodes[&child]));
                        pending.push(ReleaseStep::Enter(child));
                    }
                }
            }
            ReleaseStep::DropValue(instance, position) => released.push(Released::Value(
                leaf_resources.remove(&(instance, position)).unwrap(),
            )),
            ReleaseStep::Finish(instance) => released.push(Released::Environment(instance)),
        }
    }
    assert!(saved_children.is_empty() && leaf_resources.is_empty());
    assert_eq!(
        released,
        [
            Released::Value(101),
            Released::Environment(second_child),
            Released::Value(102),
            Released::Environment(first_child),
            Released::Environment(root_instance),
        ]
    );
    let released_environments = released.iter().filter_map(|event| match event {
        Released::Environment(instance) => Some(instance),
        Released::Value(_) => None,
    });
    assert_eq!(
        dropped_old
            .iter()
            .chain(released_environments)
            .copied()
            .collect::<BTreeSet<_>>()
            .len(),
        instance_nodes.len()
    );
    assert!(values.is_empty());
}
