//! Real production receipts; ignored faults are never executed locally.
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
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun first(source:String,flag:Boolean,choose:Boolean):Unit {while(flag){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b);consume(b)}}}\nfun second(source:String,flag:Boolean,choose:Boolean):Unit {while(flag){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b)}}}\nfun run(source:String,flag:Boolean,choose:Boolean):Unit {first(source,flag,choose);second(source,flag,choose)}";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "arm-ci.ko", text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
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
        Some(fault) => with_fault(fault, execute),
        None => observe(execute),
    }
}
fn capture() -> (Function, Receipt) {
    let (p, receipts, trace) = lower(None);
    let p = p.unwrap();
    assert_eq!(receipts.len(), 2);
    assert_eq!(trace.recorded, 2);
    assert_eq!(trace.before_verifier, 2);
    assert!(!trace.rolled_back);
    assert_eq!(trace.pending_after, 0);
    let r = receipts[0].clone();
    let f = p
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.id == r.caller)
        .unwrap()
        .clone();
    r.validate(&f).unwrap();
    (f, r)
}

fn arms(r: &Receipt) -> [crate::ssa::model::BlockId; 2] {
    let [_, a, b] = r.plan.split.unwrap();
    [r.evidence.blocks[&a], r.evidence.blocks[&b]]
}
#[test]
fn ordinary_arm_receipts_publish_and_replay_after_verifier() {
    let (f, r) = capture();
    r.validate(&f).unwrap();
}
#[test]
#[ignore = "CI-only each actual End absence and cross-arm identity; compile/list locally"]
fn arm_ends_missing_or_exchanged_reject() {
    require_ci();
    let (f, r) = capture();
    let blocks = arms(&r);
    let ids: Vec<_> = blocks
        .iter()
        .map(|b| *f.block(*b).unwrap().instructions.last().unwrap())
        .collect();
    for id in &ids {
        let mut bad = f.clone();
        let block = bad.instructions[id.index()].block;
        bad.blocks[block.index()].instructions.retain(|i| i != id);
        assert!(r.validate(&bad).is_err());
    }
    let mut bad = f.clone();
    let a = bad.instructions[ids[0].index()].operation.clone();
    let b = bad.instructions[ids[1].index()].operation.clone();
    bad.instructions[ids[0].index()].operation = b;
    bad.instructions[ids[1].index()].operation = a;
    assert!(r.validate(&bad).is_err());
}
#[test]
#[ignore = "CI-only final CFG wrong join predecessor; compile/list locally"]
fn wrong_merge_predecessor_rejects() {
    require_ci();
    let (mut f, r) = capture();
    let [a, b] = arms(&r);
    let TerminatorKind::Branch(edge) = &mut f.blocks[a.index()].terminator.as_mut().unwrap().kind
    else {
        panic!("real arm branch")
    };
    edge.target = b;
    assert!(r.validate(&f).is_err());
}
#[test]
#[ignore = "CI-only arm loan carried into merge or backedge; compile/list locally"]
fn arm_loan_carried_on_merge_or_backedge_rejects() {
    require_ci();
    let (f, r) = capture();
    let [a, _] = arms(&r);
    let fresh = f
        .instruction(f.block(a).unwrap().instructions[0])
        .unwrap()
        .results[0];
    for block in [a, r.evidence.blocks[&r.plan.join.unwrap()]] {
        let mut bad = f.clone();
        let TerminatorKind::Branch(edge) =
            &mut bad.blocks[block.index()].terminator.as_mut().unwrap().kind
        else {
            panic!("real merge/backedge")
        };
        edge.arguments.push(fresh);
        assert!(r.validate(&bad).is_err());
    }
}

#[test]
#[ignore = "CI-only actual receipt absence before finish; compile/list locally"]
fn missing_receipt_fails_closed_and_recovers() {
    require_ci();
    let (result, receipts, trace) = lower(Some(Fault::ReceiptMissing));
    assert!(result.is_err());
    assert!(receipts.is_empty());
    assert_eq!(trace.before_verifier, 0);
    assert!(trace.rolled_back);
    assert_eq!(trace.pending_after, 0);
    let _ = capture();
}
#[cfg(feature = "loan-observation-test-support")]
#[test]
#[ignore = "CI-only real final verifier rollback of two pending receipts; compile/list locally"]
fn final_verifier_error_revokes_both_receipts_and_recovers() {
    require_ci();
    let ((result, receipts, trace), verified) =
        crate::ssa::unit_lower::observe_final_verifier(|| lower(Some(Fault::FinalVerifier)));
    assert_eq!(verified.len(), 1);
    let section = verified[0]
        .strip_prefix("SECTION final.verify_result bytes=")
        .unwrap()
        .split_once('\n')
        .unwrap()
        .1
        .split_once("\nEND_SECTION final.verify_result\n")
        .unwrap()
        .0
        .trim();
    assert!(section.starts_with("Err("));
    assert!(section.contains("MissingTerminator"));
    assert!(result.is_err());
    assert_eq!(trace.recorded, 2);
    assert_eq!(trace.before_verifier, 2);
    assert!(trace.rolled_back);
    assert_eq!(trace.pending_after, 0);
    assert!(receipts.is_empty());
    let _ = capture();
}
