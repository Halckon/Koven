use lang_frontend::source::SourceMap;

use super::{
    model::{
        BlockId, Edge, EntityId, EntityType, Function, FunctionId, LoanId, LoanKind, ModuleId,
        Operation, Origin, Ownership, PlaceAccess, PlaceId, Program, SsaTypeId, SsaTypeKind,
        TerminatorKind, ValueId,
    },
    verify::{VerifyError, VerifyErrorKind, verify_program},
};

#[derive(Clone, Copy)]
enum TestType {
    Boolean,
    Integer,
    MoveOnly,
}

struct Harness {
    program: Program,
    module: ModuleId,
    function: FunctionId,
    entry: BlockId,
    boolean: SsaTypeId,
    integer: SsaTypeId,
    move_only: SsaTypeId,
    origin: Origin,
}

impl Harness {
    fn new(parameters: &[TestType], returns: &[TestType]) -> Self {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("main");
        let module = program.module_mut(module_id).expect("module must exist");
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let integer = module.intern_type(SsaTypeKind::Integer {
            bits: 64,
            signed: true,
        });
        let move_only = module.intern_type(SsaTypeKind::Opaque {
            name: "Endpoint".to_owned(),
            ownership: Ownership::MoveOnly,
        });
        let resolve = |ty| match ty {
            TestType::Boolean => boolean,
            TestType::Integer => integer,
            TestType::MoveOnly => move_only,
        };
        let function_id = module
            .add_function(
                "ownership",
                returns.iter().copied().map(resolve).collect(),
                origin.clone(),
            )
            .expect("signature must be valid");
        let function = module
            .function_mut(function_id)
            .expect("function must exist");
        let entry = function
            .add_block(
                parameters
                    .iter()
                    .copied()
                    .map(resolve)
                    .map(EntityType::Value)
                    .collect(),
                origin.clone(),
            )
            .expect("entry block must be valid");
        Self {
            program,
            module: module_id,
            function: function_id,
            entry,
            boolean,
            integer,
            move_only,
            origin,
        }
    }

    fn function(&self) -> &Function {
        self.program
            .module(self.module)
            .expect("module must exist")
            .function(self.function)
            .expect("function must exist")
    }

    fn function_mut(&mut self) -> &mut Function {
        self.program
            .module_mut(self.module)
            .expect("module must exist")
            .function_mut(self.function)
            .expect("function must exist")
    }

    fn parameter(&self, index: usize) -> ValueId {
        value(
            self.function()
                .block(self.entry)
                .expect("entry must exist")
                .parameters[index],
        )
    }

    fn add_block(&mut self, parameters: Vec<EntityType>) -> BlockId {
        let origin = self.origin.clone();
        self.function_mut()
            .add_block(parameters, origin)
            .expect("block must be valid")
    }

    fn append(
        &mut self,
        block: BlockId,
        operation: Operation,
        results: Vec<EntityType>,
    ) -> Vec<EntityId> {
        let origin = self.origin.clone();
        self.function_mut()
            .append_instruction(block, operation, results, origin)
            .expect("instruction must be valid")
            .1
    }

    fn terminate(&mut self, block: BlockId, terminator: TerminatorKind) {
        let origin = self.origin.clone();
        self.function_mut()
            .set_terminator(block, terminator, origin)
            .expect("terminator must be valid");
    }
}

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("ssa-ownership.ko", "fun own")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 3).expect("test span must be valid"))
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value, got {entity:?}");
    };
    value
}

fn entity_place(entity: EntityId) -> PlaceId {
    let EntityId::Place(place) = entity else {
        panic!("expected place, got {entity:?}");
    };
    place
}

fn entity_loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(loan) = entity else {
        panic!("expected loan, got {entity:?}");
    };
    loan
}

fn errors(program: &Program) -> Vec<VerifyError> {
    verify_program(program)
        .expect_err("fixture must fail ownership verification")
        .errors
}

fn has_kind(errors: &[VerifyError], expected: impl Fn(&VerifyErrorKind) -> bool) -> bool {
    errors.iter().any(|error| expected(&error.kind))
}

#[test]
fn copyable_values_can_be_copied_and_read_repeatedly() {
    let mut harness = Harness::new(&[TestType::Integer], &[TestType::Integer]);
    let input = harness.parameter(0);
    let entry = harness.entry;
    let integer = harness.integer;
    let first = value(
        harness.append(
            entry,
            Operation::Copy { source: input },
            vec![EntityType::Value(integer)],
        )[0],
    );
    let second = value(
        harness.append(
            entry,
            Operation::Copy { source: input },
            vec![EntityType::Value(integer)],
        )[0],
    );
    let sum = value(
        harness.append(
            entry,
            Operation::Binary {
                operator: super::model::BinaryOperator::Add,
                left: first,
                right: second,
            },
            vec![EntityType::Value(integer)],
        )[0],
    );
    harness.terminate(entry, TerminatorKind::Return { values: vec![sum] });
    assert_eq!(verify_program(&harness.program), Ok(()));
}

#[test]
fn move_only_copy_reuse_double_drop_and_missing_exit_are_distinct() {
    let mut copied = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = copied.parameter(0);
    let entry = copied.entry;
    let move_only = copied.move_only;
    let duplicate = value(
        copied.append(
            entry,
            Operation::Copy { source: owner },
            vec![EntityType::Value(move_only)],
        )[0],
    );
    copied.append(entry, Operation::Consume { owner }, Vec::new());
    copied.append(entry, Operation::Consume { owner: duplicate }, Vec::new());
    copied.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&copied.program), |kind| matches!(
        kind,
        VerifyErrorKind::CopyMoveOnly { .. }
    )));

    let mut reused = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = reused.parameter(0);
    let entry = reused.entry;
    reused.append(entry, Operation::Consume { owner }, Vec::new());
    reused.append(entry, Operation::Consume { owner }, Vec::new());
    reused.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&reused.program), |kind| matches!(
        kind,
        VerifyErrorKind::ValueUnavailable { .. }
    )));

    let mut dropped = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = dropped.parameter(0);
    let entry = dropped.entry;
    dropped.append(entry, Operation::Drop { owner }, Vec::new());
    dropped.append(entry, Operation::Drop { owner }, Vec::new());
    dropped.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&dropped.program), |kind| matches!(
        kind,
        VerifyErrorKind::ValueUnavailable { .. }
    )));

    let mut missing = Harness::new(&[TestType::MoveOnly], &[]);
    let entry = missing.entry;
    missing.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&missing.program), |kind| matches!(
        kind,
        VerifyErrorKind::MissingOwnedExit { .. }
    )));

    let mut copyable_drop = Harness::new(&[TestType::Integer], &[]);
    let value = copyable_drop.parameter(0);
    let entry = copyable_drop.entry;
    copyable_drop.append(entry, Operation::Drop { owner: value }, Vec::new());
    copyable_drop.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&copyable_drop.program), |kind| matches!(
        kind,
        VerifyErrorKind::DropCopyable { .. }
    )));
}

#[test]
fn move_only_return_mutation_and_abort_discharge_their_expected_obligations() {
    let mut returned = Harness::new(&[TestType::MoveOnly], &[TestType::MoveOnly]);
    let owner = returned.parameter(0);
    let entry = returned.entry;
    returned.terminate(
        entry,
        TerminatorKind::Return {
            values: vec![owner],
        },
    );
    assert_eq!(verify_program(&returned.program), Ok(()));

    let mut mutated = Harness::new(&[TestType::MoveOnly, TestType::MoveOnly], &[]);
    let owner = mutated.parameter(0);
    let replacement = mutated.parameter(1);
    let entry = mutated.entry;
    let move_only = mutated.move_only;
    let place = entity_place(
        mutated.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    mutated.append(
        entry,
        Operation::Mutate {
            place,
            value: replacement,
        },
        Vec::new(),
    );
    mutated.append(entry, Operation::Drop { owner }, Vec::new());
    mutated.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert_eq!(verify_program(&mutated.program), Ok(()));

    let mut aborted = Harness::new(&[TestType::MoveOnly], &[]);
    let entry = aborted.entry;
    aborted.terminate(entry, TerminatorKind::Abort);
    assert_eq!(verify_program(&aborted.program), Ok(()));
}

#[test]
fn conditional_edges_can_transfer_one_owner_on_mutually_exclusive_paths() {
    let mut harness = Harness::new(&[TestType::Boolean, TestType::MoveOnly], &[]);
    let condition = harness.parameter(0);
    let owner = harness.parameter(1);
    let entry = harness.entry;
    let move_only = harness.move_only;
    let when_true = harness.add_block(vec![EntityType::Value(move_only)]);
    let when_false = harness.add_block(vec![EntityType::Value(move_only)]);
    let join = harness.add_block(vec![EntityType::Value(move_only)]);
    harness.terminate(
        entry,
        TerminatorKind::Conditional {
            condition,
            when_true: Edge {
                target: when_true,
                arguments: vec![EntityId::Value(owner)],
            },
            when_false: Edge {
                target: when_false,
                arguments: vec![EntityId::Value(owner)],
            },
        },
    );
    let true_owner = value(
        harness
            .function()
            .block(when_true)
            .expect("block")
            .parameters[0],
    );
    let false_owner = value(
        harness
            .function()
            .block(when_false)
            .expect("block")
            .parameters[0],
    );
    harness.terminate(
        when_true,
        TerminatorKind::Branch(Edge {
            target: join,
            arguments: vec![EntityId::Value(true_owner)],
        }),
    );
    harness.terminate(
        when_false,
        TerminatorKind::Branch(Edge {
            target: join,
            arguments: vec![EntityId::Value(false_owner)],
        }),
    );
    let joined = value(harness.function().block(join).expect("block").parameters[0]);
    harness.append(join, Operation::Drop { owner: joined }, Vec::new());
    harness.terminate(join, TerminatorKind::Return { values: Vec::new() });
    assert_eq!(verify_program(&harness.program), Ok(()));
}

#[test]
fn hidden_live_in_and_one_branch_missing_transfer_are_rejected() {
    let mut hidden = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = hidden.parameter(0);
    let entry = hidden.entry;
    let target = hidden.add_block(Vec::new());
    hidden.terminate(
        entry,
        TerminatorKind::Branch(Edge {
            target,
            arguments: Vec::new(),
        }),
    );
    hidden.append(target, Operation::Drop { owner }, Vec::new());
    hidden.terminate(target, TerminatorKind::Return { values: Vec::new() });
    let failures = errors(&hidden.program);
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::HiddenLinearLiveIn { .. }
    )));

    let mut missing = Harness::new(&[TestType::Boolean, TestType::MoveOnly], &[]);
    let condition = missing.parameter(0);
    let owner = missing.parameter(1);
    let entry = missing.entry;
    let move_only = missing.move_only;
    let consumes = missing.add_block(vec![EntityType::Value(move_only)]);
    let omits = missing.add_block(Vec::new());
    missing.terminate(
        entry,
        TerminatorKind::Conditional {
            condition,
            when_true: Edge {
                target: consumes,
                arguments: vec![EntityId::Value(owner)],
            },
            when_false: Edge {
                target: omits,
                arguments: Vec::new(),
            },
        },
    );
    let consumed = value(
        missing
            .function()
            .block(consumes)
            .expect("block")
            .parameters[0],
    );
    missing.append(consumes, Operation::Drop { owner: consumed }, Vec::new());
    missing.terminate(consumes, TerminatorKind::Return { values: Vec::new() });
    missing.terminate(omits, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&missing.program), |kind| matches!(
        kind,
        VerifyErrorKind::MissingOwnedExit { .. }
    )));
}

#[test]
fn shared_loans_can_coexist_and_exclusive_loan_reads_are_legal() {
    let mut shared = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = shared.parameter(0);
    let entry = shared.entry;
    let move_only = shared.move_only;
    let place = entity_place(
        shared.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let first = entity_loan(
        shared.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: move_only,
            }],
        )[0],
    );
    let second = entity_loan(
        shared.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: move_only,
            }],
        )[0],
    );
    shared.append(entry, Operation::BorrowEnd { loan: second }, Vec::new());
    shared.append(entry, Operation::BorrowEnd { loan: first }, Vec::new());
    shared.append(entry, Operation::Drop { owner }, Vec::new());
    shared.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert_eq!(verify_program(&shared.program), Ok(()));

    let mut exclusive = Harness::new(&[TestType::Integer], &[TestType::Integer]);
    let owner = exclusive.parameter(0);
    let entry = exclusive.entry;
    let integer = exclusive.integer;
    let place = entity_place(
        exclusive.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(integer)],
        )[0],
    );
    let loan = entity_loan(
        exclusive.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: integer,
            }],
        )[0],
    );
    let read = value(
        exclusive.append(
            entry,
            Operation::Read {
                source: PlaceAccess::Loan(loan),
            },
            vec![EntityType::Value(integer)],
        )[0],
    );
    exclusive.append(entry, Operation::BorrowEnd { loan }, Vec::new());
    exclusive.terminate(entry, TerminatorKind::Return { values: vec![read] });
    assert_eq!(verify_program(&exclusive.program), Ok(()));
}

#[test]
fn root_place_take_requires_one_unborrowed_move_only_direct_root() {
    let mut valid = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = valid.parameter(0);
    let entry = valid.entry;
    let move_only = valid.move_only;
    let place = entity_place(
        valid.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let loan = entity_loan(
        valid.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: move_only,
            }],
        )[0],
    );
    valid.append(entry, Operation::BorrowEnd { loan }, Vec::new());
    let rebound = value(
        valid.append(
            entry,
            Operation::RootPlaceTake { owner, place },
            vec![EntityType::Value(move_only)],
        )[0],
    );
    valid.append(entry, Operation::Drop { owner: rebound }, Vec::new());
    valid.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert_eq!(verify_program(&valid.program), Ok(()));

    let mut borrowed = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = borrowed.parameter(0);
    let entry = borrowed.entry;
    let move_only = borrowed.move_only;
    let place = entity_place(
        borrowed.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let loan = entity_loan(
        borrowed.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: move_only,
            }],
        )[0],
    );
    let rebound = value(
        borrowed.append(
            entry,
            Operation::RootPlaceTake { owner, place },
            vec![EntityType::Value(move_only)],
        )[0],
    );
    borrowed.append(entry, Operation::BorrowEnd { loan }, Vec::new());
    borrowed.append(entry, Operation::Drop { owner: rebound }, Vec::new());
    borrowed.append(entry, Operation::Drop { owner }, Vec::new());
    borrowed.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&borrowed.program), |kind| matches!(
        kind,
        VerifyErrorKind::OwnerLoanConflict { value } if *value == owner
    )));

    let mut mismatched = Harness::new(&[TestType::MoveOnly, TestType::MoveOnly], &[]);
    let owner = mismatched.parameter(0);
    let other = mismatched.parameter(1);
    let entry = mismatched.entry;
    let move_only = mismatched.move_only;
    let other_place = entity_place(
        mismatched.append(
            entry,
            Operation::RootPlace { owner: other },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    mismatched.append(
        entry,
        Operation::RootPlaceTake {
            owner,
            place: other_place,
        },
        vec![EntityType::Value(move_only)],
    );
    mismatched.append(entry, Operation::Drop { owner: other }, Vec::new());
    mismatched.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&mismatched.program), |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let mut copyable = Harness::new(&[TestType::Integer], &[TestType::Integer]);
    let owner = copyable.parameter(0);
    let entry = copyable.entry;
    let integer = copyable.integer;
    let place = entity_place(
        copyable.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(integer)],
        )[0],
    );
    let result = value(
        copyable.append(
            entry,
            Operation::RootPlaceTake { owner, place },
            vec![EntityType::Value(integer)],
        )[0],
    );
    copyable.terminate(
        entry,
        TerminatorKind::Return {
            values: vec![result],
        },
    );
    assert!(has_kind(&errors(&copyable.program), |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let mut wrong_result = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = wrong_result.parameter(0);
    let entry = wrong_result.entry;
    let move_only = wrong_result.move_only;
    let integer = wrong_result.integer;
    let place = entity_place(
        wrong_result.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    wrong_result.append(
        entry,
        Operation::RootPlaceTake { owner, place },
        vec![EntityType::Value(integer)],
    );
    wrong_result.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&wrong_result.program), |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let mut repeated = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = repeated.parameter(0);
    let entry = repeated.entry;
    let move_only = repeated.move_only;
    let place = entity_place(
        repeated.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let first = value(
        repeated.append(
            entry,
            Operation::RootPlaceTake { owner, place },
            vec![EntityType::Value(move_only)],
        )[0],
    );
    let second = value(
        repeated.append(
            entry,
            Operation::RootPlaceTake { owner, place },
            vec![EntityType::Value(move_only)],
        )[0],
    );
    repeated.append(entry, Operation::Drop { owner: first }, Vec::new());
    repeated.append(entry, Operation::Drop { owner: second }, Vec::new());
    repeated.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    let failures = errors(&repeated.program);
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::ValueUnavailable { value } if *value == owner
    )));
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::PlaceUnavailable { place: actual } if *actual == place
    )));
}

#[test]
fn conflicting_borrow_mutation_and_owner_drop_are_rejected() {
    let mut borrow = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = borrow.parameter(0);
    let entry = borrow.entry;
    let move_only = borrow.move_only;
    let place = entity_place(
        borrow.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let shared = entity_loan(
        borrow.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: move_only,
            }],
        )[0],
    );
    let exclusive = entity_loan(
        borrow.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Exclusive,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: move_only,
            }],
        )[0],
    );
    borrow.append(entry, Operation::Drop { owner }, Vec::new());
    borrow.append(entry, Operation::BorrowEnd { loan: exclusive }, Vec::new());
    borrow.append(entry, Operation::BorrowEnd { loan: shared }, Vec::new());
    borrow.append(entry, Operation::Drop { owner }, Vec::new());
    borrow.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    let failures = errors(&borrow.program);
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::BorrowConflict { .. }
    )));
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::OwnerLoanConflict { .. }
    )));

    let mut mutation = Harness::new(&[TestType::Integer, TestType::Integer], &[]);
    let owner = mutation.parameter(0);
    let replacement = mutation.parameter(1);
    let entry = mutation.entry;
    let integer = mutation.integer;
    let place = entity_place(
        mutation.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(integer)],
        )[0],
    );
    let loan = entity_loan(
        mutation.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
        )[0],
    );
    mutation.append(
        entry,
        Operation::Mutate {
            place,
            value: replacement,
        },
        Vec::new(),
    );
    mutation.append(entry, Operation::BorrowEnd { loan }, Vec::new());
    mutation.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&mutation.program), |kind| matches!(
        kind,
        VerifyErrorKind::MutationConflict { .. }
    )));
}

#[test]
fn move_only_place_reads_and_inactive_loan_end_are_rejected() {
    let mut harness = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = harness.parameter(0);
    let entry = harness.entry;
    let move_only = harness.move_only;
    let place = entity_place(
        harness.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let loan = entity_loan(
        harness.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: move_only,
            }],
        )[0],
    );
    let read = value(
        harness.append(
            entry,
            Operation::Read {
                source: PlaceAccess::Loan(loan),
            },
            vec![EntityType::Value(move_only)],
        )[0],
    );
    harness.append(entry, Operation::Consume { owner: read }, Vec::new());
    harness.append(entry, Operation::BorrowEnd { loan }, Vec::new());
    harness.append(entry, Operation::BorrowEnd { loan }, Vec::new());
    harness.append(entry, Operation::Drop { owner }, Vec::new());
    harness.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    let failures = errors(&harness.program);
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::MoveOnlyPlaceRead { .. }
    )));
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::LoanInactive { .. }
    )));
}

#[test]
fn consuming_an_owner_invalidates_its_untransferred_places() {
    let mut harness = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = harness.parameter(0);
    let entry = harness.entry;
    let move_only = harness.move_only;
    let place = entity_place(
        harness.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    harness.append(entry, Operation::Drop { owner }, Vec::new());
    let loan = entity_loan(
        harness.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: move_only,
            }],
        )[0],
    );
    harness.append(entry, Operation::BorrowEnd { loan }, Vec::new());
    harness.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    assert!(has_kind(&errors(&harness.program), |kind| matches!(
        kind,
        VerifyErrorKind::PlaceUnavailable { .. }
    )));
}

#[test]
fn owner_place_and_loan_can_cross_an_edge_only_as_explicit_parameters() {
    let mut harness = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = harness.parameter(0);
    let entry = harness.entry;
    let move_only = harness.move_only;
    let place = entity_place(
        harness.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let loan = entity_loan(
        harness.append(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: move_only,
            }],
        )[0],
    );
    let target = harness.add_block(vec![
        EntityType::Value(move_only),
        EntityType::Place(move_only),
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: move_only,
        },
    ]);
    harness.terminate(
        entry,
        TerminatorKind::Branch(Edge {
            target,
            arguments: vec![
                EntityId::Value(owner),
                EntityId::Place(place),
                EntityId::Loan(loan),
            ],
        }),
    );
    let parameters = &harness.function().block(target).expect("target").parameters;
    let target_owner = value(parameters[0]);
    let target_loan = entity_loan(parameters[2]);
    harness.append(
        target,
        Operation::BorrowEnd { loan: target_loan },
        Vec::new(),
    );
    harness.append(
        target,
        Operation::Drop {
            owner: target_owner,
        },
        Vec::new(),
    );
    harness.terminate(target, TerminatorKind::Return { values: Vec::new() });
    assert_eq!(verify_program(&harness.program), Ok(()));
    harness.function_mut().blocks[target.index()]
        .instructions
        .swap(0, 1);
    assert!(has_kind(&errors(&harness.program), |kind| matches!(
        kind,
        VerifyErrorKind::OwnerLoanConflict { .. }
    )));

    let mut missing = Harness::new(&[TestType::MoveOnly], &[]);
    let owner = missing.parameter(0);
    let entry = missing.entry;
    let move_only = missing.move_only;
    let place = entity_place(
        missing.append(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(move_only)],
        )[0],
    );
    let _loan = missing.append(
        entry,
        Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        },
        vec![EntityType::Loan {
            kind: LoanKind::Shared,
            target: move_only,
        }],
    );
    let target = missing.add_block(vec![EntityType::Value(move_only)]);
    missing.terminate(
        entry,
        TerminatorKind::Branch(Edge {
            target,
            arguments: vec![EntityId::Value(owner)],
        }),
    );
    let target_owner = value(missing.function().block(target).expect("target").parameters[0]);
    missing.append(
        target,
        Operation::Drop {
            owner: target_owner,
        },
        Vec::new(),
    );
    missing.terminate(target, TerminatorKind::Return { values: Vec::new() });
    let failures = errors(&missing.program);
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::OwnerLoanConflict { .. }
    )));
    assert!(has_kind(&failures, |kind| matches!(
        kind,
        VerifyErrorKind::ActiveLoanAtExit { .. }
    )));
}

#[test]
fn ownership_errors_are_deterministic_and_do_not_use_frontend_diagnostics() {
    let mut harness = Harness::new(&[TestType::MoveOnly], &[]);
    let entry = harness.entry;
    harness.terminate(entry, TerminatorKind::Return { values: Vec::new() });
    let first = errors(&harness.program);
    let second = errors(&harness.program);
    assert_eq!(first, second);
    assert_eq!(first.len(), 1);
    assert!(first[0].origin.is_some());
    assert!(matches!(
        first[0].kind,
        VerifyErrorKind::MissingOwnedExit { .. }
    ));
}

#[test]
fn heap_field_replace_requires_an_active_unshadowed_exclusive_receiver() {
    let exclusive = heap_field_program(
        LoanKind::Exclusive,
        false,
        false,
        0,
        TestType::Integer,
        HeapFieldAction::Replace,
    );
    assert_eq!(verify_program(&exclusive), Ok(()));

    let shared = errors(&heap_field_program(
        LoanKind::Shared,
        false,
        false,
        0,
        TestType::Integer,
        HeapFieldAction::Replace,
    ));
    assert!(has_kind(&shared, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let inactive = errors(&heap_field_program(
        LoanKind::Exclusive,
        true,
        false,
        0,
        TestType::Integer,
        HeapFieldAction::Replace,
    ));
    assert!(has_kind(&inactive, |kind| matches!(
        kind,
        VerifyErrorKind::LoanInactive { .. }
    )));

    let dependent = errors(&heap_field_program(
        LoanKind::Exclusive,
        false,
        true,
        0,
        TestType::Integer,
        HeapFieldAction::Replace,
    ));
    assert!(has_kind(&dependent, |kind| matches!(
        kind,
        VerifyErrorKind::LoanDependencyActive { .. }
    )));

    let dependent_read = errors(&heap_field_program(
        LoanKind::Exclusive,
        false,
        true,
        0,
        TestType::Integer,
        HeapFieldAction::Read,
    ));
    assert!(has_kind(&dependent_read, |kind| matches!(
        kind,
        VerifyErrorKind::LoanDependencyActive { .. }
    )));

    let out_of_bounds = errors(&heap_field_program(
        LoanKind::Exclusive,
        false,
        false,
        1,
        TestType::Integer,
        HeapFieldAction::Replace,
    ));
    assert!(has_kind(&out_of_bounds, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let move_only = heap_field_program(
        LoanKind::Exclusive,
        false,
        false,
        0,
        TestType::MoveOnly,
        HeapFieldAction::Replace,
    );
    assert_eq!(verify_program(&move_only), Ok(()));

    let move_only_read = errors(&heap_field_program(
        LoanKind::Shared,
        false,
        false,
        0,
        TestType::MoveOnly,
        HeapFieldAction::Read,
    ));
    assert!(has_kind(&move_only_read, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));
}

#[test]
fn inline_field_replace_requires_copyable_storage_and_an_unshadowed_exclusive_receiver() {
    let valid =
        inline_field_replace_program(LoanKind::Exclusive, false, false, 0, false, false, false);
    assert_eq!(verify_program(&valid), Ok(()));

    let shared = errors(&inline_field_replace_program(
        LoanKind::Shared,
        false,
        false,
        0,
        false,
        false,
        false,
    ));
    assert!(has_kind(&shared, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let inactive = errors(&inline_field_replace_program(
        LoanKind::Exclusive,
        true,
        false,
        0,
        false,
        false,
        false,
    ));
    assert!(has_kind(&inactive, |kind| matches!(
        kind,
        VerifyErrorKind::LoanInactive { .. }
    )));

    let dependent = errors(&inline_field_replace_program(
        LoanKind::Exclusive,
        false,
        true,
        0,
        false,
        false,
        false,
    ));
    assert!(has_kind(&dependent, |kind| matches!(
        kind,
        VerifyErrorKind::LoanDependencyActive { .. }
    )));

    for invalid in [
        inline_field_replace_program(LoanKind::Exclusive, false, false, 1, false, false, false),
        inline_field_replace_program(LoanKind::Exclusive, false, false, 0, true, false, false),
        inline_field_replace_program(LoanKind::Exclusive, false, false, 0, false, true, false),
        inline_field_replace_program(LoanKind::Exclusive, false, false, 0, false, false, true),
    ] {
        assert!(has_kind(&errors(&invalid), |kind| matches!(
            kind,
            VerifyErrorKind::OperationContract { .. }
        )));
    }
}

#[allow(clippy::too_many_arguments)]
fn inline_field_replace_program(
    receiver_kind: LoanKind,
    end_receiver: bool,
    shared_child: bool,
    field: usize,
    move_only_receiver: bool,
    move_only_field: bool,
    wrong_value_type: bool,
) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("inline-field-replace");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let field_type = if move_only_field { resource } else { integer };
    let mut fields = vec![field_type];
    if move_only_receiver && !move_only_field {
        fields.push(resource);
    }
    let receiver_type = module
        .add_aggregate_type("Inline", fields)
        .expect("inline aggregate");
    let value_type = if wrong_value_type {
        boolean
    } else {
        field_type
    };
    let receiver_entity_type = EntityType::Loan {
        kind: receiver_kind,
        target: receiver_type,
    };
    let function_id = module
        .add_instance_function("replace", receiver_entity_type, Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(
            vec![receiver_entity_type, EntityType::Value(value_type)],
            origin.clone(),
        )
        .expect("entry");
    let parameters = function.block(entry).expect("entry").parameters.clone();
    let [EntityId::Loan(receiver), EntityId::Value(value)] = parameters.as_slice() else {
        panic!("receiver and replacement parameters");
    };
    if end_receiver {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: *receiver },
                Vec::new(),
                origin.clone(),
            )
            .expect("end receiver");
    }
    let child = if shared_child {
        let (_, results) = function
            .append_instruction(
                entry,
                Operation::SharedReborrow { source: *receiver },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: receiver_type,
                }],
                origin.clone(),
            )
            .expect("shared child");
        let EntityId::Loan(child) = results[0] else {
            panic!("shared child loan");
        };
        Some(child)
    } else {
        None
    };
    function
        .append_instruction(
            entry,
            Operation::InlineFieldReplace {
                receiver: *receiver,
                field,
                value: *value,
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("inline field replace");
    if let Some(child) = child {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: child },
                Vec::new(),
                origin.clone(),
            )
            .expect("end child");
    }
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");
    program
}

#[test]
fn shared_heap_field_loan_tracks_the_receiver_dependency() {
    let valid = shared_heap_field_loan_program(false, false, 0);
    assert_eq!(verify_program(&valid), Ok(()));

    let parent_ended_first = errors(&shared_heap_field_loan_program(true, false, 0));
    assert!(has_kind(&parent_ended_first, |kind| matches!(
        kind,
        VerifyErrorKind::LoanDependencyActive { .. }
    )));

    let exclusive_receiver = errors(&shared_heap_field_loan_program(false, true, 0));
    assert!(has_kind(&exclusive_receiver, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let out_of_bounds = errors(&shared_heap_field_loan_program(false, false, 1));
    assert!(has_kind(&out_of_bounds, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));
}

#[test]
fn shared_inline_field_loan_blocks_ending_its_parent_reborrow() {
    let valid = shared_inline_field_loan_program(false);
    assert_eq!(verify_program(&valid), Ok(()));

    let parent_ended_first = errors(&shared_inline_field_loan_program(true));
    assert!(has_kind(&parent_ended_first, |kind| matches!(
        kind,
        VerifyErrorKind::LoanDependencyActive { .. }
    )));
}

fn shared_inline_field_loan_program(end_parent_first: bool) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("inline-field-loan");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let receiver_type = module
        .add_aggregate_type("Counter", vec![integer])
        .expect("inline aggregate");
    let function_id = module
        .add_instance_function(
            "read",
            EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: receiver_type,
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: receiver_type,
            }],
            origin.clone(),
        )
        .expect("entry");
    let EntityId::Loan(receiver) = function.block(entry).expect("entry").parameters[0] else {
        panic!("receiver loan");
    };
    let (_, reborrow_results) = function
        .append_instruction(
            entry,
            Operation::SharedReborrow { source: receiver },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: receiver_type,
            }],
            origin.clone(),
        )
        .expect("shared reborrow");
    let EntityId::Loan(reborrow) = reborrow_results[0] else {
        panic!("shared reborrow result");
    };
    let (_, field_results) = function
        .append_instruction(
            entry,
            Operation::SharedFieldLoan {
                base: reborrow,
                field: 0,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("field loan");
    let EntityId::Loan(field_loan) = field_results[0] else {
        panic!("field loan result");
    };
    if end_parent_first {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: reborrow },
                Vec::new(),
                origin.clone(),
            )
            .expect("end parent reborrow");
    }
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: field_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("end field loan");
    if !end_parent_first {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: reborrow },
                Vec::new(),
                origin.clone(),
            )
            .expect("end parent reborrow");
    }
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");
    program
}

fn shared_heap_field_loan_program(
    end_parent_first: bool,
    exclusive_receiver: bool,
    field: usize,
) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("heap-field-loan");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let payload = module
        .add_aggregate_type("Host.payload", vec![integer])
        .expect("payload");
    let owner = module.declare_heap_owner("Host").expect("owner");
    module
        .define_heap_owner(owner, payload)
        .expect("owner payload");
    let receiver_kind = if exclusive_receiver {
        LoanKind::Exclusive
    } else {
        LoanKind::Shared
    };
    let function_id = module
        .add_instance_function(
            "project",
            EntityType::Loan {
                kind: receiver_kind,
                target: owner,
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(
            vec![EntityType::Loan {
                kind: receiver_kind,
                target: owner,
            }],
            origin.clone(),
        )
        .expect("entry");
    let EntityId::Loan(receiver) = function.block(entry).expect("entry").parameters[0] else {
        panic!("receiver loan");
    };
    let (_, results) = function
        .append_instruction(
            entry,
            Operation::SharedHeapFieldLoan {
                base: receiver,
                field,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("field loan");
    let EntityId::Loan(field_loan) = results[0] else {
        panic!("field loan result");
    };
    if end_parent_first {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: receiver },
                Vec::new(),
                origin.clone(),
            )
            .expect("end receiver");
    }
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: field_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("end field loan");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");
    program
}

#[derive(Clone, Copy)]
enum HeapFieldAction {
    Read,
    Replace,
}

fn heap_field_program(
    receiver_kind: LoanKind,
    end_receiver: bool,
    shared_child: bool,
    field: usize,
    field_type: TestType,
    action: HeapFieldAction,
) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("heap-field");
    let module = program.module_mut(module_id).expect("module");
    let field_type = match field_type {
        TestType::Boolean => module.intern_type(SsaTypeKind::Boolean),
        TestType::Integer => module.intern_type(SsaTypeKind::Integer {
            bits: 64,
            signed: true,
        }),
        TestType::MoveOnly => module.intern_type(SsaTypeKind::Opaque {
            name: "Payload".to_owned(),
            ownership: Ownership::MoveOnly,
        }),
    };
    let payload = module
        .add_aggregate_type("Cell.payload", vec![field_type])
        .expect("payload");
    let owner = module.declare_heap_owner("Cell").expect("owner");
    module
        .define_heap_owner(owner, payload)
        .expect("owner payload");
    let receiver_type = EntityType::Loan {
        kind: receiver_kind,
        target: owner,
    };
    let function_id = module
        .add_instance_function("replace", receiver_type, Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let mut parameters = vec![receiver_type];
    if matches!(action, HeapFieldAction::Replace) {
        parameters.push(EntityType::Value(field_type));
    }
    let entry = function
        .add_block(parameters, origin.clone())
        .expect("entry");
    let EntityId::Loan(receiver) = function.block(entry).expect("entry").parameters[0] else {
        panic!("receiver loan");
    };
    if end_receiver {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: receiver },
                Vec::new(),
                origin.clone(),
            )
            .expect("end receiver");
    }
    let child = if shared_child {
        let (_, results) = function
            .append_instruction(
                entry,
                Operation::SharedReborrow { source: receiver },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: owner,
                }],
                origin.clone(),
            )
            .expect("shared child");
        let EntityId::Loan(child) = results[0] else {
            panic!("shared child loan");
        };
        Some(child)
    } else {
        None
    };
    match action {
        HeapFieldAction::Read => {
            function
                .append_instruction(
                    entry,
                    Operation::HeapFieldRead { receiver, field },
                    vec![EntityType::Value(field_type)],
                    origin.clone(),
                )
                .expect("read");
        }
        HeapFieldAction::Replace => {
            let EntityId::Value(value) = function.block(entry).expect("entry").parameters[1] else {
                panic!("replacement value");
            };
            function
                .append_instruction(
                    entry,
                    Operation::HeapFieldReplace {
                        receiver,
                        field,
                        value,
                    },
                    Vec::new(),
                    origin.clone(),
                )
                .expect("replace");
        }
    }
    if let Some(child) = child {
        function
            .append_instruction(
                entry,
                Operation::BorrowEnd { loan: child },
                Vec::new(),
                origin.clone(),
            )
            .expect("end child");
    }
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");
    program
}
