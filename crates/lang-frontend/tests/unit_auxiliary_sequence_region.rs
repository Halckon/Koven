//! Ordinary original-source capability checkpoint, never synthetic relationship assembly.
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

#[test]
fn ordinary_auxiliary_sequence_requires_each_actual_return() {
    for count in 2..=8 {
        for second in [false, true] {
            let (parameters, arguments) = if second {
                ("aux: String, source: String", "aux, owner")
            } else {
                ("source: String, aux: String", "owner, aux")
            };
            let suffix = std::iter::repeat_n("consume(aux)", count)
                .collect::<Vec<_>>()
                .join("; ");
            let text = format!(
                "fun view({parameters}): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; val aux = \"aux\"; borrow val item = view({arguments}); consume(item); {suffix} }}"
            );
            let mut sources = SourceMap::new();
            let source = sources.add_source("later-aux.ko", &text).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            assert!(parsed.diagnostics().is_empty());
            let (environment, types) = standard_environments();
            for (layout, input) in [
                (
                    "rooted",
                    SourceUnitInput::new("root", "later-aux.ko", source, &parsed),
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
                assert!(catalog.distinct_auxiliary_binding_regions().is_empty());
                let [relation] = catalog.distinct_auxiliary_retirements() else {
                    panic!("relation")
                };
                assert!(relation.auxiliary_drop().is_none());
                assert_eq!(relation.source_parameter(), usize::from(second));
                assert_eq!(
                    catalog.auxiliary_sequence_regions().len(),
                    1,
                    "{layout}/source-second={second}: independently complete dynamic sequence"
                );
                let region = &catalog.auxiliary_sequence_regions()[0];
                assert!(catalog.owns_auxiliary_sequence_region(region));
                assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
                assert_eq!(region.source_parameter(), Some(usize::from(second)));
                assert_eq!(region.source_owner(), Some(relation.source_owner()));
                assert_eq!(region.auxiliary_owner(), Some(relation.auxiliary_owner()));
                let ids = region.action_ids().collect::<Vec<_>>();
                assert_eq!(ids.len(), 9 + 2 * count);
                assert_eq!(ids[usize::from(second)], relation.source_begin());
                assert_eq!(ids[1 - usize::from(second)], relation.auxiliary_begin());
                assert_eq!(ids[2], relation.handoff());
                assert!(catalog.later_auxiliary_binding_regions().is_empty());
                assert_eq!(region.uses().len(), count);
                for (i, actual) in region.uses().iter().enumerate() {
                    assert_eq!(actual.begin(), ids[8 + 2 * i]);
                    assert_eq!(actual.end(), ids[9 + 2 * i]);
                    assert_eq!(Some(actual.owner()), region.auxiliary_owner());
                    assert_eq!(
                        catalog.action(&actual.begin()),
                        Some(&UnitLoanAction::Begin(actual.loan().clone()))
                    );
                    assert!(
                        matches!(catalog.action(&actual.end()), Some(UnitLoanAction::Cleanup {point,action:lang_frontend::ownership_checking::UnitIterationCleanupAction::EndCallLoan(loan)}) if loan==actual.loan() && *point==lang_frontend::ownership_checking::UnitDropPoint::CallReturn(loan.call()))
                    );
                    assert_eq!(
                        actual.loan().target(),
                        on.ownership().borrow_call_handoffs()[0].auxiliary_loan_ends()[0].target()
                    );
                    if i > 0 {
                        assert_ne!(actual.loan().call(), region.uses()[i - 1].loan().call());
                    }
                }
                assert!(
                    matches!(catalog.action(&ids[7]), Some(UnitLoanAction::Cleanup {action:lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop),..}) if drop.target()==lang_frontend::ownership_checking::UnitDropTarget::Named(region.root()))
                );
                assert!(
                    matches!(catalog.action(ids.last().unwrap()), Some(UnitLoanAction::Cleanup {point,action:lang_frontend::ownership_checking::UnitIterationCleanupAction::Drop(drop)}) if Some(drop.target())==region.auxiliary_root().map(lang_frontend::ownership_checking::UnitDropTarget::Named) && *point==lang_frontend::ownership_checking::UnitDropPoint::CallReturn(region.uses().last().unwrap().loan().call()))
                );
            }
        }
    }
}
