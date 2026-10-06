//! Following a shared capture slot must recover a typed target loan, never a raw owner.

use super::*;
use crate::llvm::render_verified_program;
use crate::ssa::verify::{VerifyErrorKind, VerifyLocation};

struct ReadingFixture {
    fixture: Fixture,
    thunk: FunctionId,
    environment: LoanId,
    slot: LoanId,
    target: LoanId,
    follow: InstructionId,
}

/// Keep the existing generator fixture unchanged and give this case its own capture-reading thunk.
/// The captured length is 3, so this thunk returns 3/4/5 rather than the index-only 0/1/2.
fn reading_fixture(size: i32) -> ReadingFixture {
    let mut fixture = fixture(Layout::Shared, size, false);
    let module = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module");
    let callable = module
        .function(fixture.make)
        .expect("make")
        .entity(EntityId::Value(fixture.callback))
        .expect("callback")
        .ty
        .semantic_type();
    let SsaTypeKind::ConcreteClosure {
        environment,
        captures,
        ..
    } = module.type_kind(callable).expect("closure")
    else {
        panic!("closure");
    };
    let environment_ty = *environment;
    let integer = captures[0].ty;
    let reference = module
        .aggregate_fields(environment_ty)
        .expect("environment")[0];
    let origin = module.function(fixture.make).expect("make").origin.clone();
    let thunk = module
        .add_function("capture_reading_initializer", vec![integer], origin.clone())
        .expect("thunk");
    let function = module.function_mut(thunk).expect("thunk");
    let entry = function
        .add_block(
            vec![shared(environment_ty), shared(integer)],
            origin.clone(),
        )
        .expect("entry");
    let parameters = function.block(entry).expect("entry").parameters.clone();
    let environment = loan(parameters[0]);
    let index = loan(parameters[1]);
    let slot = loan(append(
        function,
        entry,
        Operation::SharedFieldLoan {
            base: environment,
            field: 0,
        },
        shared(reference),
        &origin,
    ));
    let (follow, results) = function
        .append_instruction(
            entry,
            Operation::SharedReferenceFollow { source: slot },
            vec![shared(integer)],
            origin.clone(),
        )
        .expect("follow");
    let target = loan(results[0]);
    let captured = value(append(
        function,
        entry,
        Operation::Read {
            source: PlaceAccess::Loan(target),
        },
        EntityType::Value(integer),
        &origin,
    ));
    let index = value(append(
        function,
        entry,
        Operation::Read {
            source: PlaceAccess::Loan(index),
        },
        EntityType::Value(integer),
        &origin,
    ));
    let sum = value(append(
        function,
        entry,
        Operation::Binary {
            operator: BinaryOperator::Add,
            left: captured,
            right: index,
        },
        EntityType::Value(integer),
        &origin,
    ));
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![sum] }, origin)
        .expect("return");
    let make = module.function_mut(fixture.make).expect("make");
    let construction = make
        .instructions
        .iter_mut()
        .find(|instruction| instruction.results == [EntityId::Value(fixture.callback)])
        .expect("formation");
    let Operation::ClosureConstruct {
        thunk: callback_thunk,
        ..
    } = &mut construction.operation
    else {
        panic!("formation");
    };
    *callback_thunk = thunk;
    ReadingFixture {
        fixture,
        thunk,
        environment,
        slot,
        target,
        follow,
    }
}

fn shared(target: SsaTypeId) -> EntityType {
    EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    }
}

fn thunk(fixture: &mut ReadingFixture) -> &mut Function {
    fixture
        .fixture
        .program
        .module_mut(fixture.thunk.module())
        .expect("module")
        .function_mut(fixture.thunk)
        .expect("thunk")
}

fn insert_end(function: &mut Function, position: usize, loan: LoanId) -> InstructionId {
    let entry = function.blocks[0].id;
    let terminator = function.blocks[0].terminator.take();
    let (instruction, _) = function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan },
            vec![],
            function.origin.clone(),
        )
        .expect("end");
    let last = function.blocks[0].instructions.pop().expect("appended end");
    function.blocks[0].instructions.insert(position, last);
    function.blocks[0].terminator = terminator;
    instruction
}

fn expect_error(
    program: &Program,
    location: VerifyLocation,
    predicate: impl Fn(&VerifyErrorKind) -> bool,
) {
    let errors = verify_program(program).expect_err("invalid reference follow must be rejected");
    assert!(
        errors
            .errors
            .iter()
            .any(|error| error.location == location && predicate(&error.kind)),
        "{errors:?}"
    );
}

#[test]
fn shared_reference_follow_reads_capture_and_index_in_a_real_thunk() {
    for size in [0, 1, 3] {
        let fixture = reading_fixture(size);
        verify_program(&fixture.fixture.program)
            .expect("shared capture target can be read within environment extent");
        let function = fixture
            .fixture
            .program
            .module(fixture.thunk.module())
            .expect("module")
            .function(fixture.thunk)
            .expect("thunk");
        assert_eq!(
            function
                .instruction(fixture.follow)
                .expect("follow")
                .operation
                .entities(),
            vec![EntityId::Loan(fixture.slot)]
        );
        assert!(render_program(&fixture.fixture.program).contains("shared_reference.follow"));
    }
}

#[test]
fn shared_reference_follow_llvm_loads_the_slot_pointer_before_reading_the_capture() {
    let fixture = reading_fixture(3);
    let llvm =
        render_verified_program(&fixture.fixture.program).expect("capture-reading LLVM verifies");
    let marker = format!("@f{}.capture_reading_initializer(", fixture.thunk.index());
    let body = llvm
        .split(&marker)
        .nth(1)
        .expect("reading thunk")
        .split("\n}")
        .next()
        .expect("thunk body");
    let pointer = format!(
        "%l{} = load ptr, ptr %l{}",
        fixture.target.index(),
        fixture.slot.index()
    );
    let dereference = format!("load i32, ptr %l{}", fixture.target.index());
    assert!(
        body.contains("getelementptr"),
        "capture slot must be projected: {body}"
    );
    let pointer_position = body.find(&pointer).expect("slot pointer load");
    let capture_position = body.find(&dereference).expect("target Int load");
    assert!(
        pointer_position < capture_position,
        "load the stored pointer before the target: {body}"
    );
    assert!(
        body.contains("add i32"),
        "capture and index affect the returned element: {body}"
    );
    assert!(
        !body.contains("alloca"),
        "following a reference must not allocate: {body}"
    );
    assert!(!body.contains("@malloc") && !body.contains("@free"));
}

#[test]
fn shared_reference_follow_rejects_exclusive_or_nonreference_source_and_wrong_results() {
    verify_program(&reading_fixture(3).fixture.program).expect("positive control");
    for malformed in 0..4 {
        let mut fixture = reading_fixture(3);
        let slot = fixture.slot;
        let target = fixture.target;
        let follow = fixture.follow;
        let function = thunk(&mut fixture);
        let integer = function.loans[target.index()].ty.semantic_type();
        match malformed {
            0 => {
                function.loans[slot.index()].ty = EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: function.loans[slot.index()].ty.semantic_type(),
                };
            }
            1 => {
                function.instructions[follow.index()].operation =
                    Operation::SharedReferenceFollow {
                        source: fixture_index(function),
                    };
            }
            2 => {
                function.loans[target.index()].ty = EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: integer,
                };
            }
            3 => {
                function.loans[target.index()].ty =
                    shared(function.loans[slot.index()].ty.semantic_type());
            }
            _ => unreachable!(),
        }
        expect_error(
            &fixture.fixture.program,
            VerifyLocation::Instruction(follow),
            |kind| matches!(kind, VerifyErrorKind::OperationContract { .. }),
        );
    }
}

fn fixture_index(function: &Function) -> LoanId {
    loan(function.blocks[0].parameters[1])
}

#[test]
fn shared_reference_follow_requires_active_source_and_blocks_parent_or_ancestor_end() {
    verify_program(&reading_fixture(3).fixture.program)
        .expect("positive control with implicit entry extent");
    let mut expired = reading_fixture(3);
    let slot = expired.slot;
    let follow = expired.follow;
    insert_end(thunk(&mut expired), 1, slot);
    expect_error(
        &expired.fixture.program,
        VerifyLocation::Instruction(follow),
        |kind| matches!(kind, VerifyErrorKind::LoanInactive { loan } if *loan == slot),
    );

    let mut invalid = reading_fixture(3);
    let parent = invalid.slot;
    let target = invalid.target;
    let end = insert_end(thunk(&mut invalid), 2, parent);
    expect_error(
        &invalid.fixture.program,
        VerifyLocation::Instruction(end),
        |kind| matches!(kind, VerifyErrorKind::LoanDependencyActive { parent: found, dependent } if *found == parent && *dependent == target),
    );

    let mut explicit = reading_fixture(3);
    let target = explicit.target;
    let slot = explicit.slot;
    let environment = explicit.environment;
    let function = thunk(&mut explicit);
    let length = function.blocks[0].instructions.len();
    insert_end(function, length, target);
    insert_end(function, length + 1, slot);
    insert_end(function, length + 2, environment);
    verify_program(&explicit.fixture.program)
        .expect("child then parent then environment end is valid");
}

/// With only the target carried, the environment ancestor must still protect it.
/// With both carried, the slot parent's identity changes on the edge.
fn through_successor(
    carry_slot: bool,
    end_parent_first: bool,
) -> (ReadingFixture, LoanId, LoanId, Option<InstructionId>) {
    let mut fixture = reading_fixture(3);
    let slot = fixture.slot;
    let target = fixture.target;
    let environment = fixture.environment;
    let function = thunk(&mut fixture);
    let origin = function.origin.clone();
    let entry = function.blocks[0].id;
    let tail = function.blocks[0].instructions.split_off(2);
    let terminator = function.blocks[0].terminator.take().expect("return");
    let mut types = vec![];
    let mut arguments = vec![];
    if carry_slot {
        types.push(function.loans[slot.index()].ty);
        arguments.push(EntityId::Loan(slot));
    }
    types.push(function.loans[target.index()].ty);
    arguments.push(EntityId::Loan(target));
    let next = function
        .add_block(types, origin.clone())
        .expect("successor");
    let parameters = function.block(next).expect("successor").parameters.clone();
    let parent = if carry_slot {
        loan(parameters[0])
    } else {
        environment
    };
    let child = loan(*parameters.last().expect("target"));
    let mut end = None;
    if end_parent_first {
        end = Some(
            function
                .append_instruction(
                    next,
                    Operation::BorrowEnd { loan: parent },
                    vec![],
                    origin.clone(),
                )
                .expect("early parent end")
                .0,
        );
    }
    for id in &tail {
        function.instructions[id.index()].block = next;
        if let Operation::Read {
            source: PlaceAccess::Loan(loan),
        } = &mut function.instructions[id.index()].operation
            && *loan == target
        {
            *loan = child;
        }
        function.blocks[next.index()].instructions.push(*id);
    }
    function
        .append_instruction(
            next,
            Operation::BorrowEnd { loan: child },
            vec![],
            origin.clone(),
        )
        .expect("end child");
    if !end_parent_first {
        function
            .append_instruction(
                next,
                Operation::BorrowEnd { loan: parent },
                vec![],
                origin.clone(),
            )
            .expect("end parent");
    }
    function.blocks[next.index()].terminator = Some(terminator);
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: next,
                arguments,
            }),
            origin,
        )
        .expect("transport");
    (fixture, parent, child, end)
}

#[test]
fn shared_reference_follow_preserves_parent_dependency_after_cfg_rebinding() {
    for carry_slot in [true, false] {
        let (valid, _, _, _) = through_successor(carry_slot, false);
        verify_program(&valid.fixture.program)
            .expect("explicitly rebound child remains within parent/ancestor extent");
        render_verified_program(&valid.fixture.program)
            .expect("reference target pointer survives loan phi transport");
        let (invalid, parent, child, end) = through_successor(carry_slot, true);
        expect_error(
            &invalid.fixture.program,
            VerifyLocation::Instruction(end.expect("early end")),
            |kind| matches!(kind, VerifyErrorKind::LoanDependencyActive { parent: found, dependent } if *found == parent && *dependent == child),
        );
    }
}

fn target_callable(layout: Layout) -> (Fixture, FunctionId, InstructionId) {
    let mut fixture = fixture(layout, 3, false);
    let module = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module");
    let make = module.function(fixture.make).expect("make");
    let callable = make
        .entity(EntityId::Value(fixture.callback))
        .expect("callback")
        .ty
        .semantic_type();
    let integer = make.values[0].ty.semantic_type();
    let container = make
        .instruction(fixture.generate)
        .expect("generation")
        .results[0];
    let container = make
        .entity(container)
        .expect("container")
        .ty
        .semantic_type();
    let origin = make.origin.clone();
    let reference = module
        .add_shared_reference_type(callable)
        .expect("reference");
    let probe = module
        .add_function("followed_callable", vec![container], origin.clone())
        .expect("probe");
    let function = module.function_mut(probe).expect("probe");
    let block = function
        .add_block(
            vec![shared(reference), EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("entry");
    let parameters = function.block(block).expect("entry").parameters.clone();
    let initializer = loan(append(
        function,
        block,
        Operation::SharedReferenceFollow {
            source: loan(parameters[0]),
        },
        shared(callable),
        &origin,
    ));
    let (generate, results) = function
        .append_instruction(
            block,
            Operation::ContainerGenerateBorrowed {
                container,
                length: value(parameters[1]),
                initializer,
            },
            vec![EntityType::Value(container)],
            origin.clone(),
        )
        .expect("generate");
    function
        .set_terminator(
            block,
            TerminatorKind::Return {
                values: vec![value(results[0])],
            },
            origin,
        )
        .expect("return");
    (fixture, probe, generate)
}

#[test]
fn shared_reference_follow_uses_target_contents_for_borrowed_generation() {
    let (pointer, _, _) = target_callable(Layout::Pointer);
    verify_program(&pointer.program).expect("function pointer target has no captures");
    render_verified_program(&pointer.program)
        .expect("function pointer target can be invoked after follow");
    let (captured, _, generate) = target_callable(Layout::Shared);
    expect_error(
        &captured.program,
        VerifyLocation::Instruction(generate),
        |kind| {
            matches!(
                kind,
                VerifyErrorKind::OperationContract {
                    reason: "borrowed generation requires proved current callable capture contents"
                }
            )
        },
    );
}

fn retained_target(layout: Layout) -> (Fixture, BlockId) {
    let mut fixture = fixture(layout, 3, false);
    let module = fixture
        .program
        .module_mut(fixture.make.module())
        .expect("module");
    let make = module.function(fixture.make).expect("make");
    let callable = make
        .entity(EntityId::Value(fixture.callback))
        .expect("callback")
        .ty
        .semantic_type();
    let origin = make.origin.clone();
    let owner = module
        .declare_shared_owner("BorrowedCallableRc")
        .expect("owner");
    module
        .define_shared_owner(owner, callable)
        .expect("payload");
    let reference = module.add_shared_reference_type(owner).expect("reference");
    let probe = module
        .add_function(
            "retain_followed_borrowed_callable",
            vec![owner],
            origin.clone(),
        )
        .expect("probe");
    let function = module.function_mut(probe).expect("probe");
    let block = function
        .add_block(vec![shared(reference)], origin.clone())
        .expect("entry");
    let source = loan(function.block(block).expect("entry").parameters[0]);
    let target = append(
        function,
        block,
        Operation::SharedReferenceFollow { source },
        shared(owner),
        &origin,
    );
    let retained = value(append(
        function,
        block,
        Operation::SharedRetain { owner: target },
        EntityType::Value(owner),
        &origin,
    ));
    function
        .set_terminator(
            block,
            TerminatorKind::Return {
                values: vec![retained],
            },
            origin,
        )
        .expect("return");
    (fixture, block)
}

#[test]
fn shared_reference_follow_does_not_turn_reference_slot_into_owned_content_proof() {
    let (valid, _) = retained_target(Layout::Pointer);
    verify_program(&valid.program).expect("capture-free owned target return is a legal control");
    let (invalid, block) = retained_target(Layout::Shared);
    expect_error(
        &invalid.program,
        VerifyLocation::Terminator(block),
        |kind| {
            matches!(
                kind,
                VerifyErrorKind::OperationContract {
                    reason: "borrowed closure cannot escape through owned value delivery"
                }
            )
        },
    );
}
