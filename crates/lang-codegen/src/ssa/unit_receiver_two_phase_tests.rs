use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{EntityId, LoanKind, Operation},
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn receiver_two_phase_activates_after_nested_readonly_call() {
    for kind in ["class", "value class"] {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(
            &mut sources,
            "p/main.ko",
            &format!(
                "package p\n{kind} Cell(var item: Int) {{\n\
                 fun read(): Int = item\n\
                 inout fun set(prefix: Int, own next: Int): Unit {{ item = next }}\n\
                 }}\n\
                 fun entry(): Int {{ var cell = Cell(41)\n\
                 val ignored = cell.set(99, cell.read())\nreturn cell.read() }}"
            ),
        );
        let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
        let (names_env, types_env) = standard_environments();
        let (names, typed, owned) = analyze(&sources, &inputs, &names_env, &types_env);
        for fact in owned.ownership().receiver_facts() {
            assert_eq!(
                fact.activation_point(),
                fact.is_receiver_reservation().then_some(fact.call())
            );
        }
        let (mut program, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types_env,
            &typed,
            &owned,
            declaration(&names, "p", "entry"),
        )
        .expect("nested readonly receiver argument must lower");
        let module = &program.modules[0];
        let entry = module
            .functions
            .iter()
            .find(|function| function.name.contains(".entry."))
            .unwrap();
        let reader = module
            .functions
            .iter()
            .find(|function| function.name.contains(".read."))
            .unwrap();
        let setter = module
            .functions
            .iter()
            .find(|function| function.name.contains(".set."))
            .unwrap();
        let read = entry
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.operation,
            Operation::DirectCall { callee, .. } if callee == reader.id())
            })
            .unwrap();
        let (call, receiver) = entry
            .instructions
            .iter()
            .enumerate()
            .find_map(|(index, instruction)| match instruction.operation {
                Operation::DirectCall {
                    callee,
                    receiver: Some(EntityId::Loan(loan)),
                    ..
                } if callee == setter.id() => Some((index, loan)),
                _ => None,
            })
            .unwrap();
        let activation = entry
            .instructions
            .iter()
            .position(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::BorrowBegin {
                        kind: LoanKind::Exclusive,
                        ..
                    }
                ) && instruction.results == [EntityId::Loan(receiver)]
            })
            .unwrap();
        assert!(read < activation && activation < call, "{kind}");
        let Operation::DirectCall { arguments, .. } = &entry.instructions[call].operation else {
            unreachable!();
        };
        let EntityId::Loan(argument) = arguments[0] else {
            unreachable!();
        };
        assert!(entry.instructions[call + 1..].iter().any(|instruction|
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == argument)),
            "the target callee's Borrow argument must remain active through the call");
        let read_receiver = match entry.instructions[read].operation {
            Operation::DirectCall {
                receiver: Some(EntityId::Loan(loan)),
                ..
            } => loan,
            _ => unreachable!(),
        };
        assert!(entry.instructions[read + 1..activation].iter().any(|instruction|
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == read_receiver)));
        crate::llvm::render_verified_program(&program).expect("two-phase receiver LLVM");

        // 仍活跃的 shared loan 不得靠延迟独占建立绕过 verifier。
        let entry = program.modules[0]
            .functions
            .iter_mut()
            .find(|function| function.name.contains(".entry."))
            .unwrap();
        let end_id = entry.instructions.iter().find(|instruction|
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == read_receiver))
            .unwrap().id;
        let call_id = entry.instructions[call].id;
        let block = &mut entry.blocks[0];
        let old_end = block
            .instructions
            .iter()
            .position(|id| *id == end_id)
            .unwrap();
        block.instructions.remove(old_end);
        let before_call = block
            .instructions
            .iter()
            .position(|id| *id == call_id)
            .unwrap();
        block.instructions.insert(before_call, end_id);
        assert!(
            super::verify::verify_program(&program)
                .unwrap_err()
                .errors
                .iter()
                .any(|error| matches!(
                    error.kind,
                    super::verify::VerifyErrorKind::BorrowConflict { .. }
                ))
        );
    }
}

#[test]
fn receiver_two_phase_nested_borrow_this_ends_before_exclusive_delivery() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\nclass Cell(var item: Int) {\n\
         fun read(): Int = item\n\
         inout fun set(own next: Int): Unit { item = next }\n\
         inout fun relay(): Unit { val done = set(inspect((this))) }\n}\n\
         fun inspect(cell: Cell): Int = cell.read()\n\
         fun entry(): Unit { val cell = Cell(1)\nval done = cell.relay() }",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (names_env, types_env) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &names_env, &types_env);
    let (mut program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types_env,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("Borrow this through nested synchronous call");
    crate::llvm::render_verified_program(&program).expect("Borrow this LLVM");
    let relay = program.modules[0]
        .functions
        .iter_mut()
        .find(|function| function.name.contains(".relay."))
        .unwrap();
    let EntityId::Loan(parent) = relay.blocks[0].parameters[0] else {
        unreachable!();
    };
    let (reborrow, child) = relay.instructions.iter().enumerate().find_map(|(index, instruction)| {
        if matches!(instruction.operation, Operation::SharedReborrow { source } if source == parent)
            && let [EntityId::Loan(child)] = instruction.results.as_slice() {
            Some((index, *child))
        } else { None }
    }).unwrap();
    let inspect = relay
        .instructions
        .iter()
        .position(|instruction| {
            matches!(&instruction.operation, Operation::DirectCall { receiver: None, arguments, .. }
            if arguments == &[EntityId::Loan(child)])
        })
        .unwrap();
    let end = relay.instructions.iter().position(|instruction|
        matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == child)).unwrap();
    let setter = relay.instructions.iter().position(|instruction|
        matches!(instruction.operation, Operation::DirectCall { receiver: Some(EntityId::Loan(loan)), .. }
            if loan == parent)).unwrap();
    assert!(reborrow < inspect && inspect < end && end < setter);
    assert!(!relay.instructions.iter().any(|instruction|
        matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == parent)));

    // 子借用若被错误延长到目标调用之后，原 exclusive capability 不得交付。
    let end_id = relay.instructions[end].id;
    let setter_id = relay.instructions[setter].id;
    let block = &mut relay.blocks[0];
    let old = block
        .instructions
        .iter()
        .position(|id| *id == end_id)
        .unwrap();
    block.instructions.remove(old);
    let after = block
        .instructions
        .iter()
        .position(|id| *id == setter_id)
        .unwrap()
        + 1;
    block.instructions.insert(after, end_id);
    assert!(
        super::verify::verify_program(&program)
            .unwrap_err()
            .errors
            .iter()
            .any(|error| matches!(
                error.kind,
                super::verify::VerifyErrorKind::LoanDependencyActive { .. }
            ))
    );
}
