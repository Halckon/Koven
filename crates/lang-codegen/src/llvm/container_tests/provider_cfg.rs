//! Shared provider CFG and native cleanup across element layouts.
use super::*;

#[test]
fn borrowed_provider_cfg_lowers_length_once_and_cleanup_exits() {
    #[derive(Clone, Copy)]
    enum ElementCase {
        Int,
        Empty,
        MoveOnlyToken,
    }
    for (label, case) in [
        ("provider-int", ElementCase::Int),
        ("provider-empty", ElementCase::Empty),
        ("provider-token", ElementCase::MoveOnlyToken),
    ] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module(label);
        let module = program.module_mut(module_id).expect("module exists");
        let integer = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        });
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let element = match case {
            ElementCase::Int => integer,
            ElementCase::Empty => module
                .add_aggregate_type("Empty", Vec::new())
                .expect("empty aggregate"),
            ElementCase::MoveOnlyToken => module.intern_type(SsaTypeKind::ZeroSized {
                name: "Token".to_owned(),
                ownership: Ownership::MoveOnly,
            }),
        };
        let array = module
            .add_sequential_container_type(SequentialContainerKind::Array, element)
            .expect("Array<Element>");
        let (iterate, entry, parameters) =
            add_function(module, "iterate", &[array, boolean], vec![array], &origin);
        let function = module.function_mut(iterate).expect("iterate exists");
        let source_type = EntityType::Loan {
            kind: LoanKind::Shared,
            target: array,
        };
        let state_types = vec![
            EntityType::Value(array),
            source_type,
            EntityType::Value(integer),
            EntityType::Value(integer),
            EntityType::Value(boolean),
        ];
        let header = function
            .add_block(state_types.clone(), origin.clone())
            .expect("header");
        let body = function
            .add_block(state_types, origin.clone())
            .expect("body");
        let exit = function
            .add_block(vec![EntityType::Value(array), source_type], origin.clone())
            .expect("exit");
        let early_return = if matches!(case, ElementCase::MoveOnlyToken) {
            Some(
                function
                    .add_block(vec![EntityType::Value(array), source_type], origin.clone())
                    .expect("early return"),
            )
        } else {
            None
        };
        let source_place = append_place(
            function,
            entry,
            Operation::RootPlace {
                owner: parameters[0],
            },
            array,
            &origin,
        );
        let source_loan = loan(
            function
                .append_instruction(
                    entry,
                    Operation::BorrowBegin {
                        place: source_place,
                        kind: LoanKind::Shared,
                    },
                    vec![source_type],
                    origin.clone(),
                )
                .expect("source loan")
                .1[0],
        );
        let snapshot = provider::snapshot(
            function,
            entry,
            source_loan,
            integer,
            &origin,
            lang_frontend::type_checking::IterationProvider::Sequential(
                lang_frontend::type_checking::SequentialContainerKind::Array,
            ),
        )
        .expect("provider snapshot");
        let (snapshot_length, snapshot_cursor) = (snapshot.length(), snapshot.cursor());
        let provider_header = provider::enter_header(
            function,
            entry,
            header,
            snapshot,
            vec![
                EntityId::Value(parameters[0]),
                EntityId::Loan(source_loan),
                EntityId::Value(snapshot_cursor),
                EntityId::Value(snapshot_length),
                EntityId::Value(parameters[1]),
            ],
            1,
            2,
            3,
            &origin,
        )
        .expect("provider entry fixes length/cursor slots");

        let header_params = &function.block(header).expect("header").parameters;
        let (header_owner, header_loan, header_length, header_cursor, header_stop) = (
            value(header_params[0]),
            loan(header_params[1]),
            value(header_params[2]),
            value(header_params[3]),
            value(header_params[4]),
        );
        let element = provider::guard_and_begin(
            function,
            &provider_header,
            boolean,
            element,
            Edge {
                target: body,
                arguments: vec![
                    EntityId::Value(header_owner),
                    EntityId::Loan(header_loan),
                    EntityId::Value(header_length),
                    EntityId::Value(header_cursor),
                    EntityId::Value(header_stop),
                ],
            },
            Edge {
                target: exit,
                arguments: vec![EntityId::Value(header_owner), EntityId::Loan(header_loan)],
            },
            &origin,
            lang_frontend::type_checking::IterationProvider::Sequential(
                lang_frontend::type_checking::SequentialContainerKind::Array,
            ),
        )
        .expect("guarded provider element");

        let body_params = &function.block(body).expect("body").parameters;
        let (body_owner, body_loan, body_length, body_cursor, body_stop) = (
            value(body_params[0]),
            loan(body_params[1]),
            value(body_params[2]),
            value(body_params[3]),
            value(body_params[4]),
        );
        let next = provider::finish_element_and_advance(function, element, integer, &origin)
            .expect("next cursor");
        let next_value = next.value();
        function
            .set_terminator(
                body,
                TerminatorKind::Conditional {
                    condition: body_stop,
                    when_true: Edge {
                        target: early_return.unwrap_or(exit),
                        arguments: vec![EntityId::Value(body_owner), EntityId::Loan(body_loan)],
                    },
                    when_false: provider_header
                        .backedge(
                            vec![
                                EntityId::Value(body_owner),
                                EntityId::Loan(body_loan),
                                EntityId::Value(next_value),
                                EntityId::Value(body_length),
                                EntityId::Value(body_stop),
                            ],
                            next,
                        )
                        .expect("provider backedge fixes length/cursor slots"),
                },
                origin.clone(),
            )
            .expect("body branch");

        let exit_params = &function.block(exit).expect("exit").parameters;
        let exit_owner = value(exit_params[0]);
        let exit_loan = loan(exit_params[1]);
        function
            .append_instruction(
                exit,
                Operation::BorrowEnd { loan: exit_loan },
                Vec::new(),
                origin.clone(),
            )
            .expect("source loan end");
        ret(function, exit, vec![exit_owner], &origin);
        if let Some(early_return) = early_return {
            let params = &function
                .block(early_return)
                .expect("early return")
                .parameters;
            let (owner, source_loan) = (value(params[0]), loan(params[1]));
            function
                .append_instruction(
                    early_return,
                    Operation::BorrowEnd { loan: source_loan },
                    Vec::new(),
                    origin.clone(),
                )
                .expect("early return source loan end");
            ret(function, early_return, vec![owner], &origin);
        }

        let entry_ops = function
            .block(entry)
            .expect("entry")
            .instructions
            .iter()
            .map(|id| &function.instruction(*id).expect("instruction").operation)
            .collect::<Vec<_>>();
        assert_eq!(
            entry_ops
                .iter()
                .filter(|op| matches!(op, Operation::ContainerLength { .. }))
                .count(),
            1
        );
        assert!(entry_ops.iter().any(|op| matches!(
            op,
            Operation::Constant(crate::ssa::model::ScalarConstant::Integer(0))
        )));
        assert!(entry_ops.iter().any(|op| matches!(
            op,
            Operation::Constant(crate::ssa::model::ScalarConstant::Integer(1))
        )));
        let TerminatorKind::Branch(entry_edge) = &function
            .block(entry)
            .expect("entry")
            .terminator
            .as_ref()
            .expect("entry branch")
            .kind
        else {
            panic!("provider must enter header once");
        };
        assert_eq!(entry_edge.target, header);
        assert_eq!(entry_edge.arguments[2], EntityId::Value(snapshot_length));
        assert_eq!(entry_edge.arguments[3], EntityId::Value(snapshot_cursor));
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &function
            .block(header)
            .expect("header")
            .terminator
            .as_ref()
            .expect("guard")
            .kind
        else {
            panic!("provider header must guard its body");
        };
        assert_eq!(when_true.target, body);
        assert_eq!(when_false.target, exit);
        let header_ops = function
            .block(header)
            .expect("header")
            .instructions
            .iter()
            .map(|id| &function.instruction(*id).expect("instruction").operation)
            .collect::<Vec<_>>();
        assert!(matches!(
            header_ops.as_slice(),
            [Operation::Compare {
                operator: crate::ssa::model::ComparisonOperator::LessThan,
                left,
                right,
            }] if *left == header_cursor && *right == header_length
        ));
        let body_ops = function
            .block(body)
            .expect("body")
            .instructions
            .iter()
            .map(|id| &function.instruction(*id).expect("instruction").operation)
            .collect::<Vec<_>>();
        assert!(matches!(
            body_ops[0],
            Operation::ContainerElementPlace { owner, index }
                if *owner == EntityId::Loan(body_loan) && *index == body_cursor
        ));
        assert!(matches!(body_ops[1], Operation::BorrowBegin { .. }));
        assert!(matches!(body_ops[2], Operation::BorrowEnd { .. }));
        assert!(matches!(
            body_ops[3],
            Operation::Binary {
                operator: crate::ssa::model::BinaryOperator::Add,
                left,
                right,
            } if *left == body_cursor && *right == provider_header.step()
        ));
        let TerminatorKind::Conditional { when_false, .. } = &function
            .block(body)
            .expect("body")
            .terminator
            .as_ref()
            .expect("body branch")
            .kind
        else {
            panic!("provider body must branch");
        };
        assert_eq!(when_false.target, header);
        assert_eq!(when_false.arguments[2], EntityId::Value(header_length));
        assert_eq!(when_false.arguments[3], EntityId::Value(next_value));

        verify_program(&program).expect("provider CFG must satisfy SSA ownership");
        let first = render_verified_program(&program).expect("provider LLVM must verify");
        let second =
            render_verified_program(&program).expect("provider LLVM must be deterministic");
        assert_eq!(first, second);
        let entry_ir = first
            .split("bb0:")
            .nth(1)
            .expect("entry block")
            .split("bb1:")
            .next()
            .expect("entry end");
        assert_eq!(entry_ir.matches("extractvalue %koven.container").count(), 1);
        let exit_header = first
            .lines()
            .find(|line| line.starts_with("bb3:"))
            .expect("exit block");
        assert!(exit_header.contains("%bb1"));
        match case {
            ElementCase::MoveOnlyToken => {
                assert!(!exit_header.contains("%p1.valid"));
                assert_eq!(first.matches("ret %koven.container").count(), 2);
            }
            ElementCase::Int | ElementCase::Empty => {
                assert!(exit_header.contains("%p1.valid"));
                assert_eq!(first.matches("ret %koven.container").count(), 1);
            }
        }
        assert!(first.contains("icmp slt i32"), "{first}");
        assert!(first.contains("icmp sge i32"), "{first}");
        match case {
            ElementCase::Int => assert!(first.contains("getelementptr i32, ptr"), "{first}"),
            ElementCase::Empty | ElementCase::MoveOnlyToken => {
                assert!(!first.contains("getelementptr"), "{first}");
            }
        }
        assert!(!first.contains("call ptr @malloc"), "{first}");
        assert!(!first.contains("@koven.iterator"), "{first}");
    }
}
