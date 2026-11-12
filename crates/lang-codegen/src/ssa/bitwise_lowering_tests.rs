//! Integer bitwise lowering retains typed, eager operations for both source shapes.
//! Existing test names distinguish memory/package inputs, not separate lowering engines.
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};
use lang_frontend::{
    analysis::analyze_basic_unit_ownership,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::owned_compilation_unit_view,
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use super::{
    model::Program, render::render_program, unit_lower::lower_owned_unit_with_entry,
    unit_lower_test_support::parsed,
};
use crate::llvm::render_verified_program;

const TYPES: &[&str] = &[
    "Byte", "Short", "Int", "Long", "UByte", "UShort", "UInt", "ULong",
];
const OPERATORS: &[&str] = &["and", "or", "xor", "shl", "shr", "ushr"];

fn lower_memory_unit(text: &str) -> Program {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "bitwise.ko", text);
    let inputs = [SourceUnitInput::memory_document(source, &file)];
    lower_unit(&sources, &inputs)
}

fn lower_package_unit(text: &str) -> Program {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "p/bitwise.ko", &format!("package p\n{text}"));
    let inputs = [SourceUnitInput::new("root", "p/bitwise.ko", source, &file)];
    lower_unit(&sources, &inputs)
}

fn lower_unit(sources: &SourceMap, inputs: &[SourceUnitInput<'_>]) -> Program {
    let (name_environment, type_environment) = standard_environments();
    let index = index_compilation_unit(sources, inputs).expect("valid bitwise unit input");
    let names = resolve_compilation_unit_names(sources, inputs, &index, &name_environment)
        .expect("bitwise unit names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let names = names.validate().expect("valid bitwise unit names");
    let raw = check_compilation_unit_types(sources, inputs, &names, &type_environment)
        .expect("bitwise unit types");
    assert!(raw.diagnostics().is_empty(), "{:?}", raw.diagnostics());
    complete_compilation_unit_types(sources, inputs, &names, &type_environment, &raw)
        .expect("complete bitwise unit types");
    let (typed, owned) =
        analyze_basic_unit_ownership(sources, inputs, &names, &type_environment, raw)
            .expect("bitwise unit ownership")
            .into_result()
            .expect("bitwise unit has basic type capability");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let owned = owned.validate().expect("valid bitwise unit ownership");
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| {
            declaration.name() == "entry" && declaration.kind() == SymbolKind::Function
        })
        .expect("bitwise entry exists")
        .id();
    let unit =
        owned_compilation_unit_view(sources, inputs, &names, &type_environment, &typed, &owned)
            .expect("matching bitwise unit analysis");
    lower_owned_unit_with_entry(&unit, entry)
        .expect("bitwise unit lowers")
        .0
}

fn assert_binary_matrix(lower: fn(&str) -> Program) {
    for ty in TYPES {
        for operator in OPERATORS {
            let program = lower(&format!(
                "fun entry(left: {ty}, right: {ty}): {ty} = left {operator} right"
            ));
            let ssa = render_program(&program);
            assert_diagnostic!(
                ssa.contains(&format!("bitwise.{operator}")),
                "{ty} {operator}: {ssa}"
            );
            assert_diagnostic!(!ssa.contains("checked."), "{ssa}");
            let llvm = render_verified_program(&program).expect("bitwise LLVM verifies");
            assert_diagnostic!(!llvm.contains("poison"), "{llvm}");
            assert_diagnostic!(!llvm.contains(" nsw ") && !llvm.contains(" nuw "), "{llvm}");
            assert_diagnostic!(!llvm.contains(" exact "), "{llvm}");
            let bits = match *ty {
                "Byte" | "UByte" => 8,
                "Short" | "UShort" => 16,
                "Int" | "UInt" => 32,
                _ => 64,
            };
            let instruction = match *operator {
                "shr" if !ty.starts_with('U') => "ashr",
                "shr" | "ushr" => "lshr",
                other => other,
            };
            assert_diagnostic!(llvm.contains(&format!("{instruction} i{bits}")), "{llvm}");
            if matches!(*operator, "shl" | "shr" | "ushr") {
                assert_diagnostic!(
                    llvm.contains(&format!("and i{bits} %v1, {}", bits - 1)),
                    "{llvm}"
                );
            }
        }
    }
}

#[test]
fn single_file_bitwise_lowering_all_integer_types() {
    assert_binary_matrix(lower_memory_unit);
}

#[test]
fn unit_bitwise_lowering_all_integer_types() {
    assert_binary_matrix(lower_package_unit);
}

fn assert_inv_matrix(lower: fn(&str) -> Program) {
    for ty in TYPES {
        let program = lower(&format!("fun entry(value: {ty}): {ty} = value.inv()"));
        let rendered = render_program(&program);
        assert_diagnostic!(rendered.contains("bitwise.inv"), "{rendered}");
        assert_diagnostic!(!rendered.contains("checked."), "{rendered}");
        let llvm = render_verified_program(&program).expect("integer inv LLVM verifies");
        assert_diagnostic!(llvm.contains("xor i"), "{llvm}");
    }
}

#[test]
fn single_file_bitwise_inv_lowering_all_integer_types() {
    assert_inv_matrix(lower_memory_unit);
}

#[test]
fn unit_bitwise_inv_lowering_all_integer_types() {
    assert_inv_matrix(lower_package_unit);
}

#[test]
fn bitwise_lowering_preserves_adjacent_checked_arithmetic() {
    for lower in [lower_memory_unit as fn(&str) -> Program, lower_package_unit] {
        let program = lower("fun entry(left: Int, right: Int): Int = (left shl right) + 1");
        let rendered = render_program(&program);
        assert_eq_diagnostic!(rendered.matches("bitwise.shl").count(), 1, "{rendered}");
        assert_eq_diagnostic!(rendered.matches("checked.add").count(), 1, "{rendered}");
        let llvm = render_verified_program(&program).expect("checked neighbor LLVM");
        assert_diagnostic!(llvm.contains("llvm.sadd.with.overflow.i32"), "{llvm}");
    }
}

#[test]
fn single_file_integer_index_reads_then_inverts_without_consuming_container() {
    use super::model::{EntityId, Operation, PlaceAccess};
    for (expression, inverted) in [("values[0]", false), ("values[0].inv()", true)] {
        let program =
            lower_memory_unit(&format!("fun entry(values: List<Int>): Int = {expression}"));
        let function = &program.modules[0].functions[0];
        let element = function
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::ContainerElementPlace { .. }
                )
            })
            .expect("index uses the descriptor's element place");
        assert!(matches!(
            element.operation,
            Operation::ContainerElementPlace {
                owner: EntityId::Loan(_),
                ..
            }
        ));
        let [EntityId::Place(place)] = element.results.as_slice() else {
            panic!("index must produce a place")
        };
        let read = function
            .instructions
            .iter()
            .find(|instruction| {
                matches!(
                    instruction.operation,
                    Operation::Read { source: PlaceAccess::Place(source) } if source == *place
                )
            })
            .expect("Copyable Int is read from the borrowed element");
        let inversions = function
            .instructions
            .iter()
            .filter_map(|instruction| {
                if let Operation::IntegerNot { operand } = instruction.operation {
                    Some(EntityId::Value(operand))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            inversions,
            if inverted {
                read.results.clone()
            } else {
                Vec::new()
            }
        );
        assert!(
            function
                .instructions
                .iter()
                .all(|instruction| !matches!(instruction.operation, Operation::Drop { .. }))
        );
        render_verified_program(&program)
            .expect("descriptor index and integer inversion verify through LLVM");
    }
}

pub(in crate::ssa) mod char_constant_tests {
//! SPEC-0209: Char remains a distinct Unicode scalar through SSA and LLVM.
use crate::test_support::diagnostics::{assert_diagnostic, assert_text_eq};
use lang_frontend::source::SourceMap;

use super::super::{
    model::{
        EntityId, EntityType, Operation, Origin, Program, ScalarConstant, SsaTypeKind,
        TerminatorKind,
    },
    render::render_program,
    verify::{VerifyErrorKind, verify_program},
};
use crate::llvm::render_verified_program;

fn constant_program(constant: ScalarConstant, result: SsaTypeKind) -> Program {
    let mut sources = SourceMap::new();
    let source = sources.add_source("char.ko", "fun charValue").unwrap();
    let origin = Origin::Source(sources.span(source, 0, 13).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("char");
    let module = program.module_mut(module_id).unwrap();
    let char_type = module.intern_type(SsaTypeKind::Char);
    let uint_type = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: false,
    });
    assert_ne!(char_type, uint_type, "Char must not be erased to UInt32");
    let result = module.intern_type(result);
    let function_id = module
        .add_function("char_value", vec![result], origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let entry = function.add_block(Vec::new(), origin.clone()).unwrap();
    let (_, values) = function
        .append_instruction(
            entry,
            Operation::Constant(constant),
            vec![EntityType::Value(result)],
            origin.clone(),
        )
        .unwrap();
    let EntityId::Value(value) = values[0] else {
        panic!("constant must produce value")
    };
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![value],
            },
            origin,
        )
        .unwrap();
    program
}

#[test]
fn char_constants_preserve_unicode_boundaries_and_llvm_i32() {
    for codepoint in [0, 0xd7ff, 0xe000, 0x10ffff] {
        let program = constant_program(ScalarConstant::Char(codepoint), SsaTypeKind::Char);
        verify_program(&program).expect("Unicode scalar must verify");
        let ssa = render_program(&program);
        assert_diagnostic!(ssa.contains("!t0 = char"), "{ssa}");
        assert_diagnostic!(ssa.contains(&format!("U+{codepoint:04X}")), "{ssa}");
        let llvm = render_verified_program(&program).expect("Char must lower to LLVM");
        assert_diagnostic!(llvm.contains(&format!("ret i32 {codepoint}")), "{llvm}");
        assert_text_eq!(llvm, render_verified_program(&program).unwrap());
    }
}

#[test]
fn char_constants_reject_surrogates_out_of_range_and_integer_type_erasure() {
    for (constant, ty) in [
        (ScalarConstant::Char(0xd800), SsaTypeKind::Char),
        (ScalarConstant::Char(0xdfff), SsaTypeKind::Char),
        (ScalarConstant::Char(0x110000), SsaTypeKind::Char),
        (ScalarConstant::Char(u32::MAX), SsaTypeKind::Char),
        (ScalarConstant::Integer(65), SsaTypeKind::Char),
        (
            ScalarConstant::Char(65),
            SsaTypeKind::Integer {
                bits: 32,
                signed: false,
            },
        ),
    ] {
        let program = constant_program(constant, ty);
        let failure = verify_program(&program).expect_err("invalid Char contract");
        assert!(
            failure
                .errors
                .iter()
                .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. })),
            "{failure:?}"
        );
        assert!(render_verified_program(&program).is_err());
    }
}

fn parameter_program(
    operation: impl FnOnce(super::super::model::ValueId, super::super::model::ValueId) -> Operation,
    boolean_result: bool,
) -> Program {
    let mut program = constant_program(ScalarConstant::Char(65), SsaTypeKind::Char);
    let module = &mut program.modules[0];
    let char_type = module.intern_type(SsaTypeKind::Char);
    let result_type = if boolean_result {
        module.intern_type(SsaTypeKind::Boolean)
    } else {
        char_type
    };
    let origin = module.functions[0].origin.clone();
    let identity = module.functions[0].id;
    let function_id = module
        .add_function("char_compare", vec![result_type], origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let entry = function
        .add_block(
            vec![EntityType::Value(char_type), EntityType::Value(char_type)],
            origin.clone(),
        )
        .unwrap();
    let parameters = function.block(entry).unwrap().parameters.clone();
    let EntityId::Value(left) = parameters[0] else {
        panic!("parameter")
    };
    let EntityId::Value(right) = parameters[1] else {
        panic!("parameter")
    };
    // Copy and direct-call results retain the distinct Char type through first-class values.
    let (_, copy) = function
        .append_instruction(
            entry,
            Operation::Copy { source: left },
            vec![EntityType::Value(char_type)],
            origin.clone(),
        )
        .unwrap();
    let EntityId::Value(copy) = copy[0] else {
        panic!("copy")
    };
    function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee: identity,
                receiver: None,
                arguments: Vec::new(),
            },
            vec![EntityType::Value(char_type)],
            origin.clone(),
        )
        .unwrap();
    let (_, result) = function
        .append_instruction(
            entry,
            operation(copy, right),
            vec![EntityType::Value(result_type)],
            origin.clone(),
        )
        .unwrap();
    let EntityId::Value(result) = result[0] else {
        panic!("operation")
    };
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![result],
            },
            origin,
        )
        .unwrap();
    program
}

#[test]
fn char_parameters_copy_call_and_compare_without_integer_coercion() {
    use super::super::model::ComparisonOperator;
    for (operator, llvm_operator) in [
        (ComparisonOperator::Equal, "eq"),
        (ComparisonOperator::NotEqual, "ne"),
    ] {
        let program = parameter_program(
            |left, right| Operation::Compare {
                operator,
                left,
                right,
            },
            true,
        );
        verify_program(&program).unwrap();
        let llvm = render_verified_program(&program).unwrap();
        assert_diagnostic!(
            llvm.contains(&format!("icmp {llvm_operator} i32")),
            "{llvm}"
        );
    }
}

#[test]
fn char_parameters_reject_integer_arithmetic_and_ordering() {
    use super::super::model::{BinaryOperator, ComparisonOperator};
    for operator in [
        BinaryOperator::Add,
        BinaryOperator::Subtract,
        BinaryOperator::Multiply,
    ] {
        let program = parameter_program(
            |left, right| Operation::Binary {
                operator,
                left,
                right,
            },
            false,
        );
        assert!(
            verify_program(&program)
                .unwrap_err()
                .errors
                .iter()
                .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
        );
    }
    for operator in [
        ComparisonOperator::LessThan,
        ComparisonOperator::LessThanOrEqual,
        ComparisonOperator::GreaterThan,
        ComparisonOperator::GreaterThanOrEqual,
    ] {
        let program = parameter_program(
            |left, right| Operation::Compare {
                operator,
                left,
                right,
            },
            true,
        );
        assert!(
            verify_program(&program)
                .unwrap_err()
                .errors
                .iter()
                .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
        );
    }
}
}

pub(in crate::ssa) mod multi_borrow_tests {
//! SPEC-0305: inspect only Programs lowered from ordinary source.
use super::super::{
    borrow_memory_test_support::single as memory_unit, borrow_storage_contract_tests::unit,
    model::*, verify::verify_program,
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
    for (argument, primary) in [
        ("-n", "-n"),
        ("if (true) { 1 } else { 2 }", "if (true) { 1 } else { 2 }"),
        ("n + 1", "n + 1"),
        ("when (n) { else -> n + 1 }", "n + 1"),
    ] {
        let text = format!(
            "fun view(other: Int, source: String): borrow String from source = source\nfun wrap(source: String, n: Int): borrow String from source = view({argument}, source)\nfun entry() {{ val source = \"kept\"; val n = 1; borrow val item = wrap(source, n); println(item) }}"
        );
        let with_sibling =
            format!("{text}\nfun unrelated() {{ var a = 1; var b = 2; swap(&a, &b) }}");
        for (lower, prefix_len) in [
            (memory_unit as fn(&str) -> _, 0),
            (unit, "package p\n".len()),
        ] {
            let start = text.find(primary).unwrap() + prefix_len;
            for source in [&text, &with_sibling] {
                let rejection = lower(source).err().expect(argument);
                assert_eq!(rejection.kind, super::super::LoweringErrorKind::UnsupportedNode);
                let span = rejection.span.expect("argument control split");
                assert_eq!(
                    (span.start(), span.end()),
                    (start, start + primary.len()),
                    "{argument}"
                );
            }
        }
    }
}

#[test]
fn multi_parameter_borrow_wrapper_straight_line_arguments_ignore_uncalled_swap() {
    for (parameter_type, argument) in [
        ("Boolean", "n == 1"),
        ("Int", "n and 1"),
        ("Int", "when (n) { else -> 1 }"),
    ] {
        let text = format!(
            "fun view(other: {parameter_type}, source: String): borrow String from source = source\nfun wrap(source: String, n: Int): borrow String from source = view({argument}, source)\nfun entry() {{ val source = \"kept\"; val n = 1; borrow val item = wrap(source, n); println(item) }}"
        );
        let with_sibling =
            format!("{text}\nfun unrelated() {{ var a = 1; var b = 2; swap(&a, &b) }}");
        for lower in [memory_unit, unit] {
            for source in [&text, &with_sibling] {
                let program = lower(source).unwrap_or_else(|error| panic!("{argument}: {error:?}"));
                check(&program, 1, false);
            }
        }
    }
}

#[test]
fn direct_map_borrow_return_keeps_its_straight_line_key() {
    let text = r#"fun view(source: MutableMap<Int, String>, n: Int): borrow String from source = source.requireValue(when (n) { else -> 1 })
fun entry() {
    var source = mutableMapOf<Int, String>()
    source.put(1, "kept")
    borrow val item = view(source, 1)
    println(item)
}"#;
    for result in [memory_unit(text), unit(text)] {
        let program = result.unwrap();
        check(&program, 0, false);
        assert!(program.modules[0].functions.iter().any(|function| {
            function.borrow_return == Some(0)
                && function.instructions.iter().any(|instruction| {
                    matches!(instruction.operation, Operation::MapRequireValue { .. })
                })
        }));
    }
}

#[test]
fn multi_parameter_borrow_wrong_source_and_live_source_move_remain_frontend_errors() {
    use crate::native_tests::boxed_enum_tests::unit_source::{SourceRejectionKind, UnitSource};

    for (text, code, primary) in [
        (
            "fun view(other: String, source: String): borrow String from source = other\nfun entry() {}",
            "L0162",
            "other",
        ),
        (
            "fun view(other: String, source: String): borrow String from source = source\nfun consume(own source: String) {}\nfun entry() { val other = \"other\"; val source = \"kept\"; borrow val item = view(other, source); consume(source); println(item) }",
            "L0135",
            "source",
        ),
    ] {
        let rejected = UnitSource::analyze("multi-borrow.ko", text)
            .err()
            .expect("invalid borrow cannot obtain an ownership capability");
        assert!(matches!(rejected.kind, SourceRejectionKind::Diagnostics));
        let [(stage, diagnostic)] = rejected.report.diagnostics.as_slice() else {
            panic!("{:?}", rejected.report);
        };
        assert_eq!(*stage, "ownership");
        assert_eq!(diagnostic.code().to_string(), code);
        assert_eq!(
            rejected.sources.slice(diagnostic.primary_span()).unwrap(),
            primary
        );
    }
}

#[test]
fn multi_parameter_borrow_concrete_callable_parameter_remains_unsupported() {
    let text = "fun <T> view(source: String, other: T): borrow String from source = source\nfun <T> inspect(source: String, other: T) { borrow val item = view(source, other); println(item) }\nfun entry() { val source = \"kept\".clone(); val action: () -> Unit = { println(source) }; inspect(source, action) }";
    for result in [memory_unit(text), unit(text)] {
        assert_eq!(
            result.err().unwrap().kind,
            super::super::LoweringErrorKind::UnsupportedNode
        );
    }
}

#[test]
fn multi_parameter_borrow_assertion_argument_remains_unsupported() {
    let text = "class Token\nfun extra(): Token? = Token()\nfun view(other: Token, source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(extra()!!, source)\nfun entry() { val source = \"kept\"; borrow val item = wrap(source); println(item) }";
    for result in [memory_unit(text), unit(text)] {
        assert_eq!(
            result.err().unwrap().kind,
            super::super::LoweringErrorKind::UnsupportedNode
        );
    }
}
}
