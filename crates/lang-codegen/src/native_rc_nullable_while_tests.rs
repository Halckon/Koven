//! SPEC-0332: bounded presence results through both ordinary native facades.
//! The same four sources and golden outputs are shared with the verified LLVM gate.

use std::process::{Command, Output};

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use crate::{
    emit_native_unit_object,
    native_tests::{TestDirectory, emit_link_and_run},
    ssa::rc_nullable_while_bounded_tests::presence_cases,
};

#[test]
fn single_rc_nullable_while_presence_sources_exit_normally() {
    for (case, source, expected) in presence_cases() {
        let run = emit_link_and_run(&format!("{case}.ko"), source, "entry");
        assert_result(case, "single", &run, expected);
    }
}

#[test]
fn unit_rc_nullable_while_presence_sources_exit_normally() {
    for (case, source, expected) in presence_cases() {
        let run = run_unit(case, source);
        assert_result(case, "unit", &run, expected);
    }
}

fn assert_result(case: &str, entry: &str, run: &Output, expected: &str) {
    assert_eq!(run.status.code(), Some(0), "{case} {entry}: {run:?}");
    assert_eq!(run.stdout, expected.as_bytes(), "{case} {entry}: {run:?}");
    assert!(run.stderr.is_empty(), "{case} {entry}: {run:?}");
    println!("SPEC0332 NATIVE case={case} entry={entry} exit=0 stdout=golden stderr=empty");
}

fn run_unit(case: &str, source_text: &str) -> Output {
    // Match the preflight exactly: one real p source and its own p.entry.
    let path = format!("p/{case}.ko");
    let text = format!("package p\n{source_text}");
    let mut sources = SourceMap::new();
    let source = sources.add_source(&path, &text).expect("unique source");
    let lexed = lex(&sources, source).expect("lexing succeeds");
    assert!(lexed.diagnostics().is_empty(), "{:?}", lexed.diagnostics());
    let file = parse_file(&sources, &lexed).expect("parsing succeeds");
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let inputs = [SourceUnitInput::new("root", &path, source, &file)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("name resolution succeeds")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .expect("type checking succeeds");
    complete_compilation_unit_types(&sources, &inputs, &names, &environment, &typed)
        .expect("complete types");
    let typed = typed.validate().expect("valid types");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("ownership checking succeeds");
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let owned = owned.validate().expect("valid ownership");
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "entry"
                && declaration.kind() == SymbolKind::Function
                && names.names().index().packages()[declaration.package().index()]
                    .name()
                    .expect("named package")
                    .segments()
                    .iter()
                    .map(String::as_str)
                    .eq(["p"])
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        panic!("expected exactly one p.entry in {path}");
    };
    assert_eq!(entry.name_span().source_id(), source);
    let directory = TestDirectory::create();
    let object = directory.join("program.o");
    let executable = directory.join("program");
    emit_native_unit_object(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        entry.id(),
        &object,
    )
    .expect("presence source must emit a native unit object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{case}: {linked:?}");
    Command::new(&executable)
        .output()
        .expect("linked presence executable must launch")
}
