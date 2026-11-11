//! Real legal neighboring trees remain outside the fixed finite-tree receipt frontier.
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitLoanAction, check_compilation_unit_ownership, legacy_ownership_eq,
        with_loan_observation,
    },
    type_checking::UnitCallTarget,
};

#[test]
fn ordinary_balanced_neighbors_keep_exact_unsupported_boundary() {
    for (shape, body, count) in [
        (
            "three-parameter-shared-flag",
            "if (outer) { if (left_flag) { return left(source) } else { return middle(source) } } else { if (left_flag) { return middle(source) } else { return right(source) } }",
            4,
        ),
        (
            "missing-right-else",
            "if (outer) { if (left_flag) { return left(source) } else { return middle(source) } } else { if (right_flag) { return middle(source) }\nreturn right(source) }",
            4,
        ),
        (
            "reused-inner-flag",
            "if (outer) { if (left_flag) { return left(source) } else { return middle(source) } } else { if (left_flag) { return middle(source) } else { return right(source) } }",
            4,
        ),
        (
            "extra-inner-statement",
            "if (outer) { left_flag\nif (left_flag) { return left(source) } else { return middle(source) } } else { if (right_flag) { return middle(source) } else { return right(source) } }",
            4,
        ),
        (
            "third-layer",
            "if (outer) { if (left_flag) { if (left_flag) { return left(source) } else { return middle(source) } } else { return middle(source) } } else { if (right_flag) { return middle(source) } else { return right(source) } }",
            5,
        ),
    ] {
        let text = format!(
            "fun left(source: String): borrow String from source = source\nfun middle(source: String): borrow String from source = source\nfun right(source: String): borrow String from source = source\nfun wrap(source: String, outer: Boolean, left_flag: Boolean, right_flag: Boolean): borrow String from source {{ {body} }}\nfun consume(item: String): Unit {{}}\nfun run(outer: Boolean, left_flag: Boolean, right_flag: Boolean): Unit {{ val owner = \"kept\"; borrow val item = wrap(owner, outer, left_flag, right_flag); consume(item) }}"
        );
        let text = if shape == "three-parameter-shared-flag" {
            text.replace(
                "fun wrap(source: String, outer: Boolean, left_flag: Boolean, right_flag: Boolean)",
                "fun wrap(source: String, outer: Boolean, left_flag: Boolean)",
            )
            .replace(
                "wrap(owner, outer, left_flag, right_flag)",
                "wrap(owner, outer, left_flag)",
            )
        } else {
            text
        };
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "nested-neighbor.ko", &text);
        let (environment, types) = standard_environments();
        for (layout, input) in [
            (
                "rooted",
                SourceUnitInput::new("root", "nested-neighbor.ko", source, &parsed),
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
            assert_eq!(
                on.ownership().owner_observations(),
                off.ownership().owner_observations()
            );
            assert!(off.ownership().loan_action_observations().is_empty());
            let raw = on.ownership().borrow_forward_transports();
            assert_eq!(raw.len(), count, "{shape}/{layout}");
            let mut calls = Vec::new();
            for transport in raw {
                let call = transport.returned_expression();
                assert!(!calls.contains(&call));
                calls.push(call);
                assert_eq!(transport.source_loan().call(), call);
                assert_eq!(
                    transport.source_loan().target(),
                    raw[0].source_loan().target()
                );
                assert_eq!(transport.handoff_point(), UnitDropPoint::CallReturn(call));
                assert!(matches!(
                    typed.types().call(call).unwrap().target(),
                    UnitCallTarget::Declaration(_)
                ));
                assert_eq!(
                    on.ownership()
                        .loans()
                        .iter()
                        .filter(|loan| loan.call() == call)
                        .collect::<Vec<_>>(),
                    vec![transport.source_loan()]
                );
            }
            let handoffs = on
                .ownership()
                .loan_action_observations()
                .iter()
                .flat_map(|catalog| catalog.actions())
                .filter_map(|action| match action {
                    UnitLoanAction::BorrowForwardHandoff(handoff) => Some(handoff.transaction()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(handoffs, raw.iter().collect::<Vec<_>>());
            assert!(
                on.ownership()
                    .loan_action_observations()
                    .iter()
                    .all(|catalog| catalog.nested_forward_regions().is_empty())
            );
            let declarations = names.names().index().declarations();
            let wrapper = declarations.iter().find(|d| d.name() == "wrap").unwrap();
            let entry = declarations
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
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
                    panic!("{shape}/{layout}/{setting}: unsupported neighboring tree admitted")
                });
                assert_eq!(
                    error.kind,
                    crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode
                );
                assert_eq!(error.span, Some(marker), "{shape}/{layout}/{setting}");
                assert!(verification.is_empty());
                assert!(receipts.is_empty());
            }
        }
    }
}
