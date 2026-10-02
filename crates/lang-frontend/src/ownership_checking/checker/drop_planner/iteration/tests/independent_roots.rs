use super::*;

#[test]
fn recursive_graph_keeps_independent_unexpanded_closure_roots_owned() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "mixed-recursive-roots.ko",
            "fun run(flags: List<Int>) { var f: move () -> Unit = move {}
val first_leaf: move () -> Unit = move {}
val second_leaf: move () -> Unit = move {}
var g: move () -> Unit = move { val a = first_leaf()
val b = second_leaf() }
for (_ in flags) { f = move { f() } }
val first = f()
val second = g() }",
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
    let phis = &planner.loop_phis[&statement.index()];
    let headers = phis
        .iter()
        .filter(|phi| phi.boundary() == IterationPhiBoundary::Header)
        .filter(|phi| !phi.root_nodes().is_empty())
        .collect::<Vec<_>>();
    assert_eq!(headers.len(), 2);
    let independent = headers
        .iter()
        .find(|phi| {
            phi.root_nodes().len() == 1
                && sources
                    .slice(
                        parsed
                            .ast()
                            .expressions()
                            .get(graph.nodes()[phi.root_nodes()[0]].closure())
                            .unwrap()
                            .span(),
                    )
                    .unwrap()
                    == "move { val a = first_leaf()\nval b = second_leaf() }"
        })
        .unwrap();
    let node = &graph.nodes()[independent.root_nodes()[0]];
    assert_eq!(node.sources().len(), 2);
    let formed_owner = planner
        .cleanup
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *closure == node.closure() =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    let relevant_nodes = BTreeSet::from([
        independent.root_nodes()[0],
        node.sources()[0].captured()[0],
        node.sources()[1].captured()[0],
    ]);
    assert_eq!(relevant_nodes.len(), 3);
    assert_eq!(independent.origins().len(), relevant_nodes.len());
    let mut instance_nodes = BTreeMap::new();
    let mut owner_instances = BTreeMap::new();
    let mut saved_children = BTreeMap::new();
    for (index, captured) in node.sources().iter().enumerate() {
        assert_eq!(captured.position(), index);
        assert_eq!(
            sources.slice(captured.capture().reference_span()).unwrap(),
            ["first_leaf", "second_leaf"][index]
        );
        assert_eq!(captured.capture().mode(), ClosureCaptureMode::Owned);
        assert_eq!(captured.capture().effect(), ClosureCaptureEffect::Move);
        assert_eq!(captured.captured().len(), 1);
        assert!(graph.nodes()[captured.captured()[0]].sources().is_empty());
    }
    for (point, action) in &planner.cleanup {
        match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure } => {
                let Some(node_index) = graph
                    .nodes()
                    .iter()
                    .position(|node| node.closure() == *closure)
                else {
                    continue;
                };
                if !relevant_nodes.contains(&node_index) {
                    continue;
                }
                assert_eq!(*point, DropPoint::AfterExpression(*closure));
                let instance = instance_nodes.len() + 1;
                assert!(instance_nodes.insert(instance, node_index).is_none());
                assert!(owner_instances.insert(*owner, instance).is_none());
            }
            IterationCleanupAction::SaveClosureCapture {
                owner,
                target,
                input,
            } if *owner == formed_owner => {
                assert_eq!(*point, DropPoint::AfterExpression(node.closure()));
                assert_eq!(input.condition(), CleanupConditionId::ALWAYS);
                let slot = planner.conditions.capture_slot_value(*target).unwrap();
                assert_eq!(slot.environment(), formed_owner);
                let expected = &node.sources()[slot.position()];
                assert_eq!(input.source(), expected.capture().source());
                assert_eq!(input.mode(), ClosureCaptureMode::Owned);
                assert_eq!(input.effect(), ClosureCaptureEffect::Move);
                let CleanupCaptureValue::Owner(source_owner) = input.value() else {
                    panic!("owned child must come from its formed owner")
                };
                let parent = owner_instances[owner];
                let child = owner_instances.remove(&source_owner).unwrap();
                assert!(expected.captured().contains(&instance_nodes[&child]));
                assert!(
                    saved_children
                        .insert((parent, slot.position()), child)
                        .is_none()
                );
            }
            _ => {}
        }
    }
    assert_eq!(instance_nodes.len(), 3);
    assert_eq!(saved_children.len(), 2);
    let g_instance = owner_instances[&formed_owner];
    assert_eq!(owner_instances.len(), 1);
    for header in &headers {
        let exit = phis
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == header.symbol()
            })
            .unwrap();
        let root = planner
            .facts
            .iter()
            .find(|fact| {
                fact.target() == DropTarget::Named(header.symbol())
                    && fact.owner() == Some(exit.owner())
            })
            .unwrap();
        if header.symbol() == independent.symbol() {
            assert!(planner.cleanup.iter().any(|(_, action)| matches!(action,
                    IterationCleanupAction::Drop(fact) if *fact == *root)));
        } else {
            assert!(planner.cleanup.iter().any(|(_, action)| matches!(action,
                    IterationCleanupAction::ReleaseClosureInstances { layout: ClosureReleaseLayout::Iteration(release_statement), root: released }
                        if *release_statement == statement && *released == *root)));
        }
    }
    let incomings = &planner.loop_phi_incomings[&statement.index()];
    let entry = incomings
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap()
        .bindings()
        .iter()
        .find(|binding| binding.target() == independent.owner())
        .unwrap();
    assert_eq!(entry.values().len(), 1);
    assert_eq!(entry.values()[0].source(), formed_owner);
    assert!(
        !entry.origins().is_empty(),
        "independent g keeps its child sources"
    );
    let exhausted = incomings
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap()
        .bindings()
        .iter()
        .find(|binding| {
            phis.iter().any(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit
                    && phi.symbol() == independent.symbol()
                    && phi.owner() == binding.target()
            })
        })
        .unwrap();
    assert_eq!(exhausted.values().len(), 1);
    assert_eq!(exhausted.values()[0].source(), independent.owner());
    let release_root = planner
        .facts
        .iter()
        .find(|fact| {
            fact.owner() == Some(exhausted.target())
                && matches!(fact.target(), DropTarget::Named(_))
        })
        .unwrap();
    let flat = planner
        .cleanup
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::Drop(fact) if *point == release_root.point() => (fact
                == release_root
                || fact.instance_address().is_some_and(|address| {
                    planner.conditions.instance_address(address).unwrap().root()
                        == exhausted.target()
                }))
            .then_some(*fact),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(flat.len(), 3);
    assert_eq!(flat.last(), Some(release_root));
    assert_eq!(
        flat[..2]
            .iter()
            .map(|fact| {
                planner
                    .conditions
                    .capture_slot_value(fact.capture_slot().unwrap())
                    .unwrap()
                    .position()
            })
            .collect::<Vec<_>>(),
        [1, 0]
    );
    let expected_release = [
        saved_children[&(g_instance, 1)],
        saved_children[&(g_instance, 0)],
        g_instance,
    ];
    let mut values = BTreeMap::from([(entry.values()[0].source(), g_instance)]);
    let formed = values.remove(&entry.values()[0].source()).unwrap();
    values.insert(entry.target(), formed);
    let forwarded = values.remove(&exhausted.values()[0].source()).unwrap();
    values.insert(exhausted.target(), forwarded);
    let root_instance = values.remove(&release_root.owner().unwrap()).unwrap();
    assert_eq!(root_instance, g_instance);
    assert_eq!(
        replay_owned_closure_release(
            &planner.loop_capture_graphs[&statement.index()],
            &instance_nodes,
            &mut saved_children,
            root_instance,
        ),
        expected_release
    );
    assert!(saved_children.is_empty() && values.is_empty());
}

#[test]
fn recursive_loop_keeps_independent_shared_capture_loan_end() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-with-shared-root.ko",
            "fun read(xs: List<Int>) {}\nfun run(xs: List<Int>, flags: List<Int>) {
var f: move () -> Unit = move {}
var g: () -> Unit = { read(xs) }
for (_ in flags) { f = move { f() } }
val used = g()
val done = f() }",
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
    let independent = planner.loop_phis[&statement.index()]
        .iter()
        .find(|phi| {
            phi.boundary() == IterationPhiBoundary::Exit
                && phi.root_nodes().iter().any(|&node| {
                    sources.slice(
                        parsed
                            .ast()
                            .expressions()
                            .get(graph.nodes()[node].closure())
                            .unwrap()
                            .span(),
                    ) == Ok("{ read(xs) }")
                })
        })
        .unwrap();
    let root = planner
        .facts
        .iter()
        .find(|fact| {
            fact.owner() == Some(independent.owner())
                && matches!(fact.target(), DropTarget::Named(_))
        })
        .unwrap();
    assert!(planner.cleanup.iter().any(|(point, action)| {
        *point == root.point()
            && matches!(action, IterationCleanupAction::Drop(fact) if fact == root)
    }));
    let loan_ends = planner
            .cleanup
            .iter()
            .filter(|(point, action)| {
                *point == root.point()
                    && matches!(action, IterationCleanupAction::EndCaptureLoan {
                        instance_address,
                        closure,
                        ..
                    } if *closure == graph.nodes()[independent.root_nodes()[0]].closure()
                        && planner.conditions.instance_address(*instance_address).unwrap().root() == independent.owner())
            })
            .collect::<Vec<_>>();
    assert_eq!(loan_ends.len(), 1, "the one shared capture ends once");
}

#[test]
fn independent_closure_survives_a_recursive_loop_into_the_next_loop() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-then-independent.ko",
            "fun run(flags: List<Int>, next: List<Int>) {
var f: move () -> Unit = move {}
val leaf: move () -> Unit = move {}
var g: move () -> Unit = move { leaf() }
for (_ in flags) { f = move { f() } }
val first = f()
for (_ in next) {}
val second = g() }",
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
    let statements = checker
        .iterations
        .values()
        .map(|iteration| iteration.descriptor().statement())
        .collect::<Vec<_>>();
    assert_eq!(statements.len(), 2);
    let (first, second) = (statements[0], statements[1]);
    let second_graph = &planner.loop_capture_graphs[&second.index()];
    let g = planner.loop_phis[&second.index()]
        .iter()
        .find(|phi| {
            phi.boundary() == IterationPhiBoundary::Header
                && phi.root_nodes().iter().any(|&node| {
                    sources.slice(
                        parsed
                            .ast()
                            .expressions()
                            .get(second_graph.nodes()[node].closure())
                            .unwrap()
                            .span(),
                    ) == Ok("move { leaf() }")
                })
        })
        .unwrap();
    assert!(!planner.recursive_phi_bindings.contains(&g.owner()));
    assert!(!g.origins().is_empty());
    let first_exit = planner.loop_phis[&first.index()]
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == g.symbol())
        .unwrap();
    assert!(!first_exit.origins().is_empty());
    let entry = planner.loop_phi_incomings[&second.index()]
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap()
        .bindings()
        .iter()
        .find(|binding| binding.target() == g.owner())
        .unwrap();
    assert_eq!(entry.values().len(), 1);
    assert_eq!(entry.root_sources().len(), 1);
    assert_eq!(entry.root_sources()[0].source(), entry.values()[0].source());
    assert!(!entry.origins().is_empty());
    assert!(entry.selector_writes().iter().any(|write| {
        write.node() != g.root_nodes()[0] && write.condition() != CleanupConditionId::NEVER
    }));
}
