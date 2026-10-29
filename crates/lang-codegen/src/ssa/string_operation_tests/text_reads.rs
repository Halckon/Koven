use super::*;
use crate::ssa::model::ScalarConstant;

fn read_program(bytes: &[u8], index: i32, corruption: &str) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let id = program.add_module("text-read");
    let module = program.module_mut(id).unwrap();
    let string = module.add_string_owner_type();
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let byte = module.intern_type(SsaTypeKind::Integer {
        bits: 8,
        signed: false,
    });
    let id = module
        .add_function("main", Vec::new(), origin.clone())
        .unwrap();
    let function = module.function_mut(id).unwrap();
    let entry = function.add_block(Vec::new(), origin.clone()).unwrap();
    let owner = literal(function, entry, string, bytes, &origin);
    let source = shared_loan(function, entry, owner, string, &origin);
    if corruption == "ended" {
        append(
            function,
            entry,
            Operation::BorrowEnd { loan: source },
            vec![],
            &origin,
        );
    }
    let index = value(
        append(
            function,
            entry,
            Operation::Constant(ScalarConstant::Integer(i128::from(index))),
            vec![EntityType::Value(int)],
            &origin,
        )[0],
    );
    append(
        function,
        entry,
        Operation::StringByteLength { source },
        vec![EntityType::Value(if corruption == "length-result" {
            byte
        } else {
            int
        })],
        &origin,
    );
    append(
        function,
        entry,
        Operation::StringByteAt { source, index },
        vec![EntityType::Value(if corruption == "byte-result" {
            int
        } else {
            byte
        })],
        &origin,
    );
    if corruption != "ended" {
        append(
            function,
            entry,
            Operation::BorrowEnd { loan: source },
            vec![],
            &origin,
        );
    }
    append(function, entry, Operation::Drop { owner }, vec![], &origin);
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
        .unwrap();
    program
}

#[test]
fn text_reads_reject_inactive_loans_and_wrong_scalar_results() {
    verify_program(&read_program(b"a", 0, "")).unwrap();
    for invalid in ["ended", "length-result", "byte-result"] {
        let program = read_program(b"a", 0, invalid);
        let errors = verify_program(&program).expect_err(invalid);
        if invalid == "ended" {
            for instruction in &program.modules[0].functions[0].instructions {
                if matches!(
                    instruction.operation,
                    Operation::StringByteLength { .. } | Operation::StringByteAt { .. }
                ) {
                    assert!(
                        errors.errors.iter().any(|error| error.location
                            == crate::ssa::verify::VerifyLocation::Instruction(instruction.id)
                            && matches!(error.kind, VerifyErrorKind::LoanInactive { .. })),
                        "each read must independently reject an ended loan: {errors:?}"
                    );
                }
            }
        }
        assert!(
            errors.errors.iter().any(|error| match invalid {
                "ended" => matches!(error.kind, VerifyErrorKind::LoanInactive { .. }),
                _ => matches!(error.kind, VerifyErrorKind::OperationContract { .. }),
            }),
            "{invalid}: {errors:?}"
        );
    }
}

#[test]
fn byte_read_native_checks_negative_end_and_empty_before_dereferencing() {
    use std::os::unix::process::ExitStatusExt;
    for (bytes, index, success) in [
        (b"a".as_slice(), 0, true),
        (b"a", -1, false),
        (b"a", 1, false),
        (b"a", i32::MAX, false),
        (b"", 0, false),
    ] {
        let program = read_program(bytes, index, "");
        verify_program(&program).unwrap();
        let entry = program.modules[0].functions[0].id;
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let directory = crate::native_tests::TestDirectory::create();
        let ir = directory.join("read.ll");
        let binary = directory.join("read");
        std::fs::write(&ir, llvm).unwrap();
        let linked = std::process::Command::new(crate::test_support::ir_clang())
            .arg("-Wno-override-module")
            .arg(&ir)
            .arg("-o")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = std::process::Command::new(binary).output().unwrap();
        if success {
            crate::native_tests::boxed_enum_tests::assert_success(&run, b"");
        } else {
            assert_eq!(run.status.signal(), Some(6), "{index}: {run:?}");
            assert!(String::from_utf8_lossy(&run.stderr).contains("index out of bounds"));
        }
    }
}

#[test]
fn utf8_scan_requires_exact_array_ubyte_active_shared_loan_and_int_result() {
    use crate::ssa::model::SequentialContainerKind;
    for case in [
        "valid",
        "list",
        "int-element",
        "signed-byte",
        "exclusive",
        "ended",
        "result",
    ] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("utf8-contract");
        let module = program.module_mut(module_id).unwrap();
        let int = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        });
        let byte = module.intern_type(SsaTypeKind::Integer {
            bits: 8,
            signed: case == "signed-byte",
        });
        let array = module.intern_type(SsaTypeKind::SequentialContainer {
            kind: if case == "list" {
                SequentialContainerKind::List
            } else {
                SequentialContainerKind::Array
            },
            element: if case == "int-element" { int } else { byte },
        });
        let function_id = module.add_function("scan", vec![], origin.clone()).unwrap();
        let function = module.function_mut(function_id).unwrap();
        let block = function.add_block(vec![], origin.clone()).unwrap();
        let owner = value(
            append(
                function,
                block,
                Operation::ContainerConstruct {
                    container: array,
                    elements: vec![],
                },
                vec![EntityType::Value(array)],
                &origin,
            )[0],
        );
        let EntityId::Place(place) = append(
            function,
            block,
            Operation::RootPlace { owner },
            vec![EntityType::Place(array)],
            &origin,
        )[0] else {
            panic!("place");
        };
        let kind = if case == "exclusive" {
            LoanKind::Exclusive
        } else {
            LoanKind::Shared
        };
        let source = loan(
            append(
                function,
                block,
                Operation::BorrowBegin { place, kind },
                vec![EntityType::Loan {
                    kind,
                    target: array,
                }],
                &origin,
            )[0],
        );
        if case == "ended" {
            append(
                function,
                block,
                Operation::BorrowEnd { loan: source },
                vec![],
                &origin,
            );
        }
        let (instruction, _) = function
            .append_instruction(
                block,
                Operation::Utf8ErrorOffset { source },
                vec![EntityType::Value(if case == "result" { byte } else { int })],
                origin.clone(),
            )
            .unwrap();
        if case != "ended" {
            append(
                function,
                block,
                Operation::BorrowEnd { loan: source },
                vec![],
                &origin,
            );
        }
        append(function, block, Operation::Drop { owner }, vec![], &origin);
        function
            .set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
            .unwrap();
        if case == "valid" {
            verify_program(&program).unwrap();
        } else {
            let errors = verify_program(&program).expect_err(case);
            assert!(
                errors.errors.iter().any(|error| error.location
                    == crate::ssa::verify::VerifyLocation::Instruction(instruction)
                    && if case == "ended" {
                        matches!(error.kind, VerifyErrorKind::LoanInactive { .. })
                    } else {
                        matches!(error.kind, VerifyErrorKind::OperationContract { .. })
                    }),
                "{case}: {errors:?}"
            );
        }
    }
}
