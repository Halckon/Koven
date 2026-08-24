use lang_frontend::source::SourceMap;

use super::{
    model::{
        BlockId, Edge, EntityId, EntityType, Function, InstructionId, LoanKind, Module, Operation,
        Origin, Ownership, PlaceId, Program, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
    },
    render::render_program,
    verify::{VerifyError, VerifyErrorKind, VerifyLocation, verify_program},
};

#[derive(Clone, Copy)]
struct AggregateTypes {
    integer: SsaTypeId,
    resource: SsaTypeId,
    point: SsaTypeId,
    pair: SsaTypeId,
    payload: SsaTypeId,
    owner: SsaTypeId,
}

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("aggregate-operations.ko", "value class Pair")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 5).expect("test span must be valid"))
}

fn add_types(module: &mut Module) -> AggregateTypes {
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let point = module
        .add_aggregate_type("Point", vec![integer])
        .expect("copyable aggregate must be valid");
    let pair = module
        .add_aggregate_type("Pair", vec![integer, resource])
        .expect("move-only aggregate must be valid");
    let payload = module
        .add_aggregate_type("Owner.payload", vec![pair])
        .expect("payload aggregate must be valid");
    let owner = module
        .declare_heap_owner("Owner")
        .expect("heap owner declaration must be valid");
    module
        .define_heap_owner(owner, payload)
        .expect("heap owner payload must be definable");
    AggregateTypes {
        integer,
        resource,
        point,
        pair,
        payload,
        owner,
    }
}

fn add_function(
    module: &mut Module,
    name: &str,
    parameters: &[SsaTypeId],
    returns: Vec<SsaTypeId>,
    origin: &Origin,
) -> (super::model::FunctionId, BlockId, Vec<ValueId>) {
    let id = module
        .add_function(name, returns, origin.clone())
        .expect("function signature must be valid");
    let function = module.function_mut(id).expect("function must exist");
    let entry = function
        .add_block(
            parameters.iter().copied().map(EntityType::Value).collect(),
            origin.clone(),
        )
        .expect("entry block must be valid");
    let parameters = function
        .block(entry)
        .expect("entry block must exist")
        .parameters
        .iter()
        .copied()
        .map(value)
        .collect();
    (id, entry, parameters)
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
        .expect("instruction must be appendable")
}

fn terminate(function: &mut Function, block: BlockId, origin: &Origin) {
    function
        .set_terminator(
            block,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("terminator must be settable");
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value, got {entity:?}");
    };
    value
}

fn place(entity: EntityId) -> PlaceId {
    let EntityId::Place(place) = entity else {
        panic!("expected place, got {entity:?}");
    };
    place
}

fn errors(program: &Program) -> Vec<VerifyError> {
    verify_program(program)
        .expect_err("fixture must fail verification")
        .errors
}

fn has_error_at(
    errors: &[VerifyError],
    instruction: InstructionId,
    predicate: impl Fn(&VerifyErrorKind) -> bool,
) -> bool {
    errors.iter().any(|error| {
        error.location == VerifyLocation::Instruction(instruction) && predicate(&error.kind)
    })
}

fn conditional_owner_program(heap_owner: bool, transfer_on_false: bool) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("owner_cfg");
    let module = program.module_mut(module_id).expect("module must exist");
    let types = add_types(module);
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let transferred = if heap_owner { types.owner } else { types.pair };
    let (function_id, entry, parameters) = add_function(
        module,
        "conditional_owner",
        &[boolean, transferred],
        Vec::new(),
        &origin,
    );
    let condition = parameters[0];
    let owner = parameters[1];
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let when_true = function
        .add_block(vec![EntityType::Value(transferred)], origin.clone())
        .expect("true block must be valid");
    let false_types = transfer_on_false
        .then_some(vec![EntityType::Value(transferred)])
        .unwrap_or_default();
    let when_false = function
        .add_block(false_types, origin.clone())
        .expect("false block must be valid");
    function
        .set_terminator(
            entry,
            TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: when_true,
                    arguments: vec![EntityId::Value(owner)],
                },
                when_false: Edge {
                    target: when_false,
                    arguments: transfer_on_false
                        .then_some(vec![EntityId::Value(owner)])
                        .unwrap_or_default(),
                },
            },
            origin.clone(),
        )
        .expect("conditional must be settable");
    let true_owner = value(
        function
            .block(when_true)
            .expect("true block must exist")
            .parameters[0],
    );
    append(
        function,
        when_true,
        Operation::Drop { owner: true_owner },
        Vec::new(),
        &origin,
    );
    terminate(function, when_true, &origin);
    if transfer_on_false {
        let false_owner = value(
            function
                .block(when_false)
                .expect("false block must exist")
                .parameters[0],
        );
        append(
            function,
            when_false,
            Operation::Drop { owner: false_owner },
            Vec::new(),
            &origin,
        );
    }
    terminate(function, when_false, &origin);
    program
}

#[test]
fn aggregate_heap_operations_form_one_valid_linear_flow_and_render_deterministically() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("aggregate_flow");
    let module = program.module_mut(module_id).expect("module must exist");
    let types = add_types(module);
    let (function_id, entry, parameters) = add_function(
        module,
        "flow",
        &[types.integer, types.resource],
        vec![types.integer],
        &origin,
    );
    let integer = parameters[0];
    let resource = parameters[1];
    let function = module
        .function_mut(function_id)
        .expect("function must exist");

    let pair = value(
        append(
            function,
            entry,
            Operation::AggregateConstruct {
                aggregate: types.pair,
                fields: vec![integer, resource],
            },
            vec![EntityType::Value(types.pair)],
            &origin,
        )
        .1[0],
    );
    let projected = value(
        append(
            function,
            entry,
            Operation::AggregateProject {
                aggregate: pair,
                field: 0,
            },
            vec![EntityType::Value(types.integer)],
            &origin,
        )
        .1[0],
    );
    let exploded = append(
        function,
        entry,
        Operation::AggregateExplode { aggregate: pair },
        vec![
            EntityType::Value(types.integer),
            EntityType::Value(types.resource),
        ],
        &origin,
    )
    .1;
    let pair = value(
        append(
            function,
            entry,
            Operation::AggregateConstruct {
                aggregate: types.pair,
                fields: vec![value(exploded[0]), value(exploded[1])],
            },
            vec![EntityType::Value(types.pair)],
            &origin,
        )
        .1[0],
    );
    let payload = value(
        append(
            function,
            entry,
            Operation::AggregateConstruct {
                aggregate: types.payload,
                fields: vec![pair],
            },
            vec![EntityType::Value(types.payload)],
            &origin,
        )
        .1[0],
    );
    let owner = value(
        append(
            function,
            entry,
            Operation::HeapAllocate {
                owner: types.owner,
                payload,
            },
            vec![EntityType::Value(types.owner)],
            &origin,
        )
        .1[0],
    );
    let payload_place = place(
        append(
            function,
            entry,
            Operation::HeapPayloadPlace { owner },
            vec![EntityType::Place(types.payload)],
            &origin,
        )
        .1[0],
    );
    let pair_place = place(
        append(
            function,
            entry,
            Operation::FieldPlace {
                base: payload_place,
                field: 0,
            },
            vec![EntityType::Place(types.pair)],
            &origin,
        )
        .1[0],
    );
    let loan = append(
        function,
        entry,
        Operation::BorrowBegin {
            place: pair_place,
            kind: LoanKind::Shared,
        },
        vec![EntityType::Loan {
            kind: LoanKind::Shared,
            target: types.pair,
        }],
        &origin,
    )
    .1[0];
    let EntityId::Loan(loan) = loan else {
        panic!("expected loan result");
    };
    append(
        function,
        entry,
        Operation::BorrowEnd { loan },
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
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![projected],
            },
            origin.clone(),
        )
        .expect("return must be settable");

    verify_program(&program).expect("complete aggregate/heap flow must verify");
    let rendered = render_program(&program);
    assert_eq!(rendered, render_program(&program));
    for operation in [
        "aggregate.construct",
        "aggregate.project",
        "aggregate.explode",
        "heap.allocate",
        "heap.payload_place",
        "field_place",
    ] {
        assert!(
            rendered.contains(operation),
            "missing {operation} in {rendered}"
        );
    }
}

#[test]
fn aggregate_and_heap_owner_edges_require_transfer_on_every_normal_path() {
    for heap_owner in [false, true] {
        verify_program(&conditional_owner_program(heap_owner, true))
            .expect("mutually exclusive edges may each transfer the same owner");
    }

    for heap_owner in [false, true] {
        let errors = errors(&conditional_owner_program(heap_owner, false));
        assert!(errors.iter().any(|error| {
            matches!(error.location, VerifyLocation::Edge { successor: 1, .. })
                && matches!(error.kind, VerifyErrorKind::MissingOwnedExit { .. })
        }));
    }
}

#[test]
fn aggregate_operation_contracts_reject_wrong_shapes_indices_and_capabilities() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("invalid_contracts");
    let module = program.module_mut(module_id).expect("module must exist");
    let types = add_types(module);
    let (function_id, entry, parameters) = add_function(
        module,
        "invalid",
        &[types.integer, types.resource, types.pair, types.point],
        Vec::new(),
        &origin,
    );
    let [integer, _resource, pair, point] = parameters.as_slice() else {
        panic!("expected four parameters");
    };
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let mut invalid = vec![
        append(
            function,
            entry,
            Operation::AggregateConstruct {
                aggregate: types.pair,
                fields: vec![*integer],
            },
            vec![EntityType::Value(types.pair)],
            &origin,
        )
        .0,
        append(
            function,
            entry,
            Operation::AggregateProject {
                aggregate: *pair,
                field: 1,
            },
            vec![EntityType::Value(types.resource)],
            &origin,
        )
        .0,
        append(
            function,
            entry,
            Operation::AggregateExplode { aggregate: *point },
            vec![EntityType::Value(types.integer)],
            &origin,
        )
        .0,
        append(
            function,
            entry,
            Operation::HeapAllocate {
                owner: types.owner,
                payload: *pair,
            },
            vec![EntityType::Value(types.owner)],
            &origin,
        )
        .0,
        append(
            function,
            entry,
            Operation::HeapPayloadPlace { owner: *integer },
            vec![EntityType::Place(types.payload)],
            &origin,
        )
        .0,
    ];
    let pair_place = place(
        append(
            function,
            entry,
            Operation::RootPlace { owner: *pair },
            vec![EntityType::Place(types.pair)],
            &origin,
        )
        .1[0],
    );
    invalid.push(
        append(
            function,
            entry,
            Operation::FieldPlace {
                base: pair_place,
                field: 2,
            },
            vec![EntityType::Place(types.integer)],
            &origin,
        )
        .0,
    );
    function
        .set_terminator(entry, TerminatorKind::Abort, origin.clone())
        .expect("abort must be settable");

    let errors = errors(&program);
    for instruction in invalid {
        assert!(has_error_at(&errors, instruction, |kind| matches!(
            kind,
            VerifyErrorKind::OperationContract { .. }
        )));
    }
}

#[test]
fn aggregate_construction_explosion_and_allocation_consume_move_only_inputs_once() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("linear_failures");
    let module = program.module_mut(module_id).expect("module must exist");
    let types = add_types(module);
    let pair_of_resources = module
        .add_aggregate_type("ResourcePair", vec![types.resource, types.resource])
        .expect("move-only pair must be valid");

    let (construct_id, construct_entry, parameters) = add_function(
        module,
        "duplicate_construct",
        &[types.resource],
        Vec::new(),
        &origin,
    );
    let resource = parameters[0];
    let construct = module
        .function_mut(construct_id)
        .expect("function must exist");
    let (duplicate, result) = append(
        construct,
        construct_entry,
        Operation::AggregateConstruct {
            aggregate: pair_of_resources,
            fields: vec![resource, resource],
        },
        vec![EntityType::Value(pair_of_resources)],
        &origin,
    );
    append(
        construct,
        construct_entry,
        Operation::Drop {
            owner: value(result[0]),
        },
        Vec::new(),
        &origin,
    );
    terminate(construct, construct_entry, &origin);

    let (explode_id, explode_entry, parameters) =
        add_function(module, "reuse_exploded", &[types.pair], Vec::new(), &origin);
    let pair = parameters[0];
    let explode = module
        .function_mut(explode_id)
        .expect("function must exist");
    let exploded = append(
        explode,
        explode_entry,
        Operation::AggregateExplode { aggregate: pair },
        vec![
            EntityType::Value(types.integer),
            EntityType::Value(types.resource),
        ],
        &origin,
    )
    .1;
    let (reuse_exploded, _) = append(
        explode,
        explode_entry,
        Operation::Drop { owner: pair },
        Vec::new(),
        &origin,
    );
    append(
        explode,
        explode_entry,
        Operation::Drop {
            owner: value(exploded[1]),
        },
        Vec::new(),
        &origin,
    );
    terminate(explode, explode_entry, &origin);

    let (allocate_id, allocate_entry, parameters) = add_function(
        module,
        "reuse_payload",
        &[types.payload],
        Vec::new(),
        &origin,
    );
    let payload = parameters[0];
    let allocate = module
        .function_mut(allocate_id)
        .expect("function must exist");
    let owner = value(
        append(
            allocate,
            allocate_entry,
            Operation::HeapAllocate {
                owner: types.owner,
                payload,
            },
            vec![EntityType::Value(types.owner)],
            &origin,
        )
        .1[0],
    );
    let (reuse_payload, _) = append(
        allocate,
        allocate_entry,
        Operation::Drop { owner: payload },
        Vec::new(),
        &origin,
    );
    append(
        allocate,
        allocate_entry,
        Operation::Drop { owner },
        Vec::new(),
        &origin,
    );
    terminate(allocate, allocate_entry, &origin);

    let errors = errors(&program);
    for instruction in [duplicate, reuse_exploded, reuse_payload] {
        assert!(has_error_at(&errors, instruction, |kind| matches!(
            kind,
            VerifyErrorKind::ValueUnavailable { .. }
        )));
    }
}

#[test]
fn owner_drop_and_aggregate_projection_respect_nested_loans() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("loan_conflicts");
    let module = program.module_mut(module_id).expect("module must exist");
    let types = add_types(module);

    let (owner_id, owner_entry, parameters) = add_function(
        module,
        "drop_borrowed_owner",
        &[types.owner],
        Vec::new(),
        &origin,
    );
    let owner = parameters[0];
    let owner_function = module.function_mut(owner_id).expect("function must exist");
    let payload_place = place(
        append(
            owner_function,
            owner_entry,
            Operation::HeapPayloadPlace { owner },
            vec![EntityType::Place(types.payload)],
            &origin,
        )
        .1[0],
    );
    let pair_place = place(
        append(
            owner_function,
            owner_entry,
            Operation::FieldPlace {
                base: payload_place,
                field: 0,
            },
            vec![EntityType::Place(types.pair)],
            &origin,
        )
        .1[0],
    );
    let loan = append(
        owner_function,
        owner_entry,
        Operation::BorrowBegin {
            place: pair_place,
            kind: LoanKind::Shared,
        },
        vec![EntityType::Loan {
            kind: LoanKind::Shared,
            target: types.pair,
        }],
        &origin,
    )
    .1[0];
    let EntityId::Loan(loan) = loan else {
        panic!("expected loan result");
    };
    let (drop_borrowed, _) = append(
        owner_function,
        owner_entry,
        Operation::Drop { owner },
        Vec::new(),
        &origin,
    );
    append(
        owner_function,
        owner_entry,
        Operation::BorrowEnd { loan },
        Vec::new(),
        &origin,
    );
    append(
        owner_function,
        owner_entry,
        Operation::Drop { owner },
        Vec::new(),
        &origin,
    );
    terminate(owner_function, owner_entry, &origin);

    let (project_id, project_entry, parameters) = add_function(
        module,
        "project_exclusively_borrowed",
        &[types.pair],
        Vec::new(),
        &origin,
    );
    let pair = parameters[0];
    let project_function = module
        .function_mut(project_id)
        .expect("function must exist");
    let root = place(
        append(
            project_function,
            project_entry,
            Operation::RootPlace { owner: pair },
            vec![EntityType::Place(types.pair)],
            &origin,
        )
        .1[0],
    );
    let field = place(
        append(
            project_function,
            project_entry,
            Operation::FieldPlace {
                base: root,
                field: 0,
            },
            vec![EntityType::Place(types.integer)],
            &origin,
        )
        .1[0],
    );
    let loan = append(
        project_function,
        project_entry,
        Operation::BorrowBegin {
            place: field,
            kind: LoanKind::Exclusive,
        },
        vec![EntityType::Loan {
            kind: LoanKind::Exclusive,
            target: types.integer,
        }],
        &origin,
    )
    .1[0];
    let EntityId::Loan(loan) = loan else {
        panic!("expected loan result");
    };
    let (project_borrowed, _) = append(
        project_function,
        project_entry,
        Operation::AggregateProject {
            aggregate: pair,
            field: 0,
        },
        vec![EntityType::Value(types.integer)],
        &origin,
    );
    append(
        project_function,
        project_entry,
        Operation::BorrowEnd { loan },
        Vec::new(),
        &origin,
    );
    append(
        project_function,
        project_entry,
        Operation::Drop { owner: pair },
        Vec::new(),
        &origin,
    );
    terminate(project_function, project_entry, &origin);

    let (copy_id, copy_entry, parameters) = add_function(
        module,
        "drop_copyable_aggregate",
        &[types.point],
        Vec::new(),
        &origin,
    );
    let point = parameters[0];
    let copy_function = module.function_mut(copy_id).expect("function must exist");
    let (drop_copyable, _) = append(
        copy_function,
        copy_entry,
        Operation::Drop { owner: point },
        Vec::new(),
        &origin,
    );
    terminate(copy_function, copy_entry, &origin);

    let errors = errors(&program);
    for instruction in [drop_borrowed, project_borrowed] {
        assert!(has_error_at(&errors, instruction, |kind| matches!(
            kind,
            VerifyErrorKind::OwnerLoanConflict { .. }
        )));
    }
    assert!(has_error_at(&errors, drop_copyable, |kind| matches!(
        kind,
        VerifyErrorKind::DropCopyable { .. }
    )));
}

#[test]
fn direct_call_consumes_move_only_aggregate_arguments() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("aggregate_call");
    let module = program.module_mut(module_id).expect("module must exist");
    let types = add_types(module);

    let (callee_id, callee_entry, parameters) =
        add_function(module, "consume_pair", &[types.pair], Vec::new(), &origin);
    let parameter = parameters[0];
    let callee = module.function_mut(callee_id).expect("callee must exist");
    append(
        callee,
        callee_entry,
        Operation::Drop { owner: parameter },
        Vec::new(),
        &origin,
    );
    terminate(callee, callee_entry, &origin);

    let (caller_id, caller_entry, parameters) = add_function(
        module,
        "reuse_after_call",
        &[types.pair],
        Vec::new(),
        &origin,
    );
    let argument = parameters[0];
    let caller = module.function_mut(caller_id).expect("caller must exist");
    append(
        caller,
        caller_entry,
        Operation::DirectCall {
            callee: callee_id,
            arguments: vec![argument],
        },
        Vec::new(),
        &origin,
    );
    let (reuse, _) = append(
        caller,
        caller_entry,
        Operation::Drop { owner: argument },
        Vec::new(),
        &origin,
    );
    terminate(caller, caller_entry, &origin);

    let errors = errors(&program);
    assert!(has_error_at(&errors, reuse, |kind| matches!(
        kind,
        VerifyErrorKind::ValueUnavailable { .. }
    )));
}
