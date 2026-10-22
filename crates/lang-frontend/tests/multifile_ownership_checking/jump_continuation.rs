//! Real source-qualified jump cleanup; no private liveness or ownership mutation.
use super::*;
use lang_frontend::{
    name_resolution::UnitReferenceTarget,
    ownership_checking::{
        UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit,
        check_compilation_unit_constant_ownership,
    },
};

// Equal local AST IDs and byte offsets in two packages must not alias. Reverse the
// input order as well, and inspect facts only after the public capability validates.
fn with_ownership(
    body: &str,
    mut inspect: impl FnMut(
        &SourceMap,
        &ParsedFile,
        SourceUnitId,
        &ValidatedCompilationUnitNames,
        &CompilationUnitOwnership,
    ),
) {
    for constants in [false, true] {
        for reverse in [false, true] {
            let mut sources = SourceMap::new();
            let (a, pa) = parsed(&mut sources, "p/a.ko", &format!("package p\n{body}"));
            let (b, pb) = parsed(&mut sources, "q/b.ko", &format!("package q\n{body}"));
            let mut inputs = [
                SourceUnitInput::new("root", "p/a.ko", a, &pa),
                SourceUnitInput::new("root", "q/b.ko", b, &pb),
            ];
            if reverse {
                inputs.reverse();
            }
            let (ne, te) = standard_environments();
            let names = validated_names(&sources, &inputs, &ne);
            let types = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
            let mut check = |owned: &CompilationUnitOwnership| {
                assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
                for (source, parsed) in [(a, &pa), (b, &pb)] {
                    inspect(&sources, parsed, source_unit(&names, source), &names, owned);
                }
            };
            if constants {
                let typed = types.validate_constants().unwrap();
                let owned = check_compilation_unit_constant_ownership(
                    &sources, &inputs, &names, &te, &typed,
                )
                .unwrap()
                .validate()
                .unwrap();
                check(owned.ownership());
            } else {
                let typed = types.validate().unwrap();
                let owned =
                    check_compilation_unit_ownership(&sources, &inputs, &names, &te, &typed)
                        .unwrap()
                        .validate()
                        .unwrap();
                check(owned.ownership());
            }
        }
    }
}

fn local(names: &ValidatedCompilationUnitNames, unit: SourceUnitId, name: &str) -> UnitSymbolId {
    let symbol = names.names().source_units()[unit.index()]
        .resolution()
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .unwrap()
        .id();
    names
        .names()
        .references()
        .iter()
        .find_map(|reference| match reference.target() {
            UnitReferenceTarget::Symbol(id)
                if id.source_unit() == unit && id.symbol() == symbol =>
            {
                Some(*id)
            }
            _ => None,
        })
        .unwrap()
}

fn named_drops(actions: &[Action], symbol: UnitSymbolId) -> usize {
    actions
        .iter()
        .filter(|action| {
            matches!(action, Action::Drop(fact) if fact.target() == UnitDropTarget::Named(symbol))
        })
        .count()
}

#[test]
fn dead_maps_drop_on_break_and_exhaustion_after_their_provider() {
    with_ownership(
        r#"fun run(flag: Boolean): Unit {
    var cache = mutableMapOf<Int, String>()
    var spare = mutableMapOf<Int, String>()
    for (key in arrayOf(1, 2)) {
        cache.put(key, "cache".clone())
        spare.put(key, "spare".clone())
        if (flag) { break }
        cache.contains(key)
        spare.contains(key)
    }
}"#,
        |sources, file, unit, names, owned| {
            let cache = local(names, unit, "cache");
            let spare = local(names, unit, "spare");
            let plan = owned
                .iterations()
                .iter()
                .find(|plan| plan.descriptor().statement().source_unit() == unit)
                .unwrap();
            let statement = plan.descriptor().statement();
            let jump = UnitExpressionId::new(unit, expression_with_text(sources, file, "break"));
            let exits = plan
                .exits()
                .iter()
                .filter(|exit| matches!(exit.kind(), Exit::Break(_) | Exit::Exhaustion))
                .collect::<Vec<_>>();
            assert_eq!(exits.len(), 2);
            for exit in exits {
                let point = match exit.kind() {
                    Exit::Break(id) => {
                        assert_eq!(id, jump);
                        UnitDropPoint::ControlTransfer(id)
                    }
                    Exit::Exhaustion => UnitDropPoint::LoopExit(statement),
                    _ => unreachable!(),
                };
                assert_eq!(exit.point(), point);
                assert_eq!(owned.iteration_cleanup_at(point), Some(exit.actions()));
                let actions = exit.actions();
                let position = |action| actions.iter().position(|item| *item == action).unwrap();
                let finish = position(Action::FinishProvider(statement));
                let end_source = position(Action::EndSource(statement));
                let drop_source = actions
                    .iter()
                    .position(|action| {
                        matches!(action, Action::Drop(fact) if fact.target() == UnitDropTarget::Temporary(plan.descriptor().source()))
                    })
                    .unwrap();
                let named = actions
                    .iter()
                    .enumerate()
                    .filter_map(|(index, action)| match action {
                        Action::Drop(fact) => match fact.target() {
                            UnitDropTarget::Named(symbol) => {
                                assert_eq!(symbol.source_unit(), unit);
                                assert_eq!(fact.value_origin().source_id(), file.source_id());
                                assert_eq!(fact.point(), point);
                                assert_eq!(
                                    owned.drops().iter().filter(|other| *other == fact).count(),
                                    1
                                );
                                Some((index, symbol))
                            }
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    named.iter().map(|(_, symbol)| *symbol).collect::<Vec<_>>(),
                    [spare, cache]
                );
                assert!(
                    finish < end_source && end_source < drop_source && drop_source < named[0].0
                );
                if matches!(exit.kind(), Exit::Break(_)) {
                    assert!(position(Action::EndElement(statement)) < finish);
                }
            }
            let fallthrough = plan
                .exits()
                .iter()
                .find(|exit| exit.kind() == Exit::Fallthrough)
                .unwrap();
            assert_eq!(named_drops(fallthrough.actions(), cache), 0);
            assert_eq!(named_drops(fallthrough.actions(), spare), 0);
        },
    );
}

#[test]
fn live_break_and_continue_keep_the_next_target_owner() {
    for jump in ["break", "continue"] {
        for live in [false, true] {
            let tail = if live { "cache.contains(9)" } else { "" };
            let text = format!(
                "fun run(flag: Boolean): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ cache.put(key, \"head\".clone()); if (flag) {{ {jump} }}; cache.put(9, \"tail\".clone()) }}; {tail} }}"
            );
            with_ownership(&text, |sources, file, unit, names, owned| {
                let cache = local(names, unit, "cache");
                let expression =
                    UnitExpressionId::new(unit, expression_with_text(sources, file, jump));
                let point = UnitDropPoint::ControlTransfer(expression);
                let cleanup = owned.iteration_cleanup_at(point).unwrap();
                assert_eq!(
                    named_drops(cleanup, cache),
                    usize::from(jump == "break" && !live),
                    "{jump}, live={live}"
                );
                if jump == "continue" {
                    assert!(
                        cleanup
                            .iter()
                            .any(|action| matches!(action, Action::EndElement(_)))
                    );
                    assert!(cleanup.iter().all(|action| {
                        !matches!(action, Action::FinishProvider(_) | Action::EndSource(_))
                    }));
                }
            });
        }
    }
}

#[test]
fn inner_break_keeps_outer_continuation_and_only_finishes_inner_provider() {
    with_ownership(
        r#"fun run(flag: Boolean): Unit {
    var cache = mutableMapOf<Int, String>()
    for (outer in arrayOf(1, 2)) {
        for (inner in arrayOf(3, 4)) {
            cache.put(inner, "inner".clone())
            if (flag) { break }
        }
        cache.put(outer, "outer".clone())
        if (flag) { break }
    }
}"#,
        |sources, file, unit, names, owned| {
            let cache = local(names, unit, "cache");
            let plans = owned
                .iterations()
                .iter()
                .filter(|plan| plan.descriptor().statement().source_unit() == unit)
                .collect::<Vec<_>>();
            assert_eq!(plans.len(), 2);
            for plan in &plans {
                let source = file
                    .ast()
                    .expressions()
                    .get(plan.descriptor().source().expression())
                    .unwrap()
                    .span();
                let inner = sources.slice(source).unwrap() == "arrayOf(3, 4)";
                let exit = plan
                    .exits()
                    .iter()
                    .find(|exit| matches!(exit.kind(), Exit::Break(_)))
                    .unwrap();
                let finishes = exit
                    .actions()
                    .iter()
                    .filter_map(|action| match action {
                        Action::FinishProvider(id) => Some(*id),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(finishes, [plan.descriptor().statement()]);
                assert_eq!(named_drops(exit.actions(), cache), usize::from(!inner));
            }
        },
    );
}
