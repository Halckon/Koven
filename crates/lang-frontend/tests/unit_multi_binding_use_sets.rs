//! Ordinary source research: bounded words are fixtures, not a new production limit.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitLoanAction, UnitLoanActionCoverage, check_compilation_unit_ownership,
        legacy_ownership_eq, with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

fn words() -> Vec<String> {
    let mut words = Vec::new();
    for length in 3..=6 {
        for mask in 1..(1usize << length) - 1 {
            let word: String = (0..length)
                .map(|bit| if mask & (1 << bit) == 0 { 'A' } else { 'B' })
                .collect();
            if word.bytes().filter(|b| *b == b'B').count() >= 2 {
                words.push(word);
            }
        }
    }
    words.extend(
        [
            "ABBBBBBBB",
            "BBBBBBBBA",
            "BBBBABBBB",
            "ABBBBABBBBA",
            "ABABABABABABABAB",
            "BBBBBBBBBA",
            "AAAAAAAAABB",
            "AB",
            "BA",
            "BBA",
            "ABB",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    words.sort();
    words.dedup();
    words
}
#[test]
fn ordinary_multi_binding_use_sets_close_exact_actual_returns() {
    for word in words() {
        for second in [false, true] {
            let (parameters, arguments) = if second {
                ("aux: String, source: String", "aux, owner")
            } else {
                ("source: String, aux: String", "owner, aux")
            };
            let calls = word
                .chars()
                .map(|c| {
                    if c == 'A' {
                        "consume(aux)"
                    } else {
                        "consume(item)"
                    }
                })
                .collect::<Vec<_>>()
                .join("; ");
            let text = format!(
                "fun view({parameters}): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{val owner = \"kept\"; val aux = \"other\"; borrow val item = view({arguments}); {calls}}}"
            );
            let mut sources = SourceMap::new();
            let source = sources.add_source("consumer-sequences.ko", &text).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            assert!(parsed.diagnostics().is_empty());
            let (environment, types) = standard_environments();
            for (layout, input) in [
                (
                    "rooted",
                    SourceUnitInput::new("root", "consumer-sequences.ko", source, &parsed),
                ),
                ("memory", SourceUnitInput::memory_document(source, &parsed)),
            ] {
                let inputs = [input];
                let index = index_compilation_unit(&sources, &inputs).unwrap();
                let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                    .unwrap()
                    .validate()
                    .unwrap();
                let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                    .unwrap()
                    .validate()
                    .unwrap();
                let on =
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                        .validate()
                        .unwrap();
                let off = with_loan_observation(false, || {
                    check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                        .unwrap()
                        .validate()
                        .unwrap()
                });
                assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
                assert_eq!(
                    on.ownership().owner_observations(),
                    off.ownership().owner_observations()
                );
                assert!(off.ownership().loan_action_observations().is_empty());
                let [catalog] = on.ownership().loan_action_observations() else {
                    panic!("catalog")
                };
                let [handoff] = on.ownership().borrow_call_handoffs() else {
                    panic!("{word}: raw handoff")
                };
                assert_eq!(handoff.auxiliary_loan_ends().len(), 1);
                assert_ne!(
                    handoff.source_loan().target(),
                    handoff.auxiliary_loan_ends()[0].target()
                );
                assert_eq!(on.ownership().borrow_results().bindings().len(), 1);
                assert_eq!(on.ownership().borrow_results().ends().len(), 1);
                assert_eq!(on.ownership().borrow_binding_transports().len(), 1);
                assert_eq!(on.ownership().loans().len(), 2 + word.len());
                let [relation] = catalog.distinct_auxiliary_retirements() else {
                    panic!("{word}: partial relation")
                };
                assert!(catalog.owns_distinct_auxiliary_retirement(relation));
                assert!(relation.auxiliary_drop().is_none());
                assert_eq!(relation.source_parameter(), usize::from(second));
                assert_eq!(
                    catalog.action(&relation.source_begin()),
                    Some(&UnitLoanAction::Begin(handoff.source_loan().clone()))
                );
                assert_eq!(
                    catalog.action(&relation.auxiliary_begin()),
                    Some(&UnitLoanAction::Begin(
                        handoff.auxiliary_loan_ends()[0].clone()
                    ))
                );
                let b_count = word.bytes().filter(|b| *b == b'B').count();
                let a_count = word.len() - b_count;
                let qualified = (1..=8).contains(&a_count) && (2..=8).contains(&b_count);
                let sets = catalog.binding_use_sets();
                assert_eq!(
                    sets.len(),
                    usize::from(qualified),
                    "{word}/{layout}: complete actual use set"
                );
                let retirements = catalog.binding_use_set_retirements();
                assert_eq!(
                    retirements.len(),
                    usize::from(qualified),
                    "{word}/{layout}: real source retirement and Drop"
                );
                if let [retirement] = retirements {
                    assert!(catalog.owns_binding_use_set_retirement(retirement));
                    assert_eq!(retirement.coverage(), UnitLoanActionCoverage::Partial);
                    assert_eq!(retirement.source_owner(), relation.source_owner());
                    assert!(catalog.owns_binding_use_set(retirement.use_set()));
                    assert_eq!(retirement.use_set(), &sets[0]);
                    assert!(
                        matches!(catalog.action(&retirement.source_drop()), Some(UnitLoanAction::Cleanup { action: lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop), .. }) if matches!(handoff.source_loan().target(), lang_frontend::ownership_checking::UnitLoanTarget::Place(place) if drop.target() == lang_frontend::ownership_checking::UnitDropTarget::Named(place.root())))
                    );
                }
                if let [set] = sets {
                    assert!(catalog.owns_binding_use_set(set));
                    assert_eq!(set.coverage(), UnitLoanActionCoverage::Partial);
                    assert_eq!(set.binding(), handoff.binding());
                    assert_eq!(set.source_begin(), relation.source_begin());
                    assert_eq!(set.handoff(), relation.handoff());
                    assert!(
                        matches!(catalog.action(&set.binding_end()), Some(UnitLoanAction::BorrowBindingEnd(end)) if end.binding() == set.binding())
                    );
                    let expected = on
                        .ownership()
                        .loans()
                        .iter()
                        .filter(|loan| {
                            loan.target() == handoff.source_loan().target()
                                && loan.call() != handoff.source_loan().call()
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(expected.len(), b_count);
                    assert_eq!(set.uses().len(), b_count);
                    let mut previous = None;
                    for (actual, loan) in set.uses().iter().zip(expected) {
                        assert_eq!(actual.loan(), loan);
                        assert_eq!(
                            catalog.action(&actual.begin()),
                            Some(&UnitLoanAction::Begin(loan.clone()))
                        );
                        assert_eq!(
                            actual.return_point(),
                            lang_frontend::ownership_checking::UnitDropPoint::CallReturn(
                                loan.call()
                            )
                        );
                        if let Some(cursor) = previous {
                            assert!(actual.return_cursor() > cursor);
                        }
                        assert!(actual.return_cursor() <= catalog.actions().len());
                        previous = Some(actual.return_cursor());
                    }
                    for (i, used) in set.uses().iter().enumerate() {
                        assert!(
                            set.uses()[..i]
                                .iter()
                                .all(|prior| prior.begin() != used.begin())
                        );
                    }
                }
                // Independent collection must not turn a repeated use into the old unique-use retirement.
                assert_eq!(
                    catalog
                        .actions()
                        .iter()
                        .filter(|a| matches!(a, UnitLoanAction::BorrowBindingSourceRetired(_)))
                        .count(),
                    usize::from(b_count == 1)
                );
            }
        }
    }
}
