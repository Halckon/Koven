//! Original shared-caller source split across real files; SSA/LLVM only.
use super::*;
use crate::ssa::model::{Operation, Origin};

#[test]
fn split_shared_entries_preserve_source_and_selection_identity() {
    let text = source(false, "");
    let (leaves, rest) = text.split_once("fun run(").unwrap();
    let (caller, entry) = rest.split_once("fun main(").unwrap();
    let caller = format!("fun run({caller}");
    let entries = format!(
        "fun main({entry}\nfun alias():Unit {{val rounds=1;val choose=true;run(\"A\",\"B\",rounds,choose)}}"
    );
    for reverse_inputs in [false, true] {
        let mut sources = SourceMap::new();
        let texts = [
            ("leaves.ko", leaves),
            ("caller.ko", caller.as_str()),
            ("entries.ko", entries.as_str()),
        ];
        let ids = texts.map(|(path, text)| sources.add_source(path, text).unwrap());
        let files = ids.map(|id| {
            let lexed = lex(&sources, id).unwrap();
            assert!(lexed.diagnostics().is_empty());
            parse_file(&sources, &lexed).unwrap()
        });
        assert!(files.iter().all(|file| file.diagnostics().is_empty()));
        let mut inputs = texts
            .iter()
            .zip(ids)
            .zip(&files)
            .map(|(((path, _), id), file)| SourceUnitInput::new("root", path, id, file))
            .collect::<Vec<_>>();
        if reverse_inputs {
            inputs.reverse();
        }
        let (env, types) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let named = |name| {
            names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == name)
                .unwrap()
        };
        let source_span = |name| {
            let declaration = named(name);
            let file = files
                .iter()
                .find(|file| file.source_id() == declaration.name_span().source_id())
                .unwrap();
            file.ast().items().get(declaration.root()).unwrap().span()
        };
        let main = named("main").id();
        let alias = named("alias").id();
        for requested in [[main, alias], [alias, main]] {
            let units =
                prepare_two_source_units(&sources, &inputs, &names, &types, &typed, &requested)
                    .expect("real split-source shared entries must prepare together");
            assert_eq!(
                units
                    .entries
                    .iter()
                    .map(|(entry, _)| *entry)
                    .collect::<Vec<_>>(),
                requested
            );
            assert_eq!(units.program.modules.len(), 1);
            let module = &units.program.modules[0];
            assert_eq!(
                module.functions.len(),
                7,
                "two leaves, two arm helpers, one caller, two entries"
            );
            for leaf in ["view", "consume"] {
                let span = source_span(leaf);
                assert_eq!(span.source_id(), ids[0]);
                assert_eq!(
                    module
                        .functions
                        .iter()
                        .filter(|function| function.origin == Origin::Source(span))
                        .count(),
                    1
                );
            }
            let mut callees = Vec::new();
            let mut roots = Vec::new();
            for (entry, name) in [(main, "main"), (alias, "alias")] {
                let (actual_sources, program, function) = units.parts(entry).unwrap();
                assert!(std::ptr::eq(actual_sources, &sources));
                assert!(std::ptr::eq(program, &units.program));
                let root = module.function(function).unwrap();
                assert_eq!(root.origin, Origin::Source(source_span(name)));
                assert_eq!(root.origin.span().source_id(), ids[2]);
                roots.push(function);
                let calls = root
                    .instructions
                    .iter()
                    .filter_map(|instruction| {
                        if let Operation::DirectCall { callee, .. } = instruction.operation {
                            Some(callee)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                let [callee] = calls.as_slice() else {
                    panic!("one original direct call");
                };
                assert_eq!(
                    module.function(*callee).unwrap().origin,
                    Origin::Source(source_span("run"))
                );
                assert_eq!(source_span("run").source_id(), ids[1]);
                callees.push(*callee);
            }
            assert_ne!(roots[0], roots[1]);
            assert_eq!(
                callees[0], callees[1],
                "same actual caller, not name-only matching"
            );
            crate::ssa::verify::verify_program(&units.program).unwrap();
            let llvm = crate::llvm::render_verified_program(&units.program).unwrap();
            assert!(!llvm.is_empty());
        }
    }
}
