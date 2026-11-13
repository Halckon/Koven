//! Counter-specific CI faults operate on real emitted functions and detached receipts.
use super::publication::{Fault, Trace, require_ci, with_fault};
use super::*;
use crate::ssa::{
    model::{ComparisonOperator, Operation, Program, ScalarConstant, TerminatorKind},
    unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
fn lower(fault: Option<Fault>) -> (Result<Program, LoweringError>, Vec<Receipt>, Trace) {
    let body = "(source:String,rounds:Int,choose:Boolean):Unit {var remaining=rounds;while(remaining>0){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b);consume(b)};remaining=remaining-1}}";
    let text = format!(
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun first{body}\nfun second{body}\nfun run(source:String,rounds:Int,choose:Boolean):Unit {{first(source,rounds,choose);second(source,rounds,choose)}}"
    );
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "counter-ci.ko", &text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    assert_eq!(owned.ownership().no_carry_counter_branches().len(), 2);
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
        assert!(r.plan.counter.is_some());
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
fn ordinary_counter_receipts_publish_and_replay() {
    let _ = capture();
}
#[test]
#[ignore = "CI-only counter guard direction; local compile/list only"]
fn counter_guard_direction_rejects() {
    require_ci();
    let (mut f, r) = capture();
    let h = r.evidence.blocks[&r.plan.blocks.unwrap()[1]];
    let id = f.block(h).unwrap().instructions[1];
    let Operation::Compare { operator, .. } = &mut f.instructions[id.index()].operation else {
        panic!("actual compare")
    };
    assert_eq!(*operator, ComparisonOperator::GreaterThan);
    *operator = ComparisonOperator::LessThan;
    assert!(r.validate(&f).is_err());
}
#[test]
#[ignore = "CI-only counter decrement constant; local compile/list only"]
fn counter_decrement_amount_rejects() {
    require_ci();
    let (f, r) = capture();
    let b = r.evidence.blocks[&r.plan.join.unwrap()];
    let id = f.block(b).unwrap().instructions[0];
    assert_eq!(
        f.instructions[id.index()].operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );
    for amount in [0, 2] {
        let mut bad = f.clone();
        bad.instructions[id.index()].operation =
            Operation::Constant(ScalarConstant::Integer(amount));
        assert!(r.validate(&bad).is_err());
    }
}
#[test]
#[ignore = "CI-only success alias and backedge old-counter substitution; local compile/list only"]
fn counter_success_alias_and_backedge_reject() {
    require_ci();
    let (f, r) = capture();
    let c = r.plan.counter.as_ref().unwrap();
    let update = c.update.unwrap();
    let mut bad = r.clone();
    let alias = bad
        .evidence
        .aliases
        .get_mut(&(update.block, update.index))
        .unwrap();
    assert_ne!(alias.0, c.init.unwrap().output);
    alias.0 = c.init.unwrap().output;
    assert!(bad.validate(&f).is_err());
    let header = r.plan.blocks.unwrap()[1];
    let success = r.evidence.blocks[&update.block];
    let position = r.evidence.parameters[&header]
        .iter()
        .position(|s| *s == update.output)
        .unwrap();
    let mut bad = f.clone();
    let TerminatorKind::Branch(edge) = &mut bad.blocks[success.index()]
        .terminator
        .as_mut()
        .unwrap()
        .kind
    else {
        panic!("actual backedge")
    };
    let wrong = r.evidence.entity(header, update.output, r.span).unwrap();
    assert_ne!(edge.arguments[position], wrong);
    edge.arguments[position] = wrong;
    assert!(r.validate(&bad).is_err());
}
#[test]
#[ignore = "CI-only checked failure polarity; local compile/list only"]
fn counter_failure_edge_polarity_rejects() {
    require_ci();
    let (mut f, r) = capture();
    let join = r.evidence.blocks[&r.plan.join.unwrap()];
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &mut f.blocks[join.index()].terminator.as_mut().unwrap().kind
    else {
        panic!("actual checked branch")
    };
    assert_ne!(when_true.target, when_false.target);
    std::mem::swap(when_true, when_false);
    assert!(r.validate(&f).is_err());
}
#[test]
#[ignore = "CI-only independent counter selection loss at real finish; local compile/list only"]
fn counter_required_selection_loss_revokes_and_recovers() {
    require_ci();
    let (result, receipts, trace) = lower(Some(Fault::CounterReceiptMissing));
    assert!(result.is_err());
    assert!(receipts.is_empty());
    assert_eq!(trace.before_verifier, 0);
    assert!(trace.rolled_back);
    assert_eq!(trace.pending_after, 0);
    let (f, mut r) = capture();
    r.evidence.counter_selected.clear();
    assert!(r.validate(&f).is_err());
}
#[cfg(feature = "loan-observation-test-support")]
#[test]
#[ignore = "CI-only actual verifier failure revokes counter receipts; local compile/list only"]
fn counter_final_verifier_failure_revokes_and_recovers() {
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
