//! Complete provider CFG owner/source/element-loan transport cases.

use super::*;

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
    assert_text_eq!(rendered, render_program(&program));
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
