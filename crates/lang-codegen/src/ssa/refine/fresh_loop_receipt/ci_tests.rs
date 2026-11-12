//! CI-only mutations of real fresh-body SSA; compiled locally, never executed locally.
use super::*;
use crate::ssa::{
    model::{EntityId, Function, InstructionId, Operation, TerminatorKind},
    unit_lower_test_support,
};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};
fn capture_program() -> (crate::ssa::model::Program, Receipt) {
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(source:String,flag:Boolean):Unit {while(flag){borrow val item=view(source);consume(item);consume(item)}}";
    capture_program_text(text)
}
fn capture_program_text(text: &str) -> (crate::ssa::model::Program, Receipt) {
    let mut sources = SourceMap::new();
    let (source, parsed) = unit_lower_test_support::parsed(&mut sources, "fresh-ci.ko", text);
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
    let (result, receipts) = observe(|| {
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    });
    let (program, _) = result.unwrap();
    assert_eq!(receipts.len(), 1);
    let receipt = receipts.into_iter().next().unwrap();
    let function = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.id == receipt.caller)
        .unwrap()
        .clone();
    receipt.validate(&function).unwrap();
    (program, receipt)
}
fn capture() -> (Function, Receipt) {
    let (program, receipt) = capture_program();
    let function = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.id == receipt.caller)
        .unwrap()
        .clone();
    (function, receipt)
}
fn capture_sequence() -> (Function, Receipt) {
    let loops = (0..3).map(|index| format!("while(flag){{borrow val item{index}=view(source);consume(item{index});consume(item{index})}}")).collect::<Vec<_>>().join(";");
    let text = format!(
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run(source:String,flag:Boolean):Unit {{{loops}}}"
    );
    let (program, receipt) = capture_program_text(&text);
    assert_eq!(receipt.loops.len(), 3);
    let function = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.id == receipt.caller)
        .unwrap()
        .clone();
    (function, receipt)
}
fn tagged(receipt: &Receipt, tag: Tag) -> InstructionId {
    receipt.loops[0]
        .instructions
        .iter()
        .find(|(t, _)| *t == tag)
        .unwrap()
        .1
}
#[test]
#[ignore = "CI-only actual SSA fault injection; compile locally"]
fn fresh_actual_acquisition_and_consumer_identity_reject() {
    let (function, receipt) = capture();
    let acquire = tagged(&receipt, Tag::Result);
    let consumer = tagged(&receipt, Tag::Consumer(0));
    let Operation::BorrowCall { source, .. } = function.instruction(acquire).unwrap().operation
    else {
        panic!()
    };
    let entry_source = function.blocks[0].parameters[receipt.loops[0].proof.source_parameter];
    for fault in 0..4 {
        let mut bad = function.clone();
        match fault {
            0 => {
                let Operation::BorrowCall { callee, .. } =
                    &mut bad.instructions[acquire.index()].operation
                else {
                    panic!()
                };
                *callee = function.id();
            }
            1 => {
                let Operation::BorrowCall {
                    source, arguments, ..
                } = &mut bad.instructions[acquire.index()].operation
                else {
                    panic!()
                };
                let EntityId::Loan(entry) = entry_source else {
                    panic!()
                };
                *source = entry;
                arguments[0] = entry_source;
            }
            2 => {
                let Operation::DirectCall { arguments, .. } =
                    &mut bad.instructions[consumer.index()].operation
                else {
                    panic!()
                };
                arguments[0] = EntityId::Loan(source);
            }
            _ => {
                bad.instructions[acquire.index()].operation = Operation::BorrowEnd { loan: source };
            }
        }
        assert!(receipt.validate(&bad).is_err(), "actual fault {fault}");
    }
    let (foreign, _) = capture();
    assert!(receipt.validate(&foreign).is_err());
    let mut missing = receipt.clone();
    missing.loops[0]
        .instructions
        .retain(|(t, _)| *t != Tag::Result);
    assert!(missing.validate(&function).is_err());
}
#[test]
#[ignore = "CI-only actual SSA fault injection; compile locally"]
fn fresh_end_must_be_unique_and_after_every_consumer() {
    let (function, receipt) = capture();
    let end = tagged(&receipt, Tag::ResultEnd);
    let acquire = tagged(&receipt, Tag::Result);
    let body = function.instruction(end).unwrap().block;
    let mut missing = receipt.clone();
    missing.loops[0]
        .instructions
        .retain(|(t, _)| *t != Tag::ResultEnd);
    assert!(missing.validate(&function).is_err());
    let mut duplicate = receipt.clone();
    duplicate.loops[0].instructions.push((Tag::ResultEnd, end));
    assert!(duplicate.validate(&function).is_err());
    let mut early = function.clone();
    let ids = &mut early.blocks[body.index()].instructions;
    let a = ids.iter().position(|id| *id == acquire).unwrap();
    let e = ids.iter().position(|id| *id == end).unwrap();
    ids.swap(a, e);
    assert!(receipt.validate(&early).is_err());
}
#[test]
#[ignore = "CI-only actual SSA fault injection; compile locally"]
fn fresh_result_cannot_cross_backedge_even_with_changed_snapshot() {
    let (function, receipt) = capture();
    let acquire = tagged(&receipt, Tag::Result);
    let instruction = function.instruction(acquire).unwrap();
    let result = instruction.results[0];
    let body = instruction.block;
    let Operation::BorrowCall { source, .. } = instruction.operation else {
        panic!()
    };
    let mut bad = function.clone();
    let TerminatorKind::Branch(edge) =
        &mut bad.blocks[body.index()].terminator.as_mut().unwrap().kind
    else {
        panic!()
    };
    let index = edge
        .arguments
        .iter()
        .position(|id| *id == EntityId::Loan(source))
        .unwrap();
    edge.arguments[index] = result;
    let changed = edge.clone();
    let mut r = receipt.clone();
    r.edges
        .iter_mut()
        .find(|(from, _)| *from == body)
        .unwrap()
        .1 = changed;
    assert!(r.validate(&bad).is_err());
}
#[test]
#[ignore = "CI-only actual SSA fault injection; compile locally"]
fn fresh_false_path_and_fold_authority_cannot_be_forged() {
    let (function, receipt) = capture();
    let acquire = tagged(&receipt, Tag::Result);
    let body = function.instruction(acquire).unwrap().block;
    let header = receipt.blocks[&receipt.canonical(receipt.loops[0].proof.blocks.header)];
    let mut bad = function.clone();
    let TerminatorKind::Conditional { when_false, .. } =
        &mut bad.blocks[header.index()].terminator.as_mut().unwrap().kind
    else {
        panic!()
    };
    when_false.target = body;
    let changed = when_false.clone();
    let mut r = receipt.clone();
    let original_false = receipt.blocks[&receipt.canonical(receipt.loops[0].proof.blocks.finished)];
    r.edges
        .iter_mut()
        .find(|(from, e)| *from == header && e.target == original_false)
        .unwrap()
        .1 = changed;
    assert!(r.validate(&bad).is_err());
    let mut r = receipt.clone();
    r.folds
        .push((r.loops[0].proof.blocks.header, r.loops[0].proof.blocks.body));
    assert!(r.validate(&function).is_err());
    let mut r = receipt.clone();
    r.loops[0].proof.edges.pop();
    assert!(r.validate(&function).is_err());
}
#[test]
#[ignore = "CI-only publication guard rollback after real verifier error; compile locally"]
fn fresh_pending_receipt_guard_rolls_back_after_verifier_error() {
    let (mut program, receipt) = capture_program();
    crate::ssa::verify::verify_program(&program).unwrap();
    let (_, receipts) = observe(|| {
        let _guard = UnitPublication::begin();
        let caller = receipt.caller;
        record(receipt);
        let function = program
            .modules
            .iter_mut()
            .flat_map(|m| &mut m.functions)
            .find(|f| f.id == caller)
            .unwrap();
        function.blocks[0].terminator = None;
        assert!(crate::ssa::verify::verify_program(&program).is_err());
    });
    assert!(receipts.is_empty());
}

#[test]
#[ignore = "CI-only same-source reanalysis identity substitution; compile locally"]
fn fresh_fact_from_another_analysis_is_not_the_producer_fact() {
    let (function, mut receipt) = capture();
    let (_, other) = capture();
    assert_eq!(receipt.loops[0].fact, other.loops[0].fact);
    assert!(!receipt.loops[0].fact.same_identity(&other.loops[0].fact));
    receipt.loops[0].fact = other.loops[0].fact.clone();
    assert!(receipt.validate(&function).is_err());
}

#[test]
#[ignore = "CI-only actual sequence proof mutations; compile locally"]
fn sequence_peer_identity_order_and_fold_evidence_are_independent() {
    let (function, receipt) = capture_sequence();
    receipt.validate(&function).unwrap();
    for fault in 0..5 {
        let mut bad = receipt.clone();
        match fault {
            0 => bad.loops.swap(0, 1),
            1 => bad.loops[1].fact = bad.loops[0].fact.clone(),
            2 => bad.loops[1]
                .instructions
                .retain(|(tag, _)| *tag != Tag::ResultEnd),
            3 => {
                assert!(!bad.folds.is_empty());
                bad.folds.remove(0);
            }
            _ => bad.loops.push(bad.loops[0].clone()),
        }
        assert!(bad.validate(&function).is_err(), "sequence proof {fault}");
    }
}

#[test]
#[ignore = "CI-only actual cross-body operand and false-link mutations; compile locally"]
fn sequence_fresh_results_and_false_links_do_not_cross_peers() {
    let (function, receipt) = capture_sequence();
    let first_result_id = receipt.loops[0]
        .instructions
        .iter()
        .find(|(tag, _)| *tag == Tag::Result)
        .unwrap()
        .1;
    let first_result = function.instruction(first_result_id).unwrap().results[0];
    let second_consumer = receipt.loops[1]
        .instructions
        .iter()
        .find(|(tag, _)| *tag == Tag::Consumer(0))
        .unwrap()
        .1;
    let mut bad = function.clone();
    let Operation::DirectCall { arguments, .. } =
        &mut bad.instructions[second_consumer.index()].operation
    else {
        panic!()
    };
    arguments[0] = first_result;
    assert!(receipt.validate(&bad).is_err());

    let header = receipt.blocks[&receipt.canonical(receipt.loops[0].proof.blocks.header)];
    let wrong_target = receipt.blocks[&receipt.canonical(receipt.loops[1].proof.blocks.body)];
    let mut bad = function.clone();
    let TerminatorKind::Conditional { when_false, .. } =
        &mut bad.blocks[header.index()].terminator.as_mut().unwrap().kind
    else {
        panic!()
    };
    let original = when_false.target;
    when_false.target = wrong_target;
    let changed = when_false.clone();
    let mut proof = receipt.clone();
    proof
        .edges
        .iter_mut()
        .find(|(from, edge)| *from == header && edge.target == original)
        .unwrap()
        .1 = changed;
    assert!(proof.validate(&bad).is_err());
}
