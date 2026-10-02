use super::*;

#[test]
fn recursive_body_formation_reads_the_current_header_instance() {
    let mut sources = SourceMap::new();
    let source = sources
            .add_source(
                "recursive-capture.ko",
                "fun run(flags: List<Int>) { var f: move () -> Unit = move {}\nfor (_ in flags) { f = move { f() } }\nval used = f() }",
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
    assert!(planner.recursive_capture_phi.is_some());
    let statement = checker
        .iterations
        .values()
        .next()
        .unwrap()
        .descriptor()
        .statement();
    let header = planner.loop_phis[&statement.index()]
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
        .unwrap();
    let graph = &planner.loop_capture_graphs[&statement.index()];
    let roots = planner.loop_origins[&statement.index()]
        .header()
        .iter()
        .find(|binding| binding.symbol() == header.symbol())
        .unwrap()
        .origins();
    assert_eq!(
        header
            .root_nodes()
            .iter()
            .map(|&node| graph.nodes()[node].closure())
            .collect::<Vec<_>>(),
        roots
    );
    assert!(
        !header.origins().is_empty(),
        "recursive roots need finite layout nodes"
    );
    for &node in header.root_nodes() {
        assert!(header.origins().iter().any(|origin| origin.node() == node));
    }
    assert_eq!(
        header
            .origins()
            .iter()
            .map(|origin| origin.node())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        header.origins().len(),
        "a cyclic graph allocates each reachable node once"
    );
    assert!(!header.capture_layout().is_empty());
    let steps = &planner.cleanup;
    let (capture_at, environment, target, input) = steps
        .iter()
        .enumerate()
        .find_map(|(index, (_, action))| match action {
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } if input.value() == CleanupCaptureValue::Owner(header.owner()) => {
                Some((index, *owner, *target, *input))
            }
            _ => None,
        })
        .expect("the new environment captures the current header owner");
    assert_eq!(input.condition(), header.availability_condition());
    let capture_slot = planner.conditions.capture_slot_value(target).unwrap();
    assert_eq!(capture_slot.environment(), environment);
    assert_eq!(capture_slot.source(), input.source());
    assert_eq!(capture_slot.position(), 0);
    let create_at = steps
        .iter()
        .position(|(_, action)| {
            matches!(
                action,
                IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == environment
            )
        })
        .unwrap();
    let IterationCleanupAction::CreateClosureOwner {
        closure: recursive_closure,
        ..
    } = steps[create_at].1
    else {
        unreachable!()
    };
    let (snapshot_at, snapshot) = steps
        .iter()
        .enumerate()
        .find_map(|(index, (_, action))| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, .. } => Some((index, *owner)),
            _ => None,
        })
        .expect("the replacement saves its value before commit");
    let (commit_at, committed_symbol) = steps
        .iter()
        .enumerate()
        .find_map(|(index, (_, action))| match action {
            IterationCleanupAction::CommitOwnerSnapshot { owner, target } if *owner == snapshot => {
                Some((index, *target))
            }
            _ => None,
        })
        .unwrap();
    assert!(create_at < capture_at && capture_at < snapshot_at && snapshot_at < commit_at);
    assert_eq!(committed_symbol, header.symbol());
    let snapshot_input = planner
        .conditions
        .owner_snapshot(snapshot)
        .unwrap()
        .capture_inputs();
    assert_eq!(snapshot_input.len(), 1);
    assert_eq!(snapshot_input[0].owner(), environment);
    assert_eq!(snapshot_input[0].condition(), CleanupConditionId::ALWAYS);
    let incomings = &planner.loop_phi_incomings[&statement.index()];
    let entry = incomings
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let backedge = incomings
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
        .unwrap();
    let entry = &entry.bindings()[0];
    let backedge = &backedge.bindings()[0];
    assert_eq!(entry.target(), header.owner());
    assert_eq!(backedge.target(), header.owner());
    assert!(entry.capture_slots_to_clear().is_empty());
    assert!(backedge.capture_slots_to_clear().is_empty());
    assert_eq!(entry.values().len(), 1);
    assert_eq!(backedge.values().len(), 1);
    assert_eq!(backedge.values()[0].source(), snapshot);
    assert_eq!(entry.root_sources().len(), 1);
    assert_eq!(entry.root_sources()[0].source(), entry.values()[0].source());
    assert_eq!(backedge.root_sources().len(), 1);
    assert_eq!(backedge.root_sources()[0].source(), snapshot);
    for binding in [entry, backedge] {
        assert_eq!(
            binding.presence_source(),
            IterationPhiPresenceSource::CapturedInstances
        );
        assert!(
            binding.origins().is_empty(),
            "recursive descendants are not unfolded"
        );
        assert_eq!(binding.selector_writes().len(), header.origins().len());
        let selected_root = binding.root_sources()[0];
        let write = binding
            .selector_writes()
            .iter()
            .find(|write| write.node() == selected_root.node())
            .unwrap();
        assert_eq!(write.condition(), selected_root.condition());
    }
    for incoming in incomings.iter().filter(|incoming| {
        matches!(
            incoming.kind(),
            IterationPhiIncomingKind::Entry | IterationPhiIncomingKind::Fallthrough
        )
    }) {
        assert_eq!(incoming.condition(), CleanupConditionId::ALWAYS);
        assert_eq!(
            incoming.bindings()[0].availability_selector(),
            header.availability_selector()
        );
        assert_eq!(
            incoming.bindings()[0].available_when(),
            CleanupConditionId::ALWAYS
        );
        assert_eq!(
            incoming.bindings()[0].values()[0].condition(),
            CleanupConditionId::ALWAYS
        );
    }
    let Some(CleanupCondition::Choice { selector, branches }) =
        planner.conditions.get(input.condition())
    else {
        panic!("capture must depend on header presence")
    };
    assert_eq!(*selector, header.availability_selector());
    assert_eq!(
        branches,
        &[CleanupConditionId::NEVER, CleanupConditionId::ALWAYS]
    );

    // 同一 Create 动作重复执行必须创建不同实例，旧 header 只能进入新实例的捕获槽。
    let mut next_instance = 0;
    let mut instance_nodes = BTreeMap::new();
    let mut values = BTreeMap::new();
    let mut create = |action: IterationCleanupAction,
                      values: &mut BTreeMap<_, _>,
                      instance_nodes: &mut BTreeMap<_, _>| {
        let IterationCleanupAction::CreateClosureOwner { owner, closure } = action else {
            panic!("formation must start with CreateClosureOwner")
        };
        next_instance += 1;
        assert!(values.insert(owner, next_instance).is_none());
        let node = graph
            .nodes()
            .iter()
            .position(|node| node.closure() == closure)
            .unwrap();
        assert!(instance_nodes.insert(next_instance, node).is_none());
        next_instance
    };
    let initial_create = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, .. }
                if *owner == entry.values()[0].source() =>
            {
                Some(*action)
            }
            _ => None,
        })
        .unwrap();
    let seed = create(initial_create, &mut values, &mut instance_nodes);
    let mut choices = BTreeMap::new();
    let mut captured_slots = BTreeMap::new();
    let [entry_root] = entry.root_sources() else {
        panic!("entry must provide one root")
    };
    assert!(selected(
        &planner.conditions,
        entry_root.condition(),
        &choices
    ));
    let (entry_source, entry_instance) = replay_captured_edge_presence(
        &planner.conditions,
        graph,
        header,
        incomings
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
            .unwrap(),
        entry.target(),
        ReplayInstances {
            values: &values,
            nodes: &instance_nodes,
            captured: &captured_slots,
        },
        &mut choices,
    );
    assert_eq!(entry_source, entry_root.source());
    assert_eq!(entry_instance, seed);
    assert_eq!(values.remove(&entry_source), Some(seed));
    assert!(values.insert(entry.target(), seed).is_none());
    let zero_round = values.clone();
    let mut zero_choices = choices.clone();
    let mut rounds = Vec::new();
    for _ in 0..2 {
        let new_instance = create(steps[create_at].1, &mut values, &mut instance_nodes);
        rounds.push(new_instance);
        let IterationCleanupAction::SaveClosureCapture {
            owner,
            target: saved_slot,
            input: saved_input,
        } = steps[capture_at].1
        else {
            unreachable!()
        };
        assert_eq!(
            (owner, saved_slot, saved_input),
            (environment, target, input)
        );
        let CleanupCaptureValue::Owner(source) = saved_input.value() else {
            unreachable!()
        };
        assert_eq!(source, header.owner());
        let old = values.remove(&source).unwrap();
        assert!(
            captured_slots
                .insert((new_instance, capture_slot.position()), old)
                .is_none()
        );
        let IterationCleanupAction::SaveOwnerSnapshot { owner, .. } = steps[snapshot_at].1 else {
            unreachable!()
        };
        assert_eq!(owner, snapshot);
        let created = values.remove(&snapshot_input[0].owner()).unwrap();
        values.insert(snapshot, created);
        replay_snapshot_choices(&planner.conditions, snapshot, &mut choices);
        let IterationCleanupAction::CommitOwnerSnapshot { owner, target } = steps[commit_at].1
        else {
            unreachable!()
        };
        assert_eq!((owner, target), (snapshot, committed_symbol));
        let active = backedge
            .root_sources()
            .iter()
            .filter(|root| selected(&planner.conditions, root.condition(), &choices))
            .collect::<Vec<_>>();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].source(), snapshot);
        let (source, moved) = replay_captured_edge_presence(
            &planner.conditions,
            graph,
            header,
            incomings
                .iter()
                .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
                .unwrap(),
            backedge.target(),
            ReplayInstances {
                values: &values,
                nodes: &instance_nodes,
                captured: &captured_slots,
            },
            &mut choices,
        );
        assert_eq!(source, active[0].source());
        assert_eq!(values.remove(&source), Some(moved));
        assert!(values.insert(backedge.target(), moved).is_none());
    }
    assert_eq!(
        captured_slots,
        BTreeMap::from([((rounds[0], 0), seed), ((rounds[1], 0), rounds[0]),])
    );
    assert_eq!(values[&header.owner()], rounds[1]);
    let exit = planner.loop_phis[&statement.index()]
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
        .unwrap();
    assert_eq!(exit.root_nodes(), header.root_nodes());
    let exhaustion = incomings
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    assert_eq!(exhaustion.condition(), CleanupConditionId::ALWAYS);
    let exhausted = &exhaustion.bindings()[0];
    assert_eq!(exhausted.target(), exit.owner());
    assert!(exhausted.capture_slots_to_clear().is_empty());
    assert_eq!(
        exhausted.availability_selector(),
        exit.availability_selector()
    );
    assert_eq!(exhausted.available_when(), header.availability_condition());
    assert_eq!(exhausted.values().len(), 1);
    assert_eq!(exhausted.values()[0].source(), header.owner());
    assert_eq!(exhausted.root_sources().len(), header.root_nodes().len());
    let mut conditions = planner.conditions.clone();
    let unavailable = conditions.not(exhausted.available_when());
    for root in exhausted.root_sources() {
        assert_eq!(
            conditions.and(root.condition(), unavailable),
            CleanupConditionId::NEVER,
            "an absent header cannot provide a root handle"
        );
    }
    assert!(exhausted.origins().is_empty());
    assert_eq!(exhausted.selector_writes().len(), exit.origins().len());
    for source in exhausted.root_sources() {
        let write = exhausted
            .selector_writes()
            .iter()
            .find(|write| write.node() == source.node())
            .unwrap();
        assert_eq!(write.condition(), source.condition());
    }
    assert_eq!(
        exhausted.values()[0].condition(),
        header.availability_condition()
    );
    let active = exhausted
        .root_sources()
        .iter()
        .filter(|root| {
            selected(&planner.conditions, root.condition(), &choices)
                && root.node() == instance_nodes[&values[&root.source()]]
        })
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 1);
    let two_round_node = active[0].node();
    let (source, current) = replay_captured_edge_presence(
        &planner.conditions,
        graph,
        exit,
        exhaustion,
        exit.owner(),
        ReplayInstances {
            values: &values,
            nodes: &instance_nodes,
            captured: &captured_slots,
        },
        &mut choices,
    );
    assert_eq!(source, active[0].source());
    assert_eq!(values.remove(&source), Some(current));
    assert!(values.insert(exit.owner(), current).is_none());
    let root_drops = planner
        .facts
        .iter()
        .filter(|fact| fact.target() == DropTarget::Named(header.symbol()))
        .collect::<Vec<_>>();
    assert_eq!(root_drops.len(), 1);
    let root_drop = *root_drops[0];
    assert_eq!(root_drop.owner(), Some(exit.owner()));
    assert_eq!(root_drop.condition(), Some(exit.availability_condition()));
    let release = planner
        .cleanup
        .iter()
        .find(|(point, action)| {
            *point == root_drop.point()
                && matches!(action, IterationCleanupAction::ReleaseClosureInstances {
                        layout: ClosureReleaseLayout::Iteration(release_statement),
                        root,
                    } if *release_statement == statement && *root == root_drop)
        })
        .unwrap()
        .1;
    let IterationCleanupAction::ReleaseClosureInstances {
        layout: ClosureReleaseLayout::Iteration(release_statement),
        root: release_root,
    } = release
    else {
        unreachable!()
    };
    assert_eq!(release_statement, statement);
    assert_eq!(release_root, root_drop);
    assert!(!planner.cleanup.iter().any(|(_, action)| {
        matches!(action, IterationCleanupAction::Drop(fact) if *fact == root_drop)
    }));
    let DropPoint::CallReturn(call) = root_drop.point() else {
        panic!("final f call must release the carried root")
    };
    assert_eq!(
        sources
            .slice(parsed.ast().expressions().get(call).unwrap().span())
            .unwrap(),
        "f()"
    );
    let Some(CleanupOwnerValue::Closure {
        expression: initial_closure,
        ..
    }) = planner.conditions.owner_value(entry.values()[0].source())
    else {
        panic!("entry must come from the initial closure formation")
    };
    let recursive_node = *header
        .root_nodes()
        .iter()
        .find(|&&node| graph.nodes()[node].closure() == recursive_closure)
        .unwrap();
    let initial_node = *header
        .root_nodes()
        .iter()
        .find(|&&node| graph.nodes()[node].closure() == *initial_closure)
        .unwrap();
    assert_ne!(recursive_node, initial_node);
    let captured_edge = graph.nodes()[recursive_node]
        .sources()
        .iter()
        .find(|source| source.capture().source() == input.source())
        .unwrap();
    assert_eq!(captured_edge.position(), capture_slot.position());
    assert_eq!(instance_nodes[&seed], initial_node);
    assert_eq!(instance_nodes[&rounds[0]], recursive_node);
    assert_eq!(instance_nodes[&rounds[1]], recursive_node);
    assert_eq!(entry_root.node(), initial_node);
    assert_eq!(backedge.root_sources()[0].node(), recursive_node);
    assert_eq!(two_round_node, recursive_node);
    // 零轮沿 Exhaustion 根转发，再用已记录的根 drop 释放入口实例。
    let mut zero_round = zero_round;
    let active = exhausted
        .root_sources()
        .iter()
        .filter(|root| {
            selected(&planner.conditions, root.condition(), &zero_choices)
                && root.node() == instance_nodes[&zero_round[&root.source()]]
        })
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].node(), initial_node);
    let (source, zero_root) = replay_captured_edge_presence(
        &planner.conditions,
        graph,
        exit,
        exhaustion,
        exit.owner(),
        ReplayInstances {
            values: &zero_round,
            nodes: &instance_nodes,
            captured: &BTreeMap::new(),
        },
        &mut zero_choices,
    );
    assert_eq!(source, active[0].source());
    assert_eq!(zero_round.remove(&source), Some(zero_root));
    assert_eq!(zero_root, seed);
    assert!(zero_round.insert(exhausted.target(), zero_root).is_none());
    let zero_root = zero_round.remove(&release_root.owner().unwrap()).unwrap();
    assert_eq!(
        replay_owned_closure_release(graph, &instance_nodes, &mut BTreeMap::new(), zero_root),
        [seed]
    );
    assert!(zero_round.is_empty());
    // 动作只给出根与图身份；逐实例释放仍由测试从已形成实例的槽模拟。
    let root_instance = values.remove(&release_root.owner().unwrap()).unwrap();
    let released = replay_owned_closure_release(
        &planner.loop_capture_graphs[&release_statement.index()],
        &instance_nodes,
        &mut captured_slots,
        root_instance,
    );
    assert_eq!(released, [seed, rounds[0], rounds[1]]);
    assert!(captured_slots.is_empty());
}
