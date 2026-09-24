use lang_frontend::source::SourceMap;

use super::{
    model::{
        BinaryOperator, BlockId, ComparisonOperator, Edge, EntityId, EntityType, Function,
        FunctionId, LoanId, LoanKind, ModelError, Module, Operation, Origin, Ownership, Program,
        ScalarConstant, SequentialContainerKind, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
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
        bits: 32,
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

fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(loan) = entity else {
        panic!("expected loan, got {entity:?}");
    };
    loan
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
        Operation::ContainerLength {
            owner: EntityId::Value(owner),
        },
        &[types.integer],
        &origin,
    );
    let place = append_place(
        function,
        entry,
        Operation::ContainerElementPlace {
            owner: EntityId::Value(owner),
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
fn borrowed_container_length_requires_an_active_shared_container_loan() {
    fn case(kind: LoanKind, ended: bool, container: bool, result_bits: u32) -> (Program, LoanId) {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("borrowed-length");
        let module = program.module_mut(module_id).expect("module must exist");
        let types = add_types(module);
        let owner_type = if container {
            types.array
        } else {
            types.resource
        };
        let result_type = if result_bits == 32 {
            types.integer
        } else if result_bits == 64 {
            module.intern_type(SsaTypeKind::Integer {
                bits: 64,
                signed: true,
            })
        } else {
            module.intern_type(SsaTypeKind::Boolean)
        };
        let (function_id, entry, parameters) =
            add_function(module, "length", &[owner_type], vec![owner_type], &origin);
        let function = module
            .function_mut(function_id)
            .expect("function must exist");
        let owner = parameters[0];
        let place = append_place(
            function,
            entry,
            Operation::RootPlace { owner },
            owner_type,
            &origin,
        );
        let loan = function
            .append_instruction(
                entry,
                Operation::BorrowBegin { place, kind },
                vec![EntityType::Loan {
                    kind,
                    target: owner_type,
                }],
                origin.clone(),
            )
            .expect("loan must append")
            .1;
        let [EntityId::Loan(loan)] = loan.as_slice() else {
            panic!("expected a loan result");
        };
        if ended {
            function
                .append_instruction(
                    entry,
                    Operation::BorrowEnd { loan: *loan },
                    vec![],
                    origin.clone(),
                )
                .expect("loan end must append");
        }
        append_values(
            function,
            entry,
            Operation::ContainerLength {
                owner: EntityId::Loan(*loan),
            },
            &[result_type],
            &origin,
        );
        if !ended {
            function
                .append_instruction(
                    entry,
                    Operation::BorrowEnd { loan: *loan },
                    vec![],
                    origin.clone(),
                )
                .expect("loan end must append");
        }
        function
            .set_terminator(
                entry,
                TerminatorKind::Return {
                    values: vec![owner],
                },
                origin,
            )
            .expect("return must append");
        (program, *loan)
    }

    let (valid, loan) = case(LoanKind::Shared, false, true, 32);
    verify_program(&valid).expect("active shared container loan can read length");
    assert!(render_program(&valid).contains(&format!("container.length %l{}", loan.index())));

    let (ended, loan) = case(LoanKind::Shared, true, true, 32);
    assert!(has_error(&ended, |kind| matches!(
        kind,
        VerifyErrorKind::LoanInactive { loan: actual } if *actual == loan
    )));
    for invalid in [
        case(LoanKind::Exclusive, false, true, 32).0,
        case(LoanKind::Shared, false, false, 32).0,
        case(LoanKind::Shared, false, true, 0).0,
        case(LoanKind::Shared, false, true, 64).0,
    ] {
        assert!(has_error(&invalid, |kind| matches!(
            kind,
            VerifyErrorKind::OperationContract { .. }
        )));
    }
}

#[test]
fn provider_cfg_carries_owner_and_source_loan_and_ends_each_element_loan() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("provider-cfg");
    let module = program.module_mut(module_id).expect("module must exist");
    let types = add_types(module);
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let (function_id, entry, parameters) =
        add_function(module, "iterate", &[types.array], Vec::new(), &origin);
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let owner = parameters[0];
    let source_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: types.array,
    };
    let state_types = vec![
        EntityType::Value(types.array),
        source_type,
        EntityType::Value(types.integer),
        EntityType::Value(types.integer),
    ];
    let header = function
        .add_block(state_types.clone(), origin.clone())
        .expect("header");
    let body = function
        .add_block(state_types.clone(), origin.clone())
        .expect("body");
    let continue_block = function
        .add_block(state_types, origin.clone())
        .expect("continue block");
    let exit = function
        .add_block(
            vec![EntityType::Value(types.array), source_type],
            origin.clone(),
        )
        .expect("exit");
    let zero = append_values(
        function,
        entry,
        Operation::Constant(ScalarConstant::Integer(0)),
        &[types.integer],
        &origin,
    )[0];
    let one = append_values(
        function,
        entry,
        Operation::Constant(ScalarConstant::Integer(1)),
        &[types.integer],
        &origin,
    )[0];
    let source_place = append_place(
        function,
        entry,
        Operation::RootPlace { owner },
        types.array,
        &origin,
    );
    let source_loan = append_loan(function, entry, source_place, types.array, &origin);
    let length = append_values(
        function,
        entry,
        Operation::ContainerLength {
            owner: EntityId::Loan(source_loan),
        },
        &[types.integer],
        &origin,
    )[0];
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: header,
                arguments: vec![
                    EntityId::Value(owner),
                    EntityId::Loan(source_loan),
                    EntityId::Value(length),
                    EntityId::Value(zero),
                ],
            }),
            origin.clone(),
        )
        .expect("entry branch");

    let header_params = &function.block(header).expect("header").parameters;
    let (header_owner, header_loan, header_length, header_cursor) = (
        value(header_params[0]),
        loan(header_params[1]),
        value(header_params[2]),
        value(header_params[3]),
    );
    let has_next = append_values(
        function,
        header,
        Operation::Compare {
            operator: ComparisonOperator::LessThan,
            left: header_cursor,
            right: header_length,
        },
        &[boolean],
        &origin,
    )[0];
    function
        .set_terminator(
            header,
            TerminatorKind::Conditional {
                condition: has_next,
                when_true: Edge {
                    target: body,
                    arguments: vec![
                        EntityId::Value(header_owner),
                        EntityId::Loan(header_loan),
                        EntityId::Value(header_length),
                        EntityId::Value(header_cursor),
                    ],
                },
                when_false: Edge {
                    target: exit,
                    arguments: vec![EntityId::Value(header_owner), EntityId::Loan(header_loan)],
                },
            },
            origin.clone(),
        )
        .expect("header branch");

    let body_params = &function.block(body).expect("body").parameters;
    let (body_owner, body_loan, body_length, body_cursor) = (
        value(body_params[0]),
        loan(body_params[1]),
        value(body_params[2]),
        value(body_params[3]),
    );
    let element_place = append_place(
        function,
        body,
        Operation::ContainerElementPlace {
            owner: EntityId::Loan(body_loan),
            index: body_cursor,
        },
        types.resource,
        &origin,
    );
    let element_loan = append_loan(function, body, element_place, types.resource, &origin);
    function
        .append_instruction(
            body,
            Operation::BorrowEnd { loan: element_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("element loan end");
    let next = append_values(
        function,
        body,
        Operation::Binary {
            operator: BinaryOperator::Add,
            left: body_cursor,
            right: one,
        },
        &[types.integer],
        &origin,
    )[0];
    function
        .set_terminator(
            body,
            TerminatorKind::Branch(Edge {
                target: continue_block,
                arguments: vec![
                    EntityId::Value(body_owner),
                    EntityId::Loan(body_loan),
                    EntityId::Value(body_length),
                    EntityId::Value(next),
                ],
            }),
            origin.clone(),
        )
        .expect("continue edge");
    let continue_params = &function
        .block(continue_block)
        .expect("continue block")
        .parameters;
    let (continue_owner, continue_loan, continue_length, continue_cursor) = (
        value(continue_params[0]),
        loan(continue_params[1]),
        value(continue_params[2]),
        value(continue_params[3]),
    );
    function
        .set_terminator(
            continue_block,
            TerminatorKind::Branch(Edge {
                target: header,
                arguments: vec![
                    EntityId::Value(continue_owner),
                    EntityId::Loan(continue_loan),
                    EntityId::Value(continue_length),
                    EntityId::Value(continue_cursor),
                ],
            }),
            origin.clone(),
        )
        .expect("continue backedge");

    let exit_params = &function.block(exit).expect("exit").parameters;
    let (exit_owner, exit_loan) = (value(exit_params[0]), loan(exit_params[1]));
    function
        .append_instruction(
            exit,
            Operation::BorrowEnd { loan: exit_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("source loan end");
    function
        .append_instruction(
            exit,
            Operation::Drop { owner: exit_owner },
            Vec::new(),
            origin.clone(),
        )
        .expect("source drop");
    terminate(function, exit, &origin);

    verify_program(&program).expect("provider owner and loan must remain linear across CFG");
    let rendered = render_program(&program);
    assert_eq!(rendered, render_program(&program));
    assert_eq!(rendered.matches("container.length").count(), 1);
    assert_eq!(rendered.matches("container.element_place").count(), 1);
    assert!(rendered.contains("cmp.lt"));
    assert!(rendered.contains("end_borrow"));
}

#[test]
fn provider_element_loan_can_end_after_a_block_edge() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("provider-element-edge");
    let module = program.module_mut(module_id).expect("module");
    let types = add_types(module);
    let (function_id, entry, parameters) = add_function(
        module,
        "read_element",
        &[types.array, types.integer],
        Vec::new(),
        &origin,
    );
    let function = module.function_mut(function_id).expect("function");
    let owner = parameters[0];
    let source_place = append_place(
        function,
        entry,
        Operation::RootPlace { owner },
        types.array,
        &origin,
    );
    let source_loan = append_loan(function, entry, source_place, types.array, &origin);
    let element_place = append_place(
        function,
        entry,
        Operation::ContainerElementPlace {
            owner: EntityId::Loan(source_loan),
            index: parameters[1],
        },
        types.resource,
        &origin,
    );
    let element_loan = append_loan(function, entry, element_place, types.resource, &origin);
    let successor = function
        .add_block(
            vec![
                EntityType::Value(types.array),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: types.array,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: types.resource,
                },
            ],
            origin.clone(),
        )
        .expect("successor");
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: successor,
                arguments: vec![
                    EntityId::Value(owner),
                    EntityId::Loan(source_loan),
                    EntityId::Loan(element_loan),
                ],
            }),
            origin.clone(),
        )
        .expect("edge");
    let params = &function.block(successor).expect("successor").parameters;
    let (owner, source_loan, element_loan) = (value(params[0]), loan(params[1]), loan(params[2]));
    function
        .append_instruction(
            successor,
            Operation::BorrowEnd { loan: element_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("element end");
    function
        .append_instruction(
            successor,
            Operation::BorrowEnd { loan: source_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("source end");
    function
        .append_instruction(
            successor,
            Operation::Drop { owner },
            Vec::new(),
            origin.clone(),
        )
        .expect("owner drop");
    terminate(function, successor, &origin);

    verify_program(&program).expect("both loans must remain active across the edge");
}

#[test]
fn container_element_place_requires_koven_int_index() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("container-long-index");
    let module = program.module_mut(module_id).expect("module");
    let types = add_types(module);
    let long = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let (function_id, entry, parameters) = add_function(
        module,
        "read",
        &[types.array, long],
        vec![types.array],
        &origin,
    );
    let function = module.function_mut(function_id).expect("function");
    append_place(
        function,
        entry,
        Operation::ContainerElementPlace {
            owner: EntityId::Value(parameters[0]),
            index: parameters[1],
        },
        types.resource,
        &origin,
    );
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![parameters[0]],
            },
            origin,
        )
        .expect("return");
    assert!(has_error(&program, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));
}

#[test]
fn generated_container_requires_int_to_element_initializer_contract() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("generated-container");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
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
            owner: EntityId::Value(parameters[0]),
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
            owner: EntityId::Value(parameters[0]),
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
