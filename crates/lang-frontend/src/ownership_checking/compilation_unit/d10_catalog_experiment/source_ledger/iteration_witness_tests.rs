//! Normal multi-source iteration witnesses; no artificial products or cross-run mixing.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::{index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn iteration_witnesses_preserve_five_channels_and_all_three_loan_end_kinds() {
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
            // Reuse the three ordinary multifile iteration cleanup fixtures. All remain
            // original frontend inputs, not facts reconstructed from the public ledger.
            let texts = [
                (
                    "p/a.ko",
                    "package p\nfun use(text: String, count: Int) {}\nfun make(): List<String> = listOf(\"value\")\nclass Item { fun read(n: Int) {} }",
                ),
                (
                    "q/b.ko",
                    "package q\nfun callRun(flag: Boolean) { for (item in p.make()) { p.use(item, if(flag) {return} else 1) } }",
                ),
                (
                    "q/c.ko",
                    "package q\nfun captureRun(flag: Boolean) { for (item in p.make()) { val f: () -> Unit = { println(item) }\nif(flag) {return}\nf() } }",
                ),
                (
                    "q/d.ko",
                    "package q\nfun receiverRun(xs: List<p.Item>, flag: Boolean) { for (item in xs) { item.read(if(flag) {return} else 1) } }",
                ),
                (
                    "q/e.ko",
                    "package q\nclass Guard { deinit() {} }\nfun scan(source: MutableMap<Int, Int>): Unit { for ((key, value) in source) { val local = Guard(); val keyCopy: Int = key; val valueCopy: Int = value; continue } }",
                ),
                ("z.ko", tail),
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
            assert_eq!(
                !owned.diagnostics().is_empty(),
                error,
                "{:?}",
                owned.diagnostics()
            );
            assert_eq!(
                !owned.deferred().is_empty(),
                deferred,
                "{:?}",
                owned.deferred()
            );
            if error || deferred {
                assert!(owned.source_collection_ledger.is_none());
                assert!(owned.validate().is_err());
                continue;
            }
            let owned = owned.validate().unwrap();
            let ledger = source_collection_ledger(&owned).unwrap();
            let facts = owned.ownership();
            assert_eq!(ledger.records().len(), texts.len());
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
            let collections: Vec<_> = ledger
                .records()
                .iter()
                .map(|record| record.iteration_collection().unwrap())
                .collect();
            assert!(
                collections
                    .iter()
                    .flat_map(|c| c.owner_scopes())
                    .eq(&facts.iteration_owner_scopes)
            );
            assert!(
                collections
                    .iter()
                    .flat_map(|c| c.loan_ends())
                    .eq(&facts.iteration_loan_ends)
            );
            assert!(
                collections
                    .iter()
                    .flat_map(|c| c.required_exits())
                    .eq(&facts.iteration_required_exits)
            );
            assert!(
                collections
                    .iter()
                    .flat_map(|c| c.templates())
                    .eq(&facts.iteration_templates)
            );
            assert!(
                collections
                    .iter()
                    .flat_map(|c| c.map_templates())
                    .eq(&facts.map_iteration_templates)
            );
            // Empty channels cannot make these complete-array comparisons vacuously pass.
            assert!(!facts.iteration_owner_scopes.is_empty());
            assert!(!facts.iteration_loan_ends.is_empty());
            assert!(!facts.iteration_required_exits.is_empty());
            assert!(!facts.iteration_templates.is_empty());
            assert!(!facts.map_iteration_templates.is_empty());
            let empty_source = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|source| source.source_id() == parsed[5].0)
                .unwrap()
                .id();
            let empty = ledger
                .records()
                .iter()
                .find(|record| record.source() == empty_source)
                .unwrap()
                .iteration_collection()
                .unwrap();
            assert!(empty.owner_scopes().is_empty());
            assert!(empty.loan_ends().is_empty());
            assert!(empty.required_exits().is_empty());
            assert!(empty.templates().is_empty());
            assert!(empty.map_templates().is_empty());
            use UnitIterationCleanupAction as Action;
            assert!(
                facts
                    .iteration_loan_ends
                    .iter()
                    .any(|(_, action)| matches!(action, Action::EndCallLoan(_)))
            );
            assert!(
                facts
                    .iteration_loan_ends
                    .iter()
                    .any(|(_, action)| matches!(action, Action::EndReceiverLoan(_)))
            );
            assert!(
                facts
                    .iteration_loan_ends
                    .iter()
                    .any(|(_, action)| matches!(action, Action::EndCaptureLoan { .. }))
            );
            let map_source = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|source| source.source_id() == parsed[4].0)
                .unwrap()
                .id();
            let map = ledger
                .records()
                .iter()
                .find(|record| record.source() == map_source)
                .unwrap()
                .iteration_collection()
                .unwrap();
            assert!(!map.owner_scopes().is_empty());
            assert!(!map.required_exits().is_empty());
            assert_eq!(map.map_templates().len(), 1);
            let map_continue = map
                .map_iterations()
                .iter()
                .flat_map(|plan| plan.exits())
                .find(|exit| matches!(exit.kind(), UnitIterationExitKind::Continue(_)))
                .unwrap();
            assert!(
                map_continue
                    .actions()
                    .iter()
                    .any(|action| matches!(action, Action::Drop(_)))
            );
            assert!(ledger.matches(&owned));
        }
    }
}
