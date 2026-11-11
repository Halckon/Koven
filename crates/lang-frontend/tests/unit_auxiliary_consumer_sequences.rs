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
    for length in 2..=4 {
        for mask in 1..(1usize << length) - 1 {
            words.push(
                (0..length)
                    .rev()
                    .map(|bit| if mask & (1 << bit) == 0 { 'A' } else { 'B' })
                    .collect(),
            );
        }
    }
    assert_eq!(words.len(), 22);
    assert_eq!(
        words
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        22
    );
    words.push("BAAAAAAAA".to_owned());
    words
}
#[test]
fn ordinary_auxiliary_consumer_sequences_preserve_actual_capabilities() {
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
                let mut returned = 0;
                let mut retired = 0;
                for action in catalog.actions() {
                    match action {
                        UnitLoanAction::BorrowBindingUseReturned(fact) => {
                            returned += 1;
                            assert_eq!(fact.source_begin(), Some(relation.source_begin()));
                            assert_eq!(fact.handoff(), Some(relation.handoff()));
                            assert_eq!(
                                catalog.action(&fact.begin().unwrap()),
                                Some(&UnitLoanAction::Begin(fact.loan().clone()))
                            );
                        }
                        UnitLoanAction::BorrowBindingSourceRetired(fact) => {
                            retired += 1;
                            assert_eq!(fact.source_begin(), Some(relation.source_begin()));
                            assert_eq!(fact.handoff(), Some(relation.handoff()));
                            assert!(matches!(
                                catalog.action(&fact.use_return().unwrap()),
                                Some(UnitLoanAction::BorrowBindingUseReturned(_))
                            ));
                            assert!(matches!(
                                catalog.action(&fact.binding_end().unwrap()),
                                Some(UnitLoanAction::BorrowBindingEnd(_))
                            ));
                        }
                        _ => {}
                    }
                }
                // Current observation limitation, not a language restriction: one return
                // is recorded, but repeated B loses the single-use retirement link.
                assert_eq!(returned, 1, "{word}/{layout}: first actual binding return");
                assert_eq!(
                    retired,
                    usize::from(word.bytes().filter(|b| *b == b'B').count() == 1),
                    "{word}/{layout}: single-use causal capacity"
                );
                assert!(catalog.distinct_auxiliary_binding_regions().is_empty());
                assert_eq!(
                    catalog.later_auxiliary_binding_regions().len(),
                    usize::from(word == "BA")
                );
                for region in catalog.later_auxiliary_binding_regions() {
                    assert!(catalog.owns_later_auxiliary_binding_region(region));
                    assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
                }
                assert!(catalog.auxiliary_binding_regions().is_empty());
                assert!(catalog.binding_regions().is_empty());
                println!(
                    "word={word}; source={}; layout={layout}; raw_loans={}; raw_transport=1; actual_use_return={returned}; linked_retirement={retired}; later_complete={}",
                    usize::from(second),
                    on.ownership().loans().len(),
                    catalog.later_auxiliary_binding_regions().len()
                );
            }
        }
    }
}
