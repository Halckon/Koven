use super::*;

#[test]
fn nullable_string_equality_requires_an_active_loan_and_string_layout() {
    for (string_layout, ended) in [(true, false), (true, true), (false, false)] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("nullable-string-equality");
        let module = program.module_mut(module_id).unwrap();
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let inner = if string_layout {
            module.add_string_owner_type()
        } else {
            boolean
        };
        let nullable = module.add_inline_nullable_type(inner).unwrap();
        let id = module
            .add_function("inspect", Vec::new(), origin.clone())
            .unwrap();
        let function = module.function_mut(id).unwrap();
        let entry = function
            .add_block(
                vec![EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: nullable,
                }],
                origin.clone(),
            )
            .unwrap();
        let operand = function.block(entry).unwrap().parameters[0];
        if ended {
            append(
                function,
                entry,
                Operation::BorrowEnd {
                    loan: loan(operand),
                },
                Vec::new(),
                &origin,
            );
        }
        append(
            function,
            entry,
            Operation::StringEqual {
                left: operand,
                right: operand,
            },
            vec![EntityType::Value(boolean)],
            &origin,
        );
        function
            .set_terminator(entry, TerminatorKind::Abort, origin)
            .unwrap();
        let result = verify_program(&program);
        if string_layout && !ended {
            assert_eq!(result, Ok(()));
        } else {
            let errors = result.expect_err("invalid string view must fail verification");
            assert!(
                errors.errors.iter().any(|error| match error.kind {
                    VerifyErrorKind::LoanInactive { loan: inactive } =>
                        ended && inactive == loan(operand),
                    VerifyErrorKind::OperationContract { .. } => !string_layout,
                    _ => false,
                }),
                "{errors:?}"
            );
        }
    }
}
