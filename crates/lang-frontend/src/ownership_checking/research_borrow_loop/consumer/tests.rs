//! CI-only failures injected into actual checker-produced call-return transactions.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::resolve_names,
    ownership_checking::{capture_borrow_loop_edges, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};
use std::cell::Cell;
thread_local! {
    static FAULT:Cell<u8> = const {Cell::new(0)};
    static FOREIGN:Cell<Option<SourceId>> = const {Cell::new(None)};
}
pub(super) fn before_prepare(protocol: &mut Protocol) {
    match FAULT.get() {
        1 => protocol.pending = true,
        2 => {
            if protocol.calls.len() > 1 {
                protocol.calls.swap(0, 1);
            }
        }
        3 => protocol.source = FOREIGN.get().unwrap(),
        _ => {}
    }
}
pub(super) fn skip_release() -> bool {
    FAULT.get() == 4
}
pub(super) fn before_complete<S>(prepared: &mut Prepared<S>) -> bool {
    if FAULT.get() == 5 {
        return false;
    }
    if FAULT.get() == 6 {
        prepared.event.identity = Rc::new(());
    }
    true
}
fn actual(fault: u8) -> Vec<super::super::BorrowLoopEdgeObservation> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAULT.set(0);
            FOREIGN.set(None);
        }
    }
    let _reset = Reset;
    let mut sources = SourceMap::new();
    let source=sources.add_source("consumer-ci.ko", "fun view(source: String): borrow String from source = source\nfun observe(item: String): Unit {}\nfun run(source: String, flag: Boolean): Unit { borrow val item = view(source); while (flag) { observe(item); observe(item) } }").unwrap();
    FOREIGN.set(Some(
        sources
            .add_source("foreign.ko", "fun unrelated(): Unit {}")
            .unwrap(),
    ));
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    FAULT.set(fault);
    let (checked, edges) =
        capture_borrow_loop_edges(|| check_ownership(&sources, &parsed, &names, &typed).unwrap());
    assert!(checked.diagnostics().is_empty());
    assert_eq!(edges.len(), 1);
    assert!(edges[0].preserved());
    edges
}
#[test]
#[ignore = "CI-only actual-frame protocol faults; compile locally, do not execute"]
fn consumer_tokens_reject_duplicate_reordered_and_foreign_transactions() {
    let baseline = actual(0);
    assert!(baseline[0].validate());
    assert_eq!(baseline[0].consumer_returns().len(), 2);
    for fault in [1, 2, 3] {
        assert!(!actual(fault)[0].validate(), "fault {fault}");
    }
}
#[test]
#[ignore = "CI-only actual-frame protocol faults; compile locally, do not execute"]
fn consumer_tokens_require_release_completion_and_same_loop_identity() {
    assert!(actual(0)[0].validate());
    for fault in [4, 5, 6] {
        assert!(!actual(fault)[0].validate(), "fault {fault}");
    }
}
