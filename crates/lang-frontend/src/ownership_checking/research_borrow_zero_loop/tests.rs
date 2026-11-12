//! CI-only genuine zero-loop boundary faults; no synthetic success observations.
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
thread_local! {static FAULT:Cell<u8>=const{Cell::new(0)};}
pub(super) fn skip_header() -> bool {
    FAULT.get() == 1
}
pub(super) fn before_header<S>(header: &mut Header<S>) {
    match FAULT.get() {
        2 => header.parent.checker = BorrowFrameChecker::Unit,
        3 => header.session = Rc::new(()),
        4 => header.checkpoint.0 = header.checkpoint.0.saturating_add(1),
        _ => {}
    }
}
pub(super) fn skip_condition() -> bool {
    FAULT.get() == 5
}
pub(super) fn before_condition<S>(header: &mut Header<S>) {
    if FAULT.get() == 6 {
        header.identity = Rc::new(());
    }
}
/// Adjust only actual finish inputs, never manufacture a preserved observation.
pub(super) fn finish_inputs<S>(normal: &mut Option<&S>, only_normal: &mut bool) {
    if FAULT.get() == 7 {
        *normal = None;
    }
    if FAULT.get() == 8 {
        *only_normal = false;
    }
}
fn actual(fault: u8) -> Vec<BorrowZeroLoopObservation> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAULT.set(0);
        }
    }
    let _reset = Reset;
    let mut sources = SourceMap::new();
    let text = "fun view(source:String):borrow String from source=source\nfun observe(item:String):Unit {}\nfun run(source:String,flag:Boolean):Unit {{borrow val item=view(source);observe(item)};while(flag){}}";
    let source = sources.add_source("zero-loop-ci.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    let (_, prior) = frames::capture_borrow_frames(|| {
        check_ownership(&sources, &parsed, &names, &typed).unwrap()
    });
    assert_eq!(
        prior
            .iter()
            .filter(|f| f.validate() && f.complete_closure())
            .count(),
        1
    );
    FAULT.set(fault);
    let (checked, observations) =
        capture_borrow_zero_loops(|| check_ownership(&sources, &parsed, &names, &typed).unwrap());
    assert!(checked.diagnostics().is_empty());
    observations
}
#[test]
#[ignore = "CI-only actual boundary faults; compile locally, do not execute"]
fn zero_loop_requires_real_header_parent_session_and_checkpoint() {
    let baseline = actual(0);
    assert_eq!(baseline.len(), 1);
    assert!(baseline[0].validate());
    assert!(baseline[0].preceding_frame().complete_closure());
    for fault in [1, 2, 3, 4] {
        assert!(!actual(fault).iter().any(|o| o.validate()), "fault {fault}");
    }
}
#[test]
#[ignore = "CI-only actual boundary faults; compile locally, do not execute"]
fn zero_loop_requires_condition_and_exact_normal_edge() {
    assert!(actual(0)[0].validate());
    for fault in [5, 6, 7, 8] {
        assert!(!actual(fault).iter().any(|o| o.validate()), "fault {fault}");
    }
}
