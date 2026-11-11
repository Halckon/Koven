//! Dynamic bounded terminal trees have real checked transports but no backend receipt yet.
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitLoanAction, check_compilation_unit_ownership, legacy_ownership_eq,
        with_loan_observation,
    },
    type_checking::UnitCallTarget,
};

#[test]
fn ordinary_dynamic_terminal_trees_keep_backend_unsupported() {
    for (shape, body, count, flag_count) in [
        (
            "depth-three-chain",
            "if (f0) { if (f1) { if (f2) { return c0(source) } else { return c1(source) } } else { return c2(source) } } else { return c3(source) }",
            4,
            3,
        ),
        (
            "mixed-five-leaf",
            "if (f0) { if (f1) { if (f2) { return c0(source) } else { return c1(source) } } else { return c2(source) } } else { if (f3) { return c3(source) } else { return c4(source) } }",
            5,
            4,
        ),
        (
            "full-eight-leaf",
            "if (f0) { if (f1) { if (f2) { return c0(source) } else { return c1(source) } } else { if (f3) { return c2(source) } else { return c3(source) } } } else { if (f4) { if (f5) { return c4(source) } else { return c5(source) } } else { if (f6) { return c6(source) } else { return c7(source) } } }",
            8,
            7,
        ),
    ] {
        let callees = (0..count)
            .map(|i| format!("fun c{i}(source: String): borrow String from source = source\n"))
            .collect::<String>();
        let params = (0..flag_count)
            .map(|i| format!("f{i}: Boolean"))
            .collect::<Vec<_>>()
            .join(", ");
        let args = (0..flag_count)
            .map(|i| format!("f{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!(
            "{callees}fun wrap(source: String, {params}): borrow String from source {{ {body} }}\nfun consume(item: String): Unit {{}}\nfun run({params}): Unit {{ val owner = \"kept\"; borrow val item = wrap(owner, {args}); consume(item) }}"
        );
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
