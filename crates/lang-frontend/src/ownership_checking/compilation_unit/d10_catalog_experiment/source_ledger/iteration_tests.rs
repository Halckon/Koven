//! Original sequential/Map cleanup projections; no mutated or replayed products.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn iteration_collections_preserve_scopes_shared_exits_and_source_order() {
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
                    "interface Relay { own fun consume(n: Int): Unit {}\nown fun run(xs: List<Int>, flag: Boolean): Unit { for (_ in xs) { consume(if(flag) { for (_ in xs) {return}\nreturn } else {return}) } } }",
                ),
                (
                    "b.ko",
                    "fun mapWalk(xs: Map<Int, Int>): Unit { for ((key, value) in xs) { val pair = key + value } }",
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
            let collections: Vec<_> = ledger
                .records()
                .iter()
                .map(|record| record.iteration_collection().unwrap())
                .collect();
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
                collections
                    .iter()
                    .flat_map(|c| c.iterations())
                    .eq(owned.ownership().iterations())
            );
            assert!(
                collections
                    .iter()
                    .flat_map(|c| c.map_iterations())
                    .eq(owned.ownership().map_iterations())
            );
            assert!(
                collections
                    .iter()
                    .flat_map(|c| c.conditional_scopes())
                    .eq(&owned.ownership().iteration_conditional_scopes)
            );
            assert_eq!(collections[0].iterations().len(), 2);
            assert_eq!(collections[1].map_iterations().len(), 1);
            assert!(collections[2].iterations().is_empty());
            assert!(collections[2].map_iterations().is_empty());
            assert!(collections[2].conditional_scopes().is_empty());
            use UnitIterationCleanupAction as Action;
            let exit = collections[0]
                .iterations()
                .iter()
                .flat_map(|p| p.exits())
                .find(|exit| {
                    exit.actions()
                        .iter()
                        .filter(|a| matches!(a, Action::EndSource(_)))
                        .count()
                        == 2
                })
                .unwrap();
            let inner_end = exit
                .actions()
                .iter()
                .position(|a| matches!(a, Action::EndSource(_)))
                .unwrap();
            let conditional = exit
                .actions()
                .iter()
                .position(|a| matches!(a, Action::DropConditionalReceiver(_)))
                .unwrap();
            let outer_end = exit
                .actions()
                .iter()
                .rposition(|a| matches!(a, Action::EndElement(_)))
                .unwrap();
            assert!(inner_end < conditional && conditional < outer_end);
            let shared: Vec<_> = collections[0]
                .iterations()
                .iter()
                .flat_map(|p| p.exits())
                .filter(|other| other.point() == exit.point())
                .collect();
            assert_eq!(shared.len(), 2);
            assert!(shared.iter().all(|other| other.actions() == exit.actions()));
            let Action::DropConditionalReceiver(fact) = exit.actions()[conditional] else {
                unreachable!()
            };
            let (_, scopes) = collections[0]
                .conditional_scopes()
                .iter()
                .find(|(candidate, _)| *candidate == fact)
                .unwrap();
            assert_eq!(scopes.len(), 1);
            assert!(
                ledger.records()[0]
                    .conditional_receiver_drops()
                    .contains(&fact)
            );
            assert!(ledger.matches(&owned));
        }
    }
}
