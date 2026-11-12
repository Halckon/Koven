//! CI-only self-contained certificate mutations; no Plan/current replay oracle.
use super::*;
fn tagged(s: &Snapshot, tag: Tag) -> InstructionId {
    s.receipt
        .instructions
        .iter()
        .find(|(t, _)| *t == tag)
        .unwrap()
        .1
}
#[test]
#[ignore = "CI-only fault injection; compile locally"]
fn selfcontained_actual_edge_same_type_roles_cannot_swap() {
    let s = capture();
    s.receipt.validate(&s.function).unwrap();
    let mut bad = s.function.clone();
    let source = s
        .function
        .instruction(tagged(&s, Tag::Source))
        .unwrap()
        .results[0];
    let result = s
        .function
        .instruction(tagged(&s, Tag::Result))
        .unwrap()
        .results[0];
    let entry = bad.entry_block().unwrap();
    let TerminatorKind::Branch(edge) =
        &mut bad.blocks[entry.index()].terminator.as_mut().unwrap().kind
    else {
        panic!("entry Goto")
    };
    let a = edge.arguments.iter().position(|v| *v == source).unwrap();
    let b = edge.arguments.iter().position(|v| *v == result).unwrap();
    assert_eq!(
        s.function.entity(source).unwrap().ty,
        s.function.entity(result).unwrap().ty
    );
    edge.arguments.swap(a, b);
    let changed = edge.clone();
    let mut receipt = s.receipt.clone();
    let snapshot = receipt
        .edges
        .iter_mut()
        .find(|(from, e)| *from == entry && e.target == changed.target)
        .unwrap();
    snapshot.1 = changed;
    assert!(receipt.validate(&bad).is_err());
}
#[test]
#[ignore = "CI-only fault injection; compile locally"]
fn selfcontained_consumer_and_cleanup_require_actual_result_role() {
    let s = capture();
    s.receipt.validate(&s.function).unwrap();
    let proof = &s.receipt.proof;
    let resolve = |b| s.receipt.blocks[&proof.canonical(b)];
    let source = s
        .function
        .instruction(tagged(&s, Tag::Source))
        .unwrap()
        .results[0];
    let entry = s.function.block(resolve(proof.blocks.entry)).unwrap();
    let TerminatorKind::Branch(to_header) = &entry.terminator.as_ref().unwrap().kind else {
        panic!()
    };
    let index = to_header
        .arguments
        .iter()
        .position(|e| *e == source)
        .unwrap();
    let header = s.function.block(to_header.target).unwrap();
    let header_source = header.parameters[index];
    let TerminatorKind::Conditional { when_true, .. } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!()
    };
    let body_index = when_true
        .arguments
        .iter()
        .position(|e| *e == header_source)
        .unwrap();
    let body_source = s.function.block(when_true.target).unwrap().parameters[body_index];
    let mut bad = s.function.clone();
    let call = tagged(&s, Tag::Consumer(0));
    let Operation::DirectCall { arguments, .. } = &mut bad.instructions[call.index()].operation
    else {
        panic!()
    };
    assert_eq!(
        s.function.entity(arguments[0]).unwrap().ty,
        s.function.entity(body_source).unwrap().ty
    );
    arguments[0] = body_source;
    assert!(s.receipt.validate(&bad).is_err());
    let mut bad = s.function.clone();
    let a = tagged(&s, Tag::ResultEnd);
    let b = tagged(&s, Tag::SourceEnd);
    let left = bad.instructions[a.index()].operation.clone();
    let right = bad.instructions[b.index()].operation.clone();
    bad.instructions[a.index()].operation = right;
    bad.instructions[b.index()].operation = left;
    assert!(s.receipt.validate(&bad).is_err());
}
#[test]
#[ignore = "CI-only fault injection; compile locally"]
fn selfcontained_provenance_metadata_and_foreign_function_rejected() {
    let s = capture();
    s.receipt.validate(&s.function).unwrap();
    let mut receipt = s.receipt.clone();
    receipt.proof.owner_alias = None;
    assert!(receipt.validate(&s.function).is_err());
    let mut receipt = s.receipt.clone();
    receipt.proof.folds = vec![(receipt.proof.blocks.header, receipt.proof.blocks.body)];
    assert!(receipt.validate(&s.function).is_err());
    let mut function = s.function.clone();
    function.loans.push(function.loans[0].clone());
    assert!(s.receipt.validate(&function).is_err());
    let foreign = capture();
    assert!(s.receipt.validate(&foreign.function).is_err());
}

#[test]
#[ignore = "CI-only fault injection; compile locally"]
fn selfcontained_entry_seeds_cannot_bypass_body_parameters() {
    let s = capture();
    s.receipt.validate(&s.function).unwrap();
    let original_result = s
        .function
        .instruction(tagged(&s, Tag::Result))
        .unwrap()
        .results[0];
    let mut bad = s.function.clone();
    let call = tagged(&s, Tag::Consumer(0));
    let Operation::DirectCall { arguments, .. } = &mut bad.instructions[call.index()].operation
    else {
        panic!()
    };
    assert_ne!(arguments[0], original_result);
    arguments[0] = original_result;
    assert!(s.receipt.validate(&bad).is_err());
    for role in [Tag::Source, Tag::Result] {
        let seed = s.function.instruction(tagged(&s, role)).unwrap().results[0];
        let entry = s.function.entry_block().unwrap();
        let TerminatorKind::Branch(entry_edge) = &s
            .function
            .block(entry)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind
        else {
            panic!()
        };
        let position = entry_edge
            .arguments
            .iter()
            .position(|e| *e == seed)
            .unwrap();
        let body = s.receipt.blocks[&s.receipt.proof.canonical(s.receipt.proof.blocks.body)];
        let mut bad = s.function.clone();
        let TerminatorKind::Branch(backedge) =
            &mut bad.blocks[body.index()].terminator.as_mut().unwrap().kind
        else {
            panic!()
        };
        assert_ne!(backedge.arguments[position], seed);
        backedge.arguments[position] = seed;
        let changed = backedge.clone();
        let target = changed.target;
        let mut receipt = s.receipt.clone();
        receipt
            .edges
            .iter_mut()
            .find(|(from, e)| *from == body && e.target == target)
            .unwrap()
            .1 = changed;
        assert!(receipt.validate(&bad).is_err());
    }
}
