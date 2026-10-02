use super::*;

#[test]
fn sibling_shared_capture_loans_keep_distinct_instance_paths() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "sibling-captures.ko",
            "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>, next: List<Int>) {
                    val xs = listOf(1)
                    var first: () -> Unit = {}
                    var second: () -> Unit = {}
                    for (_ in flags) {
                        second = first
                        { first = { read(xs) } }
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
    let outer_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "outer()").then_some(id))
        .unwrap();
    let mut addresses = steps
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::EndCaptureLoan {
                instance_address, ..
            } if *point == DropPoint::CallReturn(outer_call) => table
                .instance_address(*instance_address)
                .map(|address| (address.root(), address.capture_path().to_vec())),
            _ => None,
        })
        .collect::<Vec<_>>();
    addresses.sort_by(|left, right| left.1.cmp(&right.1));
    assert_eq!(addresses.len(), 2);
    assert_eq!(addresses[0].0, addresses[1].0);
    assert_eq!(addresses[0].1, [0]);
    assert_eq!(addresses[1].1, [1]);
    let mut captured_drops = candidate
        .drops
        .iter()
        .filter_map(|fact| match fact.target() {
            DropTarget::Captured { .. } if fact.point() == DropPoint::CallReturn(outer_call) => {
                let address = table.instance_address(fact.instance_address()?)?;
                let slot = table.capture_slot_value(fact.capture_slot()?)?;
                Some((
                    address.root(),
                    address.capture_path().to_vec(),
                    slot.position(),
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    captured_drops.sort_by_key(|drop| drop.2);
    assert_eq!(captured_drops.len(), 2);
    assert_eq!(captured_drops[0], (addresses[0].0, Vec::new(), 0));
    assert_eq!(captured_drops[1], (addresses[0].0, Vec::new(), 1));

    let statement = *plans.keys().max().unwrap();
    let graph = &plans[&statement].capture_graph();
    let outer = graph
        .nodes()
        .iter()
        .find(|node| {
            sources.slice(
                parsed
                    .ast()
                    .expressions()
                    .get(node.closure())
                    .unwrap()
                    .span(),
            ) == Ok("move { val a = first()\nval b = second() }")
        })
        .unwrap();
    let inner = graph
        .nodes()
        .iter()
        .find(|node| {
            sources.slice(
                parsed
                    .ast()
                    .expressions()
                    .get(node.closure())
                    .unwrap()
                    .span(),
            ) == Ok("{ read(xs) }")
        })
        .unwrap();
    assert_eq!(outer.sources().len(), 2);
    assert!(outer.sources().iter().all(|source| {
        source
            .captured()
            .iter()
            .any(|&node| graph.nodes()[node].closure() == inner.closure())
    }));
    let symbol = |index: usize| match outer.sources()[index].capture().source() {
        ClosureCaptureSource::Symbol(symbol) => symbol,
        other => panic!("outer capture must read a named binding: {other:?}"),
    };
    let first = symbol(0);
    let second = symbol(1);
    let first_loop = *plans.keys().min().unwrap();
    let first_graph = &plans[&first_loop].capture_graph();
    let first_phis = &plans[&first_loop].closure_phis();
    let phi = |symbol, boundary| {
        first_phis
            .iter()
            .find(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
            .unwrap()
    };
    let first_header = phi(first, IterationPhiBoundary::Header);
    let second_header = phi(second, IterationPhiBoundary::Header);
    let first_exit = phi(first, IterationPhiBoundary::Exit);
    let second_exit = phi(second, IterationPhiBoundary::Exit);
    let first_incomings = &plans[&first_loop].closure_phi_incomings();
    let first_edge = |kind| {
        first_incomings
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    let binding = |kind, target| {
        first_edge(kind)
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap()
    };
    let active_root = |binding: &IterationPhiIncomingBinding, choices: &BTreeMap<_, _>| {
        let roots = binding
            .root_sources()
            .iter()
            .filter(|root| selected(table, root.condition(), choices))
            .collect::<Vec<_>>();
        let [root] = roots.as_slice() else {
            panic!("one formed closure root must reach each shared-loan phi binding")
        };
        **root
    };
    let first_entry = binding(IterationPhiIncomingKind::Entry, first_header.owner());
    let second_entry = binding(IterationPhiIncomingKind::Entry, second_header.owner());
    let first_back = binding(IterationPhiIncomingKind::Fallthrough, first_header.owner());
    let second_back = binding(IterationPhiIncomingKind::Fallthrough, second_header.owner());
    let first_out = binding(IterationPhiIncomingKind::Exhaustion, first_exit.owner());
    let second_out = binding(IterationPhiIncomingKind::Exhaustion, second_exit.owner());
    for incoming in [
        first_entry,
        second_entry,
        first_back,
        second_back,
        first_out,
        second_out,
    ] {
        assert_eq!(incoming.values().len(), 1);
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
    let mut values = BTreeMap::new();
    let mut instance_closures = BTreeMap::new();
    let mut create = |action,
                      values: &mut BTreeMap<CleanupOwnerValueId, usize>,
                      instances: &mut BTreeMap<usize, ExpressionId>| {
        let IterationCleanupAction::CreateClosureOwner { owner, closure } = action else {
            unreachable!()
        };
        next_instance += 1;
        assert!(values.insert(owner, next_instance).is_none());
        assert!(instances.insert(next_instance, closure).is_none());
        next_instance
    };
    let initial_first = create(
        create_for(first_entry.values()[0].source()),
        &mut values,
        &mut instance_closures,
    );
    let initial_second = create(
        create_for(second_entry.values()[0].source()),
        &mut values,
        &mut instance_closures,
    );
    let ordinary_entries = first_edge(IterationPhiIncomingKind::Entry)
        .bindings()
        .iter()
        .filter(|binding| binding.root_sources().is_empty())
        .collect::<Vec<_>>();
    let [source_entry] = ordinary_entries.as_slice() else {
        panic!("the shared source needs one ordinary entry binding")
    };
    let [entry_value] = source_entry.values() else {
        panic!("shared source needs one entry value")
    };
    let Some(CleanupOwnerValue::Expression { expression, .. }) =
        table.owner_value(entry_value.source())
    else {
        panic!("shared source entry must be the list expression")
    };
    assert_eq!(
        sources.slice(parsed.ast().expressions().get(*expression).unwrap().span()),
        Ok("listOf(1)")
    );
    let mut source_values = BTreeMap::from([(entry_value.source(), 100_usize)]);
    let first_nodes = |instances: &BTreeMap<usize, ExpressionId>| {
        instances
            .iter()
            .filter_map(|(&instance, &closure)| {
                first_graph
                    .nodes()
                    .iter()
                    .position(|node| node.closure() == closure)
                    .map(|node| (instance, node))
            })
            .collect::<BTreeMap<_, _>>()
    };
    let mut choices = BTreeMap::new();
    let entry_roots = [first_entry, second_entry]
        .into_iter()
        .map(|incoming| {
            let root = active_root(incoming, &choices);
            assert_eq!(root.source(), incoming.values()[0].source());
            assert_eq!(
                first_graph.nodes()[root.node()].closure(),
                instance_closures[&values[&root.source()]]
            );
            (incoming.target(), root.source())
        })
        .collect::<Vec<_>>();
    let mut edge_values = values.clone();
    edge_values.extend(
        source_values
            .iter()
            .map(|(&owner, &instance)| (owner, instance)),
    );
    let entry_transport = replay_captured_edge(
        table,
        first_graph,
        first_phis,
        first_edge(IterationPhiIncomingKind::Entry),
        ReplayInstances {
            values: &edge_values,
            nodes: &first_nodes(&instance_closures),
            captured: &BTreeMap::new(),
        },
        &mut choices,
    );
    assert_eq!(
        entry_transport.len(),
        first_edge(IterationPhiIncomingKind::Entry).bindings().len()
    );
    for (target, source) in entry_roots {
        assert_eq!(entry_transport[&target], (source, values[&source]));
        let moved = values.remove(&source).unwrap();
        assert!(values.insert(target, moved).is_none());
    }
    let source_instance = source_values.remove(&entry_value.source()).unwrap();
    assert_eq!(
        entry_transport[&source_entry.target()],
        (entry_value.source(), source_instance)
    );
    assert!(
        source_values
            .insert(source_entry.target(), source_instance)
            .is_none()
    );
    let snapshot_for = |target| {
        steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CommitOwnerSnapshot {
                    owner,
                    target: actual,
                } if *actual == target => Some(*owner),
                _ => None,
            })
            .unwrap()
    };
    let first_snapshot = snapshot_for(first);
    let second_snapshot = snapshot_for(second);
    assert_eq!(
        table
            .owner_snapshot(second_snapshot)
            .unwrap()
            .capture_inputs()[0]
            .owner(),
        first_header.owner()
    );
    let inner_create = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { closure, .. }
                if *closure == inner.closure() =>
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
    assert_eq!(
        table
            .owner_snapshot(first_snapshot)
            .unwrap()
            .capture_inputs()[0]
            .owner(),
        inner_owner
    );
    let inner_save = steps
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
    assert_eq!(inner_save.1.mode(), ClosureCaptureMode::Shared);
    let CleanupCaptureValue::Owner(source_owner) = inner_save.1.value() else {
        unreachable!()
    };
    let Some(CleanupOwnerValue::IterationPhi {
        statement: _,
        boundary: IterationPhiBoundary::Header,
        symbol: source_symbol,
        ..
    }) = table.owner_value(source_owner)
    else {
        panic!("shared source must be the first loop's header phi")
    };
    assert!(
        first_phis
            .iter()
            .any(|phi| phi.owner() == source_owner && phi.symbol() == *source_symbol)
    );
    assert_eq!(source_entry.target(), source_owner);
    let source_back = binding(IterationPhiIncomingKind::Fallthrough, source_owner);
    assert_eq!(source_back.values().len(), 1);
    assert_eq!(source_back.values()[0].source(), source_owner);
    let inner_slot = table.capture_slot_value(inner_save.0).unwrap();
    assert_eq!(inner_slot.environment(), inner_owner);
    assert_eq!(inner_slot.closure(), inner.closure());
    assert_eq!(inner_slot.source(), inner_save.1.source());
    assert_eq!(inner_slot.position(), 0);
    let mut source_slots = BTreeMap::new();
    let mut live_loans = BTreeMap::from([(source_values[&source_owner], 0_usize)]);
    let mut released_old = Vec::new();
    let old_second_drop = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::Drop(fact)
                if fact.target() == DropTarget::Named(second)
                    && fact.owner() == Some(second_header.owner()) =>
            {
                Some(*fact)
            }
            _ => None,
        })
        .unwrap();
    let snapshot_index = |wanted| {
        steps
            .iter()
            .position(|(_, action)| {
                matches!(action,
                    IterationCleanupAction::SaveOwnerSnapshot { owner, .. } if *owner == wanted)
            })
            .unwrap()
    };
    let old_drop_index = steps
            .iter()
            .position(|(_, action)| {
                matches!(action, IterationCleanupAction::Drop(fact) if *fact == old_second_drop)
            })
            .unwrap();
    let early_ends = steps
        .iter()
        .enumerate()
        .filter_map(|(index, (point, action))| match action {
            IterationCleanupAction::EndCaptureLoan {
                owner,
                source,
                condition,
                ..
            } if *source == inner_save.1.source()
                && *point != DropPoint::CallReturn(outer_call) =>
            {
                assert_eq!(*owner, second_header.owner());
                Some((index, *condition))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        early_ends.len(),
        2,
        "named/retained source paths are distinct"
    );
    let mut proof = table.clone();
    assert_eq!(
        proof.and(early_ends[0].1.unwrap(), early_ends[1].1.unwrap()),
        CleanupConditionId::NEVER,
        "one instance must not end the same loan on both source paths"
    );
    assert!(snapshot_index(second_snapshot) < old_drop_index);
    for (early_end_index, _) in &early_ends {
        assert!(old_drop_index < *early_end_index);
        assert!(*early_end_index < snapshot_index(first_snapshot));
    }
    for _ in 0..2 {
        let moved = values.remove(&first_header.owner()).unwrap();
        replay_snapshot_choices(table, second_snapshot, &mut choices);
        assert!(values.insert(second_snapshot, moved).is_none());
        let old = values.remove(&old_second_drop.owner().unwrap()).unwrap();
        released_old.push(old);
        assert!(
            early_ends
                .iter()
                .all(|(_, guard)| !selected(table, guard.unwrap(), &choices)),
            "the replaced second has no shared loan on either executed round"
        );
        assert!(
            steps.iter().all(|(point, action)| {
                *point != old_second_drop.point()
                    || !matches!(action, IterationCleanupAction::Drop(fact)
                        if matches!(fact.target(), DropTarget::RetainedSource(_))
                            && selected(table, fact.condition().unwrap(), &choices))
            }),
            "a live named source must not take the retained cleanup path"
        );
        let formed = create(inner_create, &mut values, &mut instance_closures);
        assert_eq!(instance_closures[&formed], inner.closure());
        assert_eq!(
            inner_save.1.value(),
            CleanupCaptureValue::Owner(source_owner)
        );
        let source = source_values[&source_owner];
        let position = table.capture_slot_value(inner_save.0).unwrap().position();
        assert!(source_slots.insert((formed, position), source).is_none());
        *live_loans.get_mut(&source).unwrap() += 1;
        let formed = values.remove(&inner_owner).unwrap();
        replay_snapshot_choices(table, first_snapshot, &mut choices);
        assert!(values.insert(first_snapshot, formed).is_none());
        let writes = [first_back, second_back]
            .into_iter()
            .map(|incoming| {
                let root = active_root(incoming, &choices);
                assert_eq!(root.source(), incoming.values()[0].source());
                assert_eq!(
                    first_graph.nodes()[root.node()].closure(),
                    instance_closures[&values[&root.source()]]
                );
                (incoming.target(), root.source(), values[&root.source()])
            })
            .collect::<Vec<_>>();
        let mut edge_values = values.clone();
        edge_values.extend(
            source_values
                .iter()
                .map(|(&owner, &instance)| (owner, instance)),
        );
        let back_transport = replay_captured_edge(
            table,
            first_graph,
            first_phis,
            first_edge(IterationPhiIncomingKind::Fallthrough),
            ReplayInstances {
                values: &edge_values,
                nodes: &first_nodes(&instance_closures),
                captured: &BTreeMap::new(),
            },
            &mut choices,
        );
        assert_eq!(
            back_transport.len(),
            first_edge(IterationPhiIncomingKind::Fallthrough)
                .bindings()
                .len()
        );
        assert_eq!(
            back_transport[&source_owner],
            (source_owner, source_instance)
        );
        for (target, source, moved) in writes {
            assert_eq!(back_transport[&target], (source, moved));
            assert_eq!(values.remove(&source), Some(moved));
            assert!(values.insert(target, moved).is_none());
        }
    }
    assert_eq!(released_old, [initial_second, initial_first]);
    let newer = values[&first_header.owner()];
    let older = values[&second_header.owner()];
    assert_ne!(newer, older);
    let exit_roots = [first_out, second_out]
        .into_iter()
        .map(|incoming| {
            let root = active_root(incoming, &choices);
            assert_eq!(root.source(), incoming.values()[0].source());
            assert_eq!(
                first_graph.nodes()[root.node()].closure(),
                instance_closures[&values[&root.source()]]
            );
            (incoming.target(), root.source())
        })
        .collect::<Vec<_>>();
    let mut edge_values = values.clone();
    edge_values.extend(
        source_values
            .iter()
            .map(|(&owner, &instance)| (owner, instance)),
    );
    let before_exit = choices.clone();
    let exit_transport = replay_captured_edge(
        table,
        first_graph,
        first_phis,
        first_edge(IterationPhiIncomingKind::Exhaustion),
        ReplayInstances {
            values: &edge_values,
            nodes: &first_nodes(&instance_closures),
            captured: &BTreeMap::new(),
        },
        &mut choices,
    );
    assert_eq!(exit_transport.len(), 2);
    for (target, source) in exit_roots {
        assert_eq!(exit_transport[&target], (source, values[&source]));
        let moved = values.remove(&source).unwrap();
        assert!(values.insert(target, moved).is_none());
    }
    let ordinary_exits = first_edge(IterationPhiIncomingKind::Exhaustion)
        .bindings()
        .iter()
        .filter(|binding| binding.root_sources().is_empty())
        .collect::<Vec<_>>();
    let [source_exit] = ordinary_exits.as_slice() else {
        panic!("shared source needs one ordinary exit binding")
    };
    assert!(source_exit.values().is_empty());
    assert!(!selected(table, source_exit.available_when(), &before_exit));
    assert_eq!(choices[&source_exit.availability_selector()], 0);
    assert!(!exit_transport.contains_key(&source_exit.target()));
    assert_eq!(source_values[&source_owner], source_instance);
    assert_eq!(values[&first_exit.owner()], newer);
    assert_eq!(values[&second_exit.owner()], older);
    let named_source_drops = steps
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::Drop(fact)
                if fact.target() == DropTarget::Named(*source_symbol) =>
            {
                Some((*point, *fact))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(named_source_drops.len(), 2);
    let (named_point, named_drop) = named_source_drops
        .iter()
        .find(|(_, fact)| fact.owner() == Some(source_owner))
        .unwrap();
    assert!(matches!(named_point,
            DropPoint::LoopExit(drop_statement) if drop_statement.index() == first_loop));
    assert!(
        named_drop
            .condition()
            .is_some_and(|guard| !selected(table, guard, &choices))
    );
    let parent_create = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { closure, .. }
                if *closure == outer.closure() =>
            {
                Some(*action)
            }
            _ => None,
        })
        .unwrap();
    let IterationCleanupAction::CreateClosureOwner {
        owner: parent_owner,
        ..
    } = parent_create
    else {
        unreachable!()
    };
    let parent_instance = create(parent_create, &mut values, &mut instance_closures);
    let mut children = BTreeMap::new();
    for (_, action) in steps {
        let IterationCleanupAction::SaveClosureCapture {
            owner,
            target,
            input,
        } = action
        else {
            continue;
        };
        if *owner != parent_owner {
            continue;
        }
        let CleanupCaptureValue::Owner(source) = input.value() else {
            unreachable!()
        };
        let slot = table.capture_slot_value(*target).unwrap();
        let position = slot.position();
        assert_eq!(slot.environment(), parent_owner);
        assert_eq!(slot.closure(), outer.closure());
        assert_eq!(slot.source(), input.source());
        assert_eq!(slot.source(), outer.sources()[position].capture().source());
        assert_eq!(
            (input.mode(), input.effect()),
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
        );
        let child = values.remove(&source).unwrap();
        assert!(children.insert(position, child).is_none());
    }
    assert_eq!(children, BTreeMap::from([(0, newer), (1, older)]));
    let parent_snapshot = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                if *value == outer.closure() =>
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
        parent_owner
    );
    replay_snapshot_choices(table, parent_snapshot, &mut choices);
    let moved = values.remove(&parent_owner).unwrap();
    assert_eq!(moved, parent_instance);
    assert!(values.insert(parent_snapshot, moved).is_none());
    let outer_symbol = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                if *owner == parent_snapshot =>
            {
                Some(*target)
            }
            _ => None,
        })
        .unwrap();
    let outer_header = plans[&statement]
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == outer_symbol)
        .unwrap();
    let outer_exit = plans[&statement]
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == outer_symbol)
        .unwrap();
    assert_eq!(addresses[0].0, outer_exit.owner());
    let parent_aliases = BTreeSet::from([
        parent_owner,
        parent_snapshot,
        outer_header.owner(),
        outer_exit.owner(),
    ]);
    let mut parent_ends = steps
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::EndCaptureLoan {
                source,
                instance_address,
                ..
            } if *source == inner_save.1.source() => {
                let address = table.instance_address(*instance_address).unwrap();
                parent_aliases
                    .contains(&address.root())
                    .then_some((*point, address.capture_path().to_vec()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    parent_ends.sort_by(|left, right| left.1.cmp(&right.1));
    assert_eq!(
        parent_ends,
        [
            (DropPoint::CallReturn(outer_call), vec![0]),
            (DropPoint::CallReturn(outer_call), vec![1]),
        ],
        "each saved child loan must end once at the parent call return"
    );
    let instance_nodes = instance_closures
        .iter()
        .filter_map(|(&instance, &closure)| {
            graph
                .nodes()
                .iter()
                .position(|node| node.closure() == closure)
                .map(|node| (instance, node))
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        instance_nodes[&parent_instance],
        graph
            .nodes()
            .iter()
            .position(|node| node.closure() == outer.closure())
            .unwrap()
    );
    assert_eq!(instance_nodes[&newer], instance_nodes[&older]);
    let captured = children
        .iter()
        .map(|(&position, &child)| ((parent_instance, position), child))
        .collect::<BTreeMap<_, _>>();
    let second_incomings = &plans[&statement].closure_phi_incomings();
    for kind in [
        IterationPhiIncomingKind::Entry,
        IterationPhiIncomingKind::Fallthrough,
        IterationPhiIncomingKind::Exhaustion,
    ] {
        let incoming = second_incomings
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap();
        let target = if kind == IterationPhiIncomingKind::Exhaustion {
            outer_exit.owner()
        } else {
            outer_header.owner()
        };
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap();
        assert_eq!(binding.values().len(), 1);
        let mut witnessed_children = BTreeSet::new();
        for origin in binding
            .origins()
            .iter()
            .filter(|origin| selected(table, origin.condition(), &choices))
        {
            for environment in origin
                .environments()
                .iter()
                .filter(|environment| selected(table, environment.condition(), &choices))
            {
                assert_eq!(values[&environment.instance_root()], parent_instance);
                assert!(environment.capture_path().is_empty());
                for capture in environment
                    .sources()
                    .iter()
                    .filter(|capture| selected(table, capture.input().condition(), &choices))
                {
                    assert!(capture.target().is_none());
                    assert!(capture.capture_slot().is_none());
                    assert!(capture.transport_value().is_none());
                    let (address, slot) = capture.transport_read().unwrap();
                    let address = table.instance_address(address).unwrap();
                    assert_eq!(address.root(), environment.instance_root());
                    assert!(address.capture_path().is_empty());
                    let position = table.capture_slot_value(slot).unwrap().position();
                    let child = children[&position];
                    assert_eq!(instance_closures[&child], inner.closure());
                    assert!(capture.captured().iter().any(|nested| {
                        selected(table, nested.condition(), &choices)
                            && nested.environments().iter().any(|nested_environment| {
                                selected(table, nested_environment.condition(), &choices)
                                    && nested_environment.instance_root()
                                        == environment.instance_root()
                                    && nested_environment.capture_path() == [position]
                            })
                    }));
                    witnessed_children.insert(position);
                }
            }
        }
        assert_eq!(witnessed_children, BTreeSet::from([0, 1]));
        let root_source = active_root(binding, &choices);
        assert_eq!(root_source.source(), binding.values()[0].source());
        assert_eq!(graph.nodes()[root_source.node()].closure(), outer.closure());
        assert_eq!(values[&root_source.source()], parent_instance);
        let before = choices.clone();
        let mut edge_values = values.clone();
        edge_values.extend(
            source_values
                .iter()
                .map(|(&owner, &instance)| (owner, instance)),
        );
        let transported = replay_captured_edge(
            table,
            graph,
            plans[&statement].closure_phis(),
            incoming,
            ReplayInstances {
                values: &edge_values,
                nodes: &instance_nodes,
                captured: &captured,
            },
            &mut choices,
        );
        assert_eq!(
            transported.len(),
            incoming
                .bindings()
                .iter()
                .filter(|binding| { selected(table, binding.available_when(), &before) })
                .count()
        );
        assert_eq!(
            transported[&binding.target()],
            (root_source.source(), parent_instance)
        );
        for ordinary in incoming
            .bindings()
            .iter()
            .filter(|binding| binding.root_sources().is_empty())
        {
            if let Some(&(source, instance)) = transported.get(&ordinary.target()) {
                assert_eq!(source_values[&source], instance);
                if source != ordinary.target() {
                    assert_eq!(source_values.remove(&source), Some(instance));
                    assert!(source_values.insert(ordinary.target(), instance).is_none());
                }
            } else {
                assert!(!selected(table, ordinary.available_when(), &before));
                assert_eq!(choices[&ordinary.availability_selector()], 0);
            }
        }
        let old = values.remove(&root_source.source()).unwrap();
        assert_eq!(old, parent_instance);
        assert!(values.insert(binding.target(), old).is_none());
        let target_phi = if kind == IterationPhiIncomingKind::Exhaustion {
            outer_exit
        } else {
            outer_header
        };
        for root in target_phi.root_origins() {
            assert_eq!(
                choices[&root.selector()],
                usize::from(
                    root.node() == root_source.node() || root.node() == instance_nodes[&newer]
                )
            );
        }
        assert_eq!(children, BTreeMap::from([(0, newer), (1, older)]));
    }
    assert_eq!(source_values.values().copied().collect::<Vec<_>>(), [100]);
    let (second_named_point, second_named_drop) = named_source_drops
        .iter()
        .find(|(_, fact)| fact.owner() != Some(source_owner))
        .unwrap();
    assert!(matches!(second_named_point,
            DropPoint::LoopExit(drop_statement) if drop_statement.index() == statement));
    assert!(matches!(
        table
            .owner_value(second_named_drop.owner().unwrap()),
        Some(CleanupOwnerValue::IterationPhi {
            statement: drop_statement,
            symbol,
            ..
        }) if drop_statement.index() == statement && *symbol == *source_symbol
    ));
    assert!(
        second_named_drop
            .condition()
            .is_some_and(|guard| !selected(table, guard, &choices))
    );
    let root = values.remove(&outer_exit.owner()).unwrap();
    assert_eq!(root, parent_instance);
    assert_eq!(live_loans[&100], 2);

    // Replay only the actions selected by the formed snapshots and both phi loops.
    // The public plan remains deferred; this does not publish executable cleanup.
    let actions = steps
        .iter()
        .filter(|(point, _)| *point == DropPoint::CallReturn(outer_call))
        .map(|(_, action)| *action)
        .collect::<Vec<_>>();
    let mut releasing = BTreeMap::new();
    let mut ended = BTreeMap::new();
    let mut outcomes = BTreeMap::new();
    let mut test_results = Vec::new();
    let mut released_environments = Vec::new();
    let mut last_loan_guarded_drop_candidates = Vec::new();
    for action in actions {
        match action {
            IterationCleanupAction::Drop(fact)
                if matches!(fact.target(), DropTarget::Captured { .. }) =>
            {
                if fact
                    .condition()
                    .is_some_and(|guard| !selected(table, guard, &choices))
                {
                    continue;
                }
                let address = table
                    .instance_address(fact.instance_address().unwrap())
                    .unwrap();
                assert_eq!(address.root(), outer_exit.owner());
                assert!(address.capture_path().is_empty());
                let position = table
                    .capture_slot_value(fact.capture_slot().unwrap())
                    .unwrap()
                    .position();
                let child = children.remove(&position).unwrap();
                assert!(releasing.insert(position, child).is_none());
                released_environments.push(child);
            }
            IterationCleanupAction::EndCaptureLoan {
                owner,
                instance_address,
                capture_slot: Some(slot),
                condition,
                source: captured_source,
                value,
                ..
            } => {
                if condition.is_some_and(|guard| !selected(table, guard, &choices)) {
                    continue;
                }
                assert_eq!(captured_source, inner_save.1.source());
                let address = table.instance_address(instance_address).unwrap();
                assert_eq!(address.root(), outer_exit.owner());
                let [position] = address.capture_path() else {
                    panic!("loan must name one child instance")
                };
                let expected_source = outer.sources()[*position].capture().source();
                assert!(matches!(
                    table.owner_value(owner),
                    Some(CleanupOwnerValue::IterationPhiSourceOwner {
                        environment,
                        closure,
                        source,
                        ..
                    }) if *environment == outer_exit.owner()
                        && *closure == outer.closure()
                        && *source == expected_source
                ));
                let child = releasing[position];
                let capture_position = table.capture_slot_value(slot).unwrap().position();
                assert_eq!(capture_position, 0);
                let layout = table.capture_slot_value(slot).unwrap();
                assert_eq!(layout.closure(), inner.closure());
                assert_eq!(layout.source(), inner_save.1.source());
                let CleanupCaptureValue::Owner(static_source) = value else {
                    panic!("tracked source must have a phi owner")
                };
                assert!(matches!(
                    table.owner_value(static_source),
                    Some(CleanupOwnerValue::IterationPhiSourceOwner { environment, .. })
                        if *environment == outer_exit.owner()
                ));
                let source = source_slots[&(child, capture_position)];
                assert_eq!(source, 100);
                *live_loans.get_mut(&source).unwrap() -= 1;
                assert!(
                    ended
                        .insert((instance_address, slot), (condition, static_source))
                        .is_none()
                );
            }
            IterationCleanupAction::TestLastCaptureLoan {
                owner,
                instance_address,
                capture_slot,
                selector,
                condition,
                ..
            } => {
                if condition.is_some_and(|guard| !selected(table, guard, &choices)) {
                    continue;
                }
                let address = table.instance_address(instance_address).unwrap();
                assert_eq!(address.root(), outer_exit.owner());
                let [position] = address.capture_path() else {
                    panic!("test must name one child instance")
                };
                let child = releasing[position];
                let layout = table.capture_slot_value(capture_slot).unwrap();
                assert_eq!(layout.closure(), inner.closure());
                assert_eq!(layout.source(), inner_save.1.source());
                let source = source_slots[&(child, layout.position())];
                assert_eq!(
                    ended.get(&(instance_address, capture_slot)),
                    Some(&(condition, owner))
                );
                let last = live_loans[&source] == 0;
                assert!(choices.insert(selector, usize::from(last)).is_none());
                test_results.push(last);
                assert!(
                    outcomes
                        .insert((instance_address, capture_slot), (selector, last, source))
                        .is_none()
                );
            }
            IterationCleanupAction::Drop(fact)
                if matches!(fact.target(), DropTarget::RetainedSource(_)) =>
            {
                let address = fact.instance_address().unwrap();
                let slot = fact.capture_slot().unwrap();
                assert_eq!(
                    fact.target(),
                    DropTarget::RetainedSource(inner_save.1.source())
                );
                assert_eq!(fact.owner(), Some(ended[&(address, slot)].1));
                let (selector, last, source) = outcomes[&(address, slot)];
                // Other source selectors are not evaluated by this deferred test;
                // prove only that the candidate requires its own last-loan true arm.
                fn possible(
                    table: &CleanupConditions,
                    condition: CleanupConditionId,
                    wanted: crate::ownership_checking::CleanupSelectorId,
                    arm: usize,
                ) -> bool {
                    match table.get(condition).unwrap() {
                        CleanupCondition::Always => true,
                        CleanupCondition::Never => false,
                        CleanupCondition::Choice { selector, branches } if *selector == wanted => {
                            possible(table, branches[arm], wanted, arm)
                        }
                        CleanupCondition::Choice { branches, .. } => branches
                            .iter()
                            .any(|branch| possible(table, *branch, wanted, arm)),
                    }
                }
                let guard = fact.condition().unwrap();
                assert!(!possible(table, guard, selector, 0));
                assert!(possible(table, guard, selector, 1));
                assert_eq!(selected(table, guard, &choices), last);
                if selected(table, guard, &choices) {
                    last_loan_guarded_drop_candidates.push(source);
                }
            }
            IterationCleanupAction::Drop(fact) if matches!(fact.target(), DropTarget::Named(_)) => {
                if fact
                    .condition()
                    .is_some_and(|guard| !selected(table, guard, &choices))
                {
                    continue;
                }
                assert_eq!(fact.owner(), Some(outer_exit.owner()));
                assert!(children.is_empty());
                released_environments.push(root);
            }
            _ => {}
        }
    }
    assert_eq!(released_environments, [older, newer, root]);
    assert_eq!(test_results, [false, true]);
    assert_eq!(live_loans[&100], 0);
    assert_eq!(last_loan_guarded_drop_candidates, [100]);
    let retained_source_drops = steps
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::Drop(fact)
                if fact.target() == DropTarget::RetainedSource(inner_save.1.source())
                    && *point == DropPoint::CallReturn(outer_call) =>
            {
                Some((*point, *fact))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(retained_source_drops.len(), 2);
    assert!(!steps.iter().any(|(_, action)| matches!(action,
            IterationCleanupAction::Drop(fact)
                if matches!(fact.target(),
                    DropTarget::Temporary(value) if value == *expression)
                    || matches!(fact.target(),
                        DropTarget::Captured { source, .. } if source == inner_save.1.source()))));
    assert_eq!(
        retained_source_drops
            .iter()
            .filter(|(_, fact)| fact
                .condition()
                .is_some_and(|guard| selected(table, guard, &choices)))
            .count(),
        1
    );
    assert!(source_slots.len() == 2 && releasing.len() == 2);
    assert_eq!(
        released_old
            .iter()
            .chain(&released_environments)
            .copied()
            .collect::<BTreeSet<_>>()
            .len(),
        instance_closures.len()
    );
    assert!(values.is_empty());
}
