use super::*;

#[test]
fn snapshot_keeps_distinct_same_lambda_phi_and_ordinary_roots() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-overlapping-roots.ko",
            "fun run(first: List<Boolean>, second: List<Int>, pick: Boolean) {
var f: move () -> Unit = move {}
var g: move () -> Unit = move {}
for (recurse in first) {
    val next: move () -> Unit = move {}
    f = move { f() }
    if (recurse) { g = next } else { f = next }
}
var h: move () -> Unit = if (pick) f else g
for (_ in second) { h = move { h() } }
val used = h() }",
        )
        .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
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
    assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
    let liveness = super::super::super::liveness::Liveness::build(&checker).unwrap();
    let (origins, captures) = super::super::super::origins::analyze(&checker).unwrap();
    let mut planner = super::super::super::DropPlanner::new(&checker, liveness, origins, captures);
    for &root in parsed.roots() {
        planner.item(root).unwrap();
    }
    let candidate = planner.into_candidate_facts();
    assert_eq!(candidate.iterations.len(), 2);
    let plans = candidate
        .iterations
        .iter()
        .map(|plan| (plan.descriptor().statement().index(), plan))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(plans.len(), candidate.iterations.len());
    let table = &candidate.cleanup_conditions;
    let steps = &candidate.cleanup_steps;
    let statements = checker
        .iterations
        .values()
        .map(|plan| plan.descriptor().statement())
        .collect::<Vec<_>>();
    assert_eq!(statements.len(), 2);
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("f"))
        .unwrap()
        .id();
    let first_exit = plans[&statements[0].index()]
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == f)
        .unwrap();
    let g = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("g"))
        .unwrap()
        .id();
    let g_exit = plans[&statements[0].index()]
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == g)
        .unwrap();
    let h = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("h"))
        .unwrap()
        .id();
    let second_header = plans[&statements[1].index()]
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == h)
        .unwrap();
    let entry = plans[&statements[1].index()]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap()
        .bindings()
        .iter()
        .find(|binding| binding.target() == second_header.owner())
        .unwrap();
    let snapshot = entry.values()[0].source();
    let graph = plans[&statements[1].index()].capture_graph();
    let same_lambda = first_exit
        .root_origins()
        .find(|root| {
            g_exit
                .root_origins()
                .any(|other| other.closure() == root.closure())
        })
        .unwrap();
    let node = *second_header
        .root_nodes()
        .iter()
        .find(|&&node| graph.nodes()[node].closure() == same_lambda.closure())
        .unwrap();
    let same_node_sources = entry
        .root_sources()
        .iter()
        .filter(|source| source.node() == node && source.source() == snapshot)
        .collect::<Vec<_>>();
    assert_eq!(same_node_sources.len(), 2);
    assert_eq!(
        (*table).clone().and(
            same_node_sources[0].condition(),
            same_node_sources[1].condition(),
        ),
        CleanupConditionId::NEVER
    );
    let saved = table.owner_snapshot(snapshot).unwrap();
    assert_eq!(saved.value_inputs().len(), 2);
    assert!(
        saved
            .value_inputs()
            .iter()
            .any(|input| input.owner() == first_exit.owner())
    );
    assert!(
        saved
            .value_inputs()
            .iter()
            .any(|input| input.owner() == g_exit.owner())
    );
    let snapshot_point = DropPoint::AfterExpression(saved.value());
    let save_at = unique_action_index(steps, snapshot_point, |action| {
        matches!(action, IterationCleanupAction::SaveOwnerSnapshot { condition: None, owner, value }
                if *owner == snapshot && *value == saved.value())
    });
    let commit_at = unique_action_index(steps, snapshot_point, |action| {
        matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                if *owner == snapshot && *target == h)
    });
    assert!(save_at < commit_at);
    let pick_selector = saved
        .copies()
        .iter()
        .map(|copy| copy.source())
        .find(|&selector| {
            table.selector(selector).is_some_and(|selector| {
                sources
                    .slice(selector.origin())
                    .is_ok_and(|text| text.contains("pick"))
            })
        })
        .unwrap();
    let f_root = same_lambda;
    let g_root = g_exit
        .root_origins()
        .find(|root| root.closure() == same_lambda.closure())
        .unwrap();
    let edge = plans[&statements[1].index()]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let first_phis = plans[&statements[0].index()].closure_phis();
    let header = |symbol| {
        first_phis
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == symbol)
            .unwrap()
    };
    let (f_header, g_header) = (header(f), header(g));
    let first_edges = plans[&statements[0].index()].closure_phi_incomings();
    let first_edge = |kind| first_edges.iter().find(|edge| edge.kind() == kind).unwrap();
    fn binding(
        edge: &crate::ownership_checking::IterationPhiIncoming,
        owner: CleanupOwnerValueId,
    ) -> &IterationPhiIncomingBinding {
        edge.bindings()
            .iter()
            .find(|binding| binding.target() == owner)
            .unwrap()
    }
    let first_entry = first_edge(IterationPhiIncomingKind::Entry);
    let first_backedge = first_edge(IterationPhiIncomingKind::Fallthrough);
    let first_exhausted = first_edge(IterationPhiIncomingKind::Exhaustion);
    let initial = [f_header.owner(), g_header.owner()]
        .map(|owner| binding(first_entry, owner).values()[0].source());
    let f_back = binding(first_backedge, f_header.owner());
    let g_back = binding(first_backedge, g_header.owner());
    fn unique_action_index(
        actions: &[(DropPoint, IterationCleanupAction)],
        at: DropPoint,
        matches: impl Fn(&IterationCleanupAction) -> bool,
    ) -> usize {
        let found = actions
            .iter()
            .enumerate()
            .filter_map(|(index, (point, action))| matches(action).then_some((index, *point)))
            .collect::<Vec<_>>();
        let [(index, point)] = found.as_slice() else {
            panic!("one matching action must exist for {at:?}")
        };
        assert_eq!(*point, at);
        *index
    }
    fn assert_no_root_cleanup_at_edge(
        steps: &[(DropPoint, IterationCleanupAction)],
        table: &CleanupConditions,
        point: DropPoint,
        choices: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    ) {
        let unexpected = steps
            .iter()
            .filter_map(|(at, action)| {
                let root = match action {
                    IterationCleanupAction::Drop(root)
                    | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                    _ => return None,
                };
                (*at == point
                    && root
                        .condition()
                        .is_none_or(|condition| selected(table, condition, choices)))
                .then_some(*action)
            })
            .collect::<Vec<_>>();
        assert!(
            unexpected.is_empty(),
            "root cleanup during edge transport at {point:?}: {unexpected:?}"
        );
    }
    let created_closure = |wanted| {
        let matches = steps
            .iter()
            .filter_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *owner == wanted =>
                {
                    Some(*closure)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [closure] = matches.as_slice() else {
            return None;
        };
        Some(*closure)
    };
    let recursive_snapshot = f_back
        .values()
        .iter()
        .map(|value| value.source())
        .find(|&owner| {
            let source = table.owner_snapshot(owner).unwrap().value_inputs()[0].owner();
            created_closure(source).is_some_and(|closure| {
                sources.slice(parsed.ast().expressions().get(closure).unwrap().span())
                    == Ok("move { f() }")
            })
        })
        .unwrap();
    let f_branch_snapshot = f_back
        .values()
        .iter()
        .map(|value| value.source())
        .find(|&owner| owner != recursive_snapshot)
        .unwrap();
    let g_branch_snapshot = g_back
        .values()
        .iter()
        .map(|value| value.source())
        .find(|&owner| owner != g_header.owner())
        .unwrap();
    let next_snapshot = table
        .owner_snapshot(f_branch_snapshot)
        .unwrap()
        .value_inputs()[0]
        .owner();
    assert_eq!(
        table
            .owner_snapshot(g_branch_snapshot)
            .unwrap()
            .value_inputs()[0]
            .owner(),
        next_snapshot
    );
    let next_owner = table.owner_snapshot(next_snapshot).unwrap().value_inputs()[0].owner();
    let recursive_owner = table
        .owner_snapshot(recursive_snapshot)
        .unwrap()
        .value_inputs()[0]
        .owner();
    for owner in initial {
        let closure = created_closure(owner).unwrap();
        unique_action_index(steps, DropPoint::AfterExpression(closure), |action| {
            matches!(action, IterationCleanupAction::CreateClosureOwner { owner: created, closure: formed }
                    if *created == owner && *formed == closure)
        });
    }
    let next_closure = created_closure(next_owner).unwrap();
    let next_point = DropPoint::AfterExpression(next_closure);
    let next_create = unique_action_index(steps, next_point, |action| {
        matches!(action, IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *owner == next_owner && *closure == next_closure)
    });
    let next_save = unique_action_index(steps, next_point, |action| {
        matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                if *owner == next_snapshot && *value == next_closure)
    });
    let next_commit = unique_action_index(steps, next_point, |action| {
        matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                if *owner == next_snapshot && sources.slice(names.symbols()[target.index()].span()) == Ok("next"))
    });
    assert!(next_create < next_save && next_save < next_commit);
    let recursive_closure = created_closure(recursive_owner).unwrap();
    let recursive_point = DropPoint::AfterExpression(recursive_closure);
    let recursive_create = unique_action_index(steps, recursive_point, |action| {
        matches!(action, IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *owner == recursive_owner && *closure == recursive_closure)
    });
    let recursive_capture = unique_action_index(steps, recursive_point, |action| {
        matches!(action, IterationCleanupAction::SaveClosureCapture { owner, .. }
                if *owner == recursive_owner)
    });
    let recursive_save = unique_action_index(steps, recursive_point, |action| {
        matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                if *owner == recursive_snapshot && *value == recursive_closure)
    });
    let recursive_commit = unique_action_index(steps, recursive_point, |action| {
        matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                if *owner == recursive_snapshot && *target == f)
    });
    assert!(
        recursive_create < recursive_capture
            && recursive_capture < recursive_save
            && recursive_save < recursive_commit
    );
    let IterationCleanupAction::SaveClosureCapture {
        target: slot,
        input,
        ..
    } = steps[recursive_capture].1
    else {
        unreachable!()
    };
    let first_graph = plans[&statements[0].index()].capture_graph();
    let first_nodes = |instances: &BTreeMap<usize, usize>| {
        instances
            .iter()
            .map(|(&instance, &node)| {
                let closure = graph.nodes()[node].closure();
                let first_node = first_graph
                    .nodes()
                    .iter()
                    .position(|candidate| candidate.closure() == closure)
                    .unwrap();
                (instance, first_node)
            })
            .collect::<BTreeMap<_, _>>()
    };
    let mut next_instance = 0;
    let mut handles = BTreeMap::new();
    let mut instances = BTreeMap::new();
    let mut form = |owner, handles: &mut BTreeMap<_, _>, instances: &mut BTreeMap<_, _>| {
        let closure = created_closure(owner).unwrap();
        next_instance += 1;
        let node = graph
            .nodes()
            .iter()
            .position(|candidate| candidate.closure() == closure)
            .unwrap();
        assert!(instances.insert(next_instance, node).is_none());
        assert!(handles.insert(owner, next_instance).is_none());
        next_instance
    };
    let mut choices = table
        .nodes()
        .iter()
        .filter_map(|node| match node {
            CleanupCondition::Choice { selector, .. } => Some((*selector, 0)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    for owner in initial {
        form(owner, &mut handles, &mut instances);
    }
    let mut captured = BTreeMap::new();
    assert_no_root_cleanup_at_edge(steps, table, first_entry.point(), &choices);
    let entry_values = replay_captured_edge(
        table,
        first_graph,
        first_phis,
        first_entry,
        ReplayInstances {
            values: &handles,
            nodes: &first_nodes(&instances),
            captured: &captured,
        },
        &mut choices,
    );
    assert_eq!(entry_values.len(), 2);
    for (target, (source, instance)) in entry_values {
        assert_eq!(handles.remove(&source), Some(instance));
        assert!(handles.insert(target, instance).is_none());
    }
    let branch_condition =
        |owner| table.owner_snapshot(owner).unwrap().value_inputs()[0].condition();
    let selected_loop_actions = |at, aliases: &BTreeSet<_>, choices: &BTreeMap<_, _>| {
        steps
                .iter()
                .filter_map(|(point, action)| {
                    let root = match action {
                        IterationCleanupAction::Drop(root)
                        | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                        _ => return None,
                    };
                    (*point == at
                        && (matches!(root.target(), DropTarget::Named(symbol) if symbol == f || symbol == g)
                            || root.owner().is_some_and(|owner| aliases.contains(&owner))
                            || root
                                .instance_address()
                                .and_then(|address| table.instance_address(address))
                                .is_some_and(|address| aliases.contains(&address.root())))
                        && root
                            .condition()
                            .is_none_or(|condition| selected(table, condition, choices)))
                    .then_some((*point, *action))
                })
                .collect::<Vec<_>>()
    };
    let mut released = BTreeSet::new();
    // 首轮 g 接收 next，次轮 f 接收同一 lambda 的新实例；递归旧 f 链在次轮替换时释放。
    for recurse in [0, 1] {
        let next_instance = form(next_owner, &mut handles, &mut instances);
        assert_eq!(handles.remove(&next_owner), Some(next_instance));
        assert!(handles.insert(next_snapshot, next_instance).is_none());
        replay_snapshot_choices(table, next_snapshot, &mut choices);

        let recursive_instance = form(recursive_owner, &mut handles, &mut instances);
        assert_eq!(input.value(), CleanupCaptureValue::Owner(f_header.owner()));
        assert_eq!(
            (input.mode(), input.effect()),
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
        );
        assert!(selected(table, input.condition(), &choices));
        let position = table.capture_slot_value(slot).unwrap().position();
        let previous = handles.remove(&f_header.owner()).unwrap();
        let nodes = first_nodes(&instances);
        assert!(
            first_graph.nodes()[nodes[&recursive_instance]]
                .sources()
                .iter()
                .any(|source| source.position() == position
                    && source.captured().contains(&nodes[&previous]))
        );
        assert!(
            captured
                .insert((recursive_instance, position), previous)
                .is_none()
        );
        assert_eq!(handles.remove(&recursive_owner), Some(recursive_instance));
        assert!(
            handles
                .insert(recursive_snapshot, recursive_instance)
                .is_none()
        );
        replay_snapshot_choices(table, recursive_snapshot, &mut choices);

        let recurse_selectors = table
            .nodes()
            .iter()
            .filter_map(|node| match node {
                CleanupCondition::Choice { selector, .. }
                    if table.selector(*selector).is_some_and(|selector| {
                        sources
                            .slice(selector.origin())
                            .is_ok_and(|text| text.contains("recurse"))
                    }) =>
                {
                    Some(*selector)
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let matching = recurse_selectors
            .into_iter()
            .filter(|selector| {
                let mut yes = choices.clone();
                yes.insert(*selector, 0);
                let mut no = choices.clone();
                no.insert(*selector, 1);
                selected(table, branch_condition(g_branch_snapshot), &yes)
                    && !selected(table, branch_condition(g_branch_snapshot), &no)
                    && !selected(table, branch_condition(f_branch_snapshot), &yes)
                    && selected(table, branch_condition(f_branch_snapshot), &no)
            })
            .collect::<Vec<_>>();
        let [recurse_selector] = matching.as_slice() else {
            panic!("one evaluated recurse selector must choose the assignment")
        };
        choices.insert(*recurse_selector, recurse);
        let branch_snapshot = if recurse == 0 {
            g_branch_snapshot
        } else {
            f_branch_snapshot
        };
        assert!(selected(table, branch_condition(branch_snapshot), &choices));
        assert_eq!(
            table
                .owner_snapshot(branch_snapshot)
                .unwrap()
                .value_inputs()[0]
                .owner(),
            next_snapshot
        );
        let next_instance = handles.remove(&next_snapshot).unwrap();
        assert!(handles.insert(branch_snapshot, next_instance).is_none());
        replay_snapshot_choices(table, branch_snapshot, &mut choices);
        let replaced_symbol = if recurse == 0 { g } else { f };
        let point =
            DropPoint::AfterExpression(table.owner_snapshot(branch_snapshot).unwrap().value());
        let aliases = BTreeSet::from([
            f_header.owner(),
            g_header.owner(),
            recursive_owner,
            recursive_snapshot,
            next_snapshot,
            branch_snapshot,
        ]);
        let actions = selected_loop_actions(point, &aliases, &choices);
        let [(actual_point, action)] = actions.as_slice() else {
            panic!("one replaced binding must be cleaned at its assignment")
        };
        assert_eq!(*actual_point, point);
        let branch_save = unique_action_index(steps, point, |action| {
            matches!(action, IterationCleanupAction::SaveOwnerSnapshot { owner, .. }
                    if *owner == branch_snapshot)
        });
        let branch_cleanup = unique_action_index(steps, point, |candidate| candidate == action);
        let branch_commit = unique_action_index(steps, point, |action| {
            matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *owner == branch_snapshot && *target == replaced_symbol)
        });
        assert!(branch_save < branch_cleanup && branch_cleanup < branch_commit);
        match (recurse, action) {
            (0, IterationCleanupAction::Drop(root)) => {
                assert_eq!(root.owner(), Some(g_header.owner()));
            }
            (1, IterationCleanupAction::ReleaseClosureInstances { layout, root }) => {
                assert_eq!(*layout, ClosureReleaseLayout::Iteration(statements[0]));
                assert_eq!(root.owner(), Some(recursive_owner));
            }
            other => panic!("unexpected replacement cleanup: {other:?}"),
        }
        if recurse == 0 {
            assert!(released.insert(handles.remove(&g_header.owner()).unwrap()));
        } else {
            let root = handles.remove(&recursive_snapshot).unwrap();
            for instance in replay_owned_closure_release(
                first_graph,
                &first_nodes(&instances),
                &mut captured,
                root,
            ) {
                assert!(released.insert(instance));
            }
        }
        assert_no_root_cleanup_at_edge(steps, table, first_backedge.point(), &choices);
        let transported = replay_captured_edge(
            table,
            first_graph,
            first_phis,
            first_backedge,
            ReplayInstances {
                values: &handles,
                nodes: &first_nodes(&instances),
                captured: &captured,
            },
            &mut choices,
        );
        assert_eq!(transported.len(), 2);
        for (target, (source, instance)) in transported {
            assert_eq!(handles.remove(&source), Some(instance));
            assert!(handles.insert(target, instance).is_none());
        }
    }
    assert_eq!(released.len(), 4);
    assert!(captured.is_empty());
    assert_no_root_cleanup_at_edge(steps, table, first_exhausted.point(), &choices);
    let exit_values = replay_captured_edge(
        table,
        first_graph,
        first_phis,
        first_exhausted,
        ReplayInstances {
            values: &handles,
            nodes: &first_nodes(&instances),
            captured: &captured,
        },
        &mut choices,
    );
    assert_eq!(exit_values.len(), 2);
    for (target, (source, instance)) in exit_values {
        assert_eq!(handles.remove(&source), Some(instance));
        assert!(handles.insert(target, instance).is_none());
    }
    let exit_roots = BTreeSet::from([first_exit.owner(), g_exit.owner()]);
    assert!(
        selected_loop_actions(
            DropPoint::AfterStatement(statements[0]),
            &exit_roots,
            &choices,
        )
        .is_empty(),
        "both exit roots must survive until the pick snapshot"
    );
    assert_ne!(handles[&first_exit.owner()], handles[&g_exit.owner()]);
    assert_eq!(instances[&handles[&first_exit.owner()]], node);
    assert_eq!(instances[&handles[&g_exit.owner()]], node);
    assert_eq!(choices[&first_exit.availability_selector()], 1);
    assert_eq!(choices[&g_exit.availability_selector()], 1);
    assert_eq!(choices[&f_root.selector()], 1);
    assert_eq!(choices[&g_root.selector()], 1);
    let mut selected_instances = Vec::new();
    let mut selected_sources = Vec::new();
    for (pick, expects_f) in [(0, true), (1, false)] {
        let mut choices = choices.clone();
        let mut handles = handles.clone();
        let mut captured = captured.clone();
        let mut released = released.clone();
        choices.insert(pick_selector, pick);
        assert!(
            selected_loop_actions(snapshot_point, &exit_roots, &choices).is_empty(),
            "the pick snapshot must not clean either exit root"
        );
        let selected_values = saved
            .value_inputs()
            .iter()
            .filter(|input| selected(table, input.condition(), &choices))
            .collect::<Vec<_>>();
        let [selected_value] = selected_values.as_slice() else {
            panic!("the snapshot must read exactly one old root handle")
        };
        assert_eq!(
            selected_value.owner(),
            if expects_f {
                first_exit.owner()
            } else {
                g_exit.owner()
            }
        );
        let chosen = handles.remove(&selected_value.owner()).unwrap();
        let unselected_owner = if expects_f {
            g_exit.owner()
        } else {
            first_exit.owner()
        };
        let unselected = handles.remove(&unselected_owner).unwrap();
        let point = DropPoint::BranchExit {
            control: saved.value(),
            branch: pick,
        };
        let root_owners = BTreeSet::from([first_exit.owner(), g_exit.owner()]);
        let actions = steps
                .iter()
                .filter_map(|(at, action)| {
                    let root = match action {
                        IterationCleanupAction::Drop(root)
                        | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                        _ => return None,
                    };
                    (*at == point
                        && (matches!(root.target(), DropTarget::Named(symbol) if symbol == f || symbol == g)
                            || root.owner().is_some_and(|owner| root_owners.contains(&owner))
                            || root
                                .instance_address()
                                .and_then(|address| table.instance_address(address))
                                .is_some_and(|address| root_owners.contains(&address.root())))
                        && root.condition().is_none_or(|condition| selected(table, condition, &choices)))
                    .then_some((*at, *action))
                })
                .collect::<Vec<_>>();
        let [(_, action)] = actions.as_slice() else {
            panic!("only the unselected root may have branch cleanup")
        };
        let branch_release_at = unique_action_index(steps, point, |candidate| candidate == action);
        assert!(branch_release_at < save_at && save_at < commit_at);
        match (pick, action) {
            (0, IterationCleanupAction::Drop(root)) => {
                assert_eq!(root.owner(), Some(g_exit.owner()));
            }
            (1, IterationCleanupAction::ReleaseClosureInstances { layout, root }) => {
                assert_eq!(*layout, ClosureReleaseLayout::Iteration(statements[0]));
                assert_eq!(root.owner(), Some(first_exit.owner()));
            }
            other => panic!("unexpected snapshot cleanup: {other:?}"),
        }
        assert_ne!(chosen, unselected);
        for instance in replay_owned_closure_release(graph, &instances, &mut captured, unselected) {
            assert!(released.insert(instance));
        }
        replay_snapshot_choices(table, snapshot, &mut choices);
        let active = same_node_sources
            .iter()
            .filter(|source| selected(table, source.condition(), &choices))
            .collect::<Vec<_>>();
        assert_eq!(active.len(), 1);
        selected_sources.push(active[0].condition());
        assert!(handles.insert(snapshot, chosen).is_none());
        assert_no_root_cleanup_at_edge(steps, table, edge.point(), &choices);
        let transported = replay_captured_edge(
            table,
            graph,
            plans[&statements[1].index()].closure_phis(),
            edge,
            ReplayInstances {
                values: &handles,
                nodes: &instances,
                captured: &captured,
            },
            &mut choices,
        );
        assert_eq!(transported[&second_header.owner()], (snapshot, chosen));
        assert_eq!(handles.remove(&snapshot), Some(chosen));
        assert!(handles.insert(second_header.owner(), chosen).is_none());
        let selector = second_header
            .root_origins()
            .find(|root| root.node() == node)
            .unwrap()
            .selector();
        assert_eq!(choices[&selector], 1);
        selected_instances.push(chosen);
        let exhausted = plans[&statements[1].index()]
            .closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
            .unwrap();
        let h_exit = plans[&statements[1].index()]
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == h)
            .unwrap();
        assert_no_root_cleanup_at_edge(steps, table, exhausted.point(), &choices);
        let transported = replay_captured_edge(
            table,
            graph,
            plans[&statements[1].index()].closure_phis(),
            exhausted,
            ReplayInstances {
                values: &handles,
                nodes: &instances,
                captured: &captured,
            },
            &mut choices,
        );
        assert_eq!(
            transported[&h_exit.owner()],
            (second_header.owner(), chosen)
        );
        assert_eq!(handles.remove(&second_header.owner()), Some(chosen));
        assert!(handles.insert(h_exit.owner(), chosen).is_none());
        let aliases = BTreeSet::from([snapshot, second_header.owner(), h_exit.owner()]);
        let final_actions = steps
            .iter()
            .filter_map(|(point, action)| {
                let root = match action {
                    IterationCleanupAction::Drop(root)
                    | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                    _ => return None,
                };
                (root.target() == DropTarget::Named(h)
                    || root.owner().is_some_and(|owner| aliases.contains(&owner))
                    || root
                        .instance_address()
                        .and_then(|address| table.instance_address(address))
                        .is_some_and(|address| aliases.contains(&address.root())))
                .then_some((point, action, root))
            })
            .filter(|(_, _, root)| {
                root.condition()
                    .is_none_or(|condition| selected(table, condition, &choices))
            })
            .map(|(point, action, _)| (*point, *action))
            .collect::<Vec<_>>();
        let final_call = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, expression)| sources.slice(expression.span()) == Ok("h()"))
            .max_by_key(|(_, expression)| expression.span().start())
            .unwrap()
            .0;
        let [(point, IterationCleanupAction::ReleaseClosureInstances { layout, root })] =
            final_actions.as_slice()
        else {
            panic!("the selected h instance must release once at its call return")
        };
        assert_eq!(*point, DropPoint::CallReturn(final_call));
        assert_eq!(*layout, ClosureReleaseLayout::Iteration(statements[1]));
        assert_eq!(root.owner(), Some(h_exit.owner()));
        for instance in replay_owned_closure_release(
            graph,
            &instances,
            &mut captured,
            handles.remove(&h_exit.owner()).unwrap(),
        ) {
            assert!(released.insert(instance));
        }
        assert_eq!(released.len(), instances.len());
        assert!(handles.is_empty() && captured.is_empty());
    }
    assert_ne!(selected_instances[0], selected_instances[1]);
    assert_ne!(selected_sources[0], selected_sources[1]);
}
