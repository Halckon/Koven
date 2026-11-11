//! Actual bounded three-leaf lineage is Partial evidence, never region completeness.
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
const SOURCE: &str = "fun left(source: String): borrow String from source = source\nfun middle(source: String): borrow String from source = source\nfun right(source: String): borrow String from source = source\nfun wrap(source: String, outer: Boolean, inner: Boolean): borrow String from source { if (outer) { if (inner) { return left(source) } else { return middle(source) } } else { return right(source) } }";

#[test]
fn ordinary_three_leaf_lineage_keeps_catalog_partial() {
    check_lineage(SOURCE, 3, false);
}

#[test]
fn ordinary_three_leaf_terminal_region_requires_all_path_closure() {
    check_lineage(SOURCE, 3, true);
}

fn check_lineage(source_text: &str, expected: usize, require_region: bool) {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("nested-lineage.ko", source_text)
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{source_text}: {:?}",
        parsed.diagnostics()
    );
    let (environment, types) = standard_environments();
    let mut results = Vec::new();
    for (layout, input) in [
        (
            "rooted",
            SourceUnitInput::new("root", "nested-lineage.ko", source, &parsed),
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
        assert!(!raw.is_empty());
        if expected == 3 {
            assert_eq!(raw.len(), 3);
        }
        let handoffs = catalog
            .actions()
            .iter()
            .filter_map(|action| match action {
                UnitLoanAction::BorrowForwardHandoff(handoff) => Some(handoff),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(handoffs.len(), raw.len());
        for (handoff, original) in handoffs.iter().zip(raw) {
            assert_eq!(handoff.transaction(), original);
            assert!(
                handoff.branch().is_none(),
                "old one-split protocol must not claim nested ancestry"
            );
        }
        let lineages = catalog.nested_forward_lineages();
        let mut callees = Vec::new();
        let mut ids = Vec::new();
        for (index, lineage) in lineages.iter().enumerate() {
            assert_eq!(lineage.coverage(), UnitLoanActionCoverage::Partial);
            assert!(catalog.owns_nested_forward_lineage(lineage));
            assert_eq!(
                raw[index].point(),
                UnitDropPoint::ControlTransfer(lineage.control())
            );
            let UnitCallTarget::Declaration(callee) =
                typed.types().call(lineage.value()).unwrap().target()
            else {
                panic!("direct callee")
            };
            assert!(!callees.contains(&callee));
            callees.push(callee);
            assert_eq!(
                lineage.callable(),
                UnitItemId::new(
                    raw[index].returned_expression().source_unit(),
                    parsed.roots()[3]
                )
            );
            assert_eq!(lineage.value(), raw[index].returned_expression());
            assert_eq!(
                catalog.action(&lineage.handoff()),
                Some(&UnitLoanAction::BorrowForwardHandoff(
                    handoffs[index].clone()
                ))
            );
            assert_eq!(
                catalog.action(&lineage.source_begin()),
                Some(&UnitLoanAction::Begin(raw[index].source_loan().clone()))
            );
            assert_eq!(
                lineage
                    .edges()
                    .iter()
                    .map(|e| e.then_arm())
                    .collect::<Vec<_>>(),
                [vec![true, true], vec![true, false], vec![false]][index]
            );
            for id in [lineage.source_begin(), lineage.handoff()] {
                assert!(!ids.contains(&id));
                ids.push(id);
            }
        }
        if lineages.len() == 3 {
            assert_eq!(lineages[0].edges()[0], lineages[1].edges()[0]);
            assert_eq!(
                lineages[0].edges()[0].split(),
                lineages[2].edges()[0].split()
            );
            assert_eq!(
                lineages[0].edges()[1].split(),
                lineages[1].edges()[1].split()
            );
            assert_ne!(
                lineages[0].edges()[0].split(),
                lineages[0].edges()[1].split()
            );
        }
        if require_region {
            let regions = catalog.nested_forward_regions();
            for region in regions {
                assert!(catalog.owns_nested_forward_region(region));
                assert_eq!(region.coverage(), UnitLoanActionCoverage::Complete);
                assert_eq!(region.paths(), lineages);
                assert_eq!(region.paths().len(), 3);
                assert_eq!(region.callable(), lineages[0].callable());
                assert_eq!(region.source_parameter(), lineages[0].source_parameter());
            }
            results.push((layout, regions.len()));
        } else {
            if expected == 0 {
                assert!(catalog.nested_forward_regions().is_empty());
            }
            results.push((layout, lineages.len()));
        }
    }
    for (layout, count) in results {
        assert_eq!(
            count,
            if require_region { 1 } else { expected },
            "{layout}: missing requested three-leaf lineage or terminal region"
        );
    }
}

#[test]
fn ordinary_nested_lineage_rejects_neighbor_shapes() {
    let prefix = SOURCE.split("fun wrap").next().unwrap();
    for body in [
        "if (outer) { if (inner) { return left(source) } else { return middle(source) } }\nreturn right(source)",
        "outer\nif (outer) { if (inner) { return left(source) } else { return middle(source) } } else { return right(source) }",
        "if (outer) { if (inner) { if (inner) { return left(source) } else { return left(source) } } else { return middle(source) } } else { return right(source) }",
        "if (outer) { return left(source) } else { if (inner) { return middle(source) } else { return right(source) } }",
    ] {
        check_lineage(
            &format!(
                "{prefix}fun wrap(source: String, outer: Boolean, inner: Boolean): borrow String from source {{ {body} }}"
            ),
            0,
            false,
        );
    }
}
