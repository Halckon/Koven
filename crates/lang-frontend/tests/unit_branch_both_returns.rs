//! Ordinary explicit two-return source: independent local path coverage, never whole-catalog coverage.
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
    type_checking::{UnitItemId, check_compilation_unit_types, standard_environments},
};
const SOURCE: &str = "fun view(source: String): borrow String from source = source\nfun wrap(source: String, flag: Boolean): borrow String from source { if (flag) { return view(source) } else { return view(source) } }";

#[test]
fn ordinary_explicit_both_returns_requires_independent_path_coverage() {
    let mut sources = SourceMap::new();
    let source = sources.add_source("branch-coverage.ko", SOURCE).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let mut results = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "branch-coverage.ko", source, &parsed),
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
            panic!("catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert!(
            catalog.forward_regions().is_empty(),
            "old expression receipt input stays empty"
        );
        let handoffs = catalog
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowForwardHandoff(handoff) => Some(handoff),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(handoffs.len(), 2);
        assert_eq!(on.ownership().borrow_forward_transports().len(), 2);
        for (handoff, raw) in handoffs
            .iter()
            .zip(on.ownership().borrow_forward_transports())
        {
            assert_eq!(handoff.transaction(), raw);
            assert_eq!(
                handoff.callable(),
                Some(UnitItemId::new(
                    raw.returned_expression().source_unit(),
                    parsed.roots()[1]
                ))
            );
            assert_eq!(
                catalog.action(&handoff.source_begin().unwrap()),
                Some(&UnitLoanAction::Begin(raw.source_loan().clone()))
            );
            assert!(handoff.branch().is_some());
        }
        assert_ne!(handoffs[0].source_begin(), handoffs[1].source_begin());
        assert_ne!(
            handoffs[0].branch().unwrap().then_arm(),
            handoffs[1].branch().unwrap().then_arm()
        );
        let coverage = catalog
            .branch_forward_regions()
            .iter()
            .map(|region| region.coverage())
            .collect::<Vec<_>>();
        for region in catalog.branch_forward_regions() {
            assert!(catalog.owns_branch_forward_region(region));
            assert_eq!(
                region.topology(),
                UnitBorrowBranchForwardTopology::BothArmsReturn
            );
            assert_eq!(region.callable(), handoffs[0].callable().unwrap());
            let first = handoffs[0].branch().unwrap();
            assert_eq!(region.split(), first.split());
            assert_eq!(region.condition(), first.condition());
            let lang_frontend::ownership_checking::UnitLoanTarget::Place(place) =
                handoffs[0].transaction().source_loan().target()
            else {
                panic!("root source")
            };
            assert_eq!(region.source_parameter(), place.root());
            assert_eq!(
                region.paths().len(),
                2,
                "both declared exits must be certified"
            );
            let mut all = Vec::new();
            for path in region.paths() {
                let matching = handoffs
                    .iter()
                    .filter(|h| h.branch() == Some(path.branch()))
                    .collect::<Vec<_>>();
                assert_eq!(matching.len(), 1);
                let handoff = matching[0];
                assert_eq!(path.source_begin(), handoff.source_begin().unwrap());
                assert_eq!(
                    catalog.action(&path.live_handoff()),
                    Some(&UnitLoanAction::BorrowForwardHandoff((**handoff).clone()))
                );
                let ids = path.action_ids().collect::<Vec<_>>();
                assert_eq!(ids, vec![path.source_begin(), path.live_handoff()]);
                for id in ids {
                    assert!(
                        !all.contains(&id),
                        "mutually exclusive paths must not share record identities"
                    );
                    assert!(catalog.action(&id).is_some());
                    all.push(id);
                }
            }
            assert_ne!(
                region.paths()[0].branch().then_arm(),
                region.paths()[1].branch().then_arm()
            );
            assert_eq!(
                all.len(),
                catalog.actions().len(),
                "this original fixture has no hidden condition or path records"
            );
        }
        println!(
            "layout={layout}; both_modes_ownership_succeeded=true; actual_causes=2; source=Partial; branch_regions={}",
            coverage.len()
        );
        results.push((layout, coverage));
    }
    // Both layouts and all true-off preconditions must finish before the capability red.
    for (layout, coverage) in results {
        assert_eq!(
            coverage.len(),
            1,
            "{layout}: missing independent two-path region"
        );
        assert_eq!(coverage[0], UnitLoanActionCoverage::Complete);
    }
}
