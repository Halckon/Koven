use super::*;

#[test]
fn snapshot_keeps_two_recursive_phi_roots_without_tree_origins() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-only-roots.ko",
            "fun run(first: List<Int>, second: List<Int>, pick: Boolean) {
var f: move () -> Unit = move {}
var g: move () -> Unit = move {}
for (_ in first) { { f = move { f() } }
g = move { g() } }
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
    let expected_actions = planner.cleanup.clone();
    let expected_conditions = planner.conditions.clone();
    let expected_drops = planner.facts.clone();
    let expected_loan_ends = planner.loan_ends.clone();
    let expected_graphs = planner.loop_capture_graphs.clone();
    let expected_phis = planner.loop_phis.clone();
    let expected_incomings = planner.loop_phi_incomings.clone();
    let candidate = planner.into_candidate_facts();
    assert_eq!(candidate.cleanup_steps, expected_actions);
    assert_eq!(candidate.cleanup_conditions, expected_conditions);
    assert_eq!(candidate.drops, expected_drops);
    assert_eq!(candidate.loan_ends, expected_loan_ends);
    assert_eq!(candidate.iterations.len(), expected_graphs.len());
    assert_eq!(candidate.iterations.len(), 2);
    let statements = candidate
        .iterations
        .iter()
        .map(|plan| plan.descriptor().statement().index())
        .collect::<BTreeSet<_>>();
    assert_eq!(statements.len(), candidate.iterations.len());
    assert_eq!(statements, expected_graphs.keys().copied().collect());
    for plan in &candidate.iterations {
        let statement = plan.descriptor().statement().index();
        assert_eq!(plan.capture_graph(), &expected_graphs[&statement]);
        assert_eq!(plan.closure_phis(), expected_phis[&statement]);
        assert_eq!(plan.closure_phi_incomings(), expected_incomings[&statement]);
        for kind in [
            IterationPhiIncomingKind::Entry,
            IterationPhiIncomingKind::Fallthrough,
            IterationPhiIncomingKind::Exhaustion,
        ] {
            assert!(
                plan.closure_phi_incomings()
                    .iter()
                    .any(|edge| edge.kind() == kind)
            );
        }
    }
    let plans = candidate
        .iterations
        .iter()
        .map(|plan| (plan.descriptor().statement().index(), plan))
        .collect::<BTreeMap<_, _>>();
    let table = &candidate.cleanup_conditions;
    let steps = &candidate.cleanup_steps;
    let second = checker
        .iterations
        .values()
        .map(|plan| plan.descriptor().statement())
        .next_back()
        .unwrap();
    let h = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("h"))
        .unwrap()
        .id();
    let h_header = plans[&second.index()]
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == h)
        .unwrap();
    let edge = plans[&second.index()]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let entry = edge
        .bindings()
        .iter()
        .find(|binding| binding.target() == h_header.owner())
        .unwrap();
    assert_eq!(entry.values().len(), 1);
    let snapshot = entry.values()[0].source();
    let saved = table.owner_snapshot(snapshot).unwrap();
    assert!(saved.capture_inputs().is_empty());
    assert_eq!(saved.value_inputs().len(), 2);
    let first = checker
        .iterations
        .values()
        .next()
        .unwrap()
        .descriptor()
        .statement();
    let expected_roots = ["f", "g"]
        .into_iter()
        .map(|name| {
            let symbol = names
                .symbols()
                .iter()
                .find(|symbol| sources.slice(symbol.span()) == Ok(name))
                .unwrap()
                .id();
            plans[&first.index()]
                .closure_phis()
                .iter()
                .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == symbol)
                .unwrap()
                .owner()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        saved
            .value_inputs()
            .iter()
            .map(|input| input.owner())
            .collect::<BTreeSet<_>>(),
        expected_roots.iter().copied().collect()
    );
    assert_eq!(
        (*table).clone().and(
            saved.value_inputs()[0].condition(),
            saved.value_inputs()[1].condition(),
        ),
        CleanupConditionId::NEVER
    );
    assert_eq!(entry.root_sources().len(), 4);
    assert!(entry.root_sources().iter().all(
        |source| source.source() == snapshot && h_header.root_nodes().contains(&source.node())
    ));
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
    let first_edges = plans[&first.index()].closure_phi_incomings();
    let first_edge = |kind| first_edges.iter().find(|edge| edge.kind() == kind).unwrap();
    let first_entry = first_edge(IterationPhiIncomingKind::Entry);
    let first_backedge = first_edge(IterationPhiIncomingKind::Fallthrough);
    let first_exhausted = first_edge(IterationPhiIncomingKind::Exhaustion);
    fn incoming(
        edge: &crate::ownership_checking::IterationPhiIncoming,
        target: CleanupOwnerValueId,
    ) -> &IterationPhiIncomingBinding {
        edge.bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap()
    }
    fn active_root(
        table: &CleanupConditions,
        binding: &IterationPhiIncomingBinding,
        choices: &BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
        handles: &BTreeMap<CleanupOwnerValueId, usize>,
        instance_nodes: &BTreeMap<usize, usize>,
    ) -> crate::ownership_checking::IterationPhiRootSource {
        let values = binding
            .values()
            .iter()
            .filter(|value| selected(table, value.condition(), choices))
            .collect::<Vec<_>>();
        let [value] = values.as_slice() else {
            panic!("one owner value must reach this phi binding")
        };
        let roots = binding
            .root_sources()
            .iter()
            .copied()
            .filter(|root| {
                root.source() == value.source()
                    && root.node() == instance_nodes[&handles[&value.source()]]
                    && selected(table, root.condition(), choices)
            })
            .collect::<Vec<_>>();
        let [root] = roots.as_slice() else {
            panic!("one actual root handle must reach this phi binding")
        };
        *root
    }
    let paths = saved
        .value_inputs()
        .iter()
        .map(|input| {
            let exit = plans[&first.index()]
                .closure_phis()
                .iter()
                .find(|phi| phi.owner() == input.owner())
                .unwrap();
            let header = plans[&first.index()]
                .closure_phis()
                .iter()
                .find(|phi| {
                    phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == exit.symbol()
                })
                .unwrap();
            let initial = incoming(first_entry, header.owner()).values()[0].source();
            let body_snapshot = incoming(first_backedge, header.owner()).values()[0].source();
            let body_owner = table.owner_snapshot(body_snapshot).unwrap().value_inputs()[0].owner();
            assert_eq!(
                incoming(first_exhausted, exit.owner()).values()[0].source(),
                header.owner()
            );
            (
                header.owner(),
                exit.owner(),
                initial,
                body_owner,
                body_snapshot,
            )
        })
        .collect::<Vec<_>>();
    for &(header, _, initial, body_owner, body_snapshot) in &paths {
        let body_value = table.owner_snapshot(body_snapshot).unwrap().value();
        let initial_create = steps
            .iter()
            .position(|(point, action)| {
                matches!(action, IterationCleanupAction::CreateClosureOwner { owner, closure }
                        if *owner == initial && *point == DropPoint::AfterExpression(*closure))
            })
            .unwrap();
        let create = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(body_value)
                        && matches!(action, IterationCleanupAction::CreateClosureOwner { owner, closure }
                            if *owner == body_owner && *closure == body_value)
                })
                .unwrap();
        let capture = steps
            .iter()
            .position(|(point, action)| {
                *point == DropPoint::AfterExpression(body_value)
                    && matches!(action, IterationCleanupAction::SaveClosureCapture { owner, .. }
                            if *owner == body_owner)
            })
            .unwrap();
        let save = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(body_value)
                        && matches!(action, IterationCleanupAction::SaveOwnerSnapshot { condition: None, owner, value }
                            if *owner == body_snapshot && *value == body_value)
                })
                .unwrap();
        let symbol = plans[&first.index()]
            .closure_phis()
            .iter()
            .find(|phi| phi.owner() == header)
            .unwrap()
            .symbol();
        let commit = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(body_value)
                        && matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                            if *owner == body_snapshot && *target == symbol)
                })
                .unwrap();
        assert!(initial_create < create && create < capture && capture < save && save < commit);
    }
    let graph = plans[&second.index()].capture_graph();
    let first_graph = plans[&first.index()].capture_graph();
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
    let mut instance_nodes = BTreeMap::new();
    let mut handles = BTreeMap::new();
    let mut form = |owner,
                    handles: &mut BTreeMap<CleanupOwnerValueId, usize>,
                    instance_nodes: &mut BTreeMap<usize, usize>| {
        let closure = steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::CreateClosureOwner {
                    owner: created,
                    closure,
                } if *created == owner => Some(*closure),
                _ => None,
            })
            .unwrap();
        next_instance += 1;
        let node = graph
            .nodes()
            .iter()
            .position(|node| node.closure() == closure)
            .unwrap();
        assert!(instance_nodes.insert(next_instance, node).is_none());
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
    for &(_, _, initial, _, _) in &paths {
        form(initial, &mut handles, &mut instance_nodes);
    }
    let entry_nodes = first_nodes(&instance_nodes);
    let entry_roots = paths
        .iter()
        .map(|&(header, _, initial, _, _)| {
            let root = active_root(
                table,
                incoming(first_entry, header),
                &choices,
                &handles,
                &entry_nodes,
            );
            assert_eq!(root.source(), initial);
            (header, root)
        })
        .collect::<Vec<_>>();
    let mut captured = BTreeMap::new();
    let entry_transport = replay_captured_edge(
        table,
        first_graph,
        plans[&first.index()].closure_phis(),
        first_entry,
        ReplayInstances {
            values: &handles,
            nodes: &entry_nodes,
            captured: &captured,
        },
        &mut choices,
    );
    assert_eq!(entry_transport.len(), paths.len());
    for (header, root) in entry_roots {
        assert_eq!(
            entry_transport[&header],
            (root.source(), handles[&root.source()])
        );
        let moved = handles.remove(&root.source()).unwrap();
        assert_eq!(
            first_graph.nodes()[root.node()].closure(),
            graph.nodes()[instance_nodes[&moved]].closure()
        );
        assert!(handles.insert(header, moved).is_none());
    }
    for _ in 0..2 {
        for &(header, _, _, body_owner, body_snapshot) in &paths {
            let formed = form(body_owner, &mut handles, &mut instance_nodes);
            let (slot, input) = steps
                .iter()
                .find_map(|(_, action)| match action {
                    IterationCleanupAction::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if *owner == body_owner => Some((*target, *input)),
                    _ => None,
                })
                .unwrap();
            assert_eq!(input.value(), CleanupCaptureValue::Owner(header));
            assert_eq!(
                (input.mode(), input.effect()),
                (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
            );
            assert!(selected(table, input.condition(), &choices));
            let layout = table.capture_slot_value(slot).unwrap();
            assert_eq!(layout.environment(), body_owner);
            assert_eq!(
                layout.closure(),
                table.owner_snapshot(body_snapshot).unwrap().value()
            );
            assert_eq!(layout.source(), input.source());
            let position = layout.position();
            let previous = handles.remove(&header).unwrap();
            assert!(captured.insert((formed, position), previous).is_none());
            let snapshot = table.owner_snapshot(body_snapshot).unwrap();
            assert_eq!(snapshot.value_inputs()[0].owner(), body_owner);
            let formed = handles.remove(&body_owner).unwrap();
            assert!(handles.insert(body_snapshot, formed).is_none());
            replay_snapshot_choices(table, body_snapshot, &mut choices);
        }
        let old_nodes = first_nodes(&instance_nodes);
        let writes = paths
            .iter()
            .map(|&(header, _, _, _, body_snapshot)| {
                let root = active_root(
                    table,
                    incoming(first_backedge, header),
                    &choices,
                    &handles,
                    &old_nodes,
                );
                assert_eq!(root.source(), body_snapshot);
                let instance = handles[&root.source()];
                assert_eq!(
                    first_graph.nodes()[root.node()].closure(),
                    graph.nodes()[instance_nodes[&instance]].closure()
                );
                (header, root.source(), instance)
            })
            .collect::<Vec<_>>();
        let transported = replay_captured_edge(
            table,
            first_graph,
            plans[&first.index()].closure_phis(),
            first_backedge,
            ReplayInstances {
                values: &handles,
                nodes: &old_nodes,
                captured: &captured,
            },
            &mut choices,
        );
        assert_eq!(transported.len(), paths.len());
        for (header, source, instance) in writes {
            assert_eq!(transported[&header], (source, instance));
            assert_eq!(handles.remove(&source), Some(instance));
            assert!(handles.insert(header, instance).is_none());
        }
    }
    let old_nodes = first_nodes(&instance_nodes);
    let writes = paths
        .iter()
        .map(|&(header, exit, _, _, _)| {
            let root = active_root(
                table,
                incoming(first_exhausted, exit),
                &choices,
                &handles,
                &old_nodes,
            );
            assert_eq!(root.source(), header);
            let instance = handles[&root.source()];
            assert_eq!(
                first_graph.nodes()[root.node()].closure(),
                graph.nodes()[instance_nodes[&instance]].closure()
            );
            (exit, root.source(), instance)
        })
        .collect::<Vec<_>>();
    let transported = replay_captured_edge(
        table,
        first_graph,
        plans[&first.index()].closure_phis(),
        first_exhausted,
        ReplayInstances {
            values: &handles,
            nodes: &old_nodes,
            captured: &captured,
        },
        &mut choices,
    );
    assert_eq!(transported.len(), paths.len());
    for (exit, source, instance) in writes {
        assert_eq!(transported[&exit], (source, instance));
        assert_eq!(handles.remove(&source), Some(instance));
        assert!(handles.insert(exit, instance).is_none());
    }
    assert_eq!(instance_nodes.len(), 6);
    let first_instance_nodes = instance_nodes.clone();
    let active_releases = |owner, choices: &BTreeMap<_, _>| {
        steps
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(statement),
                    root,
                } if root.owner() == Some(owner)
                    && root
                        .condition()
                        .is_none_or(|condition| selected(table, condition, choices)) =>
                {
                    Some((*point, *statement, *root))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let active_root_actions = |owner, choices: &BTreeMap<_, _>| {
        steps
            .iter()
            .filter_map(|(point, action)| {
                let (root, release) = match action {
                    IterationCleanupAction::Drop(root) => (root, false),
                    IterationCleanupAction::ReleaseClosureInstances { root, .. } => (root, true),
                    _ => return None,
                };
                (root.owner() == Some(owner)
                    && root
                        .condition()
                        .is_none_or(|condition| selected(table, condition, choices)))
                .then_some((*point, release))
            })
            .collect::<Vec<_>>()
    };
    let selected_named_actions = |point, symbol: SymbolId, choices: &BTreeMap<_, _>| {
        steps
            .iter()
            .filter_map(|(at, action)| {
                let root = match action {
                    IterationCleanupAction::Drop(root)
                    | IterationCleanupAction::ReleaseClosureInstances { root, .. } => root,
                    _ => return None,
                };
                (*at == point
                    && root.target() == DropTarget::Named(symbol)
                    && root
                        .condition()
                        .is_none_or(|condition| selected(table, condition, choices)))
                .then_some(*action)
            })
            .collect::<Vec<_>>()
    };
    for (pick, expected_root) in expected_roots.iter().enumerate() {
        let mut choices = choices.clone();
        let mut handles = handles.clone();
        let mut captured = captured.clone();
        choices.insert(pick_selector, pick);
        let selected_values = saved
            .value_inputs()
            .iter()
            .filter(|input| selected(table, input.condition(), &choices))
            .collect::<Vec<_>>();
        let [chosen] = selected_values.as_slice() else {
            panic!("one old recursive root must own the evaluated RHS")
        };
        assert_eq!(chosen.owner(), *expected_root);
        let unselected = saved
            .value_inputs()
            .iter()
            .find(|input| input.owner() != chosen.owner())
            .unwrap();
        let releases = active_releases(unselected.owner(), &choices);
        let [(release_point, release_statement, release_fact)] = releases.as_slice() else {
            panic!("the unselected recursive root must have one release")
        };
        assert_eq!(*release_statement, first);
        assert_eq!(
            *release_point,
            DropPoint::BranchExit {
                control: saved.value(),
                branch: pick,
            }
        );
        let unselected_symbol = plans[&first.index()]
            .closure_phis()
            .iter()
            .find(|phi| phi.owner() == unselected.owner())
            .unwrap()
            .symbol();
        assert_eq!(release_fact.target(), DropTarget::Named(unselected_symbol));
        assert_eq!(
            active_root_actions(unselected.owner(), &choices),
            vec![(*release_point, true)]
        );
        assert_eq!(
            selected_named_actions(*release_point, unselected_symbol, &choices),
            vec![IterationCleanupAction::ReleaseClosureInstances {
                layout: ClosureReleaseLayout::Iteration(first),
                root: *release_fact,
            }]
        );
        assert!(active_root_actions(chosen.owner(), &choices).is_empty());
        let release_at = steps
            .iter()
            .position(|(point, action)| {
                point == release_point
                    && matches!(action, IterationCleanupAction::ReleaseClosureInstances { root, .. }
                            if root == release_fact)
            })
            .unwrap();
        let save_at = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(saved.value())
                        && matches!(action, IterationCleanupAction::SaveOwnerSnapshot { condition: None, owner, value }
                            if *owner == snapshot && *value == saved.value())
                })
                .unwrap();
        let commit_at = steps
                .iter()
                .position(|(point, action)| {
                    *point == DropPoint::AfterExpression(saved.value())
                        && matches!(action, IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                            if *owner == snapshot && *target == h)
                })
                .unwrap();
        assert!(release_at < save_at && save_at < commit_at);
        let unselected_instance = handles.remove(&unselected.owner()).unwrap();
        let release_graph = plans[&release_statement.index()].capture_graph();
        let release_nodes = first_instance_nodes
            .iter()
            .map(|(&instance, &node)| {
                let closure = graph.nodes()[node].closure();
                let release_node = release_graph
                    .nodes()
                    .iter()
                    .position(|candidate| candidate.closure() == closure)
                    .unwrap();
                (instance, release_node)
            })
            .collect::<BTreeMap<_, _>>();
        let released = replay_owned_closure_release(
            release_graph,
            &release_nodes,
            &mut captured,
            unselected_instance,
        );
        assert_eq!(released.len(), 3);
        assert!(released.windows(2).all(|pair| pair[0] < pair[1]));
        let chosen_instance = handles.remove(&chosen.owner()).unwrap();
        assert!(handles.insert(snapshot, chosen_instance).is_none());
        replay_snapshot_choices(table, snapshot, &mut choices);
        let active = entry
            .root_sources()
            .iter()
            .filter(|source| {
                source.source() == snapshot
                    && source.node() == instance_nodes[&chosen_instance]
                    && selected(table, source.condition(), &choices)
            })
            .collect::<Vec<_>>();
        let [root] = active.as_slice() else {
            panic!("one saved recursive root must reach the second loop")
        };
        let chosen_phi = plans[&first.index()]
            .closure_phis()
            .iter()
            .find(|phi| phi.owner() == chosen.owner())
            .unwrap();
        let selected_closure = chosen_phi.root_origins().last().unwrap().closure();
        let graph = plans[&second.index()].capture_graph();
        assert_eq!(graph.nodes()[root.node()].closure(), selected_closure);
        assert_eq!(root.source(), snapshot);
        let transported = replay_captured_edge(
            table,
            graph,
            plans[&second.index()].closure_phis(),
            edge,
            ReplayInstances {
                values: &handles,
                nodes: &instance_nodes,
                captured: &captured,
            },
            &mut choices,
        );
        assert_eq!(transported[&h_header.owner()], (snapshot, chosen_instance));
        let moved = handles.remove(&root.source()).unwrap();
        assert_eq!(moved, chosen_instance);
        assert!(handles.insert(h_header.owner(), moved).is_none());
        assert_eq!(
            choices[&h_header
                .root_origins()
                .find(|target| target.node() == root.node())
                .unwrap()
                .selector()],
            1
        );
        assert_eq!(
            h_header
                .root_origins()
                .filter(|target| choices[&target.selector()] == 1)
                .count(),
            2
        );
        assert!(active_root_actions(h_header.owner(), &choices).is_empty());
        let second_backedge = plans[&second.index()]
            .closure_phi_incomings()
            .iter()
            .find(|edge| edge.kind() == IterationPhiIncomingKind::Fallthrough)
            .unwrap();
        let back_binding = incoming(second_backedge, h_header.owner());
        assert_eq!(back_binding.values().len(), 1);
        let body_snapshot = back_binding.values()[0].source();
        let body = table.owner_snapshot(body_snapshot).unwrap();
        assert_eq!(body.value_inputs().len(), 1);
        let body_owner = body.value_inputs()[0].owner();
        let body_value = body.value();
        let actions = steps
            .iter()
            .enumerate()
            .filter(|(_, (point, _))| *point == DropPoint::AfterExpression(body_value))
            .collect::<Vec<_>>();
        let action_index = |matches: &dyn Fn(&IterationCleanupAction) -> bool| {
            actions
                .iter()
                .find_map(|(index, (_, action))| matches(action).then_some(*index))
                .unwrap()
        };
        let create = action_index(&|action| {
            matches!(action,
                IterationCleanupAction::CreateClosureOwner { owner, closure }
                    if *owner == body_owner && *closure == body_value)
        });
        let (slot, input) = actions
            .iter()
            .find_map(|(_, (_, action))| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target,
                    input,
                } if *owner == body_owner => Some((*target, *input)),
                _ => None,
            })
            .unwrap();
        let capture = action_index(&|action| {
            matches!(action,
                IterationCleanupAction::SaveClosureCapture { owner, .. }
                    if *owner == body_owner)
        });
        let save = action_index(&|action| {
            matches!(action,
                IterationCleanupAction::SaveOwnerSnapshot { condition: None, owner, value }
                    if *owner == body_snapshot && *value == body_value)
        });
        let commit = action_index(&|action| {
            matches!(action,
                IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                    if *owner == body_snapshot && *target == h)
        });
        assert!(create < capture && capture < save && save < commit);
        assert_eq!(input.value(), CleanupCaptureValue::Owner(h_header.owner()));
        assert_eq!(
            (input.mode(), input.effect()),
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
        );
        let slot_value = table.capture_slot_value(slot).unwrap();
        assert_eq!(slot_value.environment(), body_owner);
        assert_eq!(slot_value.closure(), body_value);
        assert_eq!(slot_value.source(), input.source());
        let h_exit = plans[&second.index()]
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == h)
            .unwrap();
        let exhausted = plans[&second.index()]
            .closure_phi_incomings()
            .iter()
            .find(|edge| edge.kind() == IterationPhiIncomingKind::Exhaustion)
            .unwrap();
        let exit_binding = incoming(exhausted, h_exit.owner());
        assert_eq!(exit_binding.values()[0].source(), h_header.owner());
        let final_call = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| (sources.slice(node.span()) == Ok("h()")).then_some(id))
            .last()
            .unwrap();
        for rounds in [0, 2] {
            let mut choices = choices.clone();
            let mut handles = handles.clone();
            let mut captured = captured.clone();
            let mut released = released.clone();
            for _ in 0..rounds {
                assert!(active_root_actions(h_header.owner(), &choices).is_empty());
                assert!(selected(table, input.condition(), &choices));
                let formed = form(body_owner, &mut handles, &mut instance_nodes);
                assert!(active_root_actions(body_owner, &choices).is_empty());
                let previous = handles.remove(&h_header.owner()).unwrap();
                assert!(
                    captured
                        .insert((formed, slot_value.position()), previous)
                        .is_none()
                );
                let formed = handles.remove(&body_owner).unwrap();
                assert!(handles.insert(body_snapshot, formed).is_none());
                replay_snapshot_choices(table, body_snapshot, &mut choices);
                assert!(active_root_actions(body_owner, &choices).is_empty());
                assert!(active_root_actions(body_snapshot, &choices).is_empty());
                assert!(active_root_actions(h_header.owner(), &choices).is_empty());
                let back_root =
                    active_root(table, back_binding, &choices, &handles, &instance_nodes);
                assert_eq!(back_root.source(), body_snapshot);
                assert_eq!(
                    graph.nodes()[back_root.node()].closure(),
                    graph.nodes()[instance_nodes[&formed]].closure()
                );
                let transported = replay_captured_edge(
                    table,
                    graph,
                    plans[&second.index()].closure_phis(),
                    second_backedge,
                    ReplayInstances {
                        values: &handles,
                        nodes: &instance_nodes,
                        captured: &captured,
                    },
                    &mut choices,
                );
                assert_eq!(transported[&h_header.owner()], (body_snapshot, formed));
                let moved = handles.remove(&back_root.source()).unwrap();
                assert_eq!(moved, formed);
                assert!(handles.insert(h_header.owner(), moved).is_none());
                assert!(active_root_actions(h_header.owner(), &choices).is_empty());
            }
            let exit_root = active_root(table, exit_binding, &choices, &handles, &instance_nodes);
            assert_eq!(exit_root.source(), h_header.owner());
            let root_instance = handles[&exit_root.source()];
            assert_eq!(
                graph.nodes()[exit_root.node()].closure(),
                graph.nodes()[instance_nodes[&root_instance]].closure()
            );
            let transported = replay_captured_edge(
                table,
                graph,
                plans[&second.index()].closure_phis(),
                exhausted,
                ReplayInstances {
                    values: &handles,
                    nodes: &instance_nodes,
                    captured: &captured,
                },
                &mut choices,
            );
            assert_eq!(
                transported[&h_exit.owner()],
                (h_header.owner(), root_instance)
            );
            assert_eq!(handles.remove(&exit_root.source()), Some(root_instance));
            assert!(handles.insert(h_exit.owner(), root_instance).is_none());
            let releases = active_releases(h_exit.owner(), &choices);
            let [(release_point, release_statement, release_fact)] = releases.as_slice() else {
                panic!("the exit root must have one release")
            };
            assert_eq!(*release_statement, second);
            assert_eq!(*release_point, DropPoint::CallReturn(final_call));
            assert_eq!(release_fact.target(), DropTarget::Named(h));
            assert_eq!(
                active_root_actions(h_exit.owner(), &choices),
                vec![(*release_point, true)]
            );
            assert_eq!(
                selected_named_actions(*release_point, h, &choices),
                vec![IterationCleanupAction::ReleaseClosureInstances {
                    layout: ClosureReleaseLayout::Iteration(second),
                    root: *release_fact,
                }]
            );
            released.extend(replay_owned_closure_release(
                plans[&release_statement.index()].capture_graph(),
                &instance_nodes,
                &mut captured,
                handles.remove(&h_exit.owner()).unwrap(),
            ));
            assert_eq!(released.len(), 6 + rounds);
            assert!(released[3..].windows(2).all(|pair| pair[0] < pair[1]));
            assert_eq!(
                released.into_iter().collect::<BTreeSet<_>>().len(),
                6 + rounds
            );
            assert!(captured.is_empty() && handles.is_empty());
        }
    }
}
