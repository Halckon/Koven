//! Real same-root forwarding through two independently checked borrowed callees.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitBorrowBranchForwardTopology, UnitLoanAction, UnitLoanActionCoverage,
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{UnitCallTarget, check_compilation_unit_types, standard_environments},
};

#[test]
fn ordinary_distinct_borrowed_callees_require_two_certified_paths() {
    let mut results = Vec::new();
    for (topology, body) in [
        (
            UnitBorrowBranchForwardTopology::TailContinuation,
            "{ if (flag) { return left(source) }; return right(source) }",
        ),
        (
            UnitBorrowBranchForwardTopology::BothArmsReturn,
            "{ if (flag) { return left(source) } else { return right(source) } }",
        ),
    ] {
        let text = format!(
            "fun left(source: String): borrow String from source = source\nfun right(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source {body}"
        );
        let mut sources = SourceMap::new();
        let source = sources.add_source("distinct-callees.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let (environment, types) = standard_environments();
        for (layout, input) in [
            (
                "rooted",
                SourceUnitInput::new("root", "distinct-callees.ko", source, &parsed),
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
            let raw = on.ownership().borrow_forward_transports();
            assert_eq!(raw.len(), 2);
            assert_eq!(raw[0].source_loan().target(), raw[1].source_loan().target());
            let targets = raw
                .iter()
                .map(|t| {
                    let UnitCallTarget::Declaration(target) = typed
                        .types()
                        .call(t.returned_expression())
                        .unwrap()
                        .target()
                    else {
                        panic!("actual direct callee");
                    };
                    target
                })
                .collect::<Vec<_>>();
            assert_ne!(
                targets[0], targets[1],
                "each path must call a different actual declaration"
            );
            let [catalog] = on.ownership().loan_action_observations() else {
                panic!("one catalog");
            };
            assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
            assert!(catalog.forward_regions().is_empty());
            for region in catalog.branch_forward_regions() {
                assert!(catalog.owns_branch_forward_region(region));
                assert_eq!(region.topology(), topology);
                assert_eq!(region.paths().len(), 2);
                let mut ids = Vec::new();
                let mut path_targets = Vec::new();
                for path in region.paths() {
                    let UnitLoanAction::BorrowForwardHandoff(handoff) =
                        catalog.action(&path.live_handoff()).unwrap()
                    else {
                        panic!("real handoff");
                    };
                    assert_eq!(handoff.branch(), Some(path.branch()));
                    assert_eq!(handoff.source_begin(), Some(path.source_begin()));
                    assert_eq!(handoff.callable(), Some(region.callable()));
                    assert_eq!(
                        handoff.transaction().returned_expression(),
                        path.branch().value()
                    );
                    assert_eq!(
                        catalog.action(&path.source_begin()),
                        Some(&UnitLoanAction::Begin(
                            handoff.transaction().source_loan().clone()
                        ))
                    );
                    let raw_match = raw.iter().position(|r| r == handoff.transaction()).unwrap();
                    path_targets.push(targets[raw_match]);
                    assert_eq!(
                        path.action_ids().collect::<Vec<_>>(),
                        vec![path.source_begin(), path.live_handoff()]
                    );
                    for id in path.action_ids() {
                        assert!(!ids.contains(&id));
                        ids.push(id);
                    }
                }
                assert_ne!(path_targets[0], path_targets[1]);
                assert_ne!(
                    region.paths()[0].branch().then_arm(),
                    region.paths()[1].branch().then_arm()
                );
                assert_eq!(ids.len(), catalog.actions().len());
            }
            results.push((
                topology,
                layout,
                catalog
                    .branch_forward_regions()
                    .iter()
                    .map(|r| r.coverage())
                    .collect::<Vec<_>>(),
            ));
        }
    }
    // All eight real ownership analyses and their parity checks precede the capability assertion.
    for (topology, layout, coverage) in results {
        assert_eq!(
            coverage,
            vec![UnitLoanActionCoverage::Complete],
            "{topology:?}/{layout}: missing independent two-path coverage"
        );
    }
}

#[test]
fn ordinary_distinct_callees_keep_per_path_shape_and_contract_boundaries() {
    for (name, right, argument) in [
        (
            "grouped",
            "fun right(source: String): borrow String from source = source",
            "(source)",
        ),
        (
            "extra-parameter",
            "fun right(source: String, flag: Boolean): borrow String from source = source",
            "source, flag",
        ),
    ] {
        for explicit_else in [false, true] {
            let otherwise = if explicit_else {
                format!(" else {{ return right({argument}) }}")
            } else {
                format!("; return right({argument})")
            };
            let text = format!(
                "fun left(source: String): borrow String from source = source\n{right}\nfun wrap(source: String, flag: Boolean): borrow String from source {{ if (flag) {{ return left(source) }}{otherwise} }}"
            );
            let mut sources = SourceMap::new();
            let source = sources.add_source(name, text).unwrap();
            let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
            assert!(parsed.diagnostics().is_empty());
            let (environment, types) = standard_environments();
            for input in [
                SourceUnitInput::new("root", name, source, &parsed),
                SourceUnitInput::memory_document(source, &parsed),
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
                assert_eq!(on.ownership().borrow_forward_transports().len(), 2);
                let targets = on
                    .ownership()
                    .borrow_forward_transports()
                    .iter()
                    .map(|t| {
                        typed
                            .types()
                            .call(t.returned_expression())
                            .unwrap()
                            .target()
                    })
                    .collect::<Vec<_>>();
                assert_ne!(targets[0], targets[1]);
                for catalog in on.ownership().loan_action_observations() {
                    assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
                    assert!(
                        catalog.branch_forward_regions().is_empty(),
                        "{name}/{explicit_else}: one unsuitable path cannot borrow the other path's eligibility"
                    );
                }
            }
        }
    }
}
