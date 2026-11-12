//! CI-only failures in genuine ordinary-frame consumer transactions.
use super::*;
use crate::{
    lexer::lex,
    name_resolution::resolve_names,
    ownership_checking::{BorrowFrameObservation, capture_borrow_frames, check_ownership},
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};
use std::cell::Cell;
thread_local! {
 static FAULT:Cell<u8>=const{Cell::new(0)};
 static FOREIGN:Cell<Option<Span>>=const{Cell::new(None)};
}
pub(super) fn before_prepare(frame: &mut Frame) {
    match FAULT.get() {
        1 => frame.consumers.pending = true,
        2 => {
            if let Some(Step::Consume(_, _, arguments)) =
                frame.consumers.steps.get_mut(frame.consumers.next)
                && arguments.len() == 2
            {
                arguments.swap(0, 1);
            }
        }
        3 => frame.span = FOREIGN.get().unwrap(),
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
fn actual(fault: u8) -> Vec<BorrowFrameObservation> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            FAULT.set(0);
            FOREIGN.set(None);
        }
    }
    let _reset = Reset;
    let mut sources = SourceMap::new();
    let text = "fun view(source: String): borrow String from source = source\nfun pair(left: String,right: String): Unit {}\nfun run(source: String): Unit {{borrow val item=view(source);pair(item,item);pair(item,item)}}";
    let source = sources.add_source("frame-consumer-ci.ko", text).unwrap();
    let foreign = sources
        .add_source("foreign-frame.ko", "fun unused(): Unit {}")
        .unwrap();
    FOREIGN.set(Some(sources.span(foreign, 0, 1).unwrap()));
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(parsed.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty());
    FAULT.set(fault);
    let (checked, frames) =
        capture_borrow_frames(|| check_ownership(&sources, &parsed, &names, &typed).unwrap());
    assert!(checked.diagnostics().is_empty());
    let frames = frames
        .into_iter()
        .filter(|f| f.local_binding_count() == 1 && f.end_count() == 1)
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 1);
    assert!(
        frames[0].validate(),
        "old frame remains valid for fault {fault}"
    );
    frames
}
#[test]
#[ignore = "CI-only actual-frame faults; compile locally, do not execute"]
fn frame_consumers_reject_duplicate_pending_reordered_operands_foreign_source() {
    let baseline = actual(0);
    assert!(baseline[0].complete_closure());
    assert_eq!(baseline[0].consumer_returns().len(), 2);
    for fault in [1, 2, 3] {
        assert!(!actual(fault)[0].complete_closure(), "fault {fault}");
    }
}
#[test]
#[ignore = "CI-only actual-frame faults; compile locally, do not execute"]
fn frame_consumers_require_release_normal_completion_and_owned_identity() {
    assert!(actual(0)[0].complete_closure());
    for fault in [4, 5, 6] {
        assert!(!actual(fault)[0].complete_closure(), "fault {fault}");
    }
}
