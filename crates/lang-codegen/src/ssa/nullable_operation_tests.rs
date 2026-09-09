use lang_frontend::source::SourceMap;

use super::{
    model::{
        BlockId, Edge, EntityId, EntityType, Function, LoanId, LoanKind, ModelError, Operation,
        Origin, Ownership, Program, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
    },
    render::render_program,
    verify::{VerifyErrorKind, verify_program},
};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("nullable.ko", "if (owner != null) owner")
        .expect("source");
    Origin::Source(sources.span(source, 0, 2).expect("span"))
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value");
    };
    value
}

fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(loan) = entity else {
        panic!("expected loan");
    };
    loan
}

fn append_value(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    ty: SsaTypeId,
    origin: &Origin,
) -> ValueId {
    value(
        function
            .append_instruction(
                block,
                operation,
                vec![EntityType::Value(ty)],
                origin.clone(),
            )
            .expect("instruction")
            .1[0],
    )
}

fn add_pointer_types(program: &mut Program) -> (super::model::ModuleId, SsaTypeId, SsaTypeId) {
    let module_id = program.add_module("nullable");
    let module = program.module_mut(module_id).expect("module");
    let payload = module
        .add_aggregate_type("Node.payload", Vec::new())
        .expect("payload");
    let inner = module.declare_heap_owner("Node").expect("owner");
    module
        .define_heap_owner(inner, payload)
        .expect("definition");
    let nullable = module
        .add_nullable_handle_type(inner)
        .expect("nullable handle");
    (module_id, inner, nullable)
}

#[test]
fn nullable_handle_accepts_defined_pointer_owners_and_rejects_inline_types() {
    let mut program = Program::default();
    let (module_id, inner, nullable) = add_pointer_types(&mut program);
    let module = program.module_mut(module_id).expect("module");
    assert_eq!(module.nullable_inner(nullable), Some(inner));
    assert_eq!(module.type_ownership(nullable), Some(Ownership::MoveOnly));
    assert_eq!(
        module.add_nullable_handle_type(inner),
        Ok(nullable),
        "nullable identity must be interned"
    );
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    assert_eq!(
        module.add_nullable_handle_type(integer),
        Err(ModelError::ExpectedPointerLikeOwner { ty: integer })
    );
    verify_program(&program).expect("pointer nullable type must verify");
    assert!(render_program(&program).contains("nullable_handle<!t1>"));

    let invalid = program
        .module_mut(module_id)
        .expect("module")
        .intern_type(SsaTypeKind::NullableHandle { inner: integer });
    let errors = verify_program(&program)
        .expect_err("raw invalid nullable identity must not bypass verification")
        .errors;
    assert!(errors.iter().any(|error| {
        error.location == super::verify::VerifyLocation::Type(invalid)
            && matches!(error.kind, VerifyErrorKind::InvalidTypeDefinition { .. })
    }));
}

#[derive(Clone, Copy, Debug)]
enum RepeatedConsumption {
    Take,
    DropWrapper,
    DropInner,
}

fn nullable_operations_program(
    repeated: Option<RepeatedConsumption>,
) -> (Program, ValueId, ValueId) {
    let origin = origin();
    let mut program = Program::default();
    let (module_id, inner, nullable) = add_pointer_types(&mut program);
    let module = program.module_mut(module_id).expect("module");
    let boolean = module.intern_type(SsaTypeKind::Boolean);

    let inspect_id = module
        .add_function("inspect", Vec::new(), origin.clone())
        .expect("function");
    let inspect = module.function_mut(inspect_id).expect("function");
    let entry = inspect
        .add_block(vec![EntityType::Value(nullable)], origin.clone())
        .expect("entry");
    let owner = value(inspect.block(entry).expect("block").parameters[0]);
    append_value(
        inspect,
        entry,
        Operation::NullableIsNull { owner },
        boolean,
        &origin,
    );
    let null_block = inspect
        .add_block(vec![EntityType::Value(nullable)], origin.clone())
        .expect("null block");
    let non_null_block = inspect
        .add_block(
            vec![
                EntityType::Value(nullable),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                },
            ],
            origin.clone(),
        )
        .expect("non-null block");
    let null_owner = value(inspect.block(null_block).expect("block").parameters[0]);
    let non_null_owner = value(inspect.block(non_null_block).expect("block").parameters[0]);
    let view = loan(inspect.block(non_null_block).expect("block").parameters[1]);
    inspect
        .set_terminator(
            entry,
            TerminatorKind::NullableBranch {
                owner,
                when_null: Edge {
                    target: null_block,
                    arguments: vec![EntityId::Value(owner)],
                },
                when_non_null: Edge {
                    target: non_null_block,
                    arguments: vec![EntityId::Value(owner)],
                },
                view,
            },
            origin.clone(),
        )
        .expect("nullable branch");
    inspect
        .append_instruction(
            null_block,
            Operation::Drop { owner: null_owner },
            Vec::new(),
            origin.clone(),
        )
        .expect("null drop");
    inspect
        .set_terminator(
            null_block,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("null return");
    let taken = append_value(
        inspect,
        non_null_block,
        Operation::NullableTake {
            owner: non_null_owner,
            proof: view,
        },
        inner,
        &origin,
    );
    inspect
        .append_instruction(
            non_null_block,
            Operation::Drop { owner: taken },
            Vec::new(),
            origin.clone(),
        )
        .expect("inner drop");
    // Inject each violation into the same otherwise valid ownership graph.
    if let Some(repeated) = repeated {
        let (operation, result_types) = match repeated {
            RepeatedConsumption::Take => (
                Operation::NullableTake {
                    owner: non_null_owner,
                    proof: view,
                },
                vec![EntityType::Value(inner)],
            ),
            RepeatedConsumption::DropWrapper => (
                Operation::Drop {
                    owner: non_null_owner,
                },
                Vec::new(),
            ),
            RepeatedConsumption::DropInner => (Operation::Drop { owner: taken }, Vec::new()),
        };
        inspect
            .append_instruction(non_null_block, operation, result_types, origin.clone())
            .expect("repeated consumption instruction");
    }
    inspect
        .set_terminator(
            non_null_block,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("non-null return");

    let wrap_id = module
        .add_function("wrap", Vec::new(), origin.clone())
        .expect("wrap function");
    let wrap = module.function_mut(wrap_id).expect("function");
    let wrap_entry = wrap
        .add_block(vec![EntityType::Value(inner)], origin.clone())
        .expect("entry");
    let inner_owner = value(wrap.block(wrap_entry).expect("block").parameters[0]);
    let wrapped = append_value(
        wrap,
        wrap_entry,
        Operation::NullableWrap {
            nullable,
            owner: inner_owner,
        },
        nullable,
        &origin,
    );
    wrap.append_instruction(
        wrap_entry,
        Operation::Drop { owner: wrapped },
        Vec::new(),
        origin.clone(),
    )
    .expect("drop");
    wrap.set_terminator(
        wrap_entry,
        TerminatorKind::Return { values: Vec::new() },
        origin.clone(),
    )
    .expect("return");

    let null_id = module
        .add_function("null", Vec::new(), origin.clone())
        .expect("null function");
    let null = module.function_mut(null_id).expect("function");
    let null_entry = null.add_block(Vec::new(), origin.clone()).expect("entry");
    let null_value = append_value(
        null,
        null_entry,
        Operation::NullableNull { nullable },
        nullable,
        &origin,
    );
    null.append_instruction(
        null_entry,
        Operation::Drop { owner: null_value },
        Vec::new(),
        origin.clone(),
    )
    .expect("drop");
    null.set_terminator(
        null_entry,
        TerminatorKind::Return { values: Vec::new() },
        origin,
    )
    .expect("return");

    (program, non_null_owner, taken)
}

#[test]
fn nullable_operations_and_non_null_edge_verify_and_render() {
    let (program, _, _) = nullable_operations_program(None);
    verify_program(&program).expect("nullable operations must verify");
    let rendered = render_program(&program);
    assert!(rendered.contains("nullable.is_null"));
    assert!(rendered.contains("nullable.branch"));
    assert!(rendered.contains("nullable.take"));
    assert!(rendered.contains("nullable.wrap"));
    assert!(rendered.contains("nullable.null"));
}

#[test]
fn nullable_take_rejects_repeated_take_wrapper_drop_and_inner_drop() {
    for repeated in [
        RepeatedConsumption::Take,
        RepeatedConsumption::DropWrapper,
        RepeatedConsumption::DropInner,
    ] {
        let (program, wrapper, inner) = nullable_operations_program(Some(repeated));
        let unavailable = match repeated {
            RepeatedConsumption::Take | RepeatedConsumption::DropWrapper => wrapper,
            RepeatedConsumption::DropInner => inner,
        };
        let errors = verify_program(&program)
            .expect_err("take transfers ownership exactly once; neither wrapper nor inner can be consumed twice")
            .errors;
        assert!(errors.iter().any(|error| {
            matches!(error.kind, VerifyErrorKind::ValueUnavailable { value } if value == unavailable)
        }), "{repeated:?}: {errors:?}");
    }
}

#[test]
fn nullable_take_rejects_a_borrow_parameter_without_branch_proof() {
    let origin = origin();
    let mut program = Program::default();
    let (module_id, inner, nullable) = add_pointer_types(&mut program);
    let module = program.module_mut(module_id).expect("module");
    let function_id = module
        .add_function("invalid_take", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(
            vec![
                EntityType::Value(nullable),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                },
            ],
            origin.clone(),
        )
        .expect("entry");
    let owner = value(function.block(entry).expect("block").parameters[0]);
    let fake_proof = loan(function.block(entry).expect("block").parameters[1]);
    let taken = append_value(
        function,
        entry,
        Operation::NullableTake {
            owner,
            proof: fake_proof,
        },
        inner,
        &origin,
    );
    function
        .append_instruction(
            entry,
            Operation::Drop { owner: taken },
            Vec::new(),
            origin.clone(),
        )
        .expect("drop");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");

    let errors = verify_program(&program)
        .expect_err("borrow parameter is not a nullable edge proof")
        .errors;
    assert!(
        errors
            .iter()
            .any(|error| { matches!(error.kind, VerifyErrorKind::OperationContract { .. }) })
    );
}

#[test]
fn non_null_view_blocks_owner_drop_until_the_view_ends() {
    let origin = origin();
    let mut program = Program::default();
    let (module_id, inner, nullable) = add_pointer_types(&mut program);
    let module = program.module_mut(module_id).expect("module");
    let function_id = module
        .add_function("invalid_drop", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(vec![EntityType::Value(nullable)], origin.clone())
        .expect("entry");
    let owner = value(function.block(entry).expect("block").parameters[0]);
    let null_block = function
        .add_block(vec![EntityType::Value(nullable)], origin.clone())
        .expect("null");
    let non_null_block = function
        .add_block(
            vec![
                EntityType::Value(nullable),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                },
            ],
            origin.clone(),
        )
        .expect("non-null");
    let null_owner = value(function.block(null_block).expect("block").parameters[0]);
    let non_null_owner = value(function.block(non_null_block).expect("block").parameters[0]);
    let view = loan(function.block(non_null_block).expect("block").parameters[1]);
    function
        .set_terminator(
            entry,
            TerminatorKind::NullableBranch {
                owner,
                when_null: Edge {
                    target: null_block,
                    arguments: vec![EntityId::Value(owner)],
                },
                when_non_null: Edge {
                    target: non_null_block,
                    arguments: vec![EntityId::Value(owner)],
                },
                view,
            },
            origin.clone(),
        )
        .expect("branch");
    for (block, owner) in [(null_block, null_owner), (non_null_block, non_null_owner)] {
        function
            .append_instruction(block, Operation::Drop { owner }, Vec::new(), origin.clone())
            .expect("drop");
        function
            .set_terminator(
                block,
                TerminatorKind::Return { values: Vec::new() },
                origin.clone(),
            )
            .expect("return");
    }

    let errors = verify_program(&program)
        .expect_err("active non-null view must block owner drop")
        .errors;
    assert!(errors.iter().any(|error| {
        matches!(error.kind, VerifyErrorKind::OwnerLoanConflict { value } if value == non_null_owner)
    }));
}

#[test]
fn nullable_take_rejects_a_proof_from_a_different_owner() {
    let origin = origin();
    let mut program = Program::default();
    let (module_id, inner, nullable) = add_pointer_types(&mut program);
    let module = program.module_mut(module_id).expect("module");
    let function_id = module
        .add_function("mismatched_proof", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(
            vec![EntityType::Value(nullable), EntityType::Value(nullable)],
            origin.clone(),
        )
        .expect("entry");
    let first = value(function.block(entry).expect("block").parameters[0]);
    let second = value(function.block(entry).expect("block").parameters[1]);
    let null_block = function
        .add_block(
            vec![EntityType::Value(nullable), EntityType::Value(nullable)],
            origin.clone(),
        )
        .expect("null");
    let non_null_block = function
        .add_block(
            vec![
                EntityType::Value(nullable),
                EntityType::Value(nullable),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                },
            ],
            origin.clone(),
        )
        .expect("non-null");
    let null_values = function
        .block(null_block)
        .expect("block")
        .parameters
        .iter()
        .copied()
        .map(value)
        .collect::<Vec<_>>();
    let non_null_parameters = function
        .block(non_null_block)
        .expect("block")
        .parameters
        .clone();
    let non_null_first = value(non_null_parameters[0]);
    let non_null_second = value(non_null_parameters[1]);
    let first_proof = loan(non_null_parameters[2]);
    function
        .set_terminator(
            entry,
            TerminatorKind::NullableBranch {
                owner: first,
                when_null: Edge {
                    target: null_block,
                    arguments: vec![EntityId::Value(first), EntityId::Value(second)],
                },
                when_non_null: Edge {
                    target: non_null_block,
                    arguments: vec![EntityId::Value(first), EntityId::Value(second)],
                },
                view: first_proof,
            },
            origin.clone(),
        )
        .expect("branch");
    for owner in null_values {
        function
            .append_instruction(
                null_block,
                Operation::Drop { owner },
                Vec::new(),
                origin.clone(),
            )
            .expect("drop");
    }
    function
        .set_terminator(
            null_block,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("return");
    let taken = append_value(
        function,
        non_null_block,
        Operation::NullableTake {
            owner: non_null_second,
            proof: first_proof,
        },
        inner,
        &origin,
    );
    for owner in [taken, non_null_first] {
        function
            .append_instruction(
                non_null_block,
                Operation::Drop { owner },
                Vec::new(),
                origin.clone(),
            )
            .expect("drop");
    }
    function
        .set_terminator(
            non_null_block,
            TerminatorKind::Return { values: Vec::new() },
            origin,
        )
        .expect("return");

    let errors = verify_program(&program)
        .expect_err("proof for the first owner cannot unwrap the second")
        .errors;
    assert!(errors.iter().any(|error| {
        matches!(
            error.kind,
            VerifyErrorKind::NullableProofMismatch { owner, proof }
                if owner == non_null_second && proof == first_proof
        )
    }));
}
