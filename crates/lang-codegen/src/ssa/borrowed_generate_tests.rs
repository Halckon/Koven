//! Raw SSA contracts for synchronous borrowed container initialization.

mod root_storage_tests;

use lang_frontend::source::SourceMap;

use super::{model::*, render::render_program, verify::verify_program};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Layout {
    Pointer,
    Shared,
    Owned,
}

pub(crate) struct Fixture {
    pub(crate) program: Program,
    pub(crate) make: FunctionId,
    pub(crate) generate: InstructionId,
    pub(crate) initializer: LoanId,
    pub(crate) callback: ValueId,
    pub(crate) capture: Option<LoanId>,
}

fn append(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    ty: EntityType,
    origin: &Origin,
) -> EntityId {
    function
        .append_instruction(block, operation, vec![ty], origin.clone())
        .expect("instruction")
        .1[0]
}
fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(id) = entity else {
        panic!("value")
    };
    id
}
fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(id) = entity else {
        panic!("loan")
    };
    id
}
fn borrow(
    function: &mut Function,
    block: BlockId,
    owner: ValueId,
    ty: SsaTypeId,
    origin: &Origin,
) -> LoanId {
    let EntityId::Place(place) = append(
        function,
        block,
        Operation::RootPlace { owner },
        EntityType::Place(ty),
        origin,
    ) else {
        panic!("place")
    };
    loan(append(
        function,
        block,
        Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        },
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: ty,
        },
        origin,
    ))
}

/// Three concrete callable layouts share the same Borrow(Int) invocation contract.
pub(crate) fn fixture(layout: Layout, size: i32, unit_void: bool) -> Fixture {
    build_fixture(layout, Some(size), unit_void)
}

pub(crate) fn dynamic_fixture(layout: Layout) -> Fixture {
    build_fixture(layout, None, false)
}

fn build_fixture(layout: Layout, size: Option<i32>, unit_void: bool) -> Fixture {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("borrowed-generate.ko", "Array<Int>(size, initializer)")
        .expect("source");
    let origin = Origin::Source(sources.span(source, 0, 5).expect("span"));
    let mut program = Program::default();
    let id = program.add_module("borrowed-generate");
    let module = program.module_mut(id).expect("module");
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let unit = module.intern_type(SsaTypeKind::Unit);
    let element = if unit_void { unit } else { int };
    let container = module
        .add_sequential_container_type(SequentialContainerKind::Array, element)
        .expect("container");
    let index_ty = EntityType::Loan {
        kind: LoanKind::Shared,
        target: int,
    };
    let returns = if unit_void { vec![] } else { vec![int] };
    let environment = match layout {
        Layout::Pointer => None,
        Layout::Shared => {
            let reference = module.add_shared_reference_type(int).expect("reference");
            Some(
                module
                    .add_aggregate_type("SharedEnv", vec![reference])
                    .expect("environment"),
            )
        }
        Layout::Owned => Some(
            module
                .add_aggregate_type("OwnedEnv", vec![int])
                .expect("environment"),
        ),
    };
    let callable_ty = match environment {
        None => module
            .add_function_pointer_type_with_parameters(vec![index_ty], returns.clone())
            .expect("callable"),
        Some(env) => module
            .add_concrete_closure_type_with_parameters(
                "Initializer",
                vec![index_ty],
                returns.clone(),
                env,
                vec![ClosureCaptureType {
                    mode: if matches!(layout, Layout::Shared) {
                        ClosureCaptureMode::Shared
                    } else {
                        ClosureCaptureMode::Owned
                    },
                    ty: int,
                }],
            )
            .expect("closure"),
    };
    let target = module
        .add_function("initializer", returns, origin.clone())
        .expect("target");
    let target_function = module.function_mut(target).expect("target");
    let mut parameters = vec![];
    if let Some(target) = environment {
        parameters.push(EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        });
    }
    parameters.push(index_ty);
    let entry = target_function
        .add_block(parameters, origin.clone())
        .expect("entry");
    let index = loan(
        *target_function
            .block(entry)
            .expect("entry")
            .parameters
            .last()
            .expect("index"),
    );
    let values = if unit_void {
        vec![]
    } else {
        vec![value(append(
            target_function,
            entry,
            Operation::Read {
                source: PlaceAccess::Loan(index),
            },
            EntityType::Value(int),
            &origin,
        ))]
    };
    target_function
        .set_terminator(entry, TerminatorKind::Return { values }, origin.clone())
        .expect("return");
    let make = module
        .add_function("make", vec![container], origin.clone())
        .expect("make");
    let function = module.function_mut(make).expect("make");
    let entry = function
        .add_block(
            if size.is_none() {
                vec![EntityType::Value(int)]
            } else {
                vec![]
            },
            origin.clone(),
        )
        .expect("entry");
    let length = match size {
        Some(size) => value(append(
            function,
            entry,
            Operation::Constant(ScalarConstant::Integer(size as i128)),
            EntityType::Value(int),
            &origin,
        )),
        None => value(function.block(entry).expect("entry").parameters[0]),
    };
    let capture = if matches!(layout, Layout::Shared) {
        Some(borrow(function, entry, length, int, &origin))
    } else {
        None
    };
    let operation = if environment.is_none() {
        Operation::FunctionAddress { target }
    } else {
        Operation::ClosureConstruct {
            closure: callable_ty,
            thunk: target,
            captures: vec![match capture {
                Some(id) => ClosureCaptureOperand::Shared(id),
                None => ClosureCaptureOperand::Owned(length),
            }],
        }
    };
    let callback = value(append(
        function,
        entry,
        operation,
        EntityType::Value(callable_ty),
        &origin,
    ));
    let initializer = borrow(function, entry, callback, callable_ty, &origin);
    let (generate, results) = function
        .append_instruction(
            entry,
            Operation::ContainerGenerateBorrowed {
                container,
                length,
                initializer,
            },
            vec![EntityType::Value(container)],
            origin.clone(),
        )
        .expect("generate");
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: initializer },
            vec![],
            origin.clone(),
        )
        .expect("end");
    function
        .append_instruction(
            entry,
            Operation::Drop { owner: callback },
            vec![],
            origin.clone(),
        )
        .expect("drop");
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![value(results[0])],
            },
            origin,
        )
        .expect("return");
    Fixture {
        program,
        make,
        generate,
        initializer,
        callback,
        capture,
    }
}

#[test]
fn borrowed_generation_accepts_pointer_shared_and_owned_without_consuming_initializer() {
    for layout in [Layout::Pointer, Layout::Shared, Layout::Owned] {
        let fixture = fixture(layout, 3, false);
        verify_program(&fixture.program).unwrap_or_else(|error| panic!("{layout:?}: {error:?}"));
        let text = render_program(&fixture.program);
        assert!(text.contains("container.generate_borrowed"));
        let function = fixture
            .program
            .module(fixture.make.module())
            .expect("module")
            .function(fixture.make)
            .expect("make");
        assert_eq!(
            function
                .instruction(fixture.generate)
                .expect("generate")
                .operation
                .entities()[1],
            EntityId::Loan(fixture.initializer)
        );
    }
}

#[test]
fn borrowed_generation_accepts_unit_void_as_one_logical_element() {
    for layout in [Layout::Pointer, Layout::Shared, Layout::Owned] {
        verify_program(&fixture(layout, 3, true).program)
            .expect("void initializer must construct Unit elements");
    }
}

fn has_error(fixture: &Fixture, predicate: impl Fn(&super::verify::VerifyError) -> bool) {
    let errors = verify_program(&fixture.program).expect_err("malformed generator must fail");
    assert!(
        errors.errors.iter().any(predicate),
        "unexpected errors: {errors:?}"
    );
}

#[test]
fn borrowed_generation_rejects_expired_initializer_and_owner_drop() {
    use super::verify::VerifyErrorKind;
    let mut expired = fixture(Layout::Pointer, 1, false);
    let function = expired
        .program
        .module_mut(expired.make.module())
        .expect("module")
        .function_mut(expired.make)
        .expect("make");
    let entry = &mut function.blocks[0];
    let generation = entry
        .instructions
        .iter()
        .position(|id| *id == expired.generate)
        .expect("generate");
    entry.instructions.swap(generation, generation + 1);
    has_error(
        &expired,
        |error| matches!(error.kind, VerifyErrorKind::LoanInactive { loan } if loan == expired.initializer),
    );

    let mut dropped = fixture(Layout::Owned, 1, false);
    let function = dropped
        .program
        .module_mut(dropped.make.module())
        .expect("module")
        .function_mut(dropped.make)
        .expect("make");
    let entry = &mut function.blocks[0];
    let generation = entry
        .instructions
        .iter()
        .position(|id| *id == dropped.generate)
        .expect("generate");
    entry.instructions.swap(generation, generation + 2);
    has_error(
        &dropped,
        |error| matches!(error.kind, VerifyErrorKind::OwnerLoanConflict { value } if value == dropped.callback),
    );
}

#[test]
fn borrowed_generation_keeps_shared_capture_dependency_until_callback_drop() {
    use super::verify::VerifyErrorKind;
    let mut fixture = fixture(Layout::Shared, 1, false);
    let capture = fixture.capture.expect("capture");
    let function = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module")
        .function_mut(fixture.make)
        .expect("make");
    let entry = function.blocks[0].id;
    let origin = function.origin.clone();
    let terminator = function.blocks[0].terminator.take();
    let (id, _) = function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: capture },
            vec![],
            origin,
        )
        .expect("premature end");
    function.blocks[0].terminator = terminator;
    let instructions = &mut function.blocks[0].instructions;
    instructions.pop();
    let generation = instructions
        .iter()
        .position(|id| *id == fixture.generate)
        .expect("generate");
    instructions.insert(generation, id);
    has_error(
        &fixture,
        |error| matches!(error.kind, VerifyErrorKind::OwnerLoanConflict { value } if value == fixture.callback),
    );
}

#[test]
fn borrowed_generation_rejects_exclusive_and_noncallable_initializer() {
    use super::verify::{VerifyErrorKind, VerifyLocation};
    for exclusive in [false, true] {
        let mut fixture = fixture(Layout::Pointer, 1, false);
        let module = fixture
            .program
            .module_mut(fixture.make.module())
            .expect("module");
        let integer = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        });
        let function = module.function_mut(fixture.make).expect("make");
        let ty = &mut function.loans[fixture.initializer.index()].ty;
        let EntityType::Loan { kind, target } = ty else {
            panic!("loan")
        };
        if exclusive {
            *kind = LoanKind::Exclusive;
        } else {
            *target = integer;
        }
        has_error(&fixture, |error| {
            matches!(error.kind, VerifyErrorKind::OperationContract { .. })
                && error.location == VerifyLocation::Instruction(fixture.generate)
        });
    }
}

#[test]
fn borrowed_generation_rejects_wrong_index_mode_arity_and_element_return() {
    use super::verify::{VerifyErrorKind, VerifyLocation};
    for case in 0..5 {
        let mut fixture = fixture(Layout::Pointer, 1, false);
        let module = fixture
            .program
            .module_mut(fixture.make.module())
            .expect("module");
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let function = module.function(fixture.make).expect("make");
        let ty = function.loans[fixture.initializer.index()]
            .ty
            .semantic_type();
        let SsaTypeKind::FunctionPointer { signature } = &mut module.types[ty.index()] else {
            panic!("signature")
        };
        match case {
            0 => signature.parameters.clear(),
            1 => signature.parameters.push(signature.parameters[0]),
            2 => {
                signature.parameters[0] = EntityType::Value(signature.parameters[0].semantic_type())
            }
            3 => {
                signature.parameters[0] = EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: signature.parameters[0].semantic_type(),
                }
            }
            4 => signature.returns = vec![boolean],
            _ => unreachable!(),
        }
        has_error(&fixture, |error| {
            matches!(error.kind, VerifyErrorKind::OperationContract { .. })
                && error.location == VerifyLocation::Instruction(fixture.generate)
        });
    }
}

#[test]
fn borrowed_generation_operand_registration_enforces_definition_order() {
    use super::verify::VerifyErrorKind;
    let mut fixture = fixture(Layout::Pointer, 1, false);
    let function = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module")
        .function_mut(fixture.make)
        .expect("make");
    let instructions = &mut function.blocks[0].instructions;
    let generation = instructions
        .iter()
        .position(|id| *id == fixture.generate)
        .expect("generate");
    instructions.swap(generation, generation - 1);
    has_error(
        &fixture,
        |error| matches!(error.kind, VerifyErrorKind::UseBeforeDefinition { entity } if entity == EntityId::Loan(fixture.initializer)),
    );
}

fn through_successor(fixture: &mut Fixture, transfer_loan: bool, transfer_capture: bool) {
    let function = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module")
        .function_mut(fixture.make)
        .expect("make");
    let entry = function.blocks[0].id;
    let callback_ty = function.values[fixture.callback.index()].ty;
    let loan_ty = function.loans[fixture.initializer.index()].ty;
    let mut types = vec![callback_ty];
    if transfer_loan {
        types.push(loan_ty);
    }
    if transfer_capture {
        types.push(function.loans[fixture.capture.expect("capture").index()].ty);
    }
    let next = function
        .add_block(types, function.origin.clone())
        .expect("successor");
    let callback = value(function.blocks[next.index()].parameters[0]);
    let initializer = if transfer_loan {
        loan(function.blocks[next.index()].parameters[1])
    } else {
        fixture.initializer
    };
    let position = function.blocks[0]
        .instructions
        .iter()
        .position(|id| *id == fixture.generate)
        .expect("generate");
    let tail = function.blocks[0].instructions.split_off(position);
    for id in &tail {
        let instruction = &mut function.instructions[id.index()];
        instruction.block = next;
        match &mut instruction.operation {
            Operation::ContainerGenerateBorrowed {
                initializer: operand,
                ..
            } => *operand = initializer,
            Operation::BorrowEnd { loan } => *loan = initializer,
            Operation::Drop { owner } => *owner = callback,
            _ => panic!("unexpected tail"),
        }
    }
    function.blocks[next.index()].instructions = tail;
    function.blocks[next.index()].terminator = function.blocks[0].terminator.take();
    let mut arguments = vec![EntityId::Value(fixture.callback)];
    if transfer_loan {
        arguments.push(EntityId::Loan(fixture.initializer));
    }
    if transfer_capture {
        arguments.push(EntityId::Loan(fixture.capture.expect("capture")));
    }
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: next,
                arguments,
            }),
            function.origin.clone(),
        )
        .expect("branch");
}

#[test]
fn borrowed_generation_preserves_initializer_and_capture_dependencies_across_cfg_transfer() {
    for layout in [Layout::Pointer, Layout::Shared, Layout::Owned] {
        let mut fixture = fixture(layout, 3, false);
        through_successor(&mut fixture, true, false);
        verify_program(&fixture.program).unwrap_or_else(|error| panic!("{layout:?}: {error:?}"));
        crate::llvm::render_verified_program(&fixture.program).expect("CFG LLVM must verify");
    }
}

#[test]
fn borrowed_generation_rejects_hidden_initializer_loan_across_cfg() {
    use super::verify::VerifyErrorKind;
    let mut fixture = fixture(Layout::Pointer, 1, false);
    through_successor(&mut fixture, false, false);
    has_error(
        &fixture,
        |error| matches!(error.kind, VerifyErrorKind::HiddenLinearLiveIn { entity } if entity == EntityId::Loan(fixture.initializer)),
    );
}

#[test]
fn borrowed_generation_rejects_initializer_defined_only_in_successor() {
    use super::verify::VerifyErrorKind;
    let mut fixture = fixture(Layout::Pointer, 1, false);
    let function = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module")
        .function_mut(fixture.make)
        .expect("make");
    let Definition::InstructionResult { instruction, .. } =
        function.loans[fixture.initializer.index()].definition
    else {
        panic!("loan definition")
    };
    let entry = function.blocks[0].id;
    let next = function
        .add_block(vec![], function.origin.clone())
        .expect("successor");
    function.blocks[0]
        .instructions
        .retain(|id| *id != instruction);
    function.blocks[next.index()].instructions.push(instruction);
    function.instructions[instruction.index()].block = next;
    function.blocks[next.index()].terminator = function.blocks[0].terminator.take();
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: next,
                arguments: vec![],
            }),
            function.origin.clone(),
        )
        .expect("branch");
    has_error(
        &fixture,
        |error| matches!(error.kind, VerifyErrorKind::NonDominatingUse { entity } if entity == EntityId::Loan(fixture.initializer)),
    );
}

#[test]
fn borrowed_generation_uses_rebound_active_capture_loan_in_successor() {
    let mut fixture = fixture(Layout::Shared, 3, false);
    through_successor(&mut fixture, true, true);
    verify_program(&fixture.program)
        .expect("rebound capture stays active; old owner is no longer live");
    crate::llvm::render_verified_program(&fixture.program)
        .expect("rebound capture LLVM must verify");
}

#[test]
fn borrowed_generation_invalid_unused_type_is_structurally_rejected() {
    // Invalid user-independent SSA metadata must fail before capture graph indexing.
    let mut fixture = fixture(Layout::Pointer, 1, false);
    let module = &mut fixture.program.modules[0];
    module.types.push(SsaTypeKind::SequentialContainer {
        kind: SequentialContainerKind::Array,
        element: SsaTypeId {
            module: module.id,
            index: usize::MAX,
        },
    });
    let errors = verify_program(&fixture.program).expect_err("invalid element type");
    assert!(errors.errors.iter().any(|error| matches!(
        error.kind,
        super::verify::VerifyErrorKind::InvalidTypeDefinition { .. }
    )));
}
