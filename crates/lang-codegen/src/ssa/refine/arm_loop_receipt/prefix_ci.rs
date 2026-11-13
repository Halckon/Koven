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
    lower_shape(fault, false)
}
fn lower_shape(
    fault: Option<Fault>,
    independent: bool,
) -> (Result<Program, LoweringError>, Vec<Receipt>, Trace) {
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun tick(own value:Int):Unit {}\nfun run(choose:Boolean,source:String,own flag:Boolean):Unit {if(choose){tick(1)}else{tick(2)};while(flag){if(choose){borrow val a=view(source);consume(a)}else{borrow val b=view(source);consume(b);consume(b)};break};consume(source)}";
    let text = if independent {
        text.replace(
            "choose:Boolean,source:String,own flag:Boolean",
            "flag:Boolean,before:Boolean,source:String,choose:Boolean",
        )
        .replacen("if(choose){tick", "if(before){tick", 1)
    } else {
        text.to_owned()
    };
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "prefix-ci.ko", &text);
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
    capture_shape(false)
}
fn capture_shape(independent: bool) -> (Function, Receipt) {
    let (program, receipts, trace) = lower_shape(None, independent);
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

#[test]
fn ordinary_independent_prefix_receipt_replays() {
    let _ = capture_shape(true);
}
#[test]
#[ignore = "CI-only independent prefix source-slot deletion; local compile/list only"]
fn independent_prefix_root_slot_loss_rejects() {
    require_ci();
    let (mut f, mut r) = capture_shape(true);
    let [head, a, _] = r.plan.prefix.as_ref().unwrap().split.unwrap();
    let slot = r.parameters[r.plan.roles[0]];
    let pos = r.evidence.parameters[&a]
        .iter()
        .position(|x| *x == slot)
        .unwrap();
    r.evidence.parameters.get_mut(&a).unwrap().remove(pos);
    r.evidence.entities.get_mut(&a).unwrap().remove(pos);
    let block = r.evidence.blocks[&a];
    f.blocks[block.index()].parameters.remove(pos);
    let h = r.evidence.blocks[&head];
    let TerminatorKind::Conditional { when_true, .. } =
        &mut f.blocks[h.index()].terminator.as_mut().unwrap().kind
    else {
        panic!("prefix split")
    };
    when_true.arguments.remove(pos);
    assert!(r.validate(&f).is_err());
    let _ = capture_shape(true);
}
#[test]
#[ignore = "CI-only independent prefix Boolean identity exchange; local compile/list only"]
fn independent_prefix_bool_identity_swap_rejects() {
    require_ci();
    let (mut f, r) = capture_shape(true);
    let [head, a, _] = r.plan.prefix.as_ref().unwrap().split.unwrap();
    let slots = &r.evidence.parameters[&a];
    let guard = slots
        .iter()
        .position(|x| *x == r.parameters[r.plan.roles[1]])
        .unwrap();
    let arm = slots
        .iter()
        .position(|x| *x == r.parameters[r.plan.roles[2]])
        .unwrap();
    let h = r.evidence.blocks[&head];
    let TerminatorKind::Conditional { when_true, .. } =
        &mut f.blocks[h.index()].terminator.as_mut().unwrap().kind
    else {
        panic!("prefix split")
    };
    let ids = [when_true.arguments[guard], when_true.arguments[arm]];
    when_true.arguments.swap(guard, arm);
    assert_ne!(ids[0], ids[1]);
    assert_eq!(f.entity(ids[0]).unwrap().ty, f.entity(ids[1]).unwrap().ty);
    assert!(r.validate(&f).is_err());
    let _ = capture_shape(true);
}
#[test]
#[ignore = "CI-only independent prefix extra predecessor; local compile/list only"]
fn independent_prefix_extra_predecessor_rejects() {
    require_ci();
    let (mut f, r) = capture_shape(true);
    let [head, _, _] = r.plan.prefix.as_ref().unwrap().split.unwrap();
    let h = r.evidence.blocks[&head];
    let TerminatorKind::Conditional { when_true, .. } =
        &f.blocks[h.index()].terminator.as_ref().unwrap().kind
    else {
        panic!("prefix split")
    };
    let edge = when_true.clone();
    let exit = r.evidence.blocks[&r.plan.blocks.unwrap()[4]];
    f.blocks[exit.index()].terminator.as_mut().unwrap().kind = TerminatorKind::Branch(edge);
    assert!(r.validate(&f).is_err());
    let _ = capture_shape(true);
}
