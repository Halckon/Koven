//! Tail-break-specific CI faults operate on real emitted functions and detached receipts.
use super::publication::{Fault, Trace, require_ci, with_fault};
use super::*;
use crate::ssa::{
    model::{Edge, Operation, Program, TerminatorKind},
    unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
fn lower(fault: Option<Fault>) -> (Result<Program, LoweringError>, Vec<Receipt>, Trace) {
    let body = "(source:String,flag:Boolean,choose:Boolean):Unit {while(flag){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b);consume(b)};break};consume(source)}";
    let text = format!(
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun first{body}\nfun second{body}\nfun run(source:String,flag:Boolean,choose:Boolean):Unit {{first(source,flag,choose);second(source,flag,choose)}}"
    );
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "break-ci.ko", &text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    assert_eq!(owned.ownership().no_carry_break_branches().len(), 2);
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
        assert!(r.plan.tail_break.is_some());
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
fn ordinary_break_receipts_publish_and_replay() {
    let _ = capture();
}
#[test]
#[ignore = "CI-only actual break target substitution; local compile/list only"]
fn break_wrong_exit_rejects() {
    require_ci();
    let (f, r) = capture();
    let bs = r.plan.blocks.unwrap();
    let join = r.evidence.blocks[&r.plan.join.unwrap()];
    for target in [bs[1], bs[2]] {
        let mut bad = f.clone();
        let TerminatorKind::Branch(edge) =
            &mut bad.blocks[join.index()].terminator.as_mut().unwrap().kind
        else {
            panic!("actual break")
        };
        let wrong = r.evidence.blocks[&target];
        assert_ne!(edge.target, wrong);
        edge.target = wrong;
        assert!(r.validate(&bad).is_err());
    }
}
#[test]
#[ignore = "CI-only each arm actual End omission; local compile/list only"]
fn break_each_arm_cleanup_missing_rejects() {
    require_ci();
    let (f, r) = capture();
    let [_, a, b] = r.plan.split.unwrap();
    for arm in [a, b] {
        let mut bad = f.clone();
        let block = r.evidence.blocks[&arm];
        let id = *bad.blocks[block.index()].instructions.last().unwrap();
        assert!(matches!(
            bad.instructions[id.index()].operation,
            Operation::BorrowEnd { .. }
        ));
        bad.blocks[block.index()].instructions.pop();
        assert!(r.validate(&bad).is_err());
    }
}
#[test]
#[ignore = "CI-only fresh loan escapes actual break edge; local compile/list only"]
fn break_fresh_loan_on_exit_rejects() {
    require_ci();
    let (mut f, r) = capture();
    let arm = r.evidence.blocks[&r.plan.split.unwrap()[1]];
    let fresh = f.instructions[f.blocks[arm.index()].instructions[0].index()].results[0];
    assert!(matches!(fresh, EntityId::Loan(_)));
    let join = r.evidence.blocks[&r.plan.join.unwrap()];
    let TerminatorKind::Branch(edge) =
        &mut f.blocks[join.index()].terminator.as_mut().unwrap().kind
    else {
        panic!("actual break")
    };
    assert!(!edge.arguments.contains(&fresh));
    edge.arguments.push(fresh);
    assert!(r.validate(&f).is_err());
}
#[test]
#[ignore = "CI-only hidden reentry after actual exit; local compile/list only"]
fn break_exit_hidden_reentry_rejects() {
    require_ci();
    let (f, r) = capture();
    let bs = r.plan.blocks.unwrap();
    let entry = r.evidence.blocks[&bs[0]];
    let exit = r.evidence.blocks[&bs[4]];
    assert_ne!(entry, exit);
    for hops in [0, 1, 2] {
        let mut bad = f.clone();
        let mut next = entry;
        for _ in 0..hops {
            // Preserve the recorded slice and make real extra CFG blocks, so
            // only reachability from the exit exposes this hidden reentry.
            let id = BlockId {
                function: bad.id,
                index: bad.blocks.len(),
            };
            let mut hop = bad.blocks[exit.index()].clone();
            hop.id = id;
            hop.parameters.clear();
            hop.instructions.clear();
            let term = hop.terminator.as_mut().unwrap();
            term.abort = None;
            term.kind = TerminatorKind::Branch(Edge {
                target: next,
                arguments: if next == entry {
                    bad.blocks[entry.index()].parameters.clone()
                } else {
                    Vec::new()
                },
            });
            bad.blocks.push(hop);
            next = id;
        }
        let arguments = if next == entry {
            bad.blocks[entry.index()].parameters.clone()
        } else {
            Vec::new()
        };
        bad.blocks[exit.index()].terminator.as_mut().unwrap().kind = TerminatorKind::Branch(Edge {
            target: next,
            arguments,
        });
        assert_eq!(bad.blocks.len(), f.blocks.len() + hops);
        assert!(r.validate(&bad).is_err());
    }
}
#[test]
#[ignore = "CI-only actual break capture loss; local compile/list only"]
fn break_actual_capture_loss_revokes_and_recovers() {
    require_ci();
    let (result, receipts, trace) = lower(Some(Fault::BreakCaptureMissing));
    assert!(result.is_err());
    assert!(receipts.is_empty());
    assert_eq!(trace.before_verifier, 0);
    assert!(trace.rolled_back);
    assert_eq!(trace.pending_after, 0);
    let _ = capture();
}
#[test]
#[ignore = "CI-only selected break receipt loss at finish; local compile/list only"]
fn break_selected_receipt_loss_revokes_and_recovers() {
    require_ci();
    let (result, receipts, trace) = lower(Some(Fault::BreakReceiptMissing));
    assert!(result.is_err());
    assert!(receipts.is_empty());
    assert_eq!(trace.before_verifier, 0);
    assert!(trace.rolled_back);
    assert_eq!(trace.pending_after, 0);
    let (f, mut r) = capture();
    r.plan.tail_break.as_mut().unwrap().observed = None;
    assert!(r.validate(&f).is_err());
}
#[cfg(feature = "loan-observation-test-support")]
#[test]
#[ignore = "CI-only verifier error revokes two break receipts; local compile/list only"]
fn break_final_verifier_failure_revokes_and_recovers() {
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
