use super::*;

#[path = "root_storage_tests/helper_calls.rs"]
mod helper_calls;

#[test]
fn borrowed_generation_checks_current_root_capture_after_replacement() {
    let (valid, _) = replacement(false);
    verify_program(&valid.program).expect("current shared capture stays active through generation");

    let (invalid, capture) = replacement(true);
    let errors = verify_program(&invalid.program)
        .expect_err("ending the replacement capture cannot authorize a later borrowed generation");
    assert!(errors.errors.iter().any(|error| {
        matches!(error.kind, super::super::verify::VerifyErrorKind::LoanInactive { loan } if loan == capture)
    }));
}

fn replacement(end_before_generation: bool) -> (Fixture, LoanId) {
    let mut fixture = fixture(Layout::Shared, 3, false);
    let function = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module")
        .function_mut(fixture.make)
        .expect("make");
    let block = function.blocks[0].id;
    let terminator = function.blocks[0].terminator.take().expect("return");
    let original = function.blocks[0].instructions.clone();
    let initializer_position = original
        .iter()
        .position(|id| {
            function.instructions[id.index].results == [EntityId::Loan(fixture.initializer)]
        })
        .expect("initializer loan");
    let Operation::BorrowBegin { place, .. } =
        function.instructions[original[initializer_position].index].operation
    else {
        panic!("initializer borrows the callback root");
    };
    let EntityType::Value(callable) = function
        .entity(EntityId::Value(fixture.callback))
        .expect("callback")
        .ty
    else {
        panic!("callable value");
    };
    let closure = function
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::ClosureConstruct { thunk, .. } => Some(thunk),
            _ => None,
        })
        .expect("thunk");
    let source_instruction = function.instructions[original[0].index].clone();
    let source_type = function
        .entity(source_instruction.results[0])
        .expect("Int length")
        .ty;
    let EntityType::Value(int) = source_type else {
        panic!("Int value");
    };
    let origin = source_instruction.origin;
    let insertion_start = function.blocks[0].instructions.len();
    let source = value(append(
        function,
        block,
        Operation::Constant(ScalarConstant::Integer(9)),
        source_type,
        &origin,
    ));
    let capture = borrow(function, block, source, int, &origin);
    let replacement = value(append(
        function,
        block,
        Operation::ClosureConstruct {
            closure: callable,
            thunk: closure,
            captures: vec![ClosureCaptureOperand::Shared(capture)],
        },
        EntityType::Value(callable),
        &origin,
    ));
    function
        .append_instruction(
            block,
            Operation::Mutate {
                place,
                value: replacement,
            },
            vec![],
            origin.clone(),
        )
        .expect("replace current callback contents");
    let insertion_end = function.blocks[0].instructions.len();
    let (end, _) = function
        .append_instruction(
            block,
            Operation::BorrowEnd { loan: capture },
            vec![],
            origin,
        )
        .expect("capture end");
    let inserted = function.blocks[0].instructions[insertion_start..insertion_end].to_vec();
    let mut ordered = original[..initializer_position].to_vec();
    ordered.extend(inserted);
    if end_before_generation {
        ordered.push(end);
    }
    ordered.extend_from_slice(&original[initializer_position..]);
    if !end_before_generation {
        ordered.push(end);
    }
    function.blocks[0].instructions = ordered;
    function.blocks[0].terminator = Some(terminator);
    (fixture, capture)
}

fn function(fixture: &mut Fixture) -> &mut Function {
    fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module")
        .function_mut(fixture.make)
        .expect("make")
}

fn assert_inactive(fixture: &Fixture, capture: LoanId) {
    let errors =
        verify_program(&fixture.program).expect_err("current storage capture must remain active");
    assert!(
        errors.errors.iter().any(|error| matches!(error.kind,
        super::super::verify::VerifyErrorKind::LoanInactive { loan } if loan == capture)),
        "{errors:?}"
    );
}

fn callback_parts(
    function: &Function,
    callback: ValueId,
    initializer: LoanId,
) -> (SsaTypeId, SsaTypeId, FunctionId, PlaceId) {
    let callable = function.values[callback.index()].ty.semantic_type();
    let thunk = function
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::ClosureConstruct { thunk, .. } => Some(thunk),
            _ => None,
        })
        .expect("thunk");
    let integer = function.values[0].ty.semantic_type();
    let Definition::InstructionResult { instruction, .. } =
        function.loans[initializer.index()].definition
    else {
        panic!("initializer")
    };
    let Operation::BorrowBegin { place, .. } = function.instructions[instruction.index()].operation
    else {
        panic!("borrow")
    };
    (callable, integer, thunk, place)
}

fn another_callback(
    function: &mut Function,
    block: BlockId,
    callable: SsaTypeId,
    integer: SsaTypeId,
    thunk: FunctionId,
) -> (ValueId, LoanId) {
    let origin = function.origin.clone();
    let source = value(append(
        function,
        block,
        Operation::Constant(ScalarConstant::Integer(11)),
        EntityType::Value(integer),
        &origin,
    ));
    let capture = borrow(function, block, source, integer, &origin);
    let callback = value(append(
        function,
        block,
        Operation::ClosureConstruct {
            closure: callable,
            thunk,
            captures: vec![ClosureCaptureOperand::Shared(capture)],
        },
        EntityType::Value(callable),
        &origin,
    ));
    (callback, capture)
}

fn end(function: &mut Function, block: BlockId, capture: LoanId) -> InstructionId {
    function
        .append_instruction(
            block,
            Operation::BorrowEnd { loan: capture },
            vec![],
            function.origin.clone(),
        )
        .expect("end")
        .0
}

#[test]
fn borrowed_generation_sequential_overwrite_forgets_ended_previous_capture() {
    let (valid, _) = sequential(false);
    verify_program(&valid.program).expect("ended overwritten callback is no longer read");
    let (invalid, capture) = sequential(true);
    assert_inactive(&invalid, capture);
}

fn sequential(end_current: bool) -> (Fixture, LoanId) {
    let (mut fixture, previous) = replacement(false);
    let callback = fixture.callback;
    let initializer = fixture.initializer;
    let function = function(&mut fixture);
    let block = function.blocks[0].id;
    let terminator = function.blocks[0].terminator.take();
    let original = function.blocks[0].instructions.clone();
    let (callable, integer, thunk, place) = callback_parts(function, callback, initializer);
    let start = function.blocks[0].instructions.len();
    let (replacement, capture) = another_callback(function, block, callable, integer, thunk);
    function
        .append_instruction(
            block,
            Operation::Mutate {
                place,
                value: replacement,
            },
            vec![],
            function.origin.clone(),
        )
        .expect("second overwrite");
    let stop = function.blocks[0].instructions.len();
    let current_end = end(function, block, capture);
    let previous_end = *original.iter().find(|id| matches!(function.instructions[id.index()].operation, Operation::BorrowEnd { loan } if loan == previous)).expect("previous end");
    let loan_definition = match function.loans[initializer.index()].definition {
        Definition::InstructionResult { instruction, .. } => instruction,
        _ => panic!("loan"),
    };
    let position = original
        .iter()
        .position(|id| *id == loan_definition)
        .expect("initializer");
    let mut ordered = original[..position].to_vec();
    ordered.extend_from_slice(&function.blocks[0].instructions[start..stop]);
    ordered.push(previous_end);
    if end_current {
        ordered.push(current_end);
    }
    ordered.extend(
        original[position..]
            .iter()
            .filter(|id| **id != previous_end)
            .copied(),
    );
    if !end_current {
        ordered.push(current_end);
    }
    function.blocks[0].instructions = ordered;
    function.blocks[0].terminator = terminator;
    (fixture, capture)
}

#[test]
fn borrowed_generation_rebinds_current_replacement_capture_across_cfg() {
    let (valid, _) = replacement_successor(false);
    verify_program(&valid.program).expect("current capture transported to successor");
    let (invalid, capture) = replacement_successor(true);
    assert_inactive(&invalid, capture);
}

fn replacement_successor(end_current: bool) -> (Fixture, LoanId) {
    let (mut fixture, capture) = replacement(false);
    let original_callback = fixture.callback;
    let original_initializer = fixture.initializer;
    let generate = fixture.generate;
    let function = function(&mut fixture);
    let entry = function.blocks[0].id;
    let callback_ty = function.values[original_callback.index()].ty;
    let initializer_ty = function.loans[original_initializer.index()].ty;
    let capture_ty = function.loans[capture.index()].ty;
    let next = function
        .add_block(
            vec![callback_ty, initializer_ty, capture_ty],
            function.origin.clone(),
        )
        .expect("successor");
    let callback = value(function.blocks[next.index()].parameters[0]);
    let initializer = loan(function.blocks[next.index()].parameters[1]);
    let capture_parameter = loan(function.blocks[next.index()].parameters[2]);
    let position = function.blocks[0]
        .instructions
        .iter()
        .position(|id| *id == generate)
        .expect("generate");
    let mut tail = function.blocks[0].instructions.split_off(position);
    for id in &tail {
        let instruction = &mut function.instructions[id.index()];
        instruction.block = next;
        match &mut instruction.operation {
            Operation::ContainerGenerateBorrowed {
                initializer: operand,
                ..
            } => *operand = initializer,
            Operation::Drop { owner } => *owner = callback,
            Operation::BorrowEnd { loan } => {
                *loan = if *loan == original_initializer {
                    initializer
                } else {
                    capture_parameter
                }
            }
            _ => panic!("tail"),
        }
    }
    if end_current {
        let index = tail.iter().position(|id| matches!(function.instructions[id.index()].operation, Operation::BorrowEnd { loan } if loan == capture_parameter)).expect("capture end");
        let id = tail.remove(index);
        tail.insert(0, id);
    }
    function.blocks[next.index()].instructions = tail;
    function.blocks[next.index()].terminator = function.blocks[0].terminator.take();
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: next,
                arguments: vec![
                    EntityId::Value(original_callback),
                    EntityId::Loan(original_initializer),
                    EntityId::Loan(capture),
                ],
            }),
            function.origin.clone(),
        )
        .expect("branch");
    (fixture, capture_parameter)
}

#[derive(Clone, Copy, Debug)]
enum Exchange {
    Take,
    ReplaceNew,
    SwapFirst,
}

fn check_exchange(operation: Exchange) {
    let (valid, _) = exchange(operation, false);
    verify_program(&valid.program).unwrap_or_else(|error| panic!("{operation:?}: {error:?}"));
    let (invalid, capture) = exchange(operation, true);
    assert_inactive(&invalid, capture);
}

#[test]
fn borrowed_generation_reads_current_root_contents_after_take() {
    check_exchange(Exchange::Take);
}
fn check_exchange_boundary(operation: Exchange) {
    let (fixture, _) = exchange(operation, false);
    let function = fixture
        .program
        .module(fixture.make.module())
        .expect("module")
        .function(fixture.make)
        .expect("make");
    let exchange = function
        .instructions
        .iter()
        .find(|instruction| {
            matches!(
                instruction.operation,
                Operation::RootReplace { .. } | Operation::RootSwap { .. }
            )
        })
        .expect("exchange");
    let errors = verify_program(&fixture.program)
        .expect_err("direct concrete closure exchange remains unsupported");
    assert!(
        errors.errors.iter().any(|error| matches!(
            error.kind,
            super::super::verify::VerifyErrorKind::OperationContract { .. }
        ) && error.location
            == super::super::verify::VerifyLocation::Instruction(exchange.id)),
        "{errors:?}"
    );
}

#[test]
fn borrowed_generation_keeps_concrete_closure_root_replace_rejected() {
    check_exchange_boundary(Exchange::ReplaceNew);
}
#[test]
fn borrowed_generation_keeps_concrete_closure_root_swap_rejected() {
    check_exchange_boundary(Exchange::SwapFirst);
}

fn exchange(operation: Exchange, end_current: bool) -> (Fixture, LoanId) {
    let (mut fixture, stored_capture) = replacement(false);
    let original_callback = fixture.callback;
    let initializer = fixture.initializer;
    let original_capture = fixture.capture.expect("initial capture");
    let generate = fixture.generate;
    let function = function(&mut fixture);
    let block = function.blocks[0].id;
    let terminator = function.blocks[0].terminator.take();
    let original = function.blocks[0].instructions.clone();
    let (callable, integer, thunk, place) =
        callback_parts(function, original_callback, initializer);
    let definition = match function.loans[initializer.index()].definition {
        Definition::InstructionResult { instruction, .. } => instruction,
        _ => panic!("initializer"),
    };
    let position = original
        .iter()
        .position(|id| *id == definition)
        .expect("initializer position");
    let stored_end = *original.iter().find(|id| matches!(function.instructions[id.index()].operation, Operation::BorrowEnd { loan } if loan == stored_capture)).expect("stored capture end");
    let start = function.blocks[0].instructions.len();
    end(function, block, initializer);
    let mut extra_drop = None;
    let mut extra_capture = None;
    let selected = if matches!(operation, Exchange::Take) {
        value(append(
            function,
            block,
            Operation::RootPlaceTake {
                owner: original_callback,
                place,
            },
            EntityType::Value(callable),
            &function.origin.clone(),
        ))
    } else {
        let (other, other_capture) = another_callback(function, block, callable, integer, thunk);
        extra_capture = Some(other_capture);
        let exclusive = loan(append(
            function,
            block,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: callable,
            },
            &function.origin.clone(),
        ));
        let request = if matches!(operation, Exchange::ReplaceNew) {
            Operation::RootReplace {
                owner: original_callback,
                loan: exclusive,
                replacement: other,
            }
        } else {
            let EntityId::Place(other_place) = append(
                function,
                block,
                Operation::RootPlace { owner: other },
                EntityType::Place(callable),
                &function.origin.clone(),
            ) else {
                panic!("place")
            };
            let other_loan = loan(append(
                function,
                block,
                Operation::BorrowBegin {
                    place: other_place,
                    kind: LoanKind::Exclusive,
                },
                EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: callable,
                },
                &function.origin.clone(),
            ));
            Operation::RootSwap {
                owners: [original_callback, other],
                loans: [exclusive, other_loan],
            }
        };
        let results = function
            .append_instruction(
                block,
                request,
                vec![EntityType::Value(callable); 2],
                function.origin.clone(),
            )
            .expect("exchange")
            .1;
        let selected = 0;
        extra_drop = Some(value(results[1 - selected]));
        value(results[selected])
    };
    let selected_capture = if matches!(operation, Exchange::ReplaceNew | Exchange::SwapFirst) {
        extra_capture.expect("new capture")
    } else {
        stored_capture
    };
    let new_initializer = borrow(
        function,
        block,
        selected,
        callable,
        &function.origin.clone(),
    );
    let stop = function.blocks[0].instructions.len();
    let mut tail = original[position + 1..].to_vec();
    // Replace the old generation loan and final owner with their post-exchange identities.
    for id in &tail {
        match &mut function.instructions[id.index()].operation {
            Operation::ContainerGenerateBorrowed { initializer, .. } => {
                *initializer = new_initializer
            }
            Operation::BorrowEnd { loan } if *loan == initializer => *loan = new_initializer,
            Operation::Drop { owner } => *owner = selected,
            _ => {}
        }
    }
    let current_end = if selected_capture == stored_capture {
        stored_end
    } else {
        end(function, block, selected_capture)
    };
    tail.retain(|id| !matches!(function.instructions[id.index()].operation, Operation::BorrowEnd { loan } if loan == stored_capture));
    let mut ordered = original[..position + 1].to_vec();
    ordered.extend_from_slice(&function.blocks[0].instructions[start..stop]);
    if end_current {
        ordered.push(current_end);
    }
    ordered.extend(tail);
    if let Some(owner) = extra_drop {
        ordered.push(
            function
                .append_instruction(
                    block,
                    Operation::Drop { owner },
                    vec![],
                    function.origin.clone(),
                )
                .expect("drop extra")
                .0,
        );
    }
    ordered.push(end(function, block, original_capture));
    if !end_current {
        ordered.push(current_end);
    }
    if selected_capture != stored_capture {
        ordered.push(stored_end);
    }
    if let Some(capture) = extra_capture
        && capture != selected_capture
    {
        ordered.push(end(function, block, capture));
    }
    function.blocks[0].instructions = ordered;
    function.blocks[0].terminator = terminator;
    fixture.generate = generate;
    (fixture, selected_capture)
}

fn entry_parameter(function: &mut Function, ty: EntityType) -> EntityId {
    let block = function.blocks[0].id;
    let definition = Definition::BlockParameter {
        block,
        index: function.blocks[0].parameters.len(),
    };
    let data = EntityData {
        ty,
        definition,
        origin: function.origin.clone(),
    };
    let entity = match ty {
        EntityType::Value(_) => {
            let id = ValueId {
                function: function.id,
                index: function.values.len(),
            };
            function.values.push(data);
            EntityId::Value(id)
        }
        EntityType::Loan { .. } => {
            let id = LoanId {
                function: function.id,
                index: function.loans.len(),
            };
            function.loans.push(data);
            EntityId::Loan(id)
        }
        EntityType::Place(_) => panic!("test parameters are values or loans"),
    };
    function.blocks[0].parameters.push(entity);
    entity
}

#[test]
fn borrowed_generation_accepts_external_borrow_callable_without_local_owner() {
    for layout in [Layout::Pointer, Layout::Shared, Layout::Owned] {
        let mut fixture = fixture(layout, 3, false);
        let initializer = fixture.initializer;
        let generate = fixture.generate;
        let function = function(&mut fixture);
        let ty = function.loans[initializer.index()].ty;
        let parameter = loan(entry_parameter(function, ty));
        let Operation::ContainerGenerateBorrowed { initializer, .. } =
            &mut function.instructions[generate.index()].operation
        else {
            panic!("generator")
        };
        *initializer = parameter;
        verify_program(&fixture.program).unwrap_or_else(|error| panic!("{layout:?}: {error:?}"));
        crate::llvm::render_verified_program(&fixture.program)
            .expect("Borrow(Fn) argument has existing pointer ABI");
    }
}

fn projected_fixture(layout: Layout) -> Fixture {
    let mut fixture = fixture(layout, 3, false);
    let initializer = fixture.initializer;
    let generate = fixture.generate;
    let module = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module");
    let callable = module.function(fixture.make).expect("make").loans[initializer.index()]
        .ty
        .semantic_type();
    let opaque_container = module
        .add_sequential_container_type(SequentialContainerKind::Array, callable)
        .expect("container of concrete callable");
    let function = module.function_mut(fixture.make).expect("make");
    let integer = function.values[0].ty.semantic_type();
    let container = value(entry_parameter(
        function,
        EntityType::Value(opaque_container),
    ));
    let block = function.blocks[0].id;
    let terminator = function.blocks[0].terminator.take();
    let original = function.blocks[0].instructions.clone();
    let start = function.blocks[0].instructions.len();
    let zero = value(append(
        function,
        block,
        Operation::Constant(ScalarConstant::Integer(0)),
        EntityType::Value(integer),
        &function.origin.clone(),
    ));
    let EntityId::Place(place) = append(
        function,
        block,
        Operation::ContainerElementPlace {
            owner: EntityId::Value(container),
            index: zero,
        },
        EntityType::Place(callable),
        &function.origin.clone(),
    ) else {
        panic!("place")
    };
    let projected = loan(append(
        function,
        block,
        Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        },
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: callable,
        },
        &function.origin.clone(),
    ));
    let stop = function.blocks[0].instructions.len();
    let projected_end = end(function, block, projected);
    let drop = function
        .append_instruction(
            block,
            Operation::Drop { owner: container },
            vec![],
            function.origin.clone(),
        )
        .expect("drop opaque container")
        .0;
    let Operation::ContainerGenerateBorrowed { initializer, .. } =
        &mut function.instructions[generate.index()].operation
    else {
        panic!("generate")
    };
    *initializer = projected;
    let position = original
        .iter()
        .position(|id| *id == generate)
        .expect("generate");
    let mut ordered = original[..position].to_vec();
    ordered.extend_from_slice(&function.blocks[0].instructions[start..stop]);
    ordered.extend_from_slice(&original[position..]);
    ordered.push(projected_end);
    ordered.push(drop);
    function.blocks[0].instructions = ordered;
    function.blocks[0].terminator = terminator;
    fixture
}

#[test]
fn borrowed_generation_accepts_projected_function_pointer_without_capture_proof() {
    let fixture = projected_fixture(Layout::Pointer);
    verify_program(&fixture.program).expect("function pointer layout has no capture storage");
    crate::llvm::render_verified_program(&fixture.program)
        .expect("projected function pointer LLVM must verify");
}

#[test]
fn borrowed_generation_rejects_unknown_projected_capture_contents() {
    let fixture = projected_fixture(Layout::Shared);
    let generate = fixture.generate;
    let errors = verify_program(&fixture.program)
        .expect_err("do not guess capture IDs for a projected opaque callback");
    assert!(
        errors.errors.iter().any(|error| matches!(
            error.kind,
            super::super::verify::VerifyErrorKind::OperationContract {
                reason: "borrowed generation requires proved current callable capture contents"
            }
        ) && error.location
            == super::super::verify::VerifyLocation::Instruction(generate)),
        "{errors:?}"
    );
}

#[test]
fn borrowed_generation_normalizes_current_capture_at_a_conditional_join() {
    for end_current in [false, true] {
        let (mut fixture, capture) = replacement_successor(end_current);
        let module = fixture
            .program
            .module_mut(fixture.make.module())
            .expect("module");
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let function = module.function_mut(fixture.make).expect("make");
        let entry = function.blocks[0].id;
        let previous = function.blocks[0].terminator.take().expect("branch");
        let TerminatorKind::Branch(edge) = previous.kind else {
            panic!("branch")
        };
        let condition = value(append(
            function,
            entry,
            Operation::Constant(ScalarConstant::Boolean(true)),
            EntityType::Value(boolean),
            &function.origin.clone(),
        ));
        let types = function.blocks[edge.target.index()]
            .parameters
            .iter()
            .map(|parameter| function.entity(*parameter).expect("parameter").ty)
            .collect::<Vec<_>>();
        let left = function
            .add_block(types.clone(), function.origin.clone())
            .expect("left");
        let right = function
            .add_block(types, function.origin.clone())
            .expect("right");
        for block in [left, right] {
            function
                .set_terminator(
                    block,
                    TerminatorKind::Branch(Edge {
                        target: edge.target,
                        arguments: function.blocks[block.index()].parameters.clone(),
                    }),
                    function.origin.clone(),
                )
                .expect("join edge");
        }
        function
            .set_terminator(
                entry,
                TerminatorKind::Conditional {
                    condition,
                    when_true: Edge {
                        target: left,
                        arguments: edge.arguments.clone(),
                    },
                    when_false: Edge {
                        target: right,
                        arguments: edge.arguments,
                    },
                },
                function.origin.clone(),
            )
            .expect("conditional");
        if end_current {
            assert_inactive(&fixture, capture);
        } else {
            verify_program(&fixture.program).expect("both paths carry the same current content through distinct capture loan identities");
        }
    }
}
