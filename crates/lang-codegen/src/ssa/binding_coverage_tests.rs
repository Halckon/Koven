//! Ordinary same-analysis complete coverage, without changing actual products.
use super::*;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};

#[test]
fn single_binding_requires_independent_complete_region() {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(
        &mut sources,
        "binding-coverage.ko",
        super::binding_causal_links::SOURCE,
    );
    let (environment, types) = standard_environments();
    for input in [
        SourceUnitInput::new("root", "binding-coverage.ko", source, &parsed),
        SourceUnitInput::memory_document(source, &parsed),
    ] {
        let inputs = [input];
        let (names, typed, on) =
            unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let off = with_loan_observation(false, || {
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap()
        });
        assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
        assert!(off.ownership().loan_action_observations().is_empty());
        let run = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap();
        let mut outputs = Vec::new();
        for owned in [&on, &off] {
            let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
                crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                    &sources,
                    &inputs,
                    &names,
                    &types,
                    &typed,
                    owned,
                    run.id(),
                )
            });
            let (program, entry) = lowered.unwrap();
            verify_program(&program).unwrap();
            super::binding_causal_links::verify_binding_ssa(&program, entry, &routes);
            outputs.push((
                crate::ssa::render_program(&program),
                crate::llvm::render_verified_program(&program).unwrap(),
            ));
        }
        assert_eq!(outputs[0], outputs[1]);
        let [observation] = on.ownership().loan_action_observations() else {
            panic!("one actual source catalog")
        };
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        assert_eq!(observation.regions().len(), 2);
        assert!(
            observation
                .regions()
                .iter()
                .all(|r| r.protocol_version() == 3
                    && r.coverage() == UnitLoanActionCoverage::Unsupported)
        );
        let actions = observation.actions();
        let [
            UnitLoanAction::Begin(_),
            UnitLoanAction::BorrowHandoffCommitted(commit),
            UnitLoanAction::Begin(_),
            UnitLoanAction::BorrowBindingUseReturned(used),
            UnitLoanAction::BorrowBindingEnd(end),
            UnitLoanAction::BorrowBindingSourceRetired(retired),
            UnitLoanAction::Cleanup { .. },
        ] = actions
        else {
            panic!("actual seven causal records remain intact")
        };
        assert_eq!(used.source_begin(), observation.action_id(0));
        assert_eq!(used.handoff(), observation.action_id(1));
        assert_eq!(used.begin(), observation.action_id(2));
        assert_eq!(retired.use_return(), observation.action_id(3));
        assert_eq!(retired.binding_end(), observation.action_id(4));
        assert_eq!(commit.binding(), end.binding());
        let [region] = observation.binding_regions() else {
            panic!("independent binding coverage region is required")
        };
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert!(observation.owns_binding_region(region));
        assert_eq!(region.binding(), end.binding());
        assert_eq!(
            region.action_ids().collect::<Vec<_>>(),
            (0..actions.len())
                .map(|i| observation.action_id(i).unwrap())
                .collect::<Vec<_>>()
        );
    }
}
