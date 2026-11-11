//! Legal neighboring programs must not inherit the exact later-use capability.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitLoanActionCoverage, check_compilation_unit_ownership, legacy_ownership_eq,
        with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn ordinary_interleaved_auxiliary_neighbors_preserve_old_products() {
    let mut cases = vec![("asap".to_owned(), "consume(item)".to_owned(), 0)];
    for n in 1..=8 {
        let mut calls = vec!["consume(aux)"; n];
        calls.insert(0, "consume(item)");
        cases.push((
            format!("old-prefix-zero-{n}"),
            calls.join("; "),
            if n == 1 { 1 } else { 2 },
        ));
    }
    for prefix in 0..=9 {
        let mut calls = vec!["consume(aux)"; 9];
        calls.insert(prefix, "consume(item)");
        cases.push((format!("over-budget-{prefix}"), calls.join("; "), 3));
    }
    cases.push((
        "multiple-B".into(),
        "consume(aux); consume(item); consume(item); consume(aux)".into(),
        3,
    ));
    cases.push((
        "grouped-prefix".into(),
        "consume((aux)); consume(item); consume(aux)".into(),
        3,
    ));
    for second in [false, true] {
        let (parameters, arguments) = if second {
            ("aux: String, source: String", "aux, owner")
        } else {
            ("source: String, aux: String", "owner, aux")
        };
        for (case, tail, legacy) in &cases {
            let text = format!(
                "fun view({parameters}): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(): Unit {{ val owner = \"kept\"; val aux = \"aux\"; borrow val item = view({arguments}); {tail} }}"
            );
            let mut sources = SourceMap::new();
            let source = sources.add_source("later-neighbor.ko", &text).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            assert!(
                parsed.diagnostics().is_empty(),
                "{case}: {:?}",
                parsed.diagnostics()
            );
            let (environment, types) = standard_environments();
            for (layout, input) in [
                (
                    "rooted",
                    SourceUnitInput::new("root", "later-neighbor.ko", source, &parsed),
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
                assert!(
                    legacy_ownership_eq(on.ownership(), off.ownership()),
                    "{case}/{layout}"
                );
                assert_eq!(
                    on.ownership().owner_observations(),
                    off.ownership().owner_observations(),
                    "{case}/{layout}"
                );
                assert!(off.ownership().loan_action_observations().is_empty());
                // The source program still produced its real borrow handoff. Rejection
                // here is the independent capability boundary, not invalid source.
                assert_eq!(
                    on.ownership().borrow_call_handoffs().len(),
                    1,
                    "{case}/{layout}"
                );
                let [catalog] = on.ownership().loan_action_observations() else {
                    panic!("{case}/{layout}: catalog");
                };
                assert!(
                    catalog.interleaved_auxiliary_regions().is_empty(),
                    "{case}/{layout}/source-second={second}"
                );
                if *legacy == 0 {
                    assert!(catalog.auxiliary_sequence_regions().is_empty());
                    assert!(catalog.later_auxiliary_binding_regions().is_empty());
                    let [old] = catalog.distinct_auxiliary_binding_regions() else {
                        panic!("{layout}: existing ASAP capability");
                    };
                    assert!(catalog.owns_distinct_auxiliary_binding_region(old));
                    assert_eq!(old.coverage(), UnitLoanActionCoverage::Complete);
                    assert_eq!(old.source_parameter(), Some(usize::from(second)));
                } else if *legacy == 1 {
                    assert!(catalog.auxiliary_sequence_regions().is_empty());
                    assert!(catalog.distinct_auxiliary_binding_regions().is_empty());
                    let [old] = catalog.later_auxiliary_binding_regions() else {
                        panic!("{layout}: existing Single capability");
                    };
                    assert!(catalog.owns_later_auxiliary_binding_region(old));
                    assert_eq!(old.coverage(), UnitLoanActionCoverage::Complete);
                    assert_eq!(old.source_parameter(), Some(usize::from(second)));
                } else if *legacy == 2 {
                    assert!(catalog.later_auxiliary_binding_regions().is_empty());
                    assert!(catalog.distinct_auxiliary_binding_regions().is_empty());
                    let [old] = catalog.auxiliary_sequence_regions() else {
                        panic!("old Sequence")
                    };
                    assert!(catalog.owns_auxiliary_sequence_region(old));
                    assert_eq!(old.coverage(), UnitLoanActionCoverage::Complete);
                    assert_eq!(old.source_parameter(), Some(usize::from(second)));
                } else {
                    assert!(catalog.auxiliary_sequence_regions().is_empty());
                    assert!(catalog.later_auxiliary_binding_regions().is_empty());
                    assert!(
                        catalog.distinct_auxiliary_binding_regions().is_empty(),
                        "{case}/{layout}"
                    );
                }
            }
        }
    }
}
