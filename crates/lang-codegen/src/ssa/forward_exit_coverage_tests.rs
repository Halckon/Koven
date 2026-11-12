//! Original wrapper normal exit: all four real lowerings precede the new coverage claim.
use super::forward_causal_links::{SOURCE, assert_original_forward, assert_original_ssa};
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitLoanTarget, check_compilation_unit_ownership, legacy_ownership_eq,
        with_loan_observation,
    },
    type_checking::UnitItemId,
};

#[test]
fn original_wrapper_requires_complete_live_exit_with_true_off() {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "forward-exit.ko", SOURCE);
    let (environment, types) = standard_environments();
    let mut completed = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "forward-exit.ko", source, &parsed),
        ),
        ("memory", SourceUnitInput::memory_document(source, &parsed)),
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
        assert_eq!(
            on.ownership().owner_observations(),
            off.ownership().owner_observations()
        );
        for owned in [&on, &off] {
            assert_original_forward(&sources, &parsed, &names, owned.ownership());
        }
        let declarations = names.names().index().declarations();
        let run = declarations
            .iter()
            .find(|decl| decl.name() == "run")
            .unwrap();
        let wrap = declarations
            .iter()
            .find(|decl| decl.name() == "wrap")
            .unwrap();
        let callable = UnitItemId::new(wrap.source_unit(), wrap.root());
        let [observation] = on.ownership().loan_action_observations() else {
            panic!("one actual source catalog")
        };
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        let handoffs = observation
            .actions()
            .iter()
            .filter_map(|action| {
                if let UnitLoanAction::BorrowForwardHandoff(handoff) = action {
                    Some(handoff)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let [handoff] = handoffs.as_slice() else {
            panic!("one existing actual live handoff")
        };
        assert_eq!(handoff.callable(), Some(callable));
        let begin = handoff
            .source_begin()
            .expect("existing actual source acquisition");
        assert!(
            matches!(observation.action(&begin), Some(UnitLoanAction::Begin(loan))
            if loan == handoff.transaction().source_loan())
        );
        let mut outputs = Vec::new();
        for (setting, owned) in [("on", &on), ("off", &off)] {
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
            assert_original_ssa(&program, entry, &routes);
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            println!(
                "layout={layout}; observer={setting}; existing_live_chain=true; original_ssa_verified=true; routes={routes:?}; llvm_bytes={}",
                llvm.len()
            );
            outputs.push((crate::ssa::render_program(&program), llvm));
        }
        assert_eq!(outputs[0], outputs[1]);
        completed.push((layout, on, callable));
    }
    assert_eq!(completed.len(), 2);
    println!("all_original_prerequisites_complete=2_layouts_4_lowerings");
    // Only this new certificate is missing on the frozen baseline. No force route or facts.
    for (layout, on, callable) in completed {
        let [observation] = on.ownership().loan_action_observations() else {
            panic!("one actual source catalog")
        };
        assert_eq!(
            observation.forward_regions().len(),
            1,
            "{layout}: independently entered live-exit coverage is required"
        );
        let region = &observation.forward_regions()[0];
        assert!(observation.owns_forward_region(region));
        assert_eq!(region.callable(), callable);
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        let ids = region.action_ids().collect::<Vec<_>>();
        let [begin, delivered] = ids.as_slice() else {
            panic!("the entire unfiltered interval is Begin then live handoff")
        };
        assert_eq!(region.live_handoff().as_ref(), Some(delivered));
        let Some(UnitLoanAction::Begin(loan)) = observation.action(begin) else {
            panic!("original acquisition")
        };
        let Some(UnitLoanAction::BorrowForwardHandoff(handoff)) = observation.action(delivered)
        else {
            panic!("actual live exit, not End")
        };
        assert_eq!(handoff.source_begin().as_ref(), Some(begin));
        assert_eq!(handoff.callable(), Some(callable));
        assert_eq!(handoff.transaction().source_loan(), loan);
        assert_eq!(region.body(), loan.call());
        let UnitLoanTarget::Place(place) = loan.target() else {
            panic!("original shared source parameter")
        };
        assert_eq!(region.source_parameter(), place.root());
        assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
        let cloned = observation.clone();
        assert!(cloned.owns_forward_region(region));
        assert_eq!(cloned.action(delivered), observation.action(delivered));
    }
}
