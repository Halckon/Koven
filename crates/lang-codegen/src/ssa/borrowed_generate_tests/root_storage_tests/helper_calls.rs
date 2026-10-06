//! A callee's entry Borrow contract must be established by its caller's current contents.
use super::*;

#[test]
fn borrowed_generation_helper_checks_current_root_capture_at_call_boundary() {
    let (mut valid, _) = replacement(false);
    forward_generation_to_helper(&mut valid);
    verify_program(&valid.program).expect("live capture can cross a synchronous helper Borrow");
    crate::llvm::render_verified_program(&valid.program)
        .expect("direct helper Borrow LLVM verifies");

    let (mut invalid, capture) = replacement(true);
    forward_generation_to_helper(&mut invalid);
    assert_inactive(&invalid, capture);
}

#[test]
fn borrowed_generation_pointer_helper_checks_current_root_capture_at_call_boundary() {
    let (mut valid, _) = replacement(false);
    forward_generation_to_helper(&mut valid);
    invoke_helper_pointer(&mut valid);
    verify_program(&valid.program).expect("live capture can cross an indirect helper Borrow");
    crate::llvm::render_verified_program(&valid.program)
        .expect("indirect helper Borrow LLVM verifies");

    let (mut invalid, capture) = replacement(true);
    forward_generation_to_helper(&mut invalid);
    invoke_helper_pointer(&mut invalid);
    assert_inactive(&invalid, capture);
}

/// The same source ABI can be invoked through an owning function address.
fn invoke_helper_pointer(fixture: &mut Fixture) {
    let module = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module");
    let caller = module.function(fixture.make).expect("caller");
    let instruction = &caller.instructions[fixture.generate.index()];
    let Operation::DirectCall {
        callee, arguments, ..
    } = &instruction.operation
    else {
        panic!("helper call");
    };
    let callee = *callee;
    let arguments = arguments.clone();
    let parameters = arguments
        .iter()
        .map(|entity| caller.entity(*entity).expect("argument").ty)
        .collect();
    let EntityType::Value(result_type) = caller.entity(instruction.results[0]).expect("result").ty
    else {
        panic!("container result");
    };
    let origin = instruction.origin.clone();
    let pointer = module
        .add_function_pointer_type_with_parameters(parameters, vec![result_type])
        .expect("pointer ABI");
    let caller = module.function_mut(fixture.make).expect("caller");
    let block = caller.instructions[fixture.generate.index()].block;
    let terminator = caller.blocks[block.index()].terminator.take();
    let original = caller.blocks[block.index()].instructions.clone();
    let (address_instruction, address) = caller
        .append_instruction(
            block,
            Operation::FunctionAddress { target: callee },
            vec![EntityType::Value(pointer)],
            origin.clone(),
        )
        .expect("address");
    let address = value(address[0]);
    let (drop, _) = caller
        .append_instruction(block, Operation::Drop { owner: address }, vec![], origin)
        .expect("address drop");
    let position = original
        .iter()
        .position(|id| *id == fixture.generate)
        .expect("call position");
    let mut ordered = original[..position].to_vec();
    ordered.push(address_instruction);
    ordered.push(fixture.generate);
    ordered.push(drop);
    ordered.extend_from_slice(&original[position + 1..]);
    caller.blocks[block.index()].instructions = ordered;
    caller.blocks[block.index()].terminator = terminator;
    caller.instructions[fixture.generate.index()].operation = Operation::CallableInvoke {
        callable: address,
        arguments,
    };
}

/// Preserve the caller's exact ownership flow while moving only generation into a callee.
fn forward_generation_to_helper(fixture: &mut Fixture) {
    let module = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module");
    let caller = module.function(fixture.make).expect("caller");
    let instruction = &caller.instructions[fixture.generate.index()];
    let Operation::ContainerGenerateBorrowed {
        container,
        length,
        initializer,
    } = instruction.operation
    else {
        panic!("fixture generation");
    };
    let length_type = caller.entity(EntityId::Value(length)).expect("length").ty;
    let initializer_type = caller
        .entity(EntityId::Loan(initializer))
        .expect("initializer")
        .ty;
    let origin = instruction.origin.clone();
    let helper = module
        .add_function("borrowed_helper", vec![container], origin.clone())
        .expect("helper");
    let callee = module.function_mut(helper).expect("callee");
    let entry = callee
        .add_block(vec![length_type, initializer_type], origin.clone())
        .expect("entry");
    let parameters = callee.blocks[entry.index()].parameters.clone();
    let result = value(append(
        callee,
        entry,
        Operation::ContainerGenerateBorrowed {
            container,
            length: value(parameters[0]),
            initializer: loan(parameters[1]),
        },
        EntityType::Value(container),
        &origin,
    ));
    callee
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![result],
            },
            origin,
        )
        .expect("return");
    module
        .function_mut(fixture.make)
        .expect("caller")
        .instructions[fixture.generate.index()]
    .operation = Operation::DirectCall {
        callee: helper,
        receiver: None,
        arguments: vec![EntityId::Value(length), EntityId::Loan(initializer)],
    };
}
