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

fn capture_flags(pattern: &[usize]) -> (Function, Receipt) {
    let loops = pattern
        .iter()
        .enumerate()
        .map(|(index, flag)| {
            format!("while(f{flag}){{borrow val item{index}=view(source);consume(item{index})}}")
        })
        .collect::<Vec<_>>()
        .join(";");
    let text = format!(
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {{}}\nfun run(source:String,f0:Boolean,f1:Boolean):Unit {{{loops}}}"
    );
    let (program, receipt) = capture_program_text(&text);
    assert_eq!(receipt.loops.len(), pattern.len());
    let function = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.id == receipt.caller)
        .unwrap()
        .clone();
    receipt.validate(&function).unwrap();
    (function, receipt)
}

fn actual_roles(function: &Function) -> BTreeMap<EntityId, usize> {
    let mut roles = function.blocks[0]
        .parameters
        .iter()
        .copied()
        .enumerate()
        .map(|(index, entity)| (entity, index))
        .collect::<BTreeMap<_, _>>();
    loop {
        let count = roles.len();
        for block in &function.blocks {
            let edges = match &block.terminator.as_ref().unwrap().kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                TerminatorKind::Return { .. } => Vec::new(),
                _ => panic!("fixture terminator"),
            };
            for edge in edges {
                for (argument, parameter) in edge
                    .arguments
                    .iter()
                    .zip(&function.block(edge.target).unwrap().parameters)
                {
                    if let Some(role) = roles.get(argument).copied()
                        && let Some(previous) = roles.insert(*parameter, role)
                    {
                        assert_eq!(previous, role);
                    }
                }
            }
        }
        if roles.len() == count {
            return roles;
        }
    }
}

fn actual_local(
    function: &Function,
    roles: &BTreeMap<EntityId, usize>,
    block: crate::ssa::model::BlockId,
    role: usize,
) -> EntityId {
    let values = function
        .block(block)
        .unwrap()
        .parameters
        .iter()
        .copied()
        .filter(|entity| roles.get(entity) == Some(&role))
        .collect::<Vec<_>>();
    let [value] = values.as_slice() else {
        panic!("fixture local role")
    };
    *value
}

fn synchronize_edges(receipt: &mut Receipt, function: &Function) {
    receipt.edges = function
        .blocks
        .iter()
        .flat_map(|block| match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(edge) => vec![(block.id, edge.clone())],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![
                (block.id, when_true.clone()),
                (block.id, when_false.clone()),
            ],
            TerminatorKind::Return { .. } => Vec::new(),
            _ => panic!("fixture terminator"),
        })
        .collect();
}

#[test]
#[ignore = "CI-only actual distinct-flag read and ABI identity; compile locally"]
fn distinct_flag_read_is_not_interchangeable_with_same_typed_loan() {
    let (function, receipt) = capture_flags(&[0, 1, 0]);
    let roles = actual_roles(&function);
    let condition = tagged(&receipt, Tag::Condition);
    let header = function.instruction(condition).unwrap().block;
    let correct = actual_local(&function, &roles, header, 1);
    let other = actual_local(&function, &roles, header, 2);
    assert_ne!(correct, other);
    let EntityId::Loan(other) = other else {
        panic!()
    };
    let mut bad = function.clone();
    let Operation::Read { source } = &mut bad.instructions[condition.index()].operation else {
        panic!()
    };
    *source = crate::ssa::model::PlaceAccess::Loan(other);
    assert!(receipt.validate(&bad).is_err());
    let entry_flag = function
        .block(function.entry_block().unwrap())
        .unwrap()
        .parameters[1];
    assert_ne!(entry_flag, correct);
    let EntityId::Loan(entry_flag) = entry_flag else {
        panic!()
    };
    let mut bad = function.clone();
    let Operation::Read { source } = &mut bad.instructions[condition.index()].operation else {
        panic!()
    };
    *source = crate::ssa::model::PlaceAccess::Loan(entry_flag);
    assert!(
        receipt.validate(&bad).is_err(),
        "same-role entry loan is not the local header loan"
    );
    let mut bad = receipt.clone();
    bad.loops[0].proof.flag_parameter = 2;
    assert!(bad.validate(&function).is_err());
    let mut bad = receipt.clone();
    bad.abi_types.swap(0, 1);
    assert!(bad.validate(&function).is_err());
}

#[test]
#[ignore = "CI-only actual flag transport substitutions with synchronized snapshots; compile locally"]
fn distinct_flag_edges_preserve_repeated_future_uses() {
    let (function, receipt) = capture_flags(&[0, 1, 0]);
    let roles = actual_roles(&function);
    for peer in [0, 1] {
        let acquire = receipt.loops[peer]
            .instructions
            .iter()
            .find(|(tag, _)| *tag == Tag::Result)
            .unwrap()
            .1;
        let body = function.instruction(acquire).unwrap().block;
        let first = actual_local(&function, &roles, body, 1);
        let second = actual_local(&function, &roles, body, 2);
        assert_ne!(first, second);
        let mut bad = function.clone();
        let TerminatorKind::Branch(edge) =
            &mut bad.blocks[body.index()].terminator.as_mut().unwrap().kind
        else {
            panic!()
        };
        let a = edge
            .arguments
            .iter()
            .position(|entity| *entity == first)
            .unwrap();
        let b = edge
            .arguments
            .iter()
            .position(|entity| *entity == second)
            .unwrap();
        if peer == 0 {
            edge.arguments.swap(a, b);
        } else {
            // f0 is not the current condition, but the next peer still needs its exact lease.
            edge.arguments[a] = second;
        }
        let mut changed = receipt.clone();
        synchronize_edges(&mut changed, &bad);
        assert!(changed.validate(&bad).is_err(), "flag edge peer {peer}");
    }
}

#[test]
#[ignore = "CI-only structurally complete extra dead-flag transport; compile locally"]
fn flags_dead_after_last_use_are_not_retained_by_the_certificate() {
    use crate::ssa::model::{Definition, LoanId};
    let (function, receipt) = capture_flags(&[0, 0, 1]);
    let roles = actual_roles(&function);
    let original = receipt.loops[2].proof.blocks;
    let suffix = [original.entry, original.header, original.body]
        .map(|raw| receipt.blocks[&receipt.canonical(raw)]);
    assert_eq!(suffix.into_iter().collect::<BTreeSet<_>>().len(), 3);
    for block in suffix {
        assert!(
            function
                .block(block)
                .unwrap()
                .parameters
                .iter()
                .all(|entity| roles.get(entity) != Some(&1))
        );
    }
    let mut bad = function.clone();
    let mut added = BTreeMap::new();
    let template = function.entity(function.blocks[0].parameters[1]).unwrap();
    for block in suffix {
        let index = bad.blocks[block.index()].parameters.len();
        let loan = LoanId {
            function: bad.id(),
            index: bad.loans.len(),
        };
        let mut data = template.clone();
        data.definition = Definition::BlockParameter { block, index };
        bad.loans.push(data);
        let entity = EntityId::Loan(loan);
        bad.blocks[block.index()].parameters.push(entity);
        added.insert(block, entity);
    }
    for block in &mut bad.blocks {
        let edges = match &mut block.terminator.as_mut().unwrap().kind {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            TerminatorKind::Return { .. } => Vec::new(),
            _ => panic!("fixture terminator"),
        };
        for edge in edges {
            if added.contains_key(&edge.target) {
                let extra = added
                    .get(&block.id)
                    .copied()
                    .unwrap_or_else(|| actual_local(&function, &roles, block.id, 1));
                edge.arguments.push(extra);
            }
        }
    }
    let mut changed = receipt.clone();
    synchronize_edges(&mut changed, &bad);
    // Entity definitions and every edge arity are complete; exact role liveness rejects this.
    let covered = changed.blocks.values().copied().collect::<BTreeSet<_>>();
    let instructions = changed
        .loops
        .iter()
        .flat_map(|peer| peer.instructions.iter().map(|(_, id)| *id))
        .collect::<BTreeSet<_>>();
    super::whole_function::check(&bad, &covered, &instructions, changed.span).unwrap();
    assert!(changed.validate(&bad).is_err());
}

#[test]
#[ignore = "CI-only unused ABI flag identity and synchronized future transport faults; compile locally"]
fn unused_abi_flag_dies_at_entry_and_cannot_replace_future_flags() {
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(source:String,f0:Boolean,f1:Boolean,unused:Boolean):Unit {while(f0){borrow val a=view(source);consume(a)};while(f1){borrow val b=view(source);consume(b)};while(f0){borrow val c=view(source);consume(c)}}";
    let (program, receipt) = capture_program_text(text);
    let function = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.id == receipt.caller)
        .unwrap();
    assert_eq!(receipt.loops.len(), 3);
    receipt.validate(function).unwrap();
    let entry = function.entry_block().unwrap();
    let roles = actual_roles(function);
    let unused = actual_local(function, &roles, entry, 3);
    let current = actual_local(function, &roles, entry, 1);
    let future = actual_local(function, &roles, entry, 2);
    assert_ne!(unused, current);
    assert_ne!(unused, future);
    for block in &function.blocks {
        if block.id != entry {
            assert!(
                block
                    .parameters
                    .iter()
                    .all(|parameter| roles.get(parameter) != Some(&3))
            );
        }
    }

    let condition = tagged(&receipt, Tag::Condition);
    let header = function.instruction(condition).unwrap().block;
    assert_ne!(header, entry);
    let EntityId::Loan(unused_loan) = unused else {
        panic!()
    };
    let mut bad = function.clone();
    let Operation::Read { source } = &mut bad.instructions[condition.index()].operation else {
        panic!()
    };
    *source = crate::ssa::model::PlaceAccess::Loan(unused_loan);
    assert!(receipt.validate(&bad).is_err());

    let mut changed = receipt.clone();
    changed.loops[0].proof.flag_parameter = 3;
    assert!(changed.validate(function).is_err());

    for replacement in [unused, current] {
        let mut bad = function.clone();
        let TerminatorKind::Branch(edge) =
            &mut bad.blocks[entry.index()].terminator.as_mut().unwrap().kind
        else {
            panic!()
        };
        assert_eq!(edge.target, header);
        let future_index = edge
            .arguments
            .iter()
            .position(|argument| *argument == future)
            .unwrap();
        edge.arguments[future_index] = replacement;
        let mut changed = receipt.clone();
        synchronize_edges(&mut changed, &bad);
        let covered = changed.blocks.values().copied().collect::<BTreeSet<_>>();
        let instructions = changed
            .loops
            .iter()
            .flat_map(|peer| peer.instructions.iter().map(|(_, id)| *id))
            .collect::<BTreeSet<_>>();
        // The actual graph remains complete and snapshots agree; exact roles reject it.
        super::whole_function::check(&bad, &covered, &instructions, changed.span).unwrap();
        assert!(changed.validate(&bad).is_err());
    }
}

#[test]
#[ignore = "CI-only own Boolean condition identity, producer seal and ABI modes; compile locally"]
fn own_boolean_conditions_require_the_exact_local_value_and_mode() {
    use lang_frontend::type_checking::ParameterMode;
    let text = "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun run(source:String,own f0:Boolean,own f1:Boolean,unused:Boolean):Unit {while(f0){borrow val a=view(source);consume(a)};while(f1){borrow val b=view(source);consume(b)};while(f0){borrow val c=view(source);consume(c)}}";
    let (program, receipt) = capture_program_text(text);
    let function = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.id == receipt.caller)
        .unwrap();
    receipt.validate(function).unwrap();
    assert_eq!(receipt.loops.len(), 3);
    for peer in &receipt.loops {
        assert!(peer.proof.flag_owned);
        assert!(peer.proof.condition_used);
        assert!(
            peer.instructions
                .iter()
                .all(|(tag, _)| *tag != Tag::Condition)
        );
    }
    let roles = actual_roles(function);
    let entry = function.entry_block().unwrap();
    let header = receipt.blocks[&receipt.canonical(receipt.loops[0].proof.blocks.header)];
    let correct = actual_local(function, &roles, header, 1);
    let wrong = actual_local(function, &roles, header, 2);
    let old_entry = actual_local(function, &roles, entry, 1);
    assert_ne!(correct, wrong);
    assert_ne!(correct, old_entry);
    for replacement in [wrong, old_entry] {
        let EntityId::Value(value) = replacement else {
            panic!("own Boolean parameter must be a Value")
        };
        let mut bad = function.clone();
        let TerminatorKind::Conditional { condition, .. } =
            &mut bad.blocks[header.index()].terminator.as_mut().unwrap().kind
        else {
            panic!()
        };
        *condition = value;
        assert!(receipt.validate(&bad).is_err());
    }
    let mut changed = receipt.clone();
    changed.loops[0].proof.condition_used = false;
    assert!(changed.validate(function).is_err());
    let mut changed = receipt.clone();
    changed.loops[0].proof.flag_owned = false;
    assert!(changed.validate(function).is_err());
    let mut changed = receipt.clone();
    changed.abi_modes[1] = ParameterMode::Borrow;
    changed.loops[0].proof.flag_owned = false;
    changed.loops[2].proof.flag_owned = false;
    assert!(changed.validate(function).is_err());
    let mut changed = receipt.clone();
    changed.abi_modes[0] = ParameterMode::Value;
    assert!(changed.validate(function).is_err());

    let body = receipt.blocks[&receipt.canonical(receipt.loops[1].proof.blocks.body)];
    let repeated = actual_local(function, &roles, body, 1);
    let current = actual_local(function, &roles, body, 2);
    let mut bad = function.clone();
    let TerminatorKind::Branch(edge) =
        &mut bad.blocks[body.index()].terminator.as_mut().unwrap().kind
    else {
        panic!()
    };
    let position = edge
        .arguments
        .iter()
        .position(|arg| *arg == repeated)
        .unwrap();
    edge.arguments[position] = current;
    let mut changed = receipt.clone();
    synchronize_edges(&mut changed, &bad);
    assert!(changed.validate(&bad).is_err());
}
