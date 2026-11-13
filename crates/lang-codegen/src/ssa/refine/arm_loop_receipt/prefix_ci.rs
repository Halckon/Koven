//! Real scalar-prefix capture; fault execution remains CI-only.
use super::publication::{Fault, Trace, require_ci, with_fault};
use super::*;
use crate::ssa::{
    model::{Operation, Program, ScalarConstant, TerminatorKind},
    unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
fn lower(fault: Option<Fault>) -> (Result<Program, LoweringError>, Vec<Receipt>, Trace) {
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun tick(own value:Int):Unit {}\nfun run(choose:Boolean,source:String,own flag:Boolean):Unit {if(choose){tick(1)}else{tick(2)};while(flag){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b);consume(b)};break};consume(source)}";
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "prefix-ci.ko", text);
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    assert_eq!(owned.ownership().no_carry_break_prefixes().len(), 1);
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
    let (program, receipts, trace) = lower(None);
    let program = program.unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(trace.before_verifier, 1);
    assert_eq!(trace.pending_after, 0);
    let r = receipts[0].clone();
    let f = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.id == r.caller)
        .unwrap()
        .clone();
    r.validate(&f).unwrap();
    (f, r)
}
#[test]
fn ordinary_prefix_receipt_replays() {
    let _ = capture();
}
#[test]
#[ignore = "CI-only scalar prefix target substitution; local compile/list only"]
fn prefix_bypass_rejects() {
    require_ci();
    let (mut f, r) = capture();
    let prefix = r.plan.prefix.as_ref().unwrap();
    let split = prefix.split.unwrap();
    let block = r.evidence.blocks[&split[0]];
    let entry = r.evidence.blocks[&prefix.join.unwrap()];
    let TerminatorKind::Conditional { when_true, .. } =
        &mut f.blocks[block.index()].terminator.as_mut().unwrap().kind
    else {
        panic!("prefix split")
    };
    when_true.target = entry;
    assert!(r.validate(&f).is_err());
    let _ = capture();
}
#[test]
#[ignore = "CI-only scalar prefix literal substitution; local compile/list only"]
fn prefix_literal_rejects() {
    require_ci();
    let (mut f, r) = capture();
    let prefix = r.plan.prefix.as_ref().unwrap();
    let arm = prefix.split.unwrap()[1];
    let id = r.evidence.instructions[&(arm, 0)];
    f.instructions[id.index()].operation = Operation::Constant(ScalarConstant::Integer(99));
    assert!(r.validate(&f).is_err());
    let _ = capture();
}
#[test]
#[ignore = "CI-only prefix receipt deletion; local compile/list only"]
fn prefix_receipt_missing_rolls_back() {
    require_ci();
    let (result, receipts, trace) = lower(Some(Fault::PrefixReceiptMissing));
    assert!(result.is_err());
    assert!(receipts.is_empty());
    assert_eq!(trace.pending_after, 0);
    assert!(trace.rolled_back);
    let _ = capture();
}

#[test]
#[ignore = "CI-only prefix instruction owner corruption; local compile/list only"]
fn prefix_instruction_owner_rejects() {
    require_ci();
    let (f, r) = capture();
    let prefix = r.plan.prefix.as_ref().unwrap();
    let split = prefix.split.unwrap();
    for position in [(split[0], 0), (split[1], 0), (split[1], 1)] {
        let mut bad = f.clone();
        let id = r.evidence.instructions[&position];
        bad.instructions[id.index()].block = r.evidence.blocks[&split[2]];
        assert!(r.validate(&bad).is_err());
    }
    let _ = capture();
}

#[test]
#[ignore = "CI-only prefix actual condition capture loss; local compile/list only"]
fn prefix_capture_missing_rolls_back() {
    require_ci();
    let (result, receipts, trace) = lower(Some(Fault::PrefixCaptureMissing));
    assert!(result.is_err());
    assert!(receipts.is_empty());
    assert_eq!(trace.pending_after, 0);
    assert!(trace.rolled_back);
    let _ = capture();
}
