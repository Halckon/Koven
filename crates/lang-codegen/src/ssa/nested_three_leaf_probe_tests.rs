//! Unsupported three-leaf boundary, not a nested-path completion claim.
use super::*;
use lang_frontend::{
    ownership_checking::{
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    },
    type_checking::{BorrowReturnOrigin, UnitCallTarget},
};

// Keep the prospective positive fixture separate from the current rejection assertions.
fn nested_three_leaf_source() -> &'static str {
    "fun left(source: String): borrow String from source = source\nfun middle(source: String): borrow String from source = source\nfun right(source: String): borrow String from source = source\nfun wrap(source: String, outer: Boolean, inner: Boolean): borrow String from source { if (outer) { if (inner) { return left(source) } else { return middle(source) } } else { return right(source) } }\nfun consume(item: String): Unit {}\nfun run(outer: Boolean, inner: Boolean): Unit { val owner = \"kept\"; borrow val item = wrap(owner, outer, inner); consume(item) }"
}

#[test]
fn ordinary_nested_three_leaf_remains_explicitly_unsupported() {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(
        &mut sources,
        "nested-three-leaf.ko",
        nested_three_leaf_source(),
    );
    let (environment, types) = standard_environments();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "nested-three-leaf.ko", source, &parsed),
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
        let declarations = names.names().index().declarations();
        let wrapper = declarations.iter().find(|d| d.name() == "wrap").unwrap();
        let wrapper_signature = typed
            .types()
            .signatures()
            .declaration(wrapper.id())
            .unwrap()
            .callable()
            .unwrap();
        let marker = wrapper_signature.borrow_return().unwrap().marker_span();
        assert_eq!(sources.slice(marker).unwrap(), "borrow");
        let raw = on.ownership().borrow_forward_transports();
        assert_eq!(
            raw.len(),
            3,
            "three real raw handoffs are not a nested certificate"
        );
        let expected_targets = ["left", "middle", "right"]
            .map(|name| declarations.iter().find(|d| d.name() == name).unwrap().id());
        let mut actual_targets = Vec::new();
        for handoff in raw {
            let call = handoff.returned_expression();
            let descriptor = typed.types().call(call).unwrap();
            let UnitCallTarget::Declaration(target) = descriptor.target() else {
                panic!("direct leaf callee")
            };
            assert!(expected_targets.contains(&target));
            assert!(!actual_targets.contains(&target));
            actual_targets.push(target);
            assert_eq!(
                descriptor.borrow_return().unwrap().origin(),
                BorrowReturnOrigin::Parameter(0)
            );
            let signature = typed
                .types()
                .signatures()
                .declaration(target)
                .unwrap()
                .callable()
                .unwrap();
            assert_eq!(descriptor.result_source(), signature.result_source());
            assert_eq!(handoff.source_loan().call(), call);
            assert_eq!(
                handoff.source_loan().target(),
                raw[0].source_loan().target()
            );
            let origins = on
                .ownership()
                .borrow_return_origins()
                .iter()
                .filter(|o| o.expression() == call && o.declaration_span() == marker)
                .collect::<Vec<_>>();
            assert_eq!(origins.len(), 1);
            assert_eq!(origins[0].origin(), handoff.source_loan().target());
        }
        assert_eq!(actual_targets.len(), 3);
        for catalog in on.ownership().loan_action_observations() {
            assert_eq!(catalog.coverage(), UnitLoanActionCoverage::Partial);
            assert!(catalog.branch_forward_regions().is_empty());
        }
        let run = declarations.iter().find(|d| d.name() == "run").unwrap();
        for (setting, owned) in [("on", &on), ("off", &off)] {
            let ((lowered, verification), receipts) =
                crate::ssa::refine::branch_receipt::observe(|| {
                    crate::ssa::unit_lower::observe_final_verifier(|| {
                        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                            &sources,
                            &inputs,
                            &names,
                            &types,
                            &typed,
                            owned,
                            run.id(),
                        )
                    })
                });
            let error = lowered
                .err()
                .expect("three-leaf routing must remain unsupported");
            assert_eq!(
                error.kind,
                crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode,
                "{layout}/{setting}"
            );
            assert_eq!(
                error.span,
                Some(marker),
                "actual wrapper borrow marker, not an unrelated failure"
            );
            assert!(
                verification.is_empty(),
                "must reject before original final verifier"
            );
            assert!(receipts.is_empty(), "no fabricated nested receipt");
        }
    }
}
