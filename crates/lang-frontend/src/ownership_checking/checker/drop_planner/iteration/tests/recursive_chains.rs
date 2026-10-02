use super::*;

#[test]
fn alternating_recursive_capture_releases_the_formed_instance_chain() {
    let mut sources = SourceMap::new();
    let source = sources
            .add_source(
                "alternating-recursive-capture.ko",
                "fun run(flags: List<Int>) { var f: move () -> Unit = move {}\nvar g: move () -> Unit = move {}\nfor (_ in flags) { { f = move { g() } }\ng = move { f() } }\nval used = g() }",
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
    let graph = &planner.loop_capture_graphs[&statement.index()];
    let f_node = graph
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
            ) == Ok("move { g() }")
        })
        .unwrap();
    let g_node = graph
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
            ) == Ok("move { f() }")
        })
        .unwrap();
    assert!(
        graph.nodes()[f_node].sources()[0]
            .captured()
            .contains(&g_node)
    );
    assert!(
        graph.nodes()[g_node].sources()[0]
            .captured()
            .contains(&f_node)
    );
    for node in [f_node, g_node] {
        assert_eq!(
            graph.nodes()[node].sources()[0].capture().effect(),
            ClosureCaptureEffect::Move
        );
    }
    let ClosureCaptureSource::Symbol(f_symbol) =
        graph.nodes()[g_node].sources()[0].capture().source()
    else {
        panic!("g must capture f")
    };
    let ClosureCaptureSource::Symbol(g_symbol) =
        graph.nodes()[f_node].sources()[0].capture().source()
    else {
        panic!("f must capture g")
    };
    let phis = &planner.loop_phis[&statement.index()];
    let phi = |symbol, boundary| {
        phis.iter()
            .find(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
            .unwrap()
    };
    let f_header = phi(f_symbol, IterationPhiBoundary::Header);
    let g_header = phi(g_symbol, IterationPhiBoundary::Header);
    let g_exit = phi(g_symbol, IterationPhiBoundary::Exit);
    let incoming = |kind| {
        planner.loop_phi_incomings[&statement.index()]
            .iter()
            .find(|edge| edge.kind() == kind)
            .unwrap()
    };
    let binding = |kind, target| {
        incoming(kind)
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap()
    };
    let entry_g = binding(IterationPhiIncomingKind::Entry, g_header.owner());
    let back_g = binding(IterationPhiIncomingKind::Fallthrough, g_header.owner());
    let exhausted_g = binding(IterationPhiIncomingKind::Exhaustion, g_exit.owner());
    assert_eq!(entry_g.values().len(), 1);
    assert_eq!(back_g.values().len(), 1);
    assert_eq!(exhausted_g.values().len(), 1);
    assert_eq!(exhausted_g.values()[0].source(), g_header.owner());
    for kind in [
        IterationPhiIncomingKind::Entry,
        IterationPhiIncomingKind::Fallthrough,
    ] {
        let f = binding(kind, f_header.owner());
        assert!(f.values().is_empty(), "f is moved into g in every round");
        assert_eq!(f.available_when(), CleanupConditionId::NEVER);
        assert!(f.capture_slots_to_clear().is_empty());
    }
    let steps = &planner.cleanup;
    let created = |closure| {
        steps
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::CreateClosureOwner {
                    owner,
                    closure: formed,
                } if *formed == closure => Some((index, *owner)),
                _ => None,
            })
            .unwrap()
    };
    let (f_create_at, f_created) = created(graph.nodes()[f_node].closure());
    let (g_create_at, g_created) = created(graph.nodes()[g_node].closure());
    let saved = |owner| {
        steps
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner: saved,
                    target,
                    input,
                } if *saved == owner => Some((index, *target, *input)),
                _ => None,
            })
            .unwrap()
    };
    let (f_save_at, f_slot, f_input) = saved(f_created);
    let (g_save_at, g_slot, g_input) = saved(g_created);
    let snapshot = |closure| {
        steps
            .iter()
            .enumerate()
            .find_map(|(index, (_, action))| match action {
                IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                    if *value == closure =>
                {
                    Some((index, *owner))
                }
                _ => None,
            })
            .unwrap()
    };
    let (f_snapshot_at, f_snapshot) = snapshot(graph.nodes()[f_node].closure());
    let (g_snapshot_at, g_snapshot) = snapshot(graph.nodes()[g_node].closure());
    let committed = |owner, symbol| {
        steps
            .iter()
            .position(|(_, action)| {
                matches!(action, IterationCleanupAction::CommitOwnerSnapshot {
                        owner: saved,
                        target,
                    } if *saved == owner && *target == symbol)
            })
            .unwrap()
    };
    let f_commit_at = committed(f_snapshot, f_symbol);
    let g_commit_at = committed(g_snapshot, g_symbol);
    assert_eq!(
        f_input.value(),
        CleanupCaptureValue::Owner(g_header.owner())
    );
    assert_eq!(g_input.value(), CleanupCaptureValue::Owner(f_snapshot));
    assert_eq!(back_g.values()[0].source(), g_snapshot);
    assert_eq!(
        (f_input.mode(), f_input.effect()),
        (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
    );
    assert_eq!(
        (g_input.mode(), g_input.effect()),
        (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
    );
    let f_layout = planner.conditions.capture_slot_value(f_slot).unwrap();
    let g_layout = planner.conditions.capture_slot_value(g_slot).unwrap();
    assert_eq!(
        (
            f_layout.environment(),
            f_layout.closure(),
            f_layout.source()
        ),
        (f_created, graph.nodes()[f_node].closure(), f_input.source())
    );
    assert_eq!(
        (
            g_layout.environment(),
            g_layout.closure(),
            g_layout.source()
        ),
        (g_created, graph.nodes()[g_node].closure(), g_input.source())
    );
    let f_position = f_layout.position();
    let g_position = g_layout.position();
    assert_eq!((f_position, g_position), (0, 0));
    assert!(f_create_at < f_save_at && f_save_at < f_snapshot_at && f_snapshot_at < f_commit_at);
    assert!(
        f_commit_at < g_create_at
            && g_create_at < g_save_at
            && g_save_at < g_snapshot_at
            && g_snapshot_at < g_commit_at
    );
    for (owner, formed) in [(f_snapshot, f_created), (g_snapshot, g_created)] {
        let inputs = planner
            .conditions
            .owner_snapshot(owner)
            .unwrap()
            .capture_inputs();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].owner(), formed);
    }
    let initial_owner = entry_g.values()[0].source();
    let initial_create = steps
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *owner == initial_owner =>
            {
                Some(*closure)
            }
            _ => None,
        })
        .unwrap();
    let mut next_instance = 0;
    let mut owners = BTreeMap::new();
    let mut instance_nodes = BTreeMap::new();
    let mut create = |owner, closure, owners: &mut BTreeMap<_, _>, nodes: &mut BTreeMap<_, _>| {
        next_instance += 1;
        let node = graph
            .nodes()
            .iter()
            .position(|node| node.closure() == closure)
            .unwrap();
        assert!(owners.insert(owner, next_instance).is_none());
        assert!(nodes.insert(next_instance, node).is_none());
        next_instance
    };
    let initial_g = create(
        initial_owner,
        initial_create,
        &mut owners,
        &mut instance_nodes,
    );
    let mut choices = BTreeMap::new();
    let mut captured = BTreeMap::new();
    let entry_values = replay_captured_edge(
        &planner.conditions,
        graph,
        phis,
        incoming(IterationPhiIncomingKind::Entry),
        ReplayInstances {
            values: &owners,
            nodes: &instance_nodes,
            captured: &captured,
        },
        &mut choices,
    );
    assert_eq!(entry_values.len(), 1);
    assert_eq!(entry_values[&g_header.owner()], (initial_owner, initial_g));
    assert_eq!(choices[&f_header.availability_selector()], 0);
    assert_eq!(choices[&g_header.availability_selector()], 1);
    assert_eq!(owners.remove(&initial_owner), Some(initial_g));
    assert!(owners.insert(g_header.owner(), initial_g).is_none());
    let zero_round = owners.clone();
    let zero_choices = choices.clone();
    let mut rounds = Vec::new();
    let mut one_round = None;
    let f_drops = steps
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::Drop(fact)
                if fact.owner() == Some(f_header.owner())
                    && fact.target() == DropTarget::Named(f_symbol) =>
            {
                Some(*fact)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(f_drops.len(), 2);
    let f_after_expression = f_drops
        .iter()
        .find(|fact| matches!(fact.point(), DropPoint::AfterExpression(_)))
        .unwrap();
    let f_at_exit = f_drops
        .iter()
        .find(|fact| fact.point() == DropPoint::LoopExit(statement))
        .unwrap();
    for _ in 0..2 {
        assert!(selected(&planner.conditions, f_input.condition(), &choices));
        let f = create(
            f_created,
            graph.nodes()[f_node].closure(),
            &mut owners,
            &mut instance_nodes,
        );
        let prior_g = owners.remove(&g_header.owner()).unwrap();
        assert!(captured.insert((f, f_position), prior_g).is_none());
        assert_eq!(owners.remove(&f_created), Some(f));
        assert!(owners.insert(f_snapshot, f).is_none());
        replay_snapshot_choices(&planner.conditions, f_snapshot, &mut choices);
        assert!(!selected(
            &planner.conditions,
            f_after_expression
                .condition()
                .unwrap_or(CleanupConditionId::ALWAYS),
            &choices
        ));
        let g = create(
            g_created,
            graph.nodes()[g_node].closure(),
            &mut owners,
            &mut instance_nodes,
        );
        assert!(selected(&planner.conditions, g_input.condition(), &choices));
        assert_eq!(owners.remove(&f_snapshot), Some(f));
        assert!(captured.insert((g, g_position), f).is_none());
        assert_eq!(owners.remove(&g_created), Some(g));
        assert!(owners.insert(g_snapshot, g).is_none());
        replay_snapshot_choices(&planner.conditions, g_snapshot, &mut choices);
        let back_values = replay_captured_edge(
            &planner.conditions,
            graph,
            phis,
            incoming(IterationPhiIncomingKind::Fallthrough),
            ReplayInstances {
                values: &owners,
                nodes: &instance_nodes,
                captured: &captured,
            },
            &mut choices,
        );
        assert_eq!(back_values.len(), 1);
        assert_eq!(back_values[&g_header.owner()], (g_snapshot, g));
        assert_eq!(choices[&f_header.availability_selector()], 0);
        assert_eq!(choices[&g_header.availability_selector()], 1);
        assert_eq!(owners.remove(&back_g.values()[0].source()), Some(g));
        assert!(owners.insert(back_g.target(), g).is_none());
        rounds.extend([f, g]);
        if one_round.is_none() {
            one_round = Some((
                owners.clone(),
                captured.clone(),
                instance_nodes.clone(),
                choices.clone(),
            ));
        }
    }
    let releases = steps
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::ReleaseClosureInstances {
                layout: ClosureReleaseLayout::Iteration(loop_id),
                root,
            } if *loop_id == statement && root.owner() == Some(g_exit.owner()) => Some(*root),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [release] = releases.as_slice() else {
        panic!("one exit root release is required: {releases:?}")
    };
    assert_eq!(release.target(), DropTarget::Named(g_symbol));
    let DropPoint::CallReturn(call) = release.point() else {
        panic!("the final g call must release its environment")
    };
    assert_eq!(
        sources.slice(parsed.ast().expressions().get(call).unwrap().span()),
        Ok("g()")
    );
    let exit_root = |mut owners: BTreeMap<CleanupOwnerValueId, usize>,
                     mut captured: BTreeMap<(usize, usize), usize>,
                     nodes: &BTreeMap<usize, usize>,
                     mut choices: BTreeMap<_, _>| {
        assert!(!selected(
            &planner.conditions,
            f_at_exit.condition().unwrap_or(CleanupConditionId::ALWAYS),
            &choices
        ));
        let exit_values = replay_captured_edge(
            &planner.conditions,
            graph,
            phis,
            incoming(IterationPhiIncomingKind::Exhaustion),
            ReplayInstances {
                values: &owners,
                nodes,
                captured: &captured,
            },
            &mut choices,
        );
        assert_eq!(exit_values.len(), 1);
        let (source, instance) = exit_values[&exhausted_g.target()];
        assert_eq!(source, exhausted_g.values()[0].source());
        assert_eq!(owners.remove(&source), Some(instance));
        assert!(owners.insert(exhausted_g.target(), instance).is_none());
        let active_root_actions = steps
            .iter()
            .filter_map(|(point, action)| {
                let fact = match action {
                    IterationCleanupAction::Drop(fact)
                    | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => fact,
                    _ => return None,
                };
                let from_exit = fact.owner() == Some(g_exit.owner())
                    || fact.instance_address().is_some_and(|address| {
                        planner.conditions.instance_address(address).unwrap().root()
                            == g_exit.owner()
                    })
                    || (*point == release.point() && fact.target() == DropTarget::Named(g_symbol));
                (from_exit
                    && fact
                        .condition()
                        .is_none_or(|guard| selected(&planner.conditions, guard, &choices)))
                .then_some((*point, *action))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            active_root_actions,
            [(
                release.point(),
                IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(statement),
                    root: *release,
                },
            )]
        );
        let instance = owners.remove(&release.owner().unwrap()).unwrap();
        let released = replay_owned_closure_release(graph, nodes, &mut captured, instance);
        assert!(captured.is_empty());
        assert!(owners.is_empty());
        released
    };
    assert_eq!(
        exit_root(zero_round, BTreeMap::new(), &instance_nodes, zero_choices),
        [initial_g]
    );
    let (one_owners, one_captured, one_nodes, one_choices) = one_round.unwrap();
    assert_eq!(
        exit_root(one_owners, one_captured, &one_nodes, one_choices),
        [initial_g, rounds[0], rounds[1]]
    );
    assert_eq!(
        exit_root(owners, captured, &instance_nodes, choices),
        [initial_g, rounds[0], rounds[1], rounds[2], rounds[3]]
    );
}

#[test]
fn recursive_return_releases_the_formed_chain_once() {
    let mut sources = SourceMap::new();
    let source = sources
            .add_source(
                "recursive-return.ko",
                "fun run(flags: List<Boolean>) { var f: move () -> Unit = move {}\nfor (flag in flags) { f = move { f() }\nif (flag) { return } }\nval used = f() }",
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
    let graph = &planner.loop_capture_graphs[&statement.index()];
    let recursive_node = graph
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
            ) == Ok("move { f() }")
        })
        .unwrap();
    let recursive = graph.nodes()[recursive_node].closure();
    let formed = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *closure == recursive =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    let (slot, input) = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } if *owner == formed => Some((*target, *input)),
            _ => None,
        })
        .unwrap();
    let header = planner.loop_phis[&statement.index()]
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
        .unwrap();
    assert_eq!(input.value(), CleanupCaptureValue::Owner(header.owner()));
    let entry = planner.loop_phi_incomings[&statement.index()]
        .iter()
        .find(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let root = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == header.owner())
        .unwrap();
    let [initial] = root.root_sources() else {
        panic!("entry must carry the initial environment")
    };
    let return_expression = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("return")).then_some(id))
        .unwrap();
    let release_point = DropPoint::ControlTransfer(return_expression);
    let releases = planner
        .cleanup
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::ReleaseClosureInstances {
                layout: ClosureReleaseLayout::Iteration(owner),
                root,
            } if *point == release_point && *owner == statement => Some(*root),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [release] = releases.as_slice() else {
        panic!("return must release exactly one recursive root: {releases:?}")
    };
    // 形成事实的 owner 标识新环境；return 从已提交的具名 f 交付当次句柄。
    assert_eq!(release.owner(), Some(formed));
    assert_eq!(release.target(), DropTarget::Named(header.symbol()));
    let snapshot = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                if *value == recursive =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    let mut choices = BTreeMap::new();
    let mut owners = BTreeMap::from([(initial.source(), 1_usize)]);
    let mut nodes = BTreeMap::from([(1, initial.node())]);
    let mut captured = BTreeMap::new();
    let (source, old) = replay_captured_edge_presence(
        &planner.conditions,
        graph,
        header,
        entry,
        header.owner(),
        ReplayInstances {
            values: &owners,
            nodes: &nodes,
            captured: &captured,
        },
        &mut choices,
    );
    assert_eq!(source, initial.source());
    assert_eq!(owners.remove(&source), Some(old));
    assert!(owners.insert(header.owner(), old).is_none());
    let formation = planner
        .cleanup
        .iter()
        .filter(|(point, action)| {
            *point == DropPoint::AfterExpression(recursive)
                && matches!(
                    action,
                    IterationCleanupAction::CreateClosureOwner { .. }
                        | IterationCleanupAction::SaveClosureCapture { .. }
                        | IterationCleanupAction::SaveOwnerSnapshot { .. }
                        | IterationCleanupAction::CommitOwnerSnapshot { .. }
                )
        })
        .map(|(_, action)| *action)
        .collect::<Vec<_>>();
    assert_eq!(formation.len(), 4);
    for action in formation {
        match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure } => {
                assert_eq!((owner, closure), (formed, recursive));
                assert!(owners.insert(owner, 2).is_none());
                assert!(nodes.insert(2, recursive_node).is_none());
            }
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input: saved,
            } => {
                assert_eq!((owner, target, saved), (formed, slot, input));
                assert!(selected(&planner.conditions, saved.condition(), &choices));
                let CleanupCaptureValue::Owner(source) = saved.value() else {
                    panic!("recursive capture must read the old header instance")
                };
                let prior = owners.remove(&source).unwrap();
                let position = planner
                    .conditions
                    .capture_slot_value(target)
                    .unwrap()
                    .position();
                assert!(captured.insert((owners[&owner], position), prior).is_none());
            }
            IterationCleanupAction::SaveOwnerSnapshot {
                owner,
                value,
                condition,
            } => {
                assert_eq!((owner, value), (snapshot, recursive));
                assert!(
                    condition
                        .is_none_or(|guard| { selected(&planner.conditions, guard, &choices) })
                );
                let saved = planner.conditions.owner_snapshot(owner).unwrap();
                let [source] = saved.value_inputs() else {
                    panic!("snapshot must transport the formed instance")
                };
                assert_eq!(source.owner(), formed);
                assert!(selected(&planner.conditions, source.condition(), &choices));
                let instance = owners.remove(&source.owner()).unwrap();
                assert!(owners.insert(owner, instance).is_none());
                replay_snapshot_choices(&planner.conditions, owner, &mut choices);
            }
            IterationCleanupAction::CommitOwnerSnapshot { owner, target } => {
                assert_eq!((owner, target), (snapshot, header.symbol()));
                let instance = owners.remove(&owner).unwrap();
                assert!(owners.insert(header.owner(), instance).is_none());
            }
            _ => unreachable!("formation action filter"),
        }
    }
    let chain_owners = [initial.source(), header.owner(), formed, snapshot];
    assert!(
        !planner.cleanup.iter().any(|(point, action)| {
            if *point != DropPoint::AfterExpression(recursive) {
                return false;
            }
            let fact = match action {
                IterationCleanupAction::Drop(fact)
                | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => fact,
                _ => return false,
            };
            fact.owner()
                .is_some_and(|owner| chain_owners.contains(&owner))
                && fact
                    .condition()
                    .is_none_or(|guard| selected(&planner.conditions, guard, &choices))
        }),
        "forming the new environment must not release its old captured instance"
    );
    let return_control = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (sources.slice(node.span()) == Ok("if (flag) { return }")).then_some(id)
        })
        .unwrap();
    let mut probe = planner.conditions.clone();
    let condition = probe
        .branch(
            return_control,
            parsed
                .ast()
                .expressions()
                .get(return_control)
                .unwrap()
                .span(),
            2,
            0,
        )
        .unwrap();
    let Some(CleanupCondition::Choice { selector, .. }) = probe.get(condition) else {
        panic!("return branch must have a saved control selector")
    };
    choices.insert(*selector, 0);
    assert!(selected(
        &planner.conditions,
        release.condition().unwrap_or(CleanupConditionId::ALWAYS),
        &choices
    ));
    let active_releases = planner
        .cleanup
        .iter()
        .filter_map(|(point, action)| {
            if *point != release_point {
                return None;
            }
            let fact = match action {
                IterationCleanupAction::Drop(fact)
                | IterationCleanupAction::ReleaseClosureInstances { root: fact, .. } => fact,
                _ => return None,
            };
            let from_chain = fact
                .owner()
                .is_some_and(|owner| chain_owners.contains(&owner))
                || fact.instance_address().is_some_and(|address| {
                    chain_owners
                        .contains(&planner.conditions.instance_address(address).unwrap().root())
                });
            (from_chain
                && fact
                    .condition()
                    .is_none_or(|guard| selected(&planner.conditions, guard, &choices)))
            .then_some(*action)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        active_releases,
        [IterationCleanupAction::ReleaseClosureInstances {
            layout: ClosureReleaseLayout::Iteration(statement),
            root: *release,
        }]
    );
    assert_eq!(owners.remove(&header.owner()), Some(2));
    assert!(owners.is_empty());
    assert_eq!(
        replay_owned_closure_release(graph, &nodes, &mut captured, 2),
        [1, 2]
    );
    assert!(captured.is_empty());
}
