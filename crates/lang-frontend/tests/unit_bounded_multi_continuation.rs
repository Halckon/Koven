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
fn ordinary_bounded_multi_continuation_requires_exact_live_unions() {
    for (shape, body, count, flags, routes) in [
        (
            "two",
            "if (f0) { if (f1) { return c0(source) } } else { if (f2) { return c1(source) } }; return c2(source)",
            3,
            3,
            vec!["TF", "FF"],
        ),
        (
            "three",
            "if (f0) { if (f1) { if (f2) { return c0(source) } } else { if (f3) { return c1(source) } } } else { if (f4) { return c2(source) } }; return c3(source)",
            4,
            5,
            vec!["TTF", "TFF", "FF"],
        ),
        (
            "four",
            "if (f0) { if (f1) { if (f2) { return c0(source) } } else { if (f3) { return c1(source) } } } else { if (f4) { if (f5) { return c2(source) } } else { if (f6) { return c3(source) } } }; return c4(source)",
            5,
            7,
            vec!["TTF", "TFF", "FTF", "FFF"],
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
        check(&text, count, flags, shape, &routes, true);
    }
}
fn check(
    text: &str,
    count: usize,
    split_count: usize,
    shape: &str,
    expected: &[&str],
    complete: bool,
) {
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
        let regions = catalog.bounded_multi_continuation_regions();
        if !complete {
            assert!(regions.is_empty());
            continue;
        }
        assert!(catalog.nested_forward_regions().is_empty());
        assert!(catalog.bounded_forward_regions().is_empty());
        assert!(catalog.bounded_continuation_regions().is_empty());
        assert_eq!(
            regions.len(),
            1,
            "{shape}/{layout}: independent multi-source region"
        );
        let region = &regions[0];
        assert!(catalog.owns_bounded_multi_continuation_region(region));
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        assert_eq!(region.split_count(), split_count);
        assert_eq!(region.terminal_paths().len() + 1, count);
        assert_eq!(region.origins().len(), expected.len());
        let spelling =
            |edges: &[lang_frontend::ownership_checking::UnitLoanNestedForwardEdge]| {
                edges
                    .iter()
                    .map(|e| if e.then_arm() { 'T' } else { 'F' })
                    .collect::<String>()
            };
        for (origin, expected) in region.origins().iter().zip(expected) {
            assert_eq!(spelling(origin.edges()), *expected);
        }
        let mut previous = Vec::new();
        for join in region.joins() {
            assert!(!previous.contains(&join.split()));
            previous.push(join.split());
            let incoming = join.incoming();
            assert!(incoming[0].iter().all(|v| !incoming[1].contains(v)));
            let mut union = incoming.iter().flatten().copied().collect::<Vec<_>>();
            union.sort();
            assert_eq!(union, join.origins());
            assert!(!union.is_empty());
            for (side, set) in incoming.iter().enumerate() {
                for index in set {
                    let origin = &region.origins()[*index];
                    assert!(
                        origin
                            .edges()
                            .iter()
                            .any(|edge| edge.split() == join.split()
                                && edge.condition() == join.condition()
                                && edge.then_arm() == (side == 0))
                    );
                }
            }
        }
        assert_eq!(
            region.joins().last().unwrap().origins(),
            (0..expected.len()).collect::<Vec<_>>()
        );
        let tail = region.tail().expect("one shared tail");
        assert_eq!(tail.callable(), region.callable());
        assert_eq!(tail.source_parameter(), region.source_parameter());
        assert_eq!(tail.value(), raw.last().unwrap().returned_expression());
        assert_eq!(
            raw.last().unwrap().point(),
            UnitDropPoint::ControlTransfer(tail.control())
        );
        assert_eq!(
            catalog.action(&tail.source_begin()),
            Some(&UnitLoanAction::Begin(
                raw.last().unwrap().source_loan().clone()
            ))
        );
        assert_eq!(
            catalog.action(&tail.handoff()),
            Some(&UnitLoanAction::BorrowForwardHandoff(
                (*handoffs.last().unwrap()).clone()
            ))
        );
        let tail_target = typed.types().call(tail.value()).unwrap().target();
        let expected_declaration = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == format!("c{}", count - 1))
            .unwrap();
        assert_eq!(
            tail_target,
            UnitCallTarget::Declaration(expected_declaration.id())
        );
        assert_eq!(
            catalog
                .actions()
                .iter()
                .filter(|a| matches!(a,UnitLoanAction::Begin(l) if l.call()==tail.value()))
                .count(),
            1
        );
        assert_eq!(catalog.actions().iter().filter(|a|matches!(a,UnitLoanAction::BorrowForwardHandoff(h) if h.transaction().returned_expression()==tail.value())).count(),1);
        let mut ids = vec![tail.source_begin(), tail.handoff()];
        for (i, path) in region.terminal_paths().iter().enumerate() {
            assert_eq!(path.value(), raw[i].returned_expression());
            assert_eq!(
                raw[i].point(),
                UnitDropPoint::ControlTransfer(path.control())
            );
            assert_eq!(
                catalog.action(&path.source_begin()),
                Some(&UnitLoanAction::Begin(raw[i].source_loan().clone()))
            );
            assert_eq!(
                catalog.action(&path.handoff()),
                Some(&UnitLoanAction::BorrowForwardHandoff(handoffs[i].clone()))
            );
            for id in [path.source_begin(), path.handoff()] {
                assert!(!ids.contains(&id));
                ids.push(id);
            }
        }
        assert_eq!(
            region.callable(),
            UnitItemId::new(
                raw[0].returned_expression().source_unit(),
                parsed.roots()[count]
            )
        );
    }
}

#[test]
fn ordinary_multi_continuation_rejects_neighbors_and_preserves_old_priority() {
    let two = "if (f0) { if (f1) { return c0(source) } } else { if (f2) { return c1(source) } }; return c2(source)";
    for (shape,body,count,flags) in [
        ("extra-statement",two.replace("if (f1)","f1; if (f1)"),3,3),
        ("duplicate-flag",two.replace("if (f2)","if (f1)"),3,3),
        ("wrong-order",two.replace("if (f1)","if (SWAP)").replace("if (f2)","if (f1)").replace("if (SWAP)","if (f2)"),3,3),
        ("explicit-empty",two.replace("if (f1) { return c0(source) }","if (f1) { return c0(source) } else {}"),3,3),
        ("depth-four","if (f0) { if (f1) { if (f2) { if (f3) { return c0(source) } } } } else { if (f4) { return c1(source) } }; return c2(source)".to_owned(),3,5),
        ("single-priority","if (f0) { return c0(source) } else { if (f1) { return c1(source) } else { if (f2) { return c2(source) } } }; return c3(source)".to_owned(),4,3),
        ("fixed-priority","if (f0) { return c0(source) } else { if (f1) { return c1(source) } }; return c2(source)".to_owned(),3,2),
    ] {
        let callees=(0..count).map(|i|format!("fun c{i}(source: String): borrow String from source = source\n")).collect::<String>();
        let params=(0..flags).map(|i|format!("f{i}: Boolean")).collect::<Vec<_>>().join(", ");
        let text=format!("{callees}fun wrap(source: String, {params}): borrow String from source {{ {body} }}");
        check(&text,count,flags,shape,&[],false);
    }
}
