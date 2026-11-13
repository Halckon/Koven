use crate::test_support::diagnostics::assert_text_eq;
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{CheckedArithmeticOperator, EntityId, Operation, ScalarConstant, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn replaces_a_string_owner_only_after_the_rhs_finishes() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun suffix(): String = \"!\"\n\
         fun identity(own input: String): String = input",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): String {\n\
             var current = \"old\"\n\
             { current = current + p.suffix() }\n\
             return p.identity(current)\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry_declaration = declaration(&names, "q", "entry");

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry_declaration,
    )
    .expect("root String assignment lowers to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry_declaration,
    )
    .expect("input permutation preserves assignment lowering");
    assert_text_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains("q.entry"))
        .expect("entry function exists");
    let suffix = module
        .functions
        .iter()
        .find(|function| function.name.contains("p.suffix"))
        .expect("suffix function exists")
        .id;
    let identity = module
        .functions
        .iter()
        .find(|function| function.name.contains("p.identity"))
        .expect("identity function exists")
        .id;
    let old = entry
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::StringLiteral { bytes, .. } if bytes == b"old" => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("old owner exists");
    let suffix_result = entry
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::DirectCall { callee, .. } if callee == suffix => {
                Some(instruction.results[0])
            }
            _ => None,
        })
        .expect("RHS suffix result exists");
    let (concat_index, replacement) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match instruction.operation {
            Operation::StringConcat { left, right } if [left, right] == [old, suffix_result] => {
                Some((index, instruction.results[0]))
            }
            _ => None,
        })
        .expect("RHS reads the old owner before replacement");
    assert!(matches!(
        entry.instructions[concat_index + 1].operation,
        Operation::Drop { owner } if EntityId::Value(owner) == suffix_result
    ));
    assert!(matches!(
        entry.instructions[concat_index + 2].operation,
        Operation::Drop { owner } if EntityId::Value(owner) == old
    ));

    let (identity_index, identity_result) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match &instruction.operation {
            Operation::DirectCall {
                callee, arguments, ..
            } if *callee == identity => {
                Some((index, (arguments.as_slice(), instruction.results[0])))
            }
            _ => None,
        })
        .expect("identity call exists");
    assert_eq!(identity_index, concat_index + 3);
    assert_eq!(identity_result.0, &[replacement]);
    assert!(
        !entry.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::Drop { owner } if EntityId::Value(owner) == replacement
        )),
        "the replacement owner transfers through the updated binding"
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::Drop { owner } => Some(EntityId::Value(owner)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![suffix_result, old]
    );
    let EntityId::Value(identity_result) = identity_result.1 else {
        panic!("identity returns a value");
    };
    let returned = entry
        .blocks
        .iter()
        .find_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Return { values }) => Some(values.as_slice()),
                _ => None,
            },
        )
        .expect("entry returns identity result");
    assert_eq!(returned, &[identity_result]);
}

#[test]
fn updates_the_binding_after_each_checked_compound_assignment() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun update(own input: Int): Int {\n\
             var total = input\n\
             { total += 2 }\n\
             { total -= 3 }\n\
             { total *= 4 }\n\
             { total /= 5 }\n\
             { total %= 6 }\n\
             return total\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.update(7)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "q", "entry");

    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("compound assignments lower to verified checked CFG");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("input permutation preserves compound assignment lowering");
    assert_text_eq!(render_program(&forward), render_program(&backward));

    let update = forward.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("p.update"))
        .expect("update function exists");
    let checked = update
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::CheckedArithmetic { .. }))
        .collect::<Vec<_>>();
    assert_eq!(checked.len(), 5);
    assert_eq!(
        checked
            .iter()
            .map(|instruction| match instruction.operation {
                Operation::CheckedArithmetic { operator, .. } => operator,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>(),
        vec![
            CheckedArithmeticOperator::Add,
            CheckedArithmeticOperator::Subtract,
            CheckedArithmeticOperator::Multiply,
            CheckedArithmeticOperator::Divide,
            CheckedArithmeticOperator::Remainder,
        ]
    );
    let EntityId::Value(mut expected_left) = update
        .block(update.entry_block().expect("entry block exists"))
        .expect("entry block exists")
        .parameters[0]
    else {
        panic!("input parameter is a value");
    };
    for (index, instruction) in checked.iter().enumerate() {
        let Operation::CheckedArithmetic { left, right, .. } = instruction.operation else {
            unreachable!();
        };
        assert_eq!(
            left, expected_left,
            "each assignment reads the updated binding"
        );
        let literal = update
            .instructions
            .iter()
            .find_map(|candidate| match candidate.operation {
                Operation::Constant(ScalarConstant::Integer(value))
                    if value == index as i128 + 2 =>
                {
                    Some(candidate.results[0])
                }
                _ => None,
            })
            .expect("compound RHS literal exists");
        assert_eq!(EntityId::Value(right), literal);
        let [_, EntityId::Value(failure)] = instruction.results.as_slice() else {
            panic!("checked assignment exposes value and failure");
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            ..
        } = &update
            .block(instruction.block)
            .expect("checked block exists")
            .terminator
            .as_ref()
            .expect("checked block terminates")
            .kind
        else {
            panic!("checked assignment branches on failure");
        };
        assert_eq!(condition, failure);
        assert!(
            update
                .block(when_true.target)
                .and_then(|block| block.terminator.as_ref())
                .is_some_and(|terminator| matches!(terminator.kind, TerminatorKind::Abort))
        );
        let EntityId::Value(result) = instruction.results[0] else {
            panic!("checked assignment result is a value");
        };
        expected_left = result;
    }
    let EntityId::Value(final_result) = checked[4].results[0] else {
        panic!("final compound assignment produces a value");
    };
    let returned = update
        .blocks
        .iter()
        .find_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Return { values }) if !values.is_empty() => {
                    Some(values.as_slice())
                }
                _ => None,
            },
        )
        .expect("update returns its final binding");
    assert_eq!(returned, &[final_result]);
}

#[test]
fn keeps_undefined_compound_assignment_types_outside_this_slice() {
    let source_text = "package p\n\
         fun entry(): Unit {\n\
             var text = \"left\"\n\
             { text += \"right\" }\n\
         }";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "p/main.ko", source_text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let error = match lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    ) {
        Err(error) => error,
        Ok(_) => panic!("undefined compound assignment remains an explicit boundary"),
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}

pub(in crate::ssa) mod lower_frontend_string_io_unit_comparison_tests {
    //! Original String I/O sources through strict MemoryDocument Unit admission.
    //! Original single-root interpolation rejections retain independent Unit boundary tests.
    //! Every original root is selected separately; this is not whole-file/native equivalence.
    use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};
    use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

    use super::super::{
        LoweringErrorKind,
        model::{
            AbortReason, EntityId, Function, FunctionId, Instruction, LoanKind, Operation, Origin,
            Program, ScalarConstant, TerminatorKind,
        },
        render::render_program,
        verify::verify_program,
    };
    use crate::{
        llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
    };

    fn analyze(text: &str) -> UnitSource {
        let unit =
            UnitSource::analyze("lowering.ko", text).expect("strict original Unit admission");
        unit.assert_frontend_clean();
        assert!(unit.ownership().deferred().is_empty());
        let [document] = unit.names().names().index().source_units() else {
            panic!("one unchanged MemoryDocument");
        };
        assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
        assert_eq!(document.source_id(), unit.parsed().source_id());
        assert_eq!(
            unit.sources().source_name(document.source_id()),
            Ok("lowering.ko")
        );
        assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
        unit
    }

    fn lower(unit: &UnitSource, name: &str, source: &str) -> (Program, FunctionId) {
        let declaration = unit.declaration(name, SymbolKind::Function);
        let span = unit.declaration_span(declaration);
        assert_eq!(unit.sources().slice(span), Ok(source));
        let (program, root) = unit
            .lower_entry(declaration)
            .expect("original selected Unit root");
        assert_eq!(root_function(&program, root).origin, Origin::Source(span));
        verify_program(&program).expect("original Unit SSA verifies");
        render_verified_program(&program)
            .expect("original Unit LLVM verifies, not native execution");
        (program, root)
    }

    fn root_function(program: &Program, root: FunctionId) -> &Function {
        program
            .module(root.module())
            .unwrap()
            .function(root)
            .unwrap()
    }

    #[test]
    fn dynamic_string_error_unit_comparison() {
        let unit = analyze(SOURCE_ERROR);
        let (normal, root) = lower(&unit, "normal", "fun normal(): Unit {}");
        assert!(
            root_function(&normal, root)
                .blocks
                .iter()
                .all(|block| matches!(
                    block.terminator.as_ref().unwrap().kind,
                    TerminatorKind::Return { .. }
                ))
        );
        assert!(!render_program(&normal).contains("abort @source("));
        let (program, root) = lower(
            &unit,
            "abortNow",
            &SOURCE_ERROR[SOURCE_ERROR.find("fun abortNow").unwrap()..],
        );
        let function = root_function(&program, root);
        let [block] = function.blocks.as_slice() else {
            panic!("straight-line abort")
        };
        let [entry @ EntityId::Loan(_)] = block.parameters.as_slice() else {
            panic!("original message loan")
        };
        let operations = block
            .instructions
            .iter()
            .map(|id| function.instruction(*id).unwrap())
            .collect::<Vec<_>>();
        let literals = operations
            .iter()
            .enumerate()
            .filter_map(|(at, instruction)| {
                if let Operation::StringLiteral { bytes, .. } = &instruction.operation {
                    assert_eq!(bytes, b"!");
                    Some((at, instruction.results[0]))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let [(literal_at, suffix)] = literals.as_slice() else {
            panic!("one original suffix")
        };
        let (concat_at, concat) = operations
            .iter()
            .enumerate()
            .find(|(_, instruction)| {
                instruction.operation
                    == Operation::StringConcat {
                        left: *entry,
                        right: *suffix,
                    }
            })
            .expect("concat binds actual message and suffix");
        let EntityId::Value(owner) = concat.results[0] else {
            panic!("concat owner")
        };
        let terminator = block.terminator.as_ref().unwrap();
        assert_eq!(terminator.kind, TerminatorKind::Abort);
        let Some(AbortReason::Error { message }) = terminator.abort else {
            panic!("abort uses evaluated message")
        };
        let borrow_at = assert_owner_loan(&operations, owner, message);
        assert!(*literal_at < concat_at && concat_at < borrow_at);
        assert!(
            !operations
                .iter()
                .any(|instruction| matches!(instruction.operation,
        Operation::BorrowEnd { loan } if loan == message))
        );
        assert!(
            !operations
                .iter()
                .any(|instruction| matches!(instruction.operation,
        Operation::Drop { owner: dropped } if dropped == owner))
        );
        let ssa = render_program(&program);
        assert_eq_diagnostic!(ssa.matches("abort @source(").count(), 1, "{ssa}");
        assert_diagnostic!(!ssa.contains("call @error"), "{ssa}");
        for oracle in ["string.literal", "string.concat", "borrow.shared"] {
            assert_diagnostic!(ssa.contains(oracle), "{oracle}: {ssa}");
        }
    }

    fn assert_owner_loan(
        operations: &[&Instruction],
        owner: super::super::model::ValueId,
        loan: super::super::model::LoanId,
    ) -> usize {
        let (borrow_at, place) = operations
            .iter()
            .enumerate()
            .find_map(|(at, instruction)| match instruction.operation {
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } if instruction.results == [EntityId::Loan(loan)] => Some((at, place)),
                _ => None,
            })
            .expect("exact I/O loan is a shared borrow");
        let root_at = operations
            .iter()
            .position(|instruction| {
                instruction.operation == Operation::RootPlace { owner }
                    && instruction.results == [EntityId::Place(place)]
            })
            .expect("I/O borrows the actual String owner");
        assert!(root_at < borrow_at);
        borrow_at
    }

    #[test]
    fn dynamic_string_println_unit_comparison() {
        let unit = analyze(SOURCE_PRINT);
        let (borrowed, root) = lower(
            &unit,
            "borrowed",
            "fun borrowed(input: String): Unit { println(input) }",
        );
        assert_entry_print(root_function(&borrowed, root));
        let borrowed_ssa = render_program(&borrowed);
        assert_eq!(borrowed_ssa.matches("print.string").count(), 1);
        assert_eq!(borrowed_ssa.matches("end_borrow").count(), 0);
        assert!(!borrowed_ssa.contains("print.literal"));
        let (program, root) = lower(
            &unit,
            "output",
            &SOURCE_PRINT[SOURCE_PRINT.find("fun output").unwrap()..],
        );
        let function = root_function(&program, root);
        let bytes = function
            .instructions
            .iter()
            .filter_map(|instruction| match &instruction.operation {
                Operation::StringLiteral { bytes, .. } => Some(bytes.as_slice()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected = [
            b"".as_slice(),
            b"Hello".as_slice(),
            "你好".as_bytes(),
            b"\\\'\"\n\r\t\0$".as_slice(),
        ];
        assert_eq!(bytes, expected);
        // Follow the actual constant-true CFG edges, not instruction arena order.
        let mut block = &function.blocks[0];
        let mut visited = Vec::new();
        let mut printed = Vec::new();
        loop {
            assert!(!visited.contains(&block.id), "original source has no loops");
            visited.push(block.id);
            let operations = block
                .instructions
                .iter()
                .map(|id| function.instruction(*id).unwrap())
                .collect::<Vec<_>>();
            for (print_at, instruction) in operations.iter().enumerate() {
                let Operation::PrintString { value: loan } = instruction.operation else {
                    continue;
                };
                let (literal_at, literal) = operations
                    .iter()
                    .enumerate()
                    .find(|(_, instruction)| {
                        matches!(instruction.operation, Operation::StringLiteral { .. })
                    })
                    .expect("one literal in each source if");
                let Operation::StringLiteral { bytes, .. } = &literal.operation else {
                    unreachable!()
                };
                let [EntityId::Value(owner)] = literal.results.as_slice() else {
                    panic!("literal owner")
                };
                let borrow_at = assert_owner_loan(&operations, *owner, loan);
                let end_at = operations
                    .iter()
                    .position(|instruction| instruction.operation == Operation::BorrowEnd { loan })
                    .expect("exact print loan ends");
                assert!(literal_at < borrow_at && borrow_at < print_at);
                assert_eq!(end_at, print_at + 1);
                printed.push(bytes.as_slice());
            }
            let target =
                match &block.terminator.as_ref().unwrap().kind {
                    TerminatorKind::Conditional {
                        condition,
                        when_true,
                        ..
                    } => {
                        assert!(function.instructions.iter().any(
                            |instruction| instruction.operation
                                == Operation::Constant(ScalarConstant::Boolean(true))
                                && instruction.results == [EntityId::Value(*condition)]
                        ));
                        when_true.target
                    }
                    TerminatorKind::Branch(edge) => edge.target,
                    TerminatorKind::Return { values } => {
                        assert!(values.is_empty());
                        break;
                    }
                    other => panic!("unexpected source CFG: {other:?}"),
                };
            block = function
                .blocks
                .iter()
                .find(|block| block.id == target)
                .unwrap();
        }
        assert_eq!(
            printed, expected,
            "decoded literals print in source execution order"
        );
        let ssa = render_program(&program);
        assert_eq!(ssa.matches("print.string").count(), 4);
        assert_eq!(ssa.matches("end_borrow").count(), 4);
        assert!(!ssa.contains("print.literal"));
        assert_eq!(
            borrowed_ssa.matches("print.string").count() + ssa.matches("print.string").count(),
            5
        );

        let text = "fun output(input: String): Unit { println(input) }";
        let nonliteral = analyze(text);
        let (program, root) = lower(&nonliteral, "output", text);
        let function = root_function(&program, root);
        assert_entry_print(function);
        let [EntityId::Loan(loan)] = function.blocks[0].parameters.as_slice() else {
            panic!("original nonliteral entry loan")
        };
        let ssa = render_program(&program);
        assert_diagnostic!(
            ssa.contains(&format!("print.string %l{}", loan.index())),
            "{ssa}"
        );
        assert_diagnostic!(!ssa.contains("borrow.shared"), "{ssa}");
    }

    fn assert_entry_print(function: &Function) {
        let [EntityId::Loan(loan)] = function.blocks[0].parameters.as_slice() else {
            panic!("original entry loan")
        };
        let prints = function
            .instructions
            .iter()
            .filter(|instruction| instruction.operation == Operation::PrintString { value: *loan })
            .count();
        assert_eq!(prints, 1, "println reuses the actual entry loan");
        assert!(!function.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::BorrowBegin { .. } | Operation::BorrowEnd { .. }
        )));
    }

    #[test]
    fn string_io_nested_interpolation_rejections_unit_comparison() {
        for (name, text) in [
            (
                "abortInterpolated",
                "fun abortInterpolated(): Unit { error(\"${1}\") }",
            ),
            ("output", "fun output(): Unit { println(\"${1}\") }"),
        ] {
            let unit = analyze(text);
            let declaration = unit.declaration(name, SymbolKind::Function);
            assert_eq!(
                unit.sources().slice(unit.declaration_span(declaration)),
                Ok(text)
            );
            let error = unit
                .lower_entry(declaration)
                .err()
                .expect("original nested interpolation remains unsupported");
            assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
            let span = error
                .span
                .expect("nested interpolation retains source span");
            assert_eq!(span.source_id(), unit.parsed().source_id());
            assert_eq!(unit.sources().slice(span), Ok("\"${1}\""));
            assert_eq!(span.start(), text.find('"').unwrap());
            assert_eq!(span.end(), text.rfind('"').unwrap() + 1);
        }
    }
    const SOURCE_ERROR: &str = "fun normal(): Unit {}\n\
         fun abortNow(message: String): Unit { error(message + \"!\") }";

    const SOURCE_PRINT: &str = r#"fun borrowed(input: String): Unit { println(input) }
        fun output(): Unit {
            if (true) { println("") }
            if (true) { println("Hello") }
            if (true) { println("你好") }
            if (true) { println("\\\'\"\n\r\t\0\$") }
        }"#;
}
