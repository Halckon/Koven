use lang_frontend::source::SourceMap;

use super::{
    model::{
        BlockId, EntityId, EntityType, Function, FunctionId, LoanId, LoanKind, ModelError, Module,
        Operation, Origin, Ownership, Program, SequentialContainerKind, SsaTypeId, SsaTypeKind,
        TerminatorKind, ValueId,
    },
    render::render_program,
    verify::{VerifyErrorKind, verify_program},
};

#[derive(Clone, Copy)]
struct ContainerTypes {
    integer: SsaTypeId,
    resource: SsaTypeId,
    array: SsaTypeId,
    list: SsaTypeId,
    mutable_list: SsaTypeId,
}

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("container-ssa.ko", "val items = listOf(1)")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 3).expect("test span must be valid"))
}

fn add_types(module: &mut Module) -> ContainerTypes {
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let array = module
        .add_sequential_container_type(SequentialContainerKind::Array, resource)
        .expect("Array<Resource> must be valid");
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, resource)
        .expect("List<Resource> must be valid");
    let mutable_list = module
        .add_sequential_container_type(SequentialContainerKind::MutableList, resource)
        .expect("MutableList<Resource> must be valid");
    ContainerTypes {
        integer,
        resource,
        array,
        list,
        mutable_list,
    }
}

fn add_function(
    module: &mut Module,
    name: &str,
    parameters: &[SsaTypeId],
    returns: Vec<SsaTypeId>,
    origin: &Origin,
) -> (FunctionId, BlockId, Vec<ValueId>) {
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

fn append_values(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    result_types: &[SsaTypeId],
    origin: &Origin,
) -> Vec<ValueId> {
    function
        .append_instruction(
            block,
            operation,
            result_types
                .iter()
                .copied()
                .map(EntityType::Value)
                .collect(),
            origin.clone(),
        )
        .expect("instruction must be appendable")
        .1
        .into_iter()
        .map(value)
        .collect()
}

fn append_place(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    element: SsaTypeId,
    origin: &Origin,
) -> super::model::PlaceId {
    let results = function
        .append_instruction(
            block,
            operation,
            vec![EntityType::Place(element)],
            origin.clone(),
        )
        .expect("place instruction must be appendable")
        .1;
    let [EntityId::Place(place)] = results.as_slice() else {
        panic!("expected one place result, got {results:?}");
    };
    *place
}

fn append_loan(
    function: &mut Function,
    block: BlockId,
    place: super::model::PlaceId,
    element: SsaTypeId,
    origin: &Origin,
) -> LoanId {
    let results = function
        .append_instruction(
            block,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: element,
            }],
            origin.clone(),
        )
        .expect("borrow must be appendable")
        .1;
    let [EntityId::Loan(loan)] = results.as_slice() else {
        panic!("expected one loan result, got {results:?}");
    };
    *loan
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

fn has_error(program: &Program, expected: impl Fn(&VerifyErrorKind) -> bool) -> bool {
    verify_program(program)
        .expect_err("program must fail verification")
        .errors
        .iter()
        .any(|error| expected(&error.kind))
}

#[test]
fn container_types_preserve_kind_element_identity_and_move_only_ownership() {
    let mut program = Program::default();
    let module_id = program.add_module("containers");
    let foreign_id = program.add_module("foreign");
    let foreign_element = program
        .module_mut(foreign_id)
        .expect("foreign module must exist")
        .intern_type(SsaTypeKind::Boolean);
    let module = program.module_mut(module_id).expect("module must exist");
    let types = add_types(module);

    assert_eq!(
        module.type_ownership(types.array),
        Some(Ownership::MoveOnly)
    );
    assert_eq!(module.type_ownership(types.list), Some(Ownership::MoveOnly));
    assert_eq!(
        module.type_ownership(types.mutable_list),
        Some(Ownership::MoveOnly)
    );
    assert_ne!(types.array, types.list);
    assert_ne!(types.list, types.mutable_list);
    assert_eq!(
        module
            .add_sequential_container_type(SequentialContainerKind::Array, types.resource)
            .expect("identical type must intern"),
        types.array
    );
    assert_eq!(
        module.add_sequential_container_type(SequentialContainerKind::List, foreign_element),
        Err(ModelError::WrongTypeOwner {
            expected: module_id,
            actual: foreign_id,
        })
    );

    verify_program(&program).expect("container type definitions must verify");
    let rendered = render_program(&program);
    assert_eq!(rendered, render_program(&program));
    assert!(rendered.contains("container Array<!t1>"));
    assert!(rendered.contains("container List<!t1>"));
    assert!(rendered.contains("container MutableList<!t1>"));
}

#[test]
fn verifier_rejects_corrupted_structural_container_identity_cycle() {
    let mut program = Program::default();
    let module_id = program.add_module("cyclic-container");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, integer)
        .expect("initial List<Int> must be valid");
    module.types[list.index()] = SsaTypeKind::SequentialContainer {
        kind: SequentialContainerKind::List,
        element: list,
    };

    assert!(has_error(&program, |kind| matches!(
        kind,
        VerifyErrorKind::InvalidTypeDefinition { reason }
            if *reason == "sequential container identity must not form a structural cycle"
    )));
}

#[test]
fn container_construct_length_place_replace_and_drop_verify_together() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("container-operations");
    let module = program.module_mut(module_id).expect("module must exist");
    let types = add_types(module);
    let (function_id, entry, parameters) = add_function(
        module,
        "replace",
        &[
            types.resource,
            types.resource,
            types.resource,
            types.integer,
        ],
        Vec::new(),
        &origin,
    );
    let [first, second, replacement, index] = parameters.as_slice() else {
        panic!("expected four parameters");
    };
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let owner = append_values(
        function,
        entry,
        Operation::ContainerConstruct {
            container: types.mutable_list,
            elements: vec![*first, *second],
        },
        &[types.mutable_list],
        &origin,
    )[0];
    append_values(
        function,
        entry,
        Operation::ContainerLength { owner },
        &[types.integer],
        &origin,
    );
    let place = append_place(
        function,
        entry,
        Operation::ContainerElementPlace {
            owner,
            index: *index,
        },
        types.resource,
        &origin,
    );
    let loan = append_loan(function, entry, place, types.resource, &origin);
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("borrow end must append");
    function
        .append_instruction(
            entry,
            Operation::ContainerReplace {
                owner,
                index: *index,
                value: *replacement,
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("replacement must append");
    function
        .append_instruction(entry, Operation::Drop { owner }, Vec::new(), origin.clone())
        .expect("drop must append");
    terminate(function, entry, &origin);

    verify_program(&program).expect("complete mutable container flow must verify");
    let rendered = render_program(&program);
    assert!(rendered.contains("container.construct"));
    assert!(rendered.contains("container.length"));
    assert!(rendered.contains("container.element_place"));
    assert!(rendered.contains("container.replace"));
}

#[test]
fn generated_container_requires_int_to_element_initializer_contract() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("generated-container");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, integer)
        .expect("List<Int> must be valid");

    let (initializer, initializer_entry, parameters) =
        add_function(module, "identity", &[integer], vec![integer], &origin);
    let initializer_function = module
        .function_mut(initializer)
        .expect("initializer must exist");
    initializer_function
        .set_terminator(
            initializer_entry,
            TerminatorKind::Return {
                values: vec![parameters[0]],
            },
            origin.clone(),
        )
        .expect("initializer return must be set");

    let (main, entry, parameters) = add_function(module, "main", &[integer], Vec::new(), &origin);
    let function = module.function_mut(main).expect("main must exist");
    let owner = append_values(
        function,
        entry,
        Operation::ContainerGenerate {
            container: list,
            length: parameters[0],
            initializer,
        },
        &[list],
        &origin,
    )[0];
    function
        .append_instruction(entry, Operation::Drop { owner }, Vec::new(), origin.clone())
        .expect("drop must append");
    terminate(function, entry, &origin);

    verify_program(&program).expect("direct Int -> Int initializer must verify");
    assert!(render_program(&program).contains("container.generate"));
}

#[test]
fn immutable_list_replacement_fails_operation_contract() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("bad-container-contract");
    let module = program.module_mut(module_id).expect("module must exist");
    let types = add_types(module);
    let (function_id, entry, parameters) = add_function(
        module,
        "bad",
        &[types.list, types.resource, types.integer],
        Vec::new(),
        &origin,
    );
    let [owner, replacement, index] = parameters.as_slice() else {
        panic!("expected three parameters");
    };
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    function
        .append_instruction(
            entry,
            Operation::ContainerReplace {
                owner: *owner,
                index: *index,
                value: *replacement,
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("malformed operation can be represented for verifier testing");
    terminate(function, entry, &origin);

    assert!(has_error(&program, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));
}

#[test]
fn move_after_drop_and_replacement_during_element_loan_are_rejected() {
    let origin = origin();
    let mut moved = Program::default();
    let module_id = moved.add_module("moved-container");
    let module = moved.module_mut(module_id).expect("module must exist");
    let types = add_types(module);
    let (function_id, entry, parameters) = add_function(
        module,
        "moved",
        &[types.array, types.integer],
        Vec::new(),
        &origin,
    );
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    function
        .append_instruction(
            entry,
            Operation::Drop {
                owner: parameters[0],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("drop must append");
    append_place(
        function,
        entry,
        Operation::ContainerElementPlace {
            owner: parameters[0],
            index: parameters[1],
        },
        types.resource,
        &origin,
    );
    terminate(function, entry, &origin);
    assert!(has_error(&moved, |kind| matches!(
        kind,
        VerifyErrorKind::ValueUnavailable { .. }
    )));

    let mut borrowed = Program::default();
    let module_id = borrowed.add_module("borrowed-container");
    let module = borrowed.module_mut(module_id).expect("module must exist");
    let types = add_types(module);
    let (function_id, entry, parameters) = add_function(
        module,
        "borrowed",
        &[types.array, types.resource, types.integer],
        Vec::new(),
        &origin,
    );
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let place = append_place(
        function,
        entry,
        Operation::ContainerElementPlace {
            owner: parameters[0],
            index: parameters[2],
        },
        types.resource,
        &origin,
    );
    let loan = append_loan(function, entry, place, types.resource, &origin);
    function
        .append_instruction(
            entry,
            Operation::ContainerReplace {
                owner: parameters[0],
                index: parameters[2],
                value: parameters[1],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("replacement must append");
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("borrow end must append");
    function
        .append_instruction(
            entry,
            Operation::Drop {
                owner: parameters[0],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("drop must append");
    terminate(function, entry, &origin);
    assert!(has_error(&borrowed, |kind| matches!(
        kind,
        VerifyErrorKind::OwnerLoanConflict { .. }
    )));
}
