//! Real fixed-depth balanced forwarding, not arbitrary recursive path enumeration.
#![cfg(feature = "loan-observation-test-support")]
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::{
        UnitBorrowNestedForwardTopology, UnitDropPoint, UnitLoanAction, UnitLoanActionCoverage,
        check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        UnitCallTarget, UnitItemId, check_compilation_unit_types, standard_environments,
    },
};
const SOURCE: &str = "fun first(s: String): borrow String from s = s\nfun second(s: String): borrow String from s = s\nfun third(s: String): borrow String from s = s\nfun fourth(s: String): borrow String from s = s\nfun wrap(s: String, outer: Boolean, left: Boolean, right: Boolean): borrow String from s { if (outer) { if (left) { return first(s) } else { return second(s) } } else { if (right) { return third(s) } else { return fourth(s) } } }";

#[test]
fn ordinary_balanced_four_leaf_requires_both_inner_closures() {
    check(SOURCE, true);
}

#[test]
fn ordinary_balanced_neighbors_remain_unproved() {
    for text in [
        SOURCE.replace("if (right)", "if (left)"),
        SOURCE.replace("if (outer)", "outer\nif (outer)"),
        SOURCE.replace(
            "return fourth(s)",
            "if (right) { return fourth(s) } else { return fourth(s) }",
        ),
        SOURCE.replace(
            "if (right) { return third(s) } else { return fourth(s) }",
            "if (right) { return third(s) }\nreturn fourth(s)",
        ),
    ] {
        check(&text, false);
    }
}

fn check(text: &str, complete: bool) {
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
        if complete {
            assert_eq!(raw.len(), 4);
        }
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
        let regions = catalog.nested_forward_regions();
        if !complete {
            assert!(
                regions.is_empty(),
                "{layout}: neighbor must not gain region"
            );
            assert!(catalog.nested_forward_lineages().is_empty());
            continue;
        }
        assert_eq!(regions.len(), 1, "{layout}: balanced terminal region");
        let region = &regions[0];
        assert!(catalog.owns_nested_forward_region(region));
        assert_eq!(
            region.topology(),
            UnitBorrowNestedForwardTopology::BalancedBoth
        );
        assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
        let paths = region.paths();
        assert_eq!(paths.len(), 4);
        assert_eq!(paths, catalog.nested_forward_lineages());
        let mut ids = Vec::new();
        let mut callees = Vec::new();
        for (leaf, path) in paths.iter().enumerate() {
            assert!(catalog.owns_nested_forward_lineage(path));
            assert_eq!(path.coverage(), UnitLoanActionCoverage::Partial);
            assert_eq!(path.topology(), region.topology());
            assert_eq!(path.callable(), region.callable());
            assert_eq!(path.source_parameter(), region.source_parameter());
            assert_eq!(
                path.callable(),
                UnitItemId::new(
                    raw[leaf].returned_expression().source_unit(),
                    parsed.roots()[4]
                )
            );
            assert_eq!(
                raw[leaf].point(),
                UnitDropPoint::ControlTransfer(path.control())
            );
            assert_eq!(path.value(), raw[leaf].returned_expression());
            let UnitCallTarget::Declaration(callee) =
                typed.types().call(path.value()).unwrap().target()
            else {
                panic!("direct callee")
            };
            assert!(!callees.contains(&callee));
            callees.push(callee);
            assert_eq!(
                catalog.action(&path.source_begin()),
                Some(&UnitLoanAction::Begin(raw[leaf].source_loan().clone()))
            );
            assert_eq!(
                catalog.action(&path.handoff()),
                Some(&UnitLoanAction::BorrowForwardHandoff(
                    handoffs[leaf].clone()
                ))
            );
            for id in [path.source_begin(), path.handoff()] {
                assert!(!ids.contains(&id));
                ids.push(id);
            }
            assert_eq!(
                path.edges()
                    .iter()
                    .map(|edge| edge.then_arm())
                    .collect::<Vec<_>>(),
                [
                    vec![true, true],
                    vec![true, false],
                    vec![false, true],
                    vec![false, false]
                ][leaf]
            );
        }
        assert_eq!(paths[0].edges()[0], paths[1].edges()[0]);
        assert_eq!(paths[2].edges()[0], paths[3].edges()[0]);
        assert_eq!(paths[0].edges()[0].split(), paths[2].edges()[0].split());
        assert_eq!(paths[0].edges()[1].split(), paths[1].edges()[1].split());
        assert_eq!(paths[2].edges()[1].split(), paths[3].edges()[1].split());
        let splits = [
            paths[0].edges()[0].split(),
            paths[0].edges()[1].split(),
            paths[2].edges()[1].split(),
        ];
        assert_ne!(splits[0], splits[1]);
        assert_ne!(splits[0], splits[2]);
        assert_ne!(splits[1], splits[2]);
    }
}
