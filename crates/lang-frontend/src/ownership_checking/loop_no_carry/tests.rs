//! CI-only faults at real core events, never a synthetic accepted checker state.
use crate::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{check_compilation_unit_ownership, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};
use std::cell::Cell;
thread_local! {static FAULT:Cell<u8>=const{Cell::new(0)};}
pub(super) fn at(phase: u8) -> bool {
    FAULT.get() == phase
}
fn actual(fault: u8) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAULT.set(0);
        }
    }
    let _reset = Reset;
    let mut sources = SourceMap::new();
    let source=sources.add_source("no-carry-ci.ko","fun view(source:String):borrow String from source=source\nfun observe(item:String):Unit {}\nfun run(source:String,flag:Boolean):Unit {while(flag){borrow val item=view(source);observe(item);observe(item)}}").unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (env, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &env).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    FAULT.set(fault);
    let single = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert_eq!(single.diagnostics().is_empty(), fault == 0);
    assert_eq!(
        single.borrow_results().bindings().len(),
        usize::from(fault == 0)
    );
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert_eq!(unit.diagnostics().is_empty(), fault == 0);
    assert_eq!(unit.no_carry_loops().len(), usize::from(fault == 0));
    if fault != 0 {
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164")
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164")
        );
        assert!(unit.borrow_results().bindings().is_empty());
    }
}
#[test]
#[ignore = "CI-only actual no-carry event faults; local compile only"]
fn missing_actual_acquire_release_return_never_grants_core_authority() {
    actual(0);
    for fault in [1, 2, 3] {
        actual(fault);
        actual(0);
    }
}
#[test]
#[ignore = "CI-only actual no-carry closure faults; local compile only"]
fn foreign_end_missing_precleanup_or_normal_edge_revokes_capability() {
    actual(0);
    for fault in [4, 5, 6] {
        actual(fault);
        actual(0);
    }
}
