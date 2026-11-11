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
fn ordinary_auxiliary_source_second_requires_complete_interval() {
    let cases = [(
        "source-second",
        "fun view(aux: String, source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); consume(item) }",
        1,
    )];
    for (name, text, expected_auxiliary) in cases {
        let mut sources = SourceMap::new();
        let source = sources.add_source(name, text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for (layout, input) in [
            (
                "rooted",
                SourceUnitInput::new("root", name, source, &parsed),
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
            let on = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
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
            let [raw] = on.ownership().borrow_call_handoffs() else {
                panic!("{name}/{layout}: actual handoff")
            };
            assert_eq!(raw.auxiliary_loan_ends().len(), expected_auxiliary);
            assert!(!raw.auxiliary_loan_ends().contains(raw.source_loan()));
            let [catalog] = on.ownership().loan_action_observations() else {
                panic!("one catalog")
            };
            assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
            assert!(catalog.binding_regions().is_empty());
            let [region] = catalog.auxiliary_binding_regions() else {
                panic!("one auxiliary region")
            };
            assert!(catalog.owns_auxiliary_binding_region(region));
            assert_eq!(region.action_ids().count(), 8);
            let [auxiliary] = raw.auxiliary_loan_ends() else {
                panic!("one auxiliary")
            };
            assert!(
                matches!(&catalog.actions()[0], UnitLoanAction::Begin(actual) if actual == auxiliary)
            );
            assert!(
                matches!(&catalog.actions()[1], UnitLoanAction::Begin(actual) if actual == raw.source_loan())
            );
            let [relation] = catalog.auxiliary_retirements() else {
                panic!("one relation")
            };
            assert_eq!(relation.source_begin(), catalog.action_id(1).unwrap());
            assert_eq!(relation.auxiliary_begin(), catalog.action_id(0).unwrap());
            for action in catalog.actions() {
                match action {
                    UnitLoanAction::BorrowBindingUseReturned(used) => {
                        assert_eq!(used.source_begin(), catalog.action_id(1))
                    }
                    UnitLoanAction::BorrowBindingSourceRetired(retired) => {
                        assert_eq!(retired.source_begin(), catalog.action_id(1))
                    }
                    _ => {}
                }
            }
            let actual_use_returns = catalog.actions().iter().filter(|action| {
                matches!(action, UnitLoanAction::BorrowBindingUseReturned(used) if used.binding() == raw.binding())
            }).count();
            assert_eq!(
                catalog
                    .auxiliary_binding_regions()
                    .iter()
                    .filter(|r| r.coverage() == UnitLoanActionCoverage::Complete)
                    .count(),
                1,
                "{name}/{layout}: source-second needs independent chronological proof"
            );
            let actual_source_retirements = catalog.actions().iter().filter(|action| {
                matches!(action, UnitLoanAction::BorrowBindingSourceRetired(retired) if retired.transaction().binding() == raw.binding())
            }).count();
            assert_eq!(
                actual_source_retirements, 1,
                "{name}/{layout}: one actual source retirement"
            );
            println!(
                "case={name}; layout={layout}; old_products_equal=true; raw_aux={expected_auxiliary}; auxiliary_relations={}; actual_use_returns={actual_use_returns}; actual_source_retirements={actual_source_retirements}",
                catalog.auxiliary_retirements().len()
            );
        }
    }
}
