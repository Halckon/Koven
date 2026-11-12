//! Single-continuation trees: literal paths, real survivor chains and live action identities.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitDropPoint, UnitLoanAction, UnitLoanActionCoverage, check_compilation_unit_ownership,
        legacy_ownership_eq, with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        UnitCallTarget, UnitItemId, check_compilation_unit_types, standard_environments,
    },
};
#[test]
fn ordinary_bounded_single_continuation_requires_actual_chain() {
    for (shape, body, count, flags, routes, hole) in [
        (
            "right-chain",
            "if (f0) { return c0(source) } else { if (f1) { return c1(source) } else { if (f2) { return c2(source) } } }; return c3(source)",
            4,
            3,
            vec!["T", "FT", "FFT", "FFF"],
            "FFF",
        ),
        (
            "then-survivor",
            "if (f0) { if (f1) { return c0(source) } else { if (f2) { return c1(source) } } } else { return c2(source) }; return c3(source)",
            4,
            3,
            vec!["TT", "TFT", "F", "TFF"],
            "TFF",
        ),
        (
            "mixed",
            "if (f0) { if (f1) { if (f2) { return c0(source) } } else { return c1(source) } } else { if (f3) { return c2(source) } else { return c3(source) } }; return c4(source)",
            5,
            4,
            vec!["TTT", "TF", "FT", "FF", "TTF"],
            "TTF",
        ),
    ] {
        let callees = (0..count)
            .map(|i| format!("fun c{i}(source: String): borrow String from source = source\n"))
            .collect::<String>();
        let params = (0..flags)
            .map(|i| format!("f{i}: Boolean"))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!(
            "{callees}fun wrap(source: String, {params}): borrow String from source {{ {body} }}"
        );
        check(&text, (count, flags), shape, &routes, hole, true, false);
    }
}
fn check(
    text: &str,
    sizes: (usize, usize),
    shape: &str,
    expected: &[&str],
    hole: &str,
    complete: bool,
    fixed: bool,
) {
    let (count, split_count) = sizes;
    let mut sources = SourceMap::new();
    let source = sources.add_source("balanced.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "balanced.ko", source, &parsed),
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
        assert!(catalog.branch_forward_regions().is_empty());
        assert!(catalog.forward_regions().is_empty());
        let raw = on.ownership().borrow_forward_transports();
        assert!(!raw.is_empty(), "{layout}: actual transports");
        assert_eq!(raw.len(), count);
        let handoffs = catalog
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowForwardHandoff(value) => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(handoffs.len(), raw.len());
        for (handoff, transport) in handoffs.iter().zip(raw) {
            assert_eq!(handoff.transaction(), transport);
            assert!(handoff.branch().is_none());
        }
        assert_eq!(
            catalog.nested_forward_regions().is_empty(),
            !fixed,
            "{shape}/{layout}: fixed priority"
        );
        assert!(catalog.bounded_forward_regions().is_empty());
        let regions = catalog.bounded_continuation_regions();
        if !complete {
            assert!(
                regions.is_empty(),
                "{shape}/{layout}: unsupported or fixed-priority continuation"
            );
            continue;
        }
        assert_eq!(
            regions.len(),
            1,
            "{shape}/{layout}: independent continuation region"
        );
        let region = &regions[0];
        assert!(catalog.owns_bounded_continuation_region(region));
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.split_count(), split_count);
        assert_eq!(region.paths().len(), count);
        assert_eq!(
            region.callable(),
            UnitItemId::new(
                raw[0].returned_expression().source_unit(),
                parsed.roots()[count]
            )
        );
        let continuation = region.continuation().expect("actual continuation chain");
        let spelling =
            |edges: &[lang_frontend::ownership_checking::UnitLoanNestedForwardEdge]| {
                edges
                    .iter()
                    .map(|e| if e.then_arm() { 'T' } else { 'F' })
                    .collect::<String>()
            };
        assert_eq!(spelling(continuation.entries()), hole);
        assert_eq!(
            continuation.resumptions(),
            continuation
                .entries()
                .iter()
                .rev()
                .copied()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            continuation.tail_control(),
            region.paths().last().unwrap().control()
        );
        let mut ids = Vec::new();
        let mut callees = Vec::new();
        for (i, path) in region.paths().iter().enumerate() {
            assert_eq!(
                raw[i].point(),
                UnitDropPoint::ControlTransfer(path.control())
            );
            assert_eq!(path.value(), raw[i].returned_expression());
            assert_eq!(
                catalog.action(&path.source_begin()),
                Some(&UnitLoanAction::Begin(raw[i].source_loan().clone()))
            );
            assert_eq!(
                catalog.action(&path.handoff()),
                Some(&UnitLoanAction::BorrowForwardHandoff(handoffs[i].clone()))
            );
            assert_eq!(spelling(path.edges()), expected[i]);
            let UnitCallTarget::Declaration(callee) =
                typed.types().call(path.value()).unwrap().target()
            else {
                panic!("typed callee")
            };
            assert!(!callees.contains(&callee));
            callees.push(callee);
            for id in [path.source_begin(), path.handoff()] {
                assert!(!ids.contains(&id));
                ids.push(id);
            }
        }
    }
}

#[test]
fn ordinary_bounded_continuation_neighbors_and_fixed_priority() {
    let right = "if (f0) { return c0(source) } else { if (f1) { return c1(source) } else { if (f2) { return c2(source) } } }; return c3(source)";
    for (shape,body,count,flags,fixed) in [
        ("two-holes", "if (f0) { if (f1) { return c0(source) } } else { if (f2) { return c1(source) } }; return c2(source)".to_owned(),3,3,false),
        ("extra-statement",right.replace("if (f2)","f2; if (f2)"),4,3,false),
        ("wrong-flag",right.replace("if (f2)","if (f1)"),4,3,false),
        ("fourth-level",right.replace("if (f2) { return c2(source) }","if (f2) { return c2(source) } else { if (f3) { return c4(source) } }"),5,4,false),
        ("fixed-single", "if (f0) { if (f1) { return c0(source) } else { return c1(source) } }; return c2(source)".to_owned(),3,2,true),
        ("fixed-cascade", "if (f0) { return c0(source) } else { if (f1) { return c1(source) } }; return c2(source)".to_owned(),3,2,true),
    ] {
        let callees=(0..count).map(|i|format!("fun c{i}(source: String): borrow String from source = source\n")).collect::<String>();
        let params=(0..flags).map(|i|format!("f{i}: Boolean")).collect::<Vec<_>>().join(", ");
        let text=format!("{callees}fun wrap(source: String, {params}): borrow String from source {{ {body} }}");
        check(&text,(count,flags),shape,&[],"",false,fixed);
    }
}
