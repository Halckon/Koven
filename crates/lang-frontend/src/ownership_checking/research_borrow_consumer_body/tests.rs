//! CI-only genuine temporary-loan body transaction faults.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::resolve_names,
    ownership_checking::check_ownership,
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};
use std::cell::Cell;
thread_local! {static FAULT:Cell<u8>=const{Cell::new(0)};static BEGINS:Cell<usize>=const{Cell::new(0)};static CHANGED:Cell<bool>=const{Cell::new(false)};}
pub(super) fn begun() {
    BEGINS.set(BEGINS.get() + 1);
}
fn fault() -> u8 {
    if BEGINS.get() == 1 { FAULT.get() } else { 0 }
}
pub(super) fn before_expects<S>(a: &mut Active<S>) {
    if fault() == 2
        && !CHANGED.replace(true)
        && let Some((_, args)) = a.calls.get_mut(a.next)
    {
        args.swap(0, 1);
    }
}
pub(super) fn before_prepare<S>(a: &mut Active<S>) {
    match fault() {
        1 => a.pending = true,
        3 => a.operands = 0,
        _ => {}
    }
}
pub(super) fn skip_release() -> bool {
    fault() == 4
}
pub(super) fn before_complete<S>(t: &mut Prepared<S>) -> bool {
    if fault() == 5 {
        return false;
    }
    if fault() == 6 {
        t.identity = Rc::new(());
    }
    if fault() == 9 {
        t.session = Rc::new(());
    }
    true
}
pub(super) fn before_close<S>(a: &mut Active<S>) {
    if fault() == 7 {
        a.frame = None;
    }
    if fault() == 8 {
        a.checkpoint.3 = a.checkpoint.3.saturating_add(1);
    }
}
fn actual(fault: u8) -> Vec<BorrowConsumerBodyObservation> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAULT.set(0);
            BEGINS.set(0);
            CHANGED.set(false);
        }
    }
    let _reset = Reset;
    let mut sources = SourceMap::new();
    let source=sources.add_source("consumer-body-ci.ko","fun pair(left:String,right:String):Unit {}\nfun run(source:String,flag:Boolean):Unit {while(flag){pair(source,source);pair(source,source)};while(flag){pair(source,source);pair(source,source)}}").unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    FAULT.set(fault);
    BEGINS.set(0);
    CHANGED.set(false);
    let (checked, bodies) = capture_borrow_consumer_bodies(|| {
        check_ownership(&sources, &parsed, &names, &typed).unwrap()
    });
    assert!(checked.diagnostics().is_empty());
    assert_eq!(
        bodies.iter().filter(|b| b.validate()).count(),
        if fault == 0 { 2 } else { 1 },
        "sibling recovery"
    );
    assert!(bodies.last().unwrap().validate());
    bodies.into_iter().take(1).collect()
}
#[test]
#[ignore = "CI-only actual consumer body faults; compile locally, do not execute"]
fn body_consumer_requires_every_actual_acquisition_and_ordered_operand() {
    let baseline = actual(0);
    assert_eq!(baseline.len(), 1);
    assert!(baseline[0].validate());
    assert_eq!(baseline[0].consumer_returns().len(), 2);
    for fault in [1, 2, 3] {
        assert!(!actual(fault).iter().any(|b| b.validate()), "fault {fault}");
    }
}
#[test]
#[ignore = "CI-only actual consumer body faults; compile locally, do not execute"]
fn body_consumer_requires_release_completion_and_precleanup_frame() {
    assert!(actual(0)[0].validate());
    for fault in [4, 5, 6, 7, 8, 9] {
        assert!(!actual(fault).iter().any(|b| b.validate()), "fault {fault}");
    }
}
