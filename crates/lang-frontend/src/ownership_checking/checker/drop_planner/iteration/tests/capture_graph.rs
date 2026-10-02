use super::*;

#[test]
fn coexisting_capture_paths_include_the_root_instance() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("capture-roots.ko", "fun run() { val f = move {} }")
        .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    let (closure, span) = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            matches!(node.payload(), Expression::Lambda { .. }).then_some((id, node.span()))
        })
        .unwrap();
    let graph = IterationCaptureGraph {
        nodes: vec![IterationCaptureNode {
            closure,
            release_captures: Vec::new(),
            sources: vec![IterationCaptureSource {
                capture: ClosureCaptureDescriptor::new(
                    closure,
                    ClosureCaptureSource::Symbol(SymbolId(0)),
                    TypeId::new(0),
                    ClosureCaptureMode::Owned,
                    ClosureCaptureEffect::Move,
                    span,
                ),
                position: 0,
                captured: Vec::new(),
                may_be_opaque: false,
            }],
        }],
    };
    let mut conditions = CleanupConditions::default();
    let first = conditions.create_owner(CleanupOwnerValue::Closure {
        expression: closure,
        origin: span,
        inputs: Vec::new(),
    });
    let second = conditions.create_owner(CleanupOwnerValue::Closure {
        expression: closure,
        origin: span,
        inputs: Vec::new(),
    });
    let (selector, _) = conditions.last_capture_loan(first, span);
    let origin = IterationPhiIncomingOrigin {
        node: 0,
        target: selector,
        condition: CleanupConditionId::ALWAYS,
        environments: [first, second]
            .into_iter()
            .map(|root| IterationPhiIncomingEnvironment {
                owner: root,
                instance_root: root,
                capture_path: Vec::new(),
                condition: CleanupConditionId::ALWAYS,
                sources: Vec::new(),
            })
            .collect(),
    };
    assert_eq!(
        coexisting_capture_node(&mut conditions, &graph, &[origin], &mut BTreeMap::new()),
        Some(0),
        "two distinct root instances cannot share one capture layout"
    );
}

#[test]
fn owned_cycle_release_does_not_claim_independent_leaf_roots() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("owned-cycle-roots.ko", "fun run() { val f = move {} }")
        .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    let (closure, span) = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            matches!(node.payload(), Expression::Lambda { .. }).then_some((id, node.span()))
        })
        .unwrap();
    let capture = ClosureCaptureDescriptor::new(
        closure,
        ClosureCaptureSource::Symbol(SymbolId(0)),
        TypeId::new(0),
        ClosureCaptureMode::Owned,
        ClosureCaptureEffect::Move,
        span,
    );
    let node = |children: &[usize]| super::super::PhiCaptureNode {
        closure,
        release_captures: Vec::new(),
        opaque_sources: BTreeSet::new(),
        sources: children
            .iter()
            .enumerate()
            .map(|(position, child)| (capture, position, vec![*child]))
            .collect(),
    };
    // 0 <-> 1 is the recursive chain. 2 is its terminal leaf; 3 owns
    // both that chain and an independent leaf 4. Only roots 0, 1, 3
    // require instance traversal; leaves keep ordinary capture cleanup.
    let graph = PhiCaptureGraph {
        nodes: vec![
            node(&[1, 2]),
            node(&[0]),
            node(&[]),
            node(&[0, 4]),
            node(&[]),
        ],
        by_closure: BTreeMap::new(),
    };
    assert_eq!(
        graph.owned_nodes_reaching_cycle(),
        [true, true, false, true, false]
    );
}

#[test]
fn nested_phi_selector_writes_keep_local_instance_paths() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("nested-phi-selector.ko", "fun run() { val f = move {} }")
        .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    let span = parsed.ast().expressions().iter().next().unwrap().1.span();
    let mut table = CleanupConditions::default();
    let closure = parsed.ast().expressions().iter().next().unwrap().0;
    let owner = table.create_owner(CleanupOwnerValue::Closure {
        expression: closure,
        origin: span,
        inputs: Vec::new(),
    });
    let (first, _) = table.last_capture_loan(owner, span);
    let (second, _) = table.last_capture_loan(owner, span);
    let (child_selector, _) = table.last_capture_loan(owner, span);
    let (missing_selector, _) = table.last_capture_loan(owner, span);
    let layout = [first, second, child_selector, missing_selector]
        .into_iter()
        .enumerate()
        .map(|(node, selector)| IterationClosurePhiOrigin {
            node,
            closure,
            selector,
            condition: CleanupConditionId::ALWAYS,
            sources: Vec::new(),
        })
        .collect::<Vec<_>>();
    let capture_source = ClosureCaptureSource::Symbol(SymbolId(0));
    let input = CleanupCaptureInput {
        source: capture_source,
        value: CleanupCaptureValue::Place(capture_source),
        mode: ClosureCaptureMode::Owned,
        effect: ClosureCaptureEffect::Move,
        condition: CleanupConditionId::ALWAYS,
        origin: span,
    };
    let root =
        |node, target, child_condition, child_path: Option<usize>| IterationPhiIncomingOrigin {
            node,
            target,
            condition: CleanupConditionId::ALWAYS,
            environments: vec![IterationPhiIncomingEnvironment {
                owner,
                instance_root: owner,
                capture_path: Vec::new(),
                condition: CleanupConditionId::ALWAYS,
                sources: vec![IterationPhiIncomingSource {
                    target: None,
                    capture_slot: None,
                    source_capture_slot: None,
                    read_address: None,
                    source_environment: owner,
                    nested_instance: false,
                    input,
                    captured: vec![IterationPhiIncomingOrigin {
                        node: 2,
                        target: child_selector,
                        condition: child_condition,
                        environments: child_path
                            .into_iter()
                            .map(|position| IterationPhiIncomingEnvironment {
                                owner,
                                instance_root: owner,
                                capture_path: vec![position],
                                condition: CleanupConditionId::ALWAYS,
                                sources: Vec::new(),
                            })
                            .collect(),
                    }],
                }],
            }],
        };
    let origins = vec![
        root(0, first, CleanupConditionId::NEVER, None),
        root(1, second, CleanupConditionId::ALWAYS, None),
    ];
    let writes = phi_selector_writes(&mut table, &layout, &origins, &[]);
    assert_eq!(writes.len(), layout.len());
    assert_eq!(writes[2].condition(), CleanupConditionId::ALWAYS);
    assert_eq!(writes[3].condition(), CleanupConditionId::NEVER);
    assert_eq!(
        origins[0].environments()[0].sources()[0].captured()[0].condition(),
        CleanupConditionId::NEVER
    );

    let distinct_paths = vec![
        root(0, first, CleanupConditionId::ALWAYS, Some(0)),
        root(1, second, CleanupConditionId::ALWAYS, Some(1)),
    ];
    let writes = phi_selector_writes(&mut table, &layout, &distinct_paths, &[]);
    let child_paths = distinct_paths
        .iter()
        .map(|origin| {
            origin.environments()[0].sources()[0].captured()[0]
                .environments()
                .iter()
                .map(|environment| environment.capture_path().to_vec())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(child_paths, [vec![vec![0]], vec![vec![1]]]);
    assert_eq!(writes[2].condition(), CleanupConditionId::ALWAYS);
}

#[test]
fn optional_nested_capture_requires_instance_presence() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "optional-nested-capture.ko",
            "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
                    var f: move () -> Unit = move {}
                    for (flag in flags) {
                        val ys = listOf(2)
                        var g: () -> Unit = {}
                        if (flag) { g = { read(ys) } }
                        f = move { val used = g() }
                    }
                    val used = f()
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
    assert!(planner.conditional_nested_phi.is_some());
    let statement = checker
        .iterations
        .values()
        .next()
        .unwrap()
        .descriptor()
        .statement()
        .index();
    let incomings = &planner.loop_phi_incomings[&statement];
    for (kind, boundary) in [
        (
            IterationPhiIncomingKind::Entry,
            IterationPhiBoundary::Header,
        ),
        (
            IterationPhiIncomingKind::Fallthrough,
            IterationPhiBoundary::Header,
        ),
        (
            IterationPhiIncomingKind::Exhaustion,
            IterationPhiBoundary::Exit,
        ),
    ] {
        let edge = incomings.iter().find(|edge| edge.kind() == kind).unwrap();
        let phi = planner.loop_phis[&statement]
            .iter()
            .find(|phi| {
                phi.boundary() == boundary
                    && sources.slice(names.symbols()[phi.symbol().index()].span()) == Ok("f")
            })
            .unwrap();
        let binding = edge
            .bindings()
            .iter()
            .find(|binding| binding.target() == phi.owner())
            .unwrap();
        assert_eq!(
            binding.presence_source(),
            IterationPhiPresenceSource::CapturedInstances,
            "{kind:?}"
        );
    }
}

#[test]
fn finite_capture_graph_checks_long_chains_without_recursive_traversal() {
    let mut edges = (0..8_192)
        .map(|index| {
            (index < 8_191)
                .then_some(vec![index + 1])
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    assert_eq!(cyclic_node(&edges), None);
    edges.last_mut().unwrap().push(0);
    assert_eq!(cyclic_node(&edges), Some(0));
}

#[test]
fn finite_layout_order_is_bounded_on_diamond_ladders_and_deep_chains() {
    // 菱形阶梯：每层两个节点都指向下一层两个节点，路径数指数增长，
    // 但有限布局每个节点只记录一次。
    let levels = 64;
    let mut edges = vec![Vec::new(); levels * 2];
    for level in 0..levels - 1 {
        let (left, right) = (level * 2, level * 2 + 1);
        let (next_left, next_right) = ((level + 1) * 2, (level + 1) * 2 + 1);
        edges[left] = vec![next_left, next_right];
        edges[right] = vec![next_left, next_right];
    }
    assert_eq!(finite_layout_order(&edges, &[0, 1]).len(), levels * 2);

    // 8192 深链：迭代遍历，不依赖 Rust 调用栈。
    let depth = 8_192;
    let edges = (0..depth)
        .map(|index| {
            (index < depth - 1)
                .then_some(vec![index + 1])
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    assert_eq!(finite_layout_order(&edges, &[0]).len(), depth);
}

#[test]
fn recursive_capture_layout_visits_each_static_lambda_once() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "capture-cycle.ko",
            "fun run() { val a = move {}\nval b = move {} }",
        )
        .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    let closures = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Lambda { .. }).then_some((id, node.span()))
        })
        .collect::<Vec<_>>();
    assert_eq!(closures.len(), 2);
    let mut graph = PhiCaptureGraph::default();
    let first = graph.insert(closures[0].0);
    let second = graph.insert(closures[1].0);
    let first_source = ClosureCaptureSource::Symbol(SymbolId(0));
    let second_source = ClosureCaptureSource::Symbol(SymbolId(1));
    for (node, source, next) in [
        (first, first_source, second),
        (second, second_source, first),
    ] {
        let (closure, span) = closures[node];
        graph.nodes[node].sources.push((
            ClosureCaptureDescriptor::new(
                closure,
                source,
                TypeId::new(0),
                ClosureCaptureMode::Owned,
                ClosureCaptureEffect::Move,
                span,
            ),
            0,
            vec![next],
        ));
    }
    let repeated_source = ClosureCaptureSource::Symbol(SymbolId(2));
    graph.nodes[first].sources.push((
        ClosureCaptureDescriptor::new(
            closures[0].0,
            repeated_source,
            TypeId::new(0),
            ClosureCaptureMode::Owned,
            ClosureCaptureEffect::Move,
            closures[0].1,
        ),
        2,
        vec![second],
    ));
    assert_eq!(
        graph.capture_layout(&[closures[0].0]),
        vec![
            (first, first_source, 0),
            (first, repeated_source, 2),
            (second, second_source, 0)
        ]
    );
    assert_eq!(
        graph.capture_layout(&[closures[1].0]),
        graph.capture_layout(&[closures[0].0])
    );
    let published = graph.published();
    assert_eq!(published.nodes()[first].sources()[0].captured(), &[second]);
    assert_eq!(published.nodes()[first].sources()[1].captured(), &[second]);
    assert_eq!(published.nodes()[second].sources()[0].captured(), &[first]);
    let mut conditions = CleanupConditions::default();
    let owner = conditions.create_owner(CleanupOwnerValue::Closure {
        expression: closures[0].0,
        origin: closures[0].1,
        inputs: Vec::new(),
    });
    let slots = graph.register_capture_layout(&[closures[0].0], owner, &mut conditions);
    assert_eq!(
        slots.len(),
        3,
        "cycle edges must not duplicate static slots"
    );
    for mapped in slots {
        let slot = conditions.capture_slot_value(mapped.slot()).unwrap();
        assert_eq!(slot.environment(), owner);
        assert_eq!(slot.closure(), published.nodes()[mapped.node()].closure());
        assert_eq!(slot.position(), mapped.position());
    }
    graph.nodes[first].sources[0].2 = vec![first];
    graph.nodes[first].sources[1].2 = vec![first];
    assert_eq!(
        graph.capture_layout(&[closures[0].0]),
        vec![(first, first_source, 0), (first, repeated_source, 2)]
    );
}

#[test]
fn self_recursive_capture_layout_registers_finite_static_slots() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "capture-static-cycle.ko",
            "fun run() { val a = move {}\nval b = move {}\nval seed = move {} }",
        )
        .unwrap();
    let parsed =
        crate::parser::parse_file(&sources, &crate::lexer::lex(&sources, source).unwrap()).unwrap();
    let closures = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Expression::Lambda { .. }).then_some((id, node.span()))
        })
        .collect::<Vec<_>>();
    assert_eq!(closures.len(), 3);
    let mut graph = PhiCaptureGraph::default();
    let a = graph.insert(closures[0].0);
    let b = graph.insert(closures[1].0);
    let seed = graph.insert(closures[2].0);
    for (node, candidates) in [(a, vec![a, b, seed]), (b, vec![a, seed])] {
        graph.nodes[node].sources.push((
            ClosureCaptureDescriptor::new(
                closures[node].0,
                ClosureCaptureSource::Symbol(SymbolId(node)),
                TypeId::new(0),
                ClosureCaptureMode::Owned,
                ClosureCaptureEffect::Move,
                closures[node].1,
            ),
            0,
            candidates,
        ));
    }
    assert_eq!(graph.recursive_origin(), Some(closures[a].0));
    let mut conditions = CleanupConditions::default();
    let owner = conditions.create_owner(CleanupOwnerValue::Closure {
        expression: closures[a].0,
        origin: closures[a].1,
        inputs: Vec::new(),
    });
    let layout = graph
        .register_capture_layout(&[closures[a].0], owner, &mut conditions)
        .into_iter()
        .map(|entry| (entry.node(), entry.position(), entry.slot()))
        .collect::<Vec<_>>();
    assert_eq!(layout.len(), 2, "the cycle needs only two static slots");
    assert_eq!(
        layout.iter().map(|(node, _, _)| *node).collect::<Vec<_>>(),
        [a, b]
    );
    for (node, position, slot) in layout {
        let registered = conditions.capture_slot_value(slot).unwrap();
        assert_eq!(registered.environment(), owner);
        assert_eq!(registered.closure(), graph.nodes[node].closure);
        assert_eq!(position, 0, "both captures use original position zero");
        assert_eq!(registered.position(), position);
        assert_eq!(registered.source(), graph.nodes[node].sources[0].0.source());
        assert_eq!(graph.nodes[node].sources[0].2.last(), Some(&seed));
    }
}
