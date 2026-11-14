//! Normal source fixtures only; artificial cross-run mixes belong to CI.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn conditional_batches_keep_full_fields_source_order_and_failed_publication() {
    for (tail, error, deferred) in [
        ("fun empty(): Unit {}", false, false),
        (
            "fun bad(): Unit { val a = \"x\"; val b = a; println(a); println(b) }",
            true,
            false,
        ),
        (
            "class Item {}\nclass Holder(var payload: Item)\nfun bad(holders: List<Holder>): Unit { val projected = holders[0].payload }",
            false,
            true,
        ),
    ] {
        for reversed in [false, true] {
            let mut sources = SourceMap::new();
            let texts = [
                (
                    "a.ko",
                    "interface Finishable { own fun finish(): Int = 40 }\nclass Resource: Finishable {}\nvalue class Counter(val item: Int): Finishable {}",
                ),
                (
                    "b.ko",
                    "interface Relay { own fun consume(text: String, own flag: Boolean): Unit {}\nown fun relay(own flag: Boolean): Unit { val older = \"older\"\nval done = consume(\"prefix\", if (flag) { val newer = \"newer\"\nif (flag) { return } else { newer == \"newer\" } } else { true })\nval used = println(older) } }",
                ),
                ("c.ko", tail),
            ];
            let parsed: Vec<_> = texts
                .iter()
                .map(|(path, text)| {
                    let source = sources.add_source(*path, *text).unwrap();
                    (
                        source,
                        parse_file(&sources, &lex(&sources, source).unwrap()).unwrap(),
                    )
                })
                .collect();
            let mut inputs: Vec<_> = texts
                .iter()
                .zip(&parsed)
                .map(|((path, _), (source, parsed))| {
                    SourceUnitInput::new("root", path, *source, parsed)
                })
                .collect();
            if reversed {
                inputs.reverse();
            }
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
            .unwrap();
            assert_eq!(!owned.diagnostics().is_empty(), error);
            assert_eq!(!owned.deferred().is_empty(), deferred);
            if error || deferred {
                assert!(owned.source_collection_ledger.is_none());
                assert!(owned.validate().is_err());
                continue;
            }
            let owned = owned.validate().unwrap();
            let ledger = source_collection_ledger(&owned).unwrap();
            assert_eq!(ledger.records().len(), 3);
            assert!(
                ledger
                    .records()
                    .iter()
                    .map(|record| record.source())
                    .eq(names
                        .names()
                        .index()
                        .source_units()
                        .iter()
                        .map(|source| source.id()))
            );
            assert!(
                ledger
                    .records()
                    .iter()
                    .flat_map(|record| record.conditional_receiver_drops())
                    .eq(owned.ownership().conditional_receiver_drops())
            );
            for record in ledger.records() {
                assert!(
                    record
                        .callable_drop_traversals()
                        .iter()
                        .flat_map(|batch| batch.conditional_receiver_drops())
                        .eq(record.conditional_receiver_drops())
                );
            }
            // Resolve the exact underlying function item from each original parsed source.
            // No reconstruction from drop targets or spans of final facts.
            for (source_index, function_name) in [(0, "finish"), (1, "relay")] {
                let (item, _) = parsed[source_index].1.ast().items().iter().find(|(_, node)| matches!(node.payload(), crate::parser::Item::Function { name: crate::parser::NameMarker::Present(span), .. } if sources.slice(*span).unwrap() == function_name)).unwrap();
                let record = &ledger.records()[source_index];
                let callable = DropCallable::Function {
                    item: UnitItemId::new(record.source(), item),
                    has_body: true,
                };
                let matches: Vec<_> = record
                    .callable_drop_traversals()
                    .iter()
                    .filter(|batch| batch.callable() == callable)
                    .collect();
                assert_eq!(matches.len(), 1);
                let conditional = matches[0].conditional_receiver_drops();
                assert!(!conditional.is_empty());
                if function_name == "finish" {
                    assert_eq!(conditional, record.conditional_receiver_drops());
                } else {
                    assert!(conditional.iter().any(|fact| fact.preceding_drops() == 2));
                }
            }
            let [finish] = ledger.records()[0].conditional_receiver_drops() else {
                panic!("one generic finish obligation")
            };
            assert_eq!(sources.slice(finish.value_origin()).unwrap(), "finish");
            assert!(matches!(finish.point(), UnitDropPoint::ControlTransfer(_)));
            assert_eq!(finish.preceding_drops(), 0);
            assert!(
                !owned
                    .ownership()
                    .drops()
                    .iter()
                    .any(|fact| matches!(fact.target(), UnitDropTarget::This(_)))
            );
            let conditional = ledger.records()[1]
                .conditional_receiver_drops()
                .iter()
                .find(|fact| fact.preceding_drops() == 2)
                .unwrap();
            let origins: Vec<_> = owned
                .ownership()
                .drops()
                .iter()
                .filter(|fact| fact.point() == conditional.point())
                .map(|fact| sources.slice(fact.value_origin()).unwrap())
                .collect();
            assert_eq!(origins, ["newer", "\"prefix\"", "older"]);
            assert!(ledger.records()[2].conditional_receiver_drops().is_empty());
            assert!(ledger.matches(&owned));
        }
    }
}
