//! Normal source only, inspecting original successful traversal identities and complete facts.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    parser::{FunctionBody, FunctionForm, Item, parse_file},
    type_checking::{check_compilation_unit_types, standard_environments},
};
#[test]
fn real_function_deinit_and_lambda_appends_cover_flat_drops_once() {
    for text in [
        "class Worker { fun first(): Unit { val a = \"one\"; println(a) }; fun second(): Unit { val b = \"two\"; println(b) }; deinit() { val d = \"deinit\"; println(d) } }\nfun empty(): Unit {}\nfun run(): Unit { val f: () -> Unit = { val a = \"lambda\"; println(a) }; f() }",
        "fun absent(): Unit\nfun empty(): Unit {}\nfun unused(own text: String): Unit {}\nfun transfer(flag: Boolean): Unit { val held = \"held\"; if(flag) { return }; println(held) }\nfun scan(source: List<String>): Unit { for(item in source) { val x = \"loop\"; println(x); break } }",
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("callable-drops.ko", text).unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        let inputs = [SourceUnitInput::new(
            "root",
            "callable-drops.ko",
            source,
            &parsed,
        )];
        let (environment, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let owned = analysis::analyze(
            &sources,
            &inputs,
            names.names(),
            &types,
            typed.types(),
            false,
        )
        .unwrap()
        .validate()
        .unwrap();
        let ledger = source_collection_ledger(&owned).unwrap();
        let record = &ledger.records()[0];
        let calls = record.callable_drop_traversals();
        // Compare full original points, targets, origins, order and multiplicity, not counts.
        assert!(
            calls
                .iter()
                .flat_map(|batch| batch.drops())
                .eq(owned.ownership().drops())
        );
        assert!(
            calls
                .iter()
                .flat_map(|batch| batch.drops())
                .eq(record.drops())
        );
        let expected_items: Vec<_> = parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(id, node)| {
                let item = UnitItemId::new(record.source(), id);
                match node.payload() {
                    Item::Function { form, .. } => Some(DropCallable::Function {
                        item,
                        has_body: !matches!(
                            form,
                            FunctionForm::ImplicitUnitAbsent
                                | FunctionForm::Explicit {
                                    body: FunctionBody::Absent,
                                    ..
                                }
                        ),
                    }),
                    Item::Deinit { .. } => Some(DropCallable::Deinit(item)),
                    _ => None,
                }
            })
            .collect();
        let actual_items: Vec<_> = calls
            .iter()
            .map(|b| b.callable())
            .filter(|c| !matches!(c, DropCallable::Lambda(_)))
            .collect();
        assert_eq!(actual_items.len(), expected_items.len());
        for item in expected_items {
            assert_eq!(
                actual_items
                    .iter()
                    .filter(|actual| **actual == item)
                    .count(),
                1
            );
        }
        let lambdas: Vec<_> = calls
            .iter()
            .filter_map(|batch| match batch.callable() {
                DropCallable::Lambda(lambda) => Some((lambda, batch.drops())),
                _ => None,
            })
            .collect();
        let original_lambdas: Vec<_> = record
            .drop_traversals()
            .iter()
            .filter_map(|batch| match batch.traversal() {
                DropTraversal::Lambda(lambda) => Some((lambda, batch.drops())),
                _ => None,
            })
            .collect();
        assert_eq!(lambdas, original_lambdas);
        assert!(calls.iter().any(|b| matches!(
            b.callable(),
            DropCallable::Function { has_body: true, .. }
        ) && b.drops().is_empty()));
        if text.contains("absent") {
            assert!(
                record
                    .drops()
                    .iter()
                    .any(|d| matches!(d.point(), UnitDropPoint::FunctionEntry(_)))
            );
            assert!(
                record
                    .drops()
                    .iter()
                    .any(|d| matches!(d.point(), UnitDropPoint::ControlTransfer(_)))
            );
            assert!(calls.iter().any(|b| matches!(
                b.callable(),
                DropCallable::Function {
                    has_body: false,
                    ..
                }
            ) && b.drops().is_empty()));
        } else {
            assert_eq!(lambdas.len(), 1);
            assert!(!lambdas[0].1.is_empty());
            for expected in ["first", "second"] {
                let (id, _) = parsed.ast().items().iter().find(|(_, node)| matches!(node.payload(), Item::Function { name: crate::parser::NameMarker::Present(span), .. } if sources.slice(*span).unwrap() == expected)).unwrap();
                let item = UnitItemId::new(record.source(), id);
                let batch = calls
                    .iter()
                    .find(|b| {
                        b.callable()
                            == (DropCallable::Function {
                                item,
                                has_body: true,
                            })
                    })
                    .unwrap();
                assert!(!batch.drops().is_empty());
            }
            assert!(
                calls
                    .iter()
                    .any(|b| matches!(b.callable(), DropCallable::Deinit(_))
                        && !b.drops().is_empty())
            );
        }
        assert!(ledger.matches(&owned));
    }
}
