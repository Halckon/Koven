//! SPEC-0305: inspect only Programs lowered from ordinary source.
use super::{
    borrow_memory_test_support::single as memory_unit, borrow_result_contract_tests::single,
    borrow_storage_contract_tests::unit, model::*, verify::verify_program,
};

fn check(program: &Program, source_index: usize, reused_arguments: bool) {
    verify_program(program).unwrap();
    crate::llvm::render_verified_program(program).unwrap();
    let module = &program.modules[0];
    for function in &module.functions {
        if function.borrow_return.is_some() || function.name.contains("entry") {
            let mut active = function.blocks[0]
                .parameters
                .iter()
                .filter_map(|entity| match entity {
                    EntityId::Loan(loan) => Some(*loan),
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>();
            for instruction in &function.instructions {
                if let Operation::BorrowCall { arguments, .. } = &instruction.operation {
                    for argument in arguments {
                        let EntityId::Loan(loan) = argument else {
                            panic!("shared operand")
                        };
                        assert!(
                            active.contains(loan),
                            "each ordinary call operand is active"
                        );
                    }
                }
                if let Operation::BorrowEnd { loan } = instruction.operation {
                    assert!(active.remove(&loan), "normal trace ends each loan once");
                }
                active.extend(
                    instruction
                        .results
                        .iter()
                        .filter_map(|entity| match entity {
                            EntityId::Loan(loan) => Some(*loan),
                            _ => None,
                        }),
                );
            }
        }
        if function.borrow_return.is_some() && function.name.contains("view") {
            assert_eq!(function.borrow_return, Some(source_index));
        }
        for instruction in &function.instructions {
            let Operation::BorrowCall {
                callee,
                arguments,
                source,
            } = &instruction.operation
            else {
                continue;
            };
            let callee = module.function(*callee).unwrap();
            assert_eq!(
                arguments[callee.borrow_return.unwrap()],
                EntityId::Loan(*source)
            );
            assert_eq!(arguments.len(), callee.blocks[0].parameters.len());
            if !function.name.contains("entry") {
                for argument in arguments {
                    let EntityId::Loan(loan) = argument else {
                        panic!("shared argument")
                    };
                    let ends = function.instructions.iter().filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == *loan)).count();
                    if function.blocks[0].parameters.contains(argument) || loan == source {
                        assert_eq!(ends, 0, "forwarded source and entry loans survive the call");
                    } else {
                        assert_eq!(ends, 1, "unrelated projected child ends at call return");
                    }
                }
                continue;
            }
            let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                panic!("shared result")
            };
            let end = |loan| {
                let ends = function.instructions.iter().enumerate().filter_map(|(i, instruction)|
                    matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan).then_some(i)).collect::<Vec<_>>();
                assert_eq!(ends.len(), 1, "one end per created loan");
                ends[0]
            };
            assert!(end(*result) < end(*source));
            for argument in arguments {
                let EntityId::Loan(loan) = argument else {
                    panic!("all Borrow")
                };
                if loan != source {
                    assert!(instruction.id.index() < end(*loan));
                    if reused_arguments {
                        assert!(
                            end(*result) < end(*loan),
                            "external auxiliary loan survives result"
                        );
                    } else {
                        assert!(
                            end(*loan) < end(*result),
                            "unrelated call loan ends before result"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn multi_parameter_borrow_single_source_matrix() {
    for case in crate::native_multi_borrow_cases::cases() {
        let program =
            memory_unit(case.source).unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        check(&program, case.source_index, case.reused_arguments);
    }
}
#[test]
fn multi_parameter_borrow_unit_source_matrix() {
    for case in crate::native_multi_borrow_cases::cases() {
        let program = unit(case.source).unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        check(&program, case.source_index, case.reused_arguments);
    }
}

#[test]
fn multi_parameter_borrow_wrapper_control_prefix_remains_unsupported() {
    for argument in ["-n", "if (true) { 1 } else { 2 }", "n + 1"] {
        let text = format!(
            "fun view(other: Int, source: String): borrow String from source = source\nfun wrap(source: String, n: Int): borrow String from source = view({argument}, source)\nfun entry() {{ val source = \"kept\"; val n = 1; borrow val item = wrap(source, n); println(item) }}"
        );
        for result in [single(&text), unit(&text)] {
            assert_eq!(
                result.err().unwrap().kind,
                super::LoweringErrorKind::UnsupportedNode
            );
        }
    }
}

#[test]
fn multi_parameter_borrow_wrong_source_and_live_source_move_remain_frontend_errors() {
    for text in [
        "fun view(other: String, source: String): borrow String from source = other\nfun entry() {}",
        "fun view(other: String, source: String): borrow String from source = source\nfun consume(own source: String) {}\nfun entry() { val other = \"other\"; val source = \"kept\"; borrow val item = view(other, source); consume(source); println(item) }",
    ] {
        // unit() validates ownership eagerly; single keeps the diagnostic as a lowering error.
        assert_eq!(
            single(text).err().unwrap().kind,
            super::LoweringErrorKind::FrontendDiagnostics
        );
    }
}

#[test]
fn multi_parameter_borrow_concrete_callable_parameter_remains_unsupported() {
    let text = "fun <T> view(source: String, other: T): borrow String from source = source\nfun <T> inspect(source: String, other: T) { borrow val item = view(source, other); println(item) }\nfun entry() { val source = \"kept\".clone(); val action: () -> Unit = { println(source) }; inspect(source, action) }";
    for result in [single(text), unit(text)] {
        assert_eq!(
            result.err().unwrap().kind,
            super::LoweringErrorKind::UnsupportedNode
        );
    }
}

#[test]
fn multi_parameter_borrow_assertion_argument_remains_unsupported() {
    let text = "class Token\nfun extra(): Token? = Token()\nfun view(other: Token, source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(extra()!!, source)\nfun entry() { val source = \"kept\"; borrow val item = wrap(source); println(item) }";
    for result in [single(text), unit(text)] {
        assert_eq!(
            result.err().unwrap().kind,
            super::LoweringErrorKind::UnsupportedNode
        );
    }
}
