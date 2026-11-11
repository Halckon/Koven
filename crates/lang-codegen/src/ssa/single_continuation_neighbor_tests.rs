//! Legal neighboring programs do not acquire single-continuation receipt authority.
use super::*;
use lang_frontend::{
    ownership_checking::{
        UnitDropPoint, UnitLoanAction, check_compilation_unit_ownership, legacy_ownership_eq,
        with_loan_observation,
    },
    type_checking::UnitCallTarget,
};

#[test]
fn ordinary_single_continuation_neighbors_stay_unsupported() {
    let right = "if (f0) { return c0(source) } else { if (f1) { return c1(source) } else { if (f2) { return c2(source) } } }; return c3(source)";
    for (shape, body, count, flag_count) in [
        ("two-holes", "if (f0) { if (f1) { return c0(source) } } else { if (f2) { return c1(source) } }; return c2(source)".to_owned(),3,3),
        ("extra-statement", right.replace("if (f2)","f2; if (f2)"),4,3),
        ("repeated-flag", right.replace("if (f2)","if (f1)"),4,3),
        ("wrong-flag-order",right.replace("if (f1)","if (SWAP)").replace("if (f2)","if (f1)").replace("if (SWAP)","if (f2)"),4,3),
        ("depth-four",right.replace("if (f2) { return c2(source) }","if (f2) { return c2(source) } else { if (f3) { return c4(source) } }"),5,4),
        ("explicit-empty-else",right.replace("if (f2) { return c2(source) }","if (f2) { return c2(source) } else {}"),4,3),
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
                    .all(|catalog| catalog.nested_forward_regions().is_empty()
                        && catalog.bounded_forward_regions().is_empty() && catalog.bounded_continuation_regions().is_empty())
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
                    crate::ssa::refine::single_receipt::observe(|| {
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

/// Research boundary: multiple control paths share one physical tail transaction.
/// No current single-survivor certificate may silently duplicate that transaction.
#[test]
fn ordinary_multiple_hole_trees_require_shared_tail_contract() {
    #[derive(Clone)]
    enum Tree {
        Leaf,
        Split(Box<Tree>, Box<Tree>),
    }
    fn trees(depth: usize) -> Vec<Tree> {
        let mut out = vec![Tree::Leaf];
        if depth > 0 {
            let children = trees(depth - 1);
            for left in &children {
                for right in &children {
                    out.push(Tree::Split(Box::new(left.clone()), Box::new(right.clone())));
                }
            }
        }
        out
    }
    fn holes(tree: &Tree, route: &str, out: &mut Vec<String>) {
        if let Tree::Split(left, right) = tree {
            holes(left, &format!("{route}T"), out);
            let next = format!("{route}F");
            if matches!(right.as_ref(), Tree::Leaf) {
                out.push(next);
            } else {
                holes(right, &next, out);
            }
        }
    }
    fn source(
        tree: &Tree,
        route: &str,
        holes: &[String],
        flags: &mut usize,
        returns: &mut usize,
    ) -> String {
        match tree {
            Tree::Leaf => {
                let i = *returns;
                *returns += 1;
                format!("return c{i}(source)")
            }
            Tree::Split(left, right) => {
                let flag = *flags;
                *flags += 1;
                let left = source(left, &format!("{route}T"), holes, flags, returns);
                let next = format!("{route}F");
                if holes.contains(&next) {
                    format!("if (f{flag}) {{ {left} }}")
                } else {
                    let right = source(right, &next, holes, flags, returns);
                    format!("if (f{flag}) {{ {left} }} else {{ {right} }}")
                }
            }
        }
    }
    let mut cases = 0;
    for tree in trees(3) {
        let mut candidates = Vec::new();
        holes(&tree, "", &mut candidates);
        for mask in 0usize..(1 << candidates.len()) {
            if mask.count_ones() < 2 {
                continue;
            }
            let selected = candidates
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, route)| route.clone())
                .collect::<Vec<_>>();
            cases += 1;
            let (mut flags, mut returns) = (0, 0);
            let body = source(&tree, "", &selected, &mut flags, &mut returns);
            assert_eq!(returns + selected.len(), flags + 1);
            let body = format!("{body}; return c{returns}(source)");
            let callees = (0..=returns)
                .map(|i| format!("fun c{i}(source: String): borrow String from source = source\n"))
                .collect::<String>();
            let params = (0..flags)
                .map(|i| format!("f{i}: Boolean"))
                .collect::<Vec<_>>()
                .join(", ");
            let args = (0..flags)
                .map(|i| format!("f{i}"))
                .collect::<Vec<_>>()
                .join(", ");
            let text = format!(
                "{callees}fun wrap(source: String, {params}): borrow String from source {{ {body} }}\nfun consume(item: String): Unit {{}}\nfun run({params}): Unit {{val owner = \"kept\"; borrow val item = wrap(owner, {args}); consume(item)}}"
            );
            let mut sources = SourceMap::new();
            let (source, parsed) =
                unit_lower_test_support::parsed(&mut sources, "multi-hole-trees.ko", &text);
            let inputs = [SourceUnitInput::memory_document(source, &parsed)];
            let (environment, types) = standard_environments();
            let (names, typed, on) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .validate()
                    .unwrap()
            });
            assert!(legacy_ownership_eq(on.ownership(), off.ownership()));
            let raw = on.ownership().borrow_forward_transports();
            assert_eq!(raw.len(), returns + 1, "case {cases}");
            assert!(raw.len() < flags + 1, "transactions are not control paths");
            let tail = raw.last().unwrap();
            let tail_call = tail.returned_expression();
            let expected_tail = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == format!("c{returns}"))
                .unwrap()
                .id();
            assert_eq!(
                typed.types().call(tail_call).unwrap().target(),
                UnitCallTarget::Declaration(expected_tail),
                "case {cases}: identify the physical shared tail independently"
            );
            let mut begins = 0;
            let mut handoffs = 0;
            for catalog in on.ownership().loan_action_observations() {
                assert!(catalog.bounded_forward_regions().is_empty());
                assert!(catalog.bounded_continuation_regions().is_empty());
                assert!(catalog.nested_forward_regions().is_empty());
                for action in catalog.actions() {
                    match action {
                        UnitLoanAction::Begin(loan) if loan.call() == tail_call => {
                            assert_eq!(loan, tail.source_loan());
                            begins += 1;
                        }
                        UnitLoanAction::BorrowForwardHandoff(handoff)
                            if handoff.transaction().returned_expression() == tail_call =>
                        {
                            assert_eq!(handoff.transaction(), tail);
                            handoffs += 1;
                        }
                        _ => {}
                    }
                }
            }
            assert_eq!(
                (begins, handoffs),
                (1, 1),
                "case {cases}: physical tail pair"
            );
            let declarations = names.names().index().declarations();
            let wrapper = declarations.iter().find(|d| d.name() == "wrap").unwrap();
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
            let entry = declarations
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            for (setting, owned) in [("on", &on), ("off", &off)] {
                let ((lowered, verification), receipts) =
                    crate::ssa::refine::single_receipt::observe(|| {
                        crate::ssa::unit_lower::observe_final_verifier(|| {
                            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                                &sources, &inputs, &names, &types, &typed, owned, entry,
                            )
                        })
                    });
                let error = lowered.err().unwrap_or_else(|| {
                    panic!("case {cases}/{setting}: unproved multi-hole accepted")
                });
                assert_eq!(
                    error.kind,
                    crate::ssa::lowering_support::LoweringErrorKind::UnsupportedNode
                );
                assert_eq!(error.span, Some(marker));
                assert!(verification.is_empty());
                assert!(receipts.is_empty());
            }
        }
    }
    assert_eq!(cases, 92);
}
