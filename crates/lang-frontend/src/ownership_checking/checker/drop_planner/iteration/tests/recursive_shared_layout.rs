use super::*;

#[test]
fn recursive_release_layout_keeps_untracked_shared_capture() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-copyable-shared.ko",
            "fun read(n: Int) {}\nfun run(n: Int, flags: List<Int>) {
var f: move () -> Unit = move {}
for (_ in flags) {
    val g: () -> Unit = { read(n) }
    f = move { val old = f()\nval borrowed = g() }
}
val used = f() }",
        )
        .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (names, types) = crate::type_checking::standard_environments();
    let names = crate::name_resolution::resolve_names(&sources, &parsed, &names).unwrap();
    let typed = crate::type_checking::check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let checked =
        crate::ownership_checking::check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(checked.diagnostics().is_empty());
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
    let borrowed = graph
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
            ) == Ok("{ read(n) }")
        })
        .unwrap();
    assert!(
        borrowed.sources().is_empty(),
        "Copyable capture has no phi source"
    );
    let [capture] = borrowed.release_captures() else {
        panic!("instance release must see the shared capture omitted by phi")
    };
    assert_eq!(
        checked.captures_of(borrowed.closure()).collect::<Vec<_>>(),
        [capture]
    );
    assert_eq!(capture.mode(), ClosureCaptureMode::Shared);
    assert_eq!(capture.effect(), ClosureCaptureEffect::Borrow);
    assert_eq!(sources.slice(capture.reference_span()), Ok("n"));
    let formed = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *closure == borrowed.closure() =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    let captures = planner
        .cleanup
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } if *owner == formed => Some((*target, *input)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(slot, input)] = captures.as_slice() else {
        panic!("the formed child must save its one shared capture")
    };
    let layout = planner.conditions.capture_slot_value(*slot).unwrap();
    assert_eq!(layout.environment(), formed);
    assert_eq!(layout.position(), 0);
    assert_eq!(layout.source(), capture.source());
    assert_eq!(input.source(), capture.source());
    assert_eq!(input.value(), CleanupCaptureValue::Place(capture.source()));
    assert_eq!(
        (input.mode(), input.effect()),
        (capture.mode(), capture.effect())
    );
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
            ) == Ok("move { val old = f()\nval borrowed = g() }")
        })
        .unwrap();
    let borrowed_node = graph
        .nodes()
        .iter()
        .position(|node| node.closure() == borrowed.closure())
        .unwrap();
    let initial_node = graph
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
            ) == Ok("move {}")
        })
        .unwrap();
    let recursive_closure = graph.nodes()[recursive_node].closure();
    let recursive_owner = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *closure == recursive_closure =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    let recursive_inputs = planner
        .cleanup
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } if *owner == recursive_owner => Some((*target, *input)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(recursive_inputs.len(), 2);
    for (position, (slot, input)) in recursive_inputs.iter().enumerate() {
        let saved = planner.conditions.capture_slot_value(*slot).unwrap();
        assert_eq!(saved.position(), position);
        assert_eq!(saved.source(), input.source());
        assert_eq!(
            (input.mode(), input.effect()),
            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move)
        );
    }
    let snapshot = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                if *value == recursive_closure =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    let borrowed_snapshot = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, value, .. }
                if *value == borrowed.closure() =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    let phis = &planner.loop_phis[&statement.index()];
    let header = phis
        .iter()
        .find(|phi| {
            phi.boundary() == IterationPhiBoundary::Header
                && phi.root_nodes().contains(&recursive_node)
        })
        .unwrap();
    let exit = phis
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == header.symbol())
        .unwrap();
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
    let entry = binding(IterationPhiIncomingKind::Entry, header.owner());
    let backedge = binding(IterationPhiIncomingKind::Fallthrough, header.owner());
    let exhausted = binding(IterationPhiIncomingKind::Exhaustion, exit.owner());
    assert_eq!(entry.root_sources().len(), 1, "{:?}", entry.root_sources());
    assert_eq!(
        entry.root_sources()[0].condition(),
        CleanupConditionId::ALWAYS
    );
    assert_eq!(entry.values().len(), 1);
    assert_eq!(backedge.values().len(), 1);
    assert_eq!(exhausted.values().len(), 1);
    assert_eq!(backedge.values()[0].source(), snapshot);
    assert_eq!(
        recursive_inputs[0].1.value(),
        CleanupCaptureValue::Owner(header.owner())
    );
    assert_eq!(
        recursive_inputs[1].1.value(),
        CleanupCaptureValue::Owner(borrowed_snapshot)
    );
    let release = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::ReleaseClosureInstances {
                layout: ClosureReleaseLayout::Iteration(release_statement),
                root,
            } if *release_statement == statement && root.owner() == Some(exit.owner()) => {
                Some(*root)
            }
            _ => None,
        })
        .unwrap();
    assert!(!planner.cleanup.iter().any(|(_, action)| {
        matches!(action, IterationCleanupAction::EndCaptureLoan { closure, .. }
                if *closure == borrowed.closure())
    }));

    let active_root = |binding: &IterationPhiIncomingBinding,
                       choices: &BTreeMap<_, _>,
                       values: &BTreeMap<_, usize>,
                       nodes: &BTreeMap<usize, usize>| {
        let selected_values = binding
            .values()
            .iter()
            .filter(|value| selected(&planner.conditions, value.condition(), choices))
            .collect::<Vec<_>>();
        let [value] = selected_values.as_slice() else {
            panic!("one owner value must reach this edge")
        };
        let roots = binding
            .root_sources()
            .iter()
            .filter(|root| {
                root.source() == value.source()
                    && root.node() == nodes[&values[&value.source()]]
                    && selected(&planner.conditions, root.condition(), choices)
            })
            .collect::<Vec<_>>();
        let [root] = roots.as_slice() else {
            panic!("one formed root must reach this edge")
        };
        **root
    };
    let initial_owner = entry.values()[0].source();
    assert!(planner.cleanup.iter().any(|(_, action)| matches!(
        action,
        IterationCleanupAction::CreateClosureOwner { owner, closure }
            if *owner == initial_owner && *closure == graph.nodes()[initial_node].closure()
    )));
    let mut values = BTreeMap::from([(initial_owner, 1usize)]);
    let mut instance_nodes = BTreeMap::from([(1usize, initial_node)]);
    let mut owned_edges = BTreeMap::<(usize, usize), usize>::new();
    let mut shared_loans = BTreeMap::<(usize, usize), CleanupCaptureValue>::new();
    let mut choices = BTreeMap::new();
    let entry_values = replay_captured_edge(
        &planner.conditions,
        graph,
        phis,
        incoming(IterationPhiIncomingKind::Entry),
        ReplayInstances {
            values: &values,
            nodes: &instance_nodes,
            captured: &owned_edges,
        },
        &mut choices,
    );
    assert_eq!(entry_values[&header.owner()], (initial_owner, 1));
    assert_eq!(values.remove(&initial_owner), Some(1));
    values.insert(header.owner(), 1);
    let mut checkpoints = vec![(
        values.clone(),
        instance_nodes.clone(),
        owned_edges.clone(),
        shared_loans.clone(),
        choices.clone(),
    )];
    let mut formed_instances = Vec::new();
    for round in 0..2 {
        let child = round * 2 + 2;
        let parent = child + 1;
        assert!(
            choices.contains_key(&header.availability_selector()),
            "header {:?} choices {:?}",
            header.availability_selector(),
            choices
        );
        assert!(selected(&planner.conditions, input.condition(), &choices));
        values.insert(formed, child);
        instance_nodes.insert(child, borrowed_node);
        assert!(
            shared_loans
                .insert((child, layout.position()), input.value())
                .is_none()
        );
        let formed_child = values.remove(&formed).unwrap();
        values.insert(borrowed_snapshot, formed_child);
        // g 的结果快照复制旧 header 选择位，随后形成 f 时读取的是这份独立身份。
        replay_snapshot_choices(&planner.conditions, borrowed_snapshot, &mut choices);
        for (_, capture) in &recursive_inputs {
            assert!(selected(&planner.conditions, capture.condition(), &choices));
        }
        values.insert(recursive_owner, parent);
        instance_nodes.insert(parent, recursive_node);
        for (slot, capture) in &recursive_inputs {
            let CleanupCaptureValue::Owner(source) = capture.value() else {
                panic!("owned child must read a formed owner")
            };
            let child_instance = values.remove(&source).unwrap();
            let position = planner
                .conditions
                .capture_slot_value(*slot)
                .unwrap()
                .position();
            assert!(
                owned_edges
                    .insert((parent, position), child_instance)
                    .is_none()
            );
        }
        let formed_parent = values.remove(&recursive_owner).unwrap();
        values.insert(snapshot, formed_parent);
        replay_snapshot_choices(&planner.conditions, snapshot, &mut choices);
        let source = active_root(backedge, &choices, &values, &instance_nodes).source();
        let transported = replay_captured_edge(
            &planner.conditions,
            graph,
            phis,
            incoming(IterationPhiIncomingKind::Fallthrough),
            ReplayInstances {
                values: &values,
                nodes: &instance_nodes,
                captured: &owned_edges,
            },
            &mut choices,
        );
        assert_eq!(transported[&header.owner()], (source, parent));
        let forwarded = values.remove(&source).unwrap();
        assert_eq!(forwarded, parent);
        values.insert(header.owner(), forwarded);
        formed_instances.push((child, parent));
        checkpoints.push((
            values.clone(),
            instance_nodes.clone(),
            owned_edges.clone(),
            shared_loans.clone(),
            choices.clone(),
        ));
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Released {
        Loan(usize, usize),
        Environment(usize),
    }
    enum Step {
        Enter(usize),
        EndLoan(usize, usize),
        Finish(usize),
    }
    type ReleaseCheckpoint = (
        BTreeMap<CleanupOwnerValueId, usize>,
        BTreeMap<usize, usize>,
        BTreeMap<(usize, usize), usize>,
        BTreeMap<(usize, usize), CleanupCaptureValue>,
        BTreeMap<crate::ownership_checking::CleanupSelectorId, usize>,
    );
    let release_checkpoint = |state: ReleaseCheckpoint| {
        let (mut values, nodes, mut edges, mut loans, mut choices) = state;
        let source = active_root(exhausted, &choices, &values, &nodes).source();
        let transported = replay_captured_edge(
            &planner.conditions,
            graph,
            phis,
            incoming(IterationPhiIncomingKind::Exhaustion),
            ReplayInstances {
                values: &values,
                nodes: &nodes,
                captured: &edges,
            },
            &mut choices,
        );
        assert_eq!(transported[&exit.owner()].0, source);
        let forwarded = values.remove(&source).unwrap();
        values.insert(exit.owner(), forwarded);
        assert!(selected(
            &planner.conditions,
            release.condition().unwrap_or(CleanupConditionId::ALWAYS),
            &choices
        ));
        let root = values.remove(&release.owner().unwrap()).unwrap();
        let mut pending = vec![Step::Enter(root)];
        let mut visited = BTreeSet::new();
        let mut released = Vec::new();
        let mut remaining_loans = loans.len();
        while let Some(step) = pending.pop() {
            match step {
                Step::Enter(instance) => {
                    assert!(visited.insert(instance), "an instance must release once");
                    pending.push(Step::Finish(instance));
                    let node = &graph.nodes()[nodes[&instance]];
                    for (position, capture) in checked.captures_of(node.closure()).enumerate() {
                        match (capture.mode(), capture.effect()) {
                            (ClosureCaptureMode::Owned, ClosureCaptureEffect::Move) => {
                                let child = edges.remove(&(instance, position)).unwrap();
                                let source = node
                                    .sources()
                                    .iter()
                                    .find(|source| source.position() == position)
                                    .unwrap();
                                assert!(source.captured().contains(&nodes[&child]));
                                pending.push(Step::Enter(child));
                            }
                            (ClosureCaptureMode::Shared, ClosureCaptureEffect::Borrow) => {
                                pending.push(Step::EndLoan(instance, position));
                            }
                            other => panic!("unexpected capture in replay: {other:?}"),
                        }
                    }
                }
                Step::EndLoan(instance, position) => {
                    assert_eq!(loans.remove(&(instance, position)), Some(input.value()));
                    remaining_loans -= 1;
                    released.push(Released::Loan(instance, remaining_loans));
                }
                Step::Finish(instance) => released.push(Released::Environment(instance)),
            }
        }
        assert!(values.is_empty() && edges.is_empty() && loans.is_empty());
        assert_eq!(remaining_loans, 0);
        assert_eq!(visited.len(), nodes.len());
        released
    };
    assert_eq!(
        release_checkpoint(checkpoints.remove(0)),
        [Released::Environment(1)]
    );
    assert_eq!(
        release_checkpoint(checkpoints.remove(0)),
        [
            Released::Loan(formed_instances[0].0, 0),
            Released::Environment(formed_instances[0].0),
            Released::Environment(1),
            Released::Environment(formed_instances[0].1),
        ]
    );
    assert_eq!(
        release_checkpoint(checkpoints.remove(0)),
        [
            Released::Loan(formed_instances[1].0, 1),
            Released::Environment(formed_instances[1].0),
            Released::Loan(formed_instances[0].0, 0),
            Released::Environment(formed_instances[0].0),
            Released::Environment(1),
            Released::Environment(formed_instances[0].1),
            Released::Environment(formed_instances[1].1),
        ]
    );
}

#[test]
fn untracked_copyable_shared_capture_keeps_loan_ends() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "copyable-sibling-loans.ko",
            "fun read(n: Int) {}\nfun run(flags: List<Int>, next: List<Int>) {
                    val n = 1
                    var first: () -> Unit = {}
                    var second: () -> Unit = {}
                    for (_ in flags) {
                        second = first
                        { first = { read(n) } }
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
    let outer_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "outer()").then_some(id))
        .unwrap();
    let actions = planner
        .cleanup
        .iter()
        .filter(|(point, _)| *point == DropPoint::CallReturn(outer_call))
        .map(|(_, action)| action)
        .collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action, IterationCleanupAction::EndCaptureLoan { .. }))
            .count(),
        2,
        "each formed sibling must end its own shared capture loan"
    );
    assert!(!actions.iter().any(|action| matches!(
        action,
        IterationCleanupAction::ReleaseClosureInstances { .. }
    )));
}
