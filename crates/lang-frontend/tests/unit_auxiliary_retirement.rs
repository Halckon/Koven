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

const SOURCE: &str = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); consume(item) }";

#[test]
fn ordinary_auxiliary_retirement_requires_actual_acquisition_links() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("auxiliary-retirement.ko", SOURCE)
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let mut completed = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "auxiliary-retirement.ko", source, &parsed),
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
        let [catalog] = on.ownership().loan_action_observations() else {
            panic!("one catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert!(catalog.binding_regions().is_empty());
        let [raw] = on.ownership().borrow_call_handoffs() else {
            panic!("one raw handoff")
        };
        let [aux] = raw.auxiliary_loan_ends() else {
            panic!("one raw auxiliary retirement")
        };
        assert_ne!(raw.source_loan().argument(), aux.argument());
        assert_eq!(raw.source_loan().target(), aux.target());
        assert!(catalog.distinct_auxiliary_retirements().is_empty());
        assert!(catalog.distinct_auxiliary_binding_regions().is_empty());
        assert_eq!(raw.source_loan().call(), aux.call());
        assert!(!raw.auxiliary_loan_ends().contains(raw.source_loan()));
        for loan in [raw.source_loan(), aux] {
            assert_eq!(
                catalog
                    .actions()
                    .iter()
                    .filter(|action| {
                        matches!(action, UnitLoanAction::Begin(actual) if actual == loan)
                    })
                    .count(),
                1
            );
        }
        assert_eq!(catalog.actions().iter().filter(|action| {
            matches!(action, UnitLoanAction::BorrowHandoffCommitted(actual) if actual == raw)
        }).count(), 1);
        println!(
            "layout={layout}; old_products_equal=true; raw_aux=1; relations={}",
            catalog.auxiliary_retirements().len()
        );
        completed.push((layout, catalog.clone(), raw.clone()));
    }
    // Both real analyses and true-off oracles finish before the missing-capability assertion.
    for (layout, catalog, raw) in completed {
        assert_eq!(
            catalog.auxiliary_retirements().len(),
            1,
            "{layout}: actual relation required"
        );
        let relation = &catalog.auxiliary_retirements()[0];
        assert_ne!(relation.source_begin(), relation.auxiliary_begin());
        assert_eq!(
            catalog.action(&relation.source_begin()),
            Some(&UnitLoanAction::Begin(raw.source_loan().clone()))
        );
        assert_eq!(
            catalog.action(&relation.auxiliary_begin()),
            Some(&UnitLoanAction::Begin(raw.auxiliary_loan_ends()[0].clone()))
        );
        assert_eq!(
            catalog.action(&relation.handoff()),
            Some(&UnitLoanAction::BorrowHandoffCommitted(raw))
        );
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
    }
}

#[test]
fn ordinary_auxiliary_retirement_neighbors_keep_partial_raw_handoffs() {
    let cases = [
        (
            "distinct-roots",
            "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; val aux = \"aux\"; borrow val item = view(owner, aux); consume(item) }",
            1,
        ),
        (
            "two-auxiliaries",
            "fun view(source: String, first: String, second: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner, owner); consume(item) }",
            2,
        ),
    ];
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
            if name == "distinct-roots" {
                let [relation] = catalog.distinct_auxiliary_retirements() else {
                    panic!("{name}/{layout}: separate two-owner relation");
                };
                assert!(catalog.owns_distinct_auxiliary_retirement(relation));
                let [region] = catalog.distinct_auxiliary_binding_regions() else {
                    panic!("{name}/{layout}: separate ASAP binding interval");
                };
                assert!(catalog.owns_distinct_auxiliary_binding_region(region));
                assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
                assert_eq!(region.action_ids().len(), 9);
                assert_eq!(region.source_owner(), Some(relation.source_owner()));
                assert_eq!(region.auxiliary_owner(), Some(relation.auxiliary_owner()));

                assert_ne!(relation.source_owner(), relation.auxiliary_owner());
                assert_eq!(
                    catalog.action(&relation.source_begin()),
                    Some(&UnitLoanAction::Begin(raw.source_loan().clone()))
                );
                assert_eq!(
                    catalog.action(&relation.auxiliary_begin()),
                    Some(&UnitLoanAction::Begin(raw.auxiliary_loan_ends()[0].clone()))
                );
                assert_eq!(
                    catalog.action(&relation.handoff()),
                    Some(&UnitLoanAction::BorrowHandoffCommitted(raw.clone()))
                );
            } else {
                assert!(catalog.distinct_auxiliary_retirements().is_empty());
            }

            assert!(catalog.binding_regions().is_empty());
            assert!(
                catalog.auxiliary_retirements().is_empty(),
                "{name}/{layout}: narrow relation remains absent"
            );
            println!(
                "case={name}; layout={layout}; old_products_equal=true; raw_aux={expected_auxiliary}; same_root_relations=0"
            );
        }
    }
}
