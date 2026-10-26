//! SPEC-0357: real public Unit producers, never hand-constructed ownership facts.
use lang_frontend::{
    analysis::{
        UnitNameSnapshot, UnitSourceDescriptor, analyze_basic_unit_ownership, analyze_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipDeferredReason, UnitDropPoint, UnitDropTarget,
    },
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn analyze(body: &str, reverse: bool) -> (UnitNameSnapshot, CompilationUnitOwnership) {
    let packages = if reverse { ["b", "a"] } else { ["a", "b"] };
    analyze_sources(body, &packages)
}

fn analyze_sources(body: &str, packages: &[&str]) -> (UnitNameSnapshot, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let mut descriptors = Vec::new();
    for &package in packages {
        let path = format!("{package}/entry.ko");
        let source = sources
            .add_source(&path, format!("package {package}\n{body}"))
            .unwrap();
        descriptors.push(UnitSourceDescriptor::new(package, path, source));
    }
    let (names, types) = standard_environments();
    let snapshot = analyze_unit_names(sources, descriptors, names).unwrap();
    let inputs = snapshot.inputs();
    let names = snapshot.validated_names().unwrap();
    let typed = check_compilation_unit_types(snapshot.sources(), &inputs, names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let (_, owned) =
        analyze_basic_unit_ownership(snapshot.sources(), &inputs, names, &types, typed)
            .unwrap()
            .into_result()
            .unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_source_identities(&snapshot, &owned);
    (snapshot, owned)
}

#[test]
fn literal_candidates_have_comparison_completed_identity_in_both_source_orders() {
    for reverse in [false, true] {
        let (sources, owned) = analyze(
            "fun entry(): Unit { val subject = \"subject\"\nwhen (subject) { \"early\", (\"late\") -> {}\n\"last\" -> {} } }",
            reverse,
        );
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        assert!(owned.clone().validate().is_ok());
        let completed: Vec<_> = owned
            .drops()
            .iter()
            .filter_map(|fact| {
                if let UnitDropPoint::WhenComparisonCompleted {
                    control,
                    entry,
                    alternative,
                } = fact.point()
                {
                    Some((control, entry, alternative, fact))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(completed.len(), 6);
        for (control, entry, alternative, fact) in completed {
            let UnitDropTarget::Temporary(candidate) = fact.target() else {
                panic!("candidate owner")
            };
            assert_eq!(candidate.source_unit(), control.source_unit());
            assert_eq!(
                (entry, alternative),
                match sources.sources().slice(fact.value_origin()).unwrap() {
                    "\"early\"" => (0, 0),
                    "\"late\"" => (0, 1),
                    "\"last\"" => (1, 0),
                    other => panic!("{other}"),
                }
            );
            assert!(
                !owned
                    .drops()
                    .iter()
                    .any(|other| other.target() == fact.target() && other.point() != fact.point()),
                "one completion point per candidate"
            );
        }
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(
                    |fact| matches!(fact.point(), UnitDropPoint::WhenAlternativeMatch { .. })
                        && matches!(fact.target(), UnitDropTarget::Named(_))
                )
                .count(),
            6
        );
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(|fact| matches!(
                    fact.point(),
                    UnitDropPoint::WhenAlternativeNoMatch {
                        entry: 1,
                        alternative: 0,
                        ..
                    }
                ) && matches!(fact.target(), UnitDropTarget::Named(_)))
                .count(),
            2
        );
    }
}

#[test]
fn subject_temporaries_end_on_selected_edges_before_body_transfers() {
    for body in ["{}", "{ return }"] {
        let (sources, owned) = analyze(
            &format!(
                "fun entry(): Unit {{ when (\"subject\") {{ \"early\", \"late\" -> {body}\nelse -> {body} }} }}"
            ),
            false,
        );
        assert!(owned.deferred().is_empty());
        let subject_drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|fact| sources.sources().slice(fact.value_origin()).unwrap() == "\"subject\"")
            .collect();
        assert_eq!(subject_drops.len(), 6);
        for source in sources.names().source_units() {
            let edges: Vec<_> = subject_drops
                .iter()
                .filter_map(|fact| match fact.point() {
                    UnitDropPoint::WhenAlternativeMatch {
                        control,
                        entry,
                        alternative,
                    } if control.source_unit() == source.source_unit() => {
                        Some((true, entry, alternative))
                    }
                    UnitDropPoint::WhenAlternativeNoMatch {
                        control,
                        entry,
                        alternative,
                    } if control.source_unit() == source.source_unit() => {
                        Some((false, entry, alternative))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(edges, vec![(true, 0, 0), (true, 0, 1), (false, 0, 1)]);
        }

        assert!(subject_drops.iter().all(|fact| matches!(
            fact.point(),
            UnitDropPoint::WhenAlternativeMatch { .. }
                | UnitDropPoint::WhenAlternativeNoMatch {
                    entry: 0,
                    alternative: 1,
                    ..
                }
        )));
        assert!(
            subject_drops
                .iter()
                .all(|fact| matches!(fact.target(), UnitDropTarget::Temporary(_)))
        );
    }
}

#[test]
fn reused_and_borrowed_subjects_are_not_released_at_comparisons() {
    for body in [
        "fun entry(): Unit { val subject = \"subject\"\nwhen (subject) { \"early\" -> { println(subject) } }\nprintln(subject) }",
        "fun entry(subject: String): Unit { when (subject) { \"early\" -> {} } }",
    ] {
        let (_, owned) = analyze(body, false);
        assert!(owned.deferred().is_empty());
        assert!(!owned.drops().iter().any(|fact| matches!(
            fact.target(),
            UnitDropTarget::Named(_)
        ) && matches!(
            fact.point(),
            UnitDropPoint::WhenAlternativeMatch { .. }
                | UnitDropPoint::WhenAlternativeNoMatch { .. }
        )));
    }
}

#[test]
fn unsupported_candidate_forms_are_explicit_deferred_not_complete_plans() {
    for candidate in [
        "subject",
        "make()",
        "if (true) { return } else { \"late\" }",
    ] {
        let (_, owned) = analyze(
            &format!(
                "fun make(): String = \"made\"\nfun entry(): Unit {{ val subject = \"subject\"\nwhen (subject) {{ {candidate} -> {{}} }} }}"
            ),
            false,
        );
        assert_eq!(
            owned
                .deferred()
                .iter()
                .filter(
                    |fact| fact.reason() == OwnershipDeferredReason::StringWhenComparisonLifetime
                )
                .count(),
            2
        );
        assert!(owned.clone().validate().is_err());
        assert!(!owned.drops().iter().any(|fact| matches!(
            fact.point(),
            UnitDropPoint::WhenComparisonCompleted { .. }
                | UnitDropPoint::WhenAlternativeNoMatch { .. }
        )));
    }
}

#[test]
fn empty_and_only_else_subjects_have_no_fake_comparison_edge() {
    for entries in ["", "else -> {}"] {
        let (sources, owned) = analyze(
            &format!("fun entry(): Unit {{ when ((\"subject\")) {{ {entries} }} }}"),
            false,
        );
        assert!(owned.deferred().is_empty());
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|fact| sources.sources().slice(fact.value_origin()).unwrap() == "\"subject\"")
            .collect();
        assert_eq!(drops.len(), 2);
        assert!(
            drops
                .iter()
                .all(|fact| matches!(fact.point(), UnitDropPoint::AfterExpression(_)))
        );
        assert!(!owned.drops().iter().any(|fact| matches!(
            fact.point(),
            UnitDropPoint::WhenComparisonCompleted { .. }
                | UnitDropPoint::WhenAlternativeNoMatch { .. }
        )));
    }
}

#[test]
fn constant_capability_also_rejects_unsupported_string_when() {
    use lang_frontend::ownership_checking::check_compilation_unit_constant_ownership;
    let (snapshot, _) = analyze(
        "fun entry(): Unit { val subject = \"subject\"\nwhen (subject) { subject -> {} } }",
        false,
    );
    let (_, types) = standard_environments();
    let inputs = snapshot.inputs();
    let names = snapshot.validated_names().unwrap();
    let typed = check_compilation_unit_types(snapshot.sources(), &inputs, names, &types)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned = check_compilation_unit_constant_ownership(
        snapshot.sources(),
        &inputs,
        names,
        &types,
        &typed,
    )
    .unwrap();
    assert_eq!(
        owned
            .ownership()
            .deferred()
            .iter()
            .filter(|fact| fact.reason() == OwnershipDeferredReason::StringWhenComparisonLifetime)
            .count(),
        2
    );
    assert!(owned.validate().is_err());
}

#[test]
fn owned_call_subject_and_body_abrupt_edges_do_not_duplicate_temporary_cleanup() {
    for selected in ["return", "error(\"stop\")", "break", "continue"] {
        let body = format!(
            "fun make(): String = \"subject\"\nfun entry(): Unit {{ loop {{ when (make()) {{ \"early\" -> {{ {selected} }}\nelse -> {{ break }} }}\nbreak }} }}"
        );
        let (sources, owned) = analyze(&body, false);
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|fact| sources.sources().slice(fact.value_origin()).unwrap() == "make()")
            .collect();
        assert_eq!(drops.len(), 4, "one match and one no-match per real source");
        assert!(drops.iter().all(|fact| matches!(
            fact.point(),
            UnitDropPoint::WhenAlternativeMatch { .. }
                | UnitDropPoint::WhenAlternativeNoMatch { .. }
        )));
    }
}

fn assert_source_identities(snapshot: &UnitNameSnapshot, owned: &CompilationUnitOwnership) {
    use lang_frontend::{
        parser::{Expression, WhenCondition},
        type_checking::UnitExpressionId,
    };
    for fact in owned.drops() {
        let (control, entry, alternative, completed) = match fact.point() {
            UnitDropPoint::WhenComparisonCompleted {
                control,
                entry,
                alternative,
            } => (control, entry, alternative, true),
            UnitDropPoint::WhenAlternativeMatch {
                control,
                entry,
                alternative,
            }
            | UnitDropPoint::WhenAlternativeNoMatch {
                control,
                entry,
                alternative,
            } => (control, entry, alternative, false),
            _ => continue,
        };
        let names = snapshot
            .names()
            .source_units()
            .iter()
            .find(|source| source.source_unit() == control.source_unit())
            .unwrap();
        let parsed = &snapshot.parsed_files()[control.source_unit().index()];
        let arena = parsed.ast().expressions();
        let Expression::When {
            subject: Some(subject),
            entries,
            ..
        } = arena.get(control.expression()).unwrap().payload()
        else {
            panic!("real subject when identity")
        };
        let normalize = |mut expression| {
            while let Expression::Group { expression: inner } =
                arena.get(expression).unwrap().payload()
            {
                expression = *inner;
            }
            expression
        };
        let WhenCondition::Expression(candidate) = &entries[entry].conditions[alternative] else {
            panic!("literal candidate identity")
        };
        assert_eq!(
            fact.value_origin().source_id(),
            arena.get(control.expression()).unwrap().span().source_id()
        );
        if completed {
            let candidate = normalize(*candidate);
            assert_eq!(
                fact.target(),
                UnitDropTarget::Temporary(UnitExpressionId::new(control.source_unit(), candidate))
            );
            assert_eq!(fact.value_origin(), arena.get(candidate).unwrap().span());
        } else {
            let subject = normalize(*subject);
            match arena.get(subject).unwrap().payload() {
                Expression::Name => {
                    let text = snapshot
                        .sources()
                        .slice(arena.get(subject).unwrap().span())
                        .unwrap();
                    let symbols: Vec<_> = names
                        .resolution()
                        .symbols()
                        .iter()
                        .filter(|symbol| symbol.name() == text)
                        .collect();
                    assert_eq!(
                        symbols.len(),
                        1,
                        "unique real subject binding in these fixtures"
                    );
                    let UnitDropTarget::Named(symbol) = fact.target() else {
                        panic!("named subject")
                    };
                    assert_eq!(symbol.source_unit(), control.source_unit());
                    assert_eq!(symbol.symbol(), symbols[0].id());
                }
                _ => assert_eq!(
                    fact.target(),
                    UnitDropTarget::Temporary(UnitExpressionId::new(
                        control.source_unit(),
                        subject
                    ))
                ),
            }
        }
    }
}

#[test]
fn body_only_use_retains_subject_on_match_but_not_on_final_miss() {
    let (_, owned) = analyze(
        "fun entry(): Unit { val subject = \"subject\"\nwhen (subject) { \"early\" -> { println(subject) } } }",
        false,
    );
    assert!(
        !owned
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), UnitDropTarget::Named(_))
                && matches!(fact.point(), UnitDropPoint::WhenAlternativeMatch { .. }))
    );
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), UnitDropTarget::Named(_))
                && matches!(
                    fact.point(),
                    UnitDropPoint::WhenAlternativeNoMatch {
                        entry: 0,
                        alternative: 0,
                        ..
                    }
                ))
            .count(),
        2
    );
    let (_, owned) = analyze(
        "fun entry(): Unit { val subject = \"subject\"\nwhen (subject) { \"early\" -> {} }\nprintln(subject) }",
        false,
    );
    assert!(
        !owned
            .drops()
            .iter()
            .any(|fact| matches!(fact.target(), UnitDropTarget::Named(_))
                && matches!(
                    fact.point(),
                    UnitDropPoint::WhenAlternativeMatch { .. }
                        | UnitDropPoint::WhenAlternativeNoMatch { .. }
                ))
    );
}

#[test]
fn non_string_and_subjectless_controls_keep_their_existing_facts() {
    for body in [
        "fun entry(): Unit { when (1) { 1 -> {} } }",
        "fun entry(): Unit { when { true -> {} } }",
        "fun entry(subject: String?): Unit { when (subject) { null -> {}\nelse -> {} } }",
    ] {
        let (_, owned) = analyze(body, false);
        assert!(
            !owned
                .deferred()
                .iter()
                .any(|fact| fact.reason() == OwnershipDeferredReason::StringWhenComparisonLifetime)
        );
        assert!(!owned.drops().iter().any(|fact| matches!(
            fact.point(),
            UnitDropPoint::WhenComparisonCompleted { .. }
                | UnitDropPoint::WhenAlternativeNoMatch { .. }
        )));
    }
}

#[test]
fn projected_borrow_result_and_interpolating_forms_remain_deferred() {
    for body in [
        "class Holder(val text: String)\nfun entry(holder: Holder): Unit { when (holder.text) { \"early\" -> {} } }",
        "fun entry(items: List<String>): Unit { when (items[0]) { \"early\" -> {} } }",
        "fun view(subject: String): borrow String from subject = subject\nfun entry(subject: String): Unit { when (view(subject)) { \"early\" -> {} } }",
        "fun entry(subject: String): Unit { when (subject) { \"${subject}\" -> {} } }",
    ] {
        let (_, owned) = analyze(body, false);
        assert!(
            owned
                .deferred()
                .iter()
                .any(|fact| fact.reason() == OwnershipDeferredReason::StringWhenComparisonLifetime)
        );
        assert!(owned.validate().is_err());
    }
}

#[test]
fn single_source_frontend_publishes_the_same_explicit_edge_contract() {
    let (snapshot, owned) = analyze_sources(
        "fun entry(): Unit { when (\"subject\") { \"early\" -> {} } }",
        &["a"],
    );
    assert_eq!(snapshot.parsed_files().len(), 1);
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.drops().len(), 3);
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|fact| matches!(
                fact.point(),
                UnitDropPoint::WhenComparisonCompleted {
                    entry: 0,
                    alternative: 0,
                    ..
                }
            ))
            .count(),
        1
    );
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|fact| matches!(
                fact.point(),
                UnitDropPoint::WhenAlternativeMatch {
                    entry: 0,
                    alternative: 0,
                    ..
                }
            ))
            .count(),
        1
    );
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|fact| matches!(
                fact.point(),
                UnitDropPoint::WhenAlternativeNoMatch {
                    entry: 0,
                    alternative: 0,
                    ..
                }
            ))
            .count(),
        1
    );
}

#[test]
fn existing_string_binary_cleanup_retains_exact_operand_and_point_identities() {
    use lang_frontend::{parser::Expression, type_checking::UnitExpressionId};
    let (snapshot, owned) = analyze("fun entry(): Boolean = \"left\" == \"right\"", false);
    assert!(owned.clone().validate().is_ok());
    assert_eq!(owned.drops().len(), 4);
    for (parsed, names) in snapshot
        .parsed_files()
        .iter()
        .zip(snapshot.names().source_units())
    {
        let binaries: Vec<_> = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| match node.payload() {
                Expression::Binary { left, right, .. } => Some((id, *left, *right)),
                _ => None,
            })
            .collect();
        let [(binary, left, right)] = binaries.as_slice() else {
            panic!("one actual binary")
        };
        let unit = |expression| UnitExpressionId::new(names.source_unit(), expression);
        let drops: Vec<_> = owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == UnitDropPoint::AfterBinaryOperands(unit(*binary)))
            .collect();
        assert_eq!(drops.len(), 2);
        assert_eq!(drops[0].target(), UnitDropTarget::Temporary(unit(*right)));
        assert_eq!(drops[1].target(), UnitDropTarget::Temporary(unit(*left)));
    }
}

#[test]
fn outer_pending_borrow_and_loop_successor_keep_named_subject_alive() {
    for body in [
        "fun outer(text: String, marker: Int): Unit {}\nfun entry(): Unit { val subject = \"subject\"\nouter(subject, when (subject) { \"early\" -> 1\nelse -> 2 }) }",
        "fun entry(): Unit { val subject = \"subject\"\nloop { when (subject) { \"early\" -> { break }\nelse -> { break } } }\nprintln(subject) }",
    ] {
        let (snapshot, owned) = analyze(body, false);
        assert!(owned.clone().validate().is_ok());
        assert!(!owned.drops().iter().any(|fact| matches!(
            fact.target(),
            UnitDropTarget::Named(_)
        ) && matches!(
            fact.point(),
            UnitDropPoint::WhenAlternativeMatch { .. }
                | UnitDropPoint::WhenAlternativeNoMatch { .. }
        )));
        for names in snapshot.names().source_units() {
            let subject = names
                .resolution()
                .symbols()
                .iter()
                .find(|symbol| symbol.name() == "subject")
                .unwrap();
            let drops: Vec<_> = owned.drops().iter().filter(|fact| matches!(fact.target(), UnitDropTarget::Named(symbol) if symbol.source_unit() == names.source_unit() && symbol.symbol() == subject.id())).collect();
            assert_eq!(
                drops.len(),
                1,
                "outer call/loop successor eventually releases its owner"
            );
            let point = match drops[0].point() {
                UnitDropPoint::AfterExpression(expression)
                | UnitDropPoint::CallReturn(expression) => expression,
                other => panic!("expected the later actual call/read point, got {other:?}"),
            };
            let parsed = &snapshot.parsed_files()[names.source_unit().index()];
            let text = snapshot
                .sources()
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(point.expression())
                        .unwrap()
                        .span(),
                )
                .unwrap();
            assert!(
                text.starts_with("outer(") || text.starts_with("println(") || text == "subject"
            );
        }
    }
}
