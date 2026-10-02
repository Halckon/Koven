use super::*;

#[test]
fn planner_enclosing_environment_drop_reaches_owned_descendant() {
    let mut sources = SourceMap::new();
    let source = sources
            .add_source(
                "enclosing-release.ko",
                "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {\nval base: move () -> Unit = move { read(xs) }\nval outer: move () -> Unit = move {\nvar f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
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
    assert!(planner.enclosing_capture_phi.is_none());
    let f_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("f()")).then_some(id))
        .unwrap();
    let captured = planner
        .facts
        .iter()
        .filter(|fact| {
            fact.point() == DropPoint::CallReturn(f_call)
                && matches!(fact.target(), DropTarget::Captured { .. })
        })
        .collect::<Vec<_>>();
    assert!(
        captured.len() >= 2,
        "f must release both base and its owned xs"
    );
    // This checks the planner's private intermediate facts; the public plan remains deferred.
    let mut paths = captured
        .iter()
        .map(|fact| {
            planner
                .conditions
                .instance_address(fact.instance_address().unwrap())
                .unwrap()
                .capture_path()
                .to_vec()
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert!(paths.contains(&Vec::<usize>::new()));
    assert_eq!(paths.iter().filter(|path| *path == &vec![0]).count(), 1);
}

#[test]
fn enclosing_environment_keeps_opaque_parent_drop() {
    let mut sources = SourceMap::new();
    let source = sources
            .add_source(
                "enclosing-opaque.ko",
                "fun make(): move () -> Unit = move {}\nfun read(xs: List<Int>) {}\nfun run(flag: Boolean, own xs: List<Int>) {\nval base: move () -> Unit = if (flag) (move { read(xs) }) else (make())\nval outer: move () -> Unit = move {\nval inner: move () -> Unit = move { base() }\nval used = inner() }\nval used = outer() }",
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
    let inner_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("inner()")).then_some(id))
        .unwrap();
    let parent = planner
        .facts
        .iter()
        .filter(|fact| {
            fact.point() == DropPoint::CallReturn(inner_call)
                && matches!(
                    fact.target(),
                    DropTarget::Captured {
                        value: CleanupCaptureValue::Environment { .. },
                        ..
                    }
                )
        })
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(parent.len(), 2, "known and opaque branches both own base");
    let known = parent.iter().find(|fact| fact.owner().is_some()).unwrap();
    let opaque = parent.iter().find(|fact| fact.owner().is_none()).unwrap();
    assert_eq!(known.capture_slot(), opaque.capture_slot());
    assert_eq!(known.instance_address(), opaque.instance_address());
    let overlap = planner.conditions.and(
        known.condition().unwrap_or(CleanupConditionId::ALWAYS),
        opaque.condition().unwrap_or(CleanupConditionId::ALWAYS),
    );
    assert_eq!(overlap, CleanupConditionId::NEVER);
    let covered = planner.conditions.or(
        known.condition().unwrap_or(CleanupConditionId::ALWAYS),
        opaque.condition().unwrap_or(CleanupConditionId::ALWAYS),
    );
    assert_eq!(covered, CleanupConditionId::ALWAYS);
}
