use super::*;

#[test]
fn moved_recursive_phi_releases_instances_through_new_binding() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-moved-owner.ko",
            "fun run(flags: List<Int>) {
var f: move () -> Unit = move {}
for (_ in flags) { f = move { f() } }
val h: move () -> Unit = f
val used = h() }",
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
    let h = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("h"))
        .unwrap()
        .id();
    let statement = checker
        .iterations
        .values()
        .next()
        .unwrap()
        .descriptor()
        .statement();
    let releases = planner
        .cleanup
        .iter()
        .filter(|(_, action)| {
            matches!(action, IterationCleanupAction::ReleaseClosureInstances {
                layout: ClosureReleaseLayout::Iteration(release_statement),
                root,
            } if *release_statement == statement && root.target() == DropTarget::Named(h))
        })
        .count();
    assert_eq!(releases, 1);
    assert!(!planner.cleanup.iter().any(|(_, action)| {
            matches!(action, IterationCleanupAction::Drop(fact) if fact.target() == DropTarget::Named(h))
        }));
}

#[test]
fn conditional_snapshot_of_recursive_phi_keeps_instance_release() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-snapshot-owner.ko",
            "fun run(flags: List<Int>, pick: Boolean) {
var f: move () -> Unit = move {}
for (_ in flags) { f = move { f() } }
val h: move () -> Unit = if (pick) f else (move {})
val used = h() }",
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
    let h = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("h"))
        .unwrap()
        .id();
    let release = planner.cleanup.iter().find_map(|(_, action)| match action {
        IterationCleanupAction::ReleaseClosureInstances { root, .. }
            if root.target() == DropTarget::Named(h) =>
        {
            Some(*root)
        }
        _ => None,
    });
    let release = release.expect("the recursive branch retains instance release");
    let ordinary = planner.cleanup.iter().find_map(|(_, action)| match action {
        IterationCleanupAction::Drop(fact) if fact.target() == DropTarget::Named(h) => Some(*fact),
        _ => None,
    });
    let ordinary = ordinary.expect("the other branch retains ordinary cleanup");
    assert_eq!(
        planner.conditions.and(
            release.condition().unwrap_or(CleanupConditionId::ALWAYS),
            ordinary.condition().unwrap_or(CleanupConditionId::ALWAYS)
        ),
        CleanupConditionId::NEVER
    );
}

#[test]
fn conditional_snapshot_of_independent_phi_does_not_duplicate_child_drop() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-independent-snapshot.ko",
            "fun run(flags: List<Int>, pick: Boolean) {
var f: move () -> Unit = move {}
val leaf: move () -> Unit = move {}
var g: move () -> Unit = move { leaf() }
for (_ in flags) { f = move { f() } }
val h: move () -> Unit = if (pick) g else (move {})
val a = f()
val b = h() }",
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
    let h = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("h"))
        .unwrap()
        .id();
    let root_drops = planner
        .cleanup
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::Drop(fact) if fact.target() == DropTarget::Named(h) => {
                Some((*point, *fact))
            }
            _ => None,
        });
    let root_drops = root_drops.collect::<Vec<_>>();
    assert!(!root_drops.is_empty());
    assert!(!planner.cleanup.iter().any(|(_, action)| matches!(action,
            IterationCleanupAction::ReleaseClosureInstances { root, .. }
                if root.target() == DropTarget::Named(h))));
    let point = root_drops[0].0;
    let children = planner
        .cleanup
        .iter()
        .filter(|(drop_point, action)| {
            matches!(action, IterationCleanupAction::Drop(fact)
                if *drop_point == point
                    && matches!(fact.target(), DropTarget::Captured { .. })
                    && sources.slice(fact.value_origin()) == Ok("leaf"))
        })
        .collect::<Vec<_>>();
    assert_eq!(children.len(), 1);
    let IterationCleanupAction::Drop(child) = children[0].1 else {
        unreachable!()
    };
    let mut conditions = planner.conditions.clone();
    let covered = root_drops
        .iter()
        .fold(CleanupConditionId::NEVER, |covered, (_, fact)| {
            conditions.or(
                covered,
                fact.condition().unwrap_or(CleanupConditionId::ALWAYS),
            )
        });
    let uncovered = conditions.not(covered);
    assert_eq!(
        conditions.and(
            child.condition().unwrap_or(CleanupConditionId::ALWAYS),
            uncovered
        ),
        CleanupConditionId::NEVER
    );
}

#[test]
fn captured_phi_release_does_not_duplicate_child_drop() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-captured-owner.ko",
            "fun run(flags: List<Int>) {
var f: move () -> Unit = move {}
val leaf: move () -> Unit = move {}
var g: move () -> Unit = move { leaf() }
for (_ in flags) { f = move { f() } }
val outer: move () -> Unit = move { val used = g() }
val used = outer()
val also = f() }",
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
    let g = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("g"))
        .unwrap()
        .id();
    let captured_drop = planner.cleanup.iter().find_map(|(point, action)| {
        let IterationCleanupAction::Drop(fact) = action else {
            return None;
        };
        match planner.conditions.owner_value(fact.owner()?) {
            Some(CleanupOwnerValue::IterationPhi { symbol, .. })
                if *symbol == g && matches!(fact.target(), DropTarget::Captured { .. }) =>
            {
                Some((*point, *fact))
            }
            _ => None,
        }
    });
    let (point, _) = captured_drop.expect("captured independent g keeps its ordinary drop");
    let children = planner
        .cleanup
        .iter()
        .filter(|(drop_point, action)| {
            matches!(action, IterationCleanupAction::Drop(fact)
                if *drop_point == point
                    && matches!(fact.target(), DropTarget::Captured { .. })
                    && sources.slice(fact.value_origin()) == Ok("leaf"))
        })
        .collect::<Vec<_>>();
    assert_eq!(children.len(), 1);
}

#[test]
fn mixed_phi_and_new_closure_versions_do_not_duplicate_instance_release() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "recursive-mixed-owner.ko",
            "fun run(flags: List<Int>, flag: Boolean) {
var f: move () -> Unit = move {}
val old_leaf: move () -> Unit = move {}
var g: move () -> Unit = move { old_leaf() }
for (_ in flags) { f = move { f() } }
if (flag) {
    val new_leaf: move () -> Unit = move {}
    g = move { new_leaf() }
}
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
    let g = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()) == Ok("g"))
        .unwrap()
        .id();
    let old_roots = planner
        .cleanup
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::Drop(fact)
                if fact.target() == DropTarget::Named(g)
                    && fact.owner().is_some_and(|owner| {
                        matches!(
                            planner.conditions.owner_value(owner),
                            Some(CleanupOwnerValue::IterationPhi { .. })
                        )
                    }) =>
            {
                Some(*fact)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !old_roots.is_empty(),
        "the independent g phi keeps its root drop"
    );
    assert!(planner.cleanup.iter().any(|(_, action)| match action {
        IterationCleanupAction::Drop(fact) => {
            sources.slice(fact.value_origin()) == Ok("old_leaf")
        }
        _ => false,
    }));
    assert!(
        planner.cleanup.iter().any(|(_, action)| match action {
            IterationCleanupAction::Drop(fact) => {
                sources.slice(fact.value_origin()) == Ok("new_leaf")
            }
            _ => false,
        }),
        "the ordinary new closure still releases its captured child"
    );
}
