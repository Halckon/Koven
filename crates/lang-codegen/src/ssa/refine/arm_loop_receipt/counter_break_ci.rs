//! CI-only mutations of independently captured conditional-break receipts.
use super::publication::{Fault, Trace, require_ci, with_fault};
use super::*;
use crate::ssa::{
    model::{Program, TerminatorKind},
    unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
fn lower(fault: Option<Fault>) -> (Result<Program, LoweringError>, Vec<Receipt>, Trace) {
    let body = "(source:String,rounds:Int,choose:Boolean,stop:Boolean,other:String):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b);consume(b)};if(stop){break};remaining=remaining-1};consume(source);consume(other)}";
    let text = format!(
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun first{body}\nfun second{body}\nfun run(source:String,rounds:Int,choose:Boolean,stop:Boolean):Unit {{first(source,rounds,choose,stop,source);second(source,rounds,choose,stop,source)}}"
    );
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "break-ci.ko", &text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    assert_eq!(owned.ownership().no_carry_counter_break_branches().len(), 2);
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap()
        .id();
    let execute = || {
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
        .map(|(p, _)| p)
    };
    match fault {
        Some(f) => with_fault(f, execute),
        None => observe(execute),
    }
}
fn capture() -> (Function, Receipt) {
    let (p, receipts, trace) = lower(None);
    let p = p.unwrap();
    assert_eq!(receipts.len(), 2);
    assert_eq!(trace.recorded, 2);
    assert_eq!(trace.before_verifier, 2);
    assert_eq!(trace.pending_after, 0);
    assert!(!trace.rolled_back);
    for r in &receipts {
        assert!(r.plan.counter_break.is_some());
        r.validate(
            p.modules
                .iter()
                .flat_map(|m| &m.functions)
                .find(|f| f.id == r.caller)
                .unwrap(),
        )
        .unwrap();
    }
    let r = receipts[0].clone();
    let f = p
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.id == r.caller)
        .unwrap()
        .clone();
    (f, r)
}
#[test]
fn ordinary_counter_break_receipts_publish_and_replay() {
    let _ = capture();
}

#[test]
#[ignore = "CI-only wrong actual exit; local compile/list only"]
fn counter_break_wrong_exit_rejects() {
    require_ci();
    let (mut f, r) = capture();
    let cb = r.plan.counter_break.as_ref().unwrap();
    let yes = r.evidence.blocks[&cb.split.unwrap()[1]];
    let wrong = r.evidence.blocks[&r.plan.blocks.unwrap()[1]];
    let TerminatorKind::Branch(edge) = &mut f.blocks[yes.index()].terminator.as_mut().unwrap().kind
    else {
        panic!("break")
    };
    assert_ne!(edge.target, wrong);
    edge.target = wrong;
    assert!(r.validate(&f).is_err());
}
#[test]
#[ignore = "CI-only stop polarity reversal; local compile/list only"]
fn counter_break_stop_polarity_rejects() {
    require_ci();
    let (mut f, r) = capture();
    let gate = r.evidence.blocks[&r.plan.counter_break.as_ref().unwrap().split.unwrap()[0]];
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &mut f.blocks[gate.index()].terminator.as_mut().unwrap().kind
    else {
        panic!("gate")
    };
    assert_ne!(when_true.target, when_false.target);
    std::mem::swap(when_true, when_false);
    assert!(r.validate(&f).is_err());
}
#[test]
#[ignore = "CI-only checked continue backedge substitution; local compile/list only"]
fn counter_break_continue_backedge_rejects() {
    require_ci();
    let (mut f, r) = capture();
    let success = r.evidence.blocks[&r.plan.counter.as_ref().unwrap().checked.unwrap().0[1]];
    let wrong = r.evidence.blocks[&r.plan.blocks.unwrap()[4]];
    let TerminatorKind::Branch(edge) =
        &mut f.blocks[success.index()].terminator.as_mut().unwrap().kind
    else {
        panic!("backedge")
    };
    assert_ne!(edge.target, wrong);
    edge.target = wrong;
    assert!(r.validate(&f).is_err());
}
#[test]
#[ignore = "CI-only missing actual arm End; local compile/list only"]
fn counter_break_each_arm_end_rejects() {
    require_ci();
    let (f, r) = capture();
    for arm in &r.plan.split.unwrap()[1..] {
        let mut bad = f.clone();
        let block = r.evidence.blocks[arm];
        let id = bad.blocks[block.index()].instructions.pop().unwrap();
        assert!(matches!(
            bad.instruction(id).unwrap().operation,
            crate::ssa::model::Operation::BorrowEnd { .. }
        ));
        assert!(r.validate(&bad).is_err());
    }
}
#[test]
#[ignore = "CI-only source role substitution; local compile/list only"]
fn counter_break_source_identity_rejects() {
    require_ci();
    let (f, r) = capture();
    // Substitute a different, live String parameter, not just receipt metadata.
    for arm in &r.plan.split.unwrap()[1..] {
        let mut bad = f.clone();
        let block = r.evidence.blocks[arm];
        let replacement = r.evidence.entity(*arm, r.parameters[4], r.span).unwrap();
        let EntityId::Loan(other) = replacement else {
            panic!("other String loan")
        };
        let id = bad.blocks[block.index()].instructions[0];
        let inst = bad.instructions.iter_mut().find(|i| i.id == id).unwrap();
        let crate::ssa::model::Operation::BorrowCall {
            arguments, source, ..
        } = &mut inst.operation
        else {
            panic!("actual BorrowCall")
        };
        assert_ne!(*source, other);
        assert_ne!(arguments[0], replacement);
        *source = other;
        arguments[0] = replacement;
        assert!(r.validate(&bad).is_err());
    }
}

fn rejected_and_recovered(fault: Fault) {
    let (result, receipts, trace) = lower(Some(fault));
    assert!(result.is_err());
    assert!(receipts.is_empty());
    assert!(trace.rolled_back);
    assert_eq!(trace.pending_after, 0);
    assert_eq!(trace.before_verifier, 0);
    let _ = capture();
}
#[test]
#[ignore = "CI-only actual break capture loss; local compile/list only"]
fn counter_break_capture_loss_revokes_and_recovers() {
    require_ci();
    rejected_and_recovered(Fault::CounterBreakCaptureMissing);
}
#[test]
#[ignore = "CI-only selected counter break receipt loss; local compile/list only"]
fn counter_break_receipt_loss_revokes_and_recovers() {
    require_ci();
    rejected_and_recovered(Fault::CounterBreakReceiptMissing);
}
#[cfg(feature = "loan-observation-test-support")]
#[test]
#[ignore = "CI-only verifier failure revokes both receipts; local compile/list only"]
fn counter_break_final_verifier_revokes_and_recovers() {
    require_ci();
    let ((result, receipts, trace), verified) =
        crate::ssa::unit_lower::observe_final_verifier(|| lower(Some(Fault::FinalVerifier)));
    assert_eq!(verified.len(), 1);
    assert!(verified[0].contains("MissingTerminator"));
    assert!(result.is_err());
    assert!(receipts.is_empty());
    assert_eq!(trace.recorded, 2);
    assert_eq!(trace.before_verifier, 2);
    assert!(trace.rolled_back);
    assert_eq!(trace.pending_after, 0);
    let _ = capture();
}
