//! Real depth-three terminal trees: independent literal paths and live action identities.
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
fn ordinary_bounded_terminal_trees_require_complete_actual_paths() {
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
        check(&text, count, flag_count, shape, true, false);
    }
}
fn check(text: &str, count: usize, split_count: usize, shape: &str, complete: bool, fixed: bool) {
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
        let regions = catalog.bounded_forward_regions();
        if !complete {
            assert!(regions.is_empty(), "{shape}/{layout}: no dynamic region");
            continue;
        }
        assert_eq!(
            regions.len(),
            1,
            "{shape}/{layout}: bounded terminal region"
        );
        let region = &regions[0];
        assert!(catalog.owns_bounded_forward_region(region));
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
        let expected = match count {
            4 => vec![
                vec![true, true, true],
                vec![true, true, false],
                vec![true, false],
                vec![false],
            ],
            5 => vec![
                vec![true, true, true],
                vec![true, true, false],
                vec![true, false],
                vec![false, true],
                vec![false, false],
            ],
            8 => (0..8)
                .map(|n| vec![n & 4 == 0, n & 2 == 0, n & 1 == 0])
                .collect(),
            _ => unreachable!(),
        };
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
            assert_eq!(
                path.edges()
                    .iter()
                    .map(|e| e.then_arm())
                    .collect::<Vec<_>>(),
                expected[i]
            );
            let UnitCallTarget::Declaration(callee) =
                typed.types().call(path.value()).unwrap().target()
            else {
                panic!("callee")
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
fn ordinary_bounded_neighbors_and_fixed_priority_remain_separate() {
    let chain = "if (f0) { if (f1) { if (f2) { return c0(source) } else { return c1(source) } } else { return c2(source) } } else { return c3(source) }";
    let then = "if (f0) { if (f1) { return c0(source) } else { return c1(source) } } else { return c2(source) }";
    let other = "if (f0) { return c0(source) } else { if (f1) { return c1(source) } else { return c2(source) } }";
    let balanced = "if (f0) { if (f1) { return c0(source) } else { return c1(source) } } else { if (f2) { return c2(source) } else { return c3(source) } }";
    for (shape, body, count, flags, fixed) in [
        ("fourth-level", chain.replace("return c0(source)", "if (f3) { return c0(source) } else { return c4(source) }"), 5, 4, false),
        ("missing-else-tail", chain.replace("if (f2) { return c0(source) } else { return c1(source) }", "if (f2) { return c0(source) }; return c1(source)"), 4, 3, false),
        ("repeat-flag", chain.replace("if (f2)", "if (f1)"), 4, 3, false),
        ("wrong-flag-order", chain.replace("if (f1)", "if (SWAP)").replace("if (f2)", "if (f1)").replace("if (SWAP)", "if (f2)"), 4, 3, false),
        ("extra-statement", format!("f0; {chain}"), 4, 3, false),
        ("fixed-then", then.to_string(), 3, 2, true),
        ("fixed-else", other.to_string(), 3, 2, true),
        ("fixed-balanced", balanced.to_string(), 4, 3, true),
        ("fixed-shared", balanced.replace("if (f2)", "if (f1)"), 4, 2, true),
        ("fixed-single-tail", "if (f0) { if (f1) { return c0(source) } else { return c1(source) } }; return c2(source)".into(), 3, 2, true),
        ("fixed-cascade", "if (f0) { return c0(source) } else { if (f1) { return c1(source) } }; return c2(source)".into(), 3, 2, true),
    ] {
        let callees = (0..count).map(|i| format!("fun c{i}(source: String): borrow String from source = source\n")).collect::<String>();
        let params = (0..flags).map(|i| format!("f{i}: Boolean")).collect::<Vec<_>>().join(", ");
        let text = format!("{callees}fun wrap(source: String, {params}): borrow String from source {{ {body} }}");
        check(&text, count, flags, shape, false, fixed);
    }
}
