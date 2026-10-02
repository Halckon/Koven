use super::*;

#[test]
fn next_loop_entry_keeps_recursive_root_after_conditional_snapshot() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-snapshot-next-loop.ko",
            "fun run(first: List<Int>, second: List<Int>, pick: Boolean) {
var f: move () -> Unit = move {}
for (_ in first) { f = move { f() } }
var g: move () -> Unit = if (pick) f else (move {})
for (_ in second) { g = move { g() } }
val used = g() }",
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
    let first_exit = planner.loop_phis[&statements[0].index()]
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == f)
        .unwrap();
    let g = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("g"))
        .unwrap()
        .id();
    let second_header = planner.loop_phis[&statements[1].index()]
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == g)
        .unwrap();
    let entry = planner.loop_phi_incomings[&statements[1].index()]
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap()
        .bindings()
        .iter()
        .find(|binding| binding.target() == second_header.owner())
        .unwrap();
    assert_eq!(entry.values().len(), 1);
    let snapshot = entry.values()[0].source();
    let saved = planner.conditions.owner_snapshot(snapshot).unwrap();
    let graph = &planner.loop_capture_graphs[&statements[1].index()];
    let mut conditions = planner.conditions.clone();
    for old_root in first_exit.root_origins() {
        assert!(
            saved
                .copies()
                .iter()
                .any(|copy| copy.source() == old_root.selector())
        );
        let node = *second_header
            .root_nodes()
            .iter()
            .find(|&&node| graph.nodes()[node].closure() == old_root.closure())
            .unwrap();
        let root = entry
            .root_sources()
            .iter()
            .find(|source| source.node() == node && source.source() == snapshot)
            .unwrap();
        let saved_root = planner.snapshot_phi_roots[&snapshot]
            .iter()
            .find(|(closure, _, _)| *closure == old_root.closure())
            .unwrap();
        assert_eq!(
            root.condition(),
            conditions.and(entry.values()[0].condition(), saved_root.2)
        );
    }
}

#[test]
fn next_recursive_loop_entry_keeps_previous_phi_root_sources() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-next-loop.ko",
            "fun run(first: List<Int>, second: List<Int>) {
var f: move () -> Unit = move {}
for (_ in first) { f = move { f() } }
for (_ in second) { f = move { f() } }
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
    let statements = checker
        .iterations
        .values()
        .map(|plan| plan.descriptor().statement())
        .collect::<Vec<_>>();
    assert_eq!(statements.len(), 2);
    let first_exit = planner.loop_phis[&statements[0].index()]
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
        .unwrap();
    let second_entry = planner.loop_phi_incomings[&statements[1].index()]
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("f"))
        .unwrap()
        .id();
    let second_header = planner.loop_phis[&statements[1].index()]
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == f)
        .unwrap();
    let entry = second_entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == second_header.owner())
        .unwrap();
    assert_eq!(entry.values().len(), 1);
    assert_eq!(entry.values()[0].source(), first_exit.owner());
    assert!(!entry.root_sources().is_empty());
    assert!(entry.root_sources().iter().all(|source| {
        source.source() == first_exit.owner() && second_header.root_nodes().contains(&source.node())
    }));
    let graph = &planner.loop_capture_graphs[&statements[1].index()];
    let mut conditions = planner.conditions.clone();
    for source in entry.root_sources() {
        let closure = graph.nodes()[source.node()].closure();
        let previous = first_exit
            .root_origins()
            .find(|root| root.closure() == closure)
            .unwrap();
        assert_eq!(
            source.condition(),
            conditions.and(entry.values()[0].condition(), previous.condition())
        );
    }
}
