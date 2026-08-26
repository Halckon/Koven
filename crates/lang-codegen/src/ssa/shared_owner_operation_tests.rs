use lang_frontend::source::SourceMap;

use super::{
    model::{
        BlockId, EntityId, EntityType, Function, InstructionId, Operation, Origin, Ownership,
        Program, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
    },
    render::render_program,
    verify::{VerifyErrorKind, VerifyLocation, verify_program},
};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("shared-owner.ko", "Rc(value)")
        .expect("source");
    Origin::Source(sources.span(source, 0, 2).expect("span"))
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value");
    };
    value
}

fn append(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    results: Vec<EntityType>,
    origin: &Origin,
) -> (InstructionId, Vec<EntityId>) {
    function
        .append_instruction(block, operation, results, origin.clone())
        .expect("instruction")
}

fn add_function(
    program: &mut Program,
    payload_move_only: bool,
) -> (
    super::model::ModuleId,
    SsaTypeId,
    SsaTypeId,
    super::model::FunctionId,
    BlockId,
    ValueId,
) {
    let origin = origin();
    let module_id = program.add_module("shared");
    let module = program.module_mut(module_id).expect("module");
    let payload = if payload_move_only {
        module.intern_type(SsaTypeKind::Opaque {
            name: "Payload".to_owned(),
            ownership: Ownership::MoveOnly,
        })
    } else {
        module.intern_type(SsaTypeKind::Integer {
            bits: 64,
            signed: true,
        })
    };
    let owner = module.declare_shared_owner("Rc").expect("declare");
    module.define_shared_owner(owner, payload).expect("define");
    let function_id = module
        .add_function("shared", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(vec![EntityType::Value(payload)], origin)
        .expect("entry");
    let parameter = value(function.block(entry).expect("block").parameters[0]);
    (module_id, payload, owner, function_id, entry, parameter)
}

#[test]
fn shared_allocate_retain_payload_place_and_drop_verify_and_render() {
    let origin = origin();
    let mut program = Program::default();
    let (module_id, payload, owner_ty, function_id, entry, payload_value) =
        add_function(&mut program, false);
    let function = program
        .module_mut(module_id)
        .unwrap()
        .function_mut(function_id)
        .unwrap();
    let owner = value(
        append(
            function,
            entry,
            Operation::SharedAllocate {
                owner: owner_ty,
                payload: payload_value,
            },
            vec![EntityType::Value(owner_ty)],
            &origin,
        )
        .1[0],
    );
    let retained = value(
        append(
            function,
            entry,
            Operation::SharedRetain { owner },
            vec![EntityType::Value(owner_ty)],
            &origin,
        )
        .1[0],
    );
    append(
        function,
        entry,
        Operation::SharedPayloadPlace { owner },
        vec![EntityType::Place(payload)],
        &origin,
    );
    append(
        function,
        entry,
        Operation::Drop { owner: retained },
        Vec::new(),
        &origin,
    );
    append(
        function,
        entry,
        Operation::Drop { owner },
        Vec::new(),
        &origin,
    );
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .unwrap();

    verify_program(&program).expect("shared owner operations must verify");
    let rendered = render_program(&program);
    assert!(rendered.contains("shared.allocate"));
    assert!(rendered.contains("shared.retain"));
    assert!(rendered.contains("shared.payload_place"));
}

#[test]
fn shared_operation_type_contracts_reject_non_shared_owners() {
    let origin = origin();
    let mut program = Program::default();
    let (module_id, payload, owner_ty, function_id, entry, payload_value) =
        add_function(&mut program, false);
    let function = program
        .module_mut(module_id)
        .unwrap()
        .function_mut(function_id)
        .unwrap();
    let invalid = [
        append(
            function,
            entry,
            Operation::SharedAllocate {
                owner: payload,
                payload: payload_value,
            },
            vec![EntityType::Value(owner_ty)],
            &origin,
        )
        .0,
        append(
            function,
            entry,
            Operation::SharedRetain {
                owner: payload_value,
            },
            vec![EntityType::Value(owner_ty)],
            &origin,
        )
        .0,
        append(
            function,
            entry,
            Operation::SharedPayloadPlace {
                owner: payload_value,
            },
            vec![EntityType::Place(payload)],
            &origin,
        )
        .0,
    ];
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .unwrap();
    let errors = verify_program(&program)
        .expect_err("invalid operations")
        .errors;
    for instruction in invalid {
        assert!(errors.iter().any(|error| {
            error.location == VerifyLocation::Instruction(instruction)
                && matches!(error.kind, VerifyErrorKind::OperationContract { .. })
        }));
    }
}

#[test]
fn shared_allocate_consumes_move_only_payload_but_retain_preserves_source_owner() {
    let origin = origin();
    let mut program = Program::default();
    let (module_id, _payload, owner_ty, function_id, entry, payload_value) =
        add_function(&mut program, true);
    let function = program
        .module_mut(module_id)
        .unwrap()
        .function_mut(function_id)
        .unwrap();
    let owner = value(
        append(
            function,
            entry,
            Operation::SharedAllocate {
                owner: owner_ty,
                payload: payload_value,
            },
            vec![EntityType::Value(owner_ty)],
            &origin,
        )
        .1[0],
    );
    let retained = value(
        append(
            function,
            entry,
            Operation::SharedRetain { owner },
            vec![EntityType::Value(owner_ty)],
            &origin,
        )
        .1[0],
    );
    let invalid = append(
        function,
        entry,
        Operation::Drop {
            owner: payload_value,
        },
        Vec::new(),
        &origin,
    )
    .0;
    append(
        function,
        entry,
        Operation::Drop { owner: retained },
        Vec::new(),
        &origin,
    );
    append(
        function,
        entry,
        Operation::Drop { owner },
        Vec::new(),
        &origin,
    );
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .unwrap();

    let errors = verify_program(&program)
        .expect_err("payload must be unavailable after allocation")
        .errors;
    assert!(errors.iter().any(|error| {
        error.location == VerifyLocation::Instruction(invalid)
            && matches!(
                error.kind,
                VerifyErrorKind::ValueUnavailable { value } if value == payload_value
            )
    }));
}
