//! Strict balanced four-leaf producer frontier: actual lowering, not synthetic receipts.
use super::*;
use lang_frontend::ownership_checking::{
    UnitLoanAction, check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
fn balanced_four_leaf_source() -> &'static str {
    "fun first(source: String): borrow String from source = source\nfun second(source: String): borrow String from source = source\nfun third(source: String): borrow String from source = source\nfun fourth(source: String): borrow String from source = source\nfun wrap(source: String, outer: Boolean, left_flag: Boolean, right_flag: Boolean): borrow String from source { if (outer) { if (left_flag) { return first(source) } else { return second(source) } } else { if (right_flag) { return third(source) } else { return fourth(source) } } }\nfun consume(item: String): Unit {}\nfun run(outer: Boolean, left_flag: Boolean, right_flag: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner, outer, left_flag, right_flag); consume(item) }"
}

#[test]
fn ordinary_balanced_four_leaf_frontend_complete_backend_unsupported() {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(
        &mut sources,
        "balanced-four-leaf.ko",
        balanced_four_leaf_source(),
    );
    let (environment, types) = standard_environments();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "balanced-four-leaf.ko", source, &parsed),
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
        let [catalog] = on.ownership().loan_action_observations() else {
            panic!("one catalog")
        };
        assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
        assert_eq!(on.ownership().borrow_forward_transports().len(), 4);
        let [region] = catalog.nested_forward_regions() else {
            panic!("one complete region")
        };
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert!(catalog.owns_nested_forward_region(region));
        assert_eq!(
            region.topology(),
            lang_frontend::ownership_checking::UnitBorrowNestedForwardTopology::BalancedBoth
        );
        assert_eq!(region.paths().len(), 4);
        let raw = on.ownership().borrow_forward_transports();
        for (i, path) in region.paths().iter().enumerate() {
            assert!(catalog.owns_nested_forward_lineage(path));
            assert_eq!(path.coverage(), UnitLoanActionCoverage::Partial);
            assert_eq!(path.callable(), region.callable());
            assert_eq!(path.topology(), region.topology());
            assert_eq!(
                path.edges()
                    .iter()
                    .map(|e| e.then_arm())
                    .collect::<Vec<_>>(),
                vec![i < 2, i % 2 == 0]
            );
            assert_eq!(path.value(), raw[i].returned_expression());
            assert_eq!(
                catalog.action(&path.source_begin()),
                Some(&UnitLoanAction::Begin(raw[i].source_loan().clone()))
            );
            let Some(UnitLoanAction::BorrowForwardHandoff(handoff)) =
                catalog.action(&path.handoff())
            else {
                panic!("real handoff")
            };
            assert_eq!(handoff.transaction(), &raw[i]);
            assert!(handoff.branch().is_none());
            assert_eq!(raw[i].source_loan().call(), path.value());
            assert_eq!(raw[i].source_loan().target(), raw[0].source_loan().target());
            assert!(
                region.paths()[..i]
                    .iter()
                    .all(|prior| prior.value() != path.value()
                        && prior.source_begin() != path.source_begin()
                        && prior.handoff() != path.handoff())
            );
        }
        let wrapper = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "wrap")
            .unwrap();
        let marker = typed
            .types()
            .signatures()
            .declaration(wrapper.id())
            .unwrap()
            .callable()
            .unwrap()
            .borrow_return()
            .unwrap()
            .marker_span();
        assert_eq!(sources.slice(marker).unwrap(), "borrow");
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == "run")
            .unwrap()
            .id();
        for (setting, owned) in [("on", &on), ("off", &off)] {
            let ((lowered, verification), receipts) =
                crate::ssa::refine::nested_receipt::observe(|| {
                    crate::ssa::unit_lower::observe_final_verifier(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources, &inputs, &names, &types, &typed, owned, entry,
                        )
                    })
                });
            let error = lowered.err().unwrap_or_else(|| {
                panic!("{layout}/{setting}: balanced backend is not supported yet")
            });
            assert_eq!(
                error.kind,
                crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode
            );
            assert_eq!(error.span, Some(marker), "{layout}/{setting}");
            assert!(verification.is_empty());
            assert!(receipts.is_empty());
        }
    }
}
