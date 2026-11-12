//! CI-only actual body lifecycle faults; every fault is limited to the first sibling.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::resolve_names,
    ownership_checking::{
        BorrowFrameChecker, BorrowZeroLoopObservation, capture_borrow_zero_loops, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};
use std::cell::Cell;
thread_local! {static FAULT:Cell<u8>=const{Cell::new(0)};static MINTS:Cell<usize>=const{Cell::new(0)};}
pub(super) fn minted() {
    MINTS.set(MINTS.get() + 1);
}
fn fault() -> u8 {
    if MINTS.get() == 1 { FAULT.get() } else { 0 }
}
pub(super) fn skip_enter() -> bool {
    fault() == 1
}
pub(super) fn skip_close() -> bool {
    fault() == 2
}
pub(super) fn before_enter<S>(p: &mut Permit<S>) {
    match fault() {
        3 => p.session = Rc::new(()),
        4 => p.loop_identity = Rc::new(()),
        5 => p.checkpoint.0 = p.checkpoint.0.saturating_add(1),
        _ => {}
    }
}
pub(super) fn before_close<S>(t: &mut BodyTicket<S>) {
    if fault() == 6 {
        t.frame.checker = BorrowFrameChecker::Unit;
    }
}
pub(super) fn before_consume<S>(e: &mut Exit<S>) {
    match fault() {
        7 => e.permit.loop_identity = Rc::new(()),
        8 => e.permit.checkpoint.1 = e.permit.checkpoint.1.saturating_add(1),
        _ => {}
    }
}
fn actual(fault: u8) -> Vec<BorrowZeroLoopObservation> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAULT.set(0);
            MINTS.set(0);
        }
    }
    let _reset = Reset;
    let mut sources = SourceMap::new();
    let source=sources.add_source("zero-body-ci.ko","fun view(source:String):borrow String from source=source\nfun observe(item:String):Unit {}\nfun run(source:String,flag:Boolean):Unit {{borrow val first=view(source);observe(first)};while(flag){};{borrow val second=view(source);observe(second)};while(flag){}}").unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    FAULT.set(fault);
    MINTS.set(0);
    let (checked, observations) =
        capture_borrow_zero_loops(|| check_ownership(&sources, &parsed, &names, &typed).unwrap());
    assert!(checked.diagnostics().is_empty());
    assert_eq!(observations.len(), 2);
    assert!(observations.iter().all(|o| o.validate()));
    observations
}
#[test]
#[ignore = "CI-only actual body lifecycle faults; compile locally, do not execute"]
fn body_missing_claim_or_close_cannot_poison_next_sibling() {
    assert!(actual(0).iter().all(|o| o.body_complete()));
    for fault in [1, 2] {
        let observed = actual(fault);
        assert!(!observed[0].body_complete());
        assert!(
            observed[1].body_complete(),
            "next real sibling must recover {fault}"
        );
    }
}
#[test]
#[ignore = "CI-only actual body lifecycle faults; compile locally, do not execute"]
fn body_actual_identity_and_checkpoint_fail_closed() {
    assert!(actual(0).iter().all(|o| o.body_complete()));
    for fault in [3, 4, 5, 6, 7, 8] {
        let observed = actual(fault);
        assert!(!observed[0].body_complete(), "fault {fault}");
        assert!(observed[1].body_complete());
    }
}
