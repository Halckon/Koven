//! E2 Stage A: two unchanged scalar fixtures, twelve literal and twenty-three ABI roots.
//! Both legacy tests, including the L0090 rejection, remain independent and unchanged.
//! Selected-root verification and full LLVM determinism are not whole-file equivalence
//! or native execution. Strict admission failures are failures, never filtered coverage.
use lang_frontend::name_resolution::{SourceUnitKey, SymbolKind};

use super::{
    model::{
        CheckedArithmeticOperator, EntityId, EntityType, Function, FunctionId, LoanKind, Module,
        Operation, Origin, Program, ScalarConstant, SsaTypeKind, TerminatorKind,
    },
    render::render_program,
    verify::verify_program,
};
use crate::{
    llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
};

fn analyze(text: &str, root_count: usize) -> UnitSource {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict unchanged Unit admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    let [document] = unit.names().names().index().source_units() else {
        panic!("one unchanged MemoryDocument, without wrappers or source splitting");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    assert_eq!(unit.parsed().roots().len(), root_count);
    unit
}

fn lower_root(unit: &UnitSource, name: &str) -> (Program, FunctionId, String) {
    let declaration = unit.declaration(name, SymbolKind::Function);
    let origin = Origin::Source(unit.declaration_span(declaration));
    let (program, root) = unit
        .lower_entry(declaration)
        .unwrap_or_else(|error| panic!("original {name} root failed: {error:?}"));
    verify_program(&program).expect("complete original-root SSA verifier");
    let module = program.module(root.module()).expect("root module");
    let function = module.function(root).expect("actual root function");
    assert_eq!(function.origin, origin);
    assert!(!function.blocks.is_empty());
    assert_eq!(
        module
            .functions
            .iter()
            .filter(|f| f.origin == origin)
            .count(),
        1
    );
    assert_eq!(module.functions.len(), if name == "invoke" { 2 } else { 1 });
    let llvm = render_verified_program(&program).expect("complete original-root LLVM verifier");
    // Preserve the original full-text repeat-render oracle, with no normalization.
    assert_eq!(
        llvm,
        render_verified_program(&program).expect("repeat LLVM rendering")
    );
    let (repeated, repeated_root) = unit.lower_entry(declaration).expect("repeat exact root");
    verify_program(&repeated).expect("complete repeated-root SSA verifier");
    assert_eq!(root.module().index(), repeated_root.module().index());
    assert_eq!(root.index(), repeated_root.index());
    assert_eq!(
        repeated
            .module(repeated_root.module())
            .unwrap()
            .function(repeated_root)
            .unwrap()
            .origin,
        origin
    );
    assert_eq!(
        render_program(&program),
        render_program(&repeated),
        "{name}"
    );
    assert_eq!(
        llvm,
        render_verified_program(&repeated).expect("repeat complete LLVM lowering"),
        "{name}"
    );
    (program, root, llvm)
}

fn llvm_body<'a>(llvm: &'a str, function: &Function) -> &'a str {
    let marker = format!("@f{}.{}(", function.id.index(), function.name);
    let definitions = llvm
        .split("define ")
        .skip(1)
        .filter(|definition| {
            definition
                .split_once('{')
                .is_some_and(|(header, _)| header.contains(&marker))
        })
        .collect::<Vec<_>>();
    let [definition] = definitions.as_slice() else {
        panic!("unique source-selected LLVM definition {marker}");
    };
    definition.split_once("\n}").expect("LLVM function end").0
}

fn integer(bits: u16, signed: bool) -> SsaTypeKind {
    SsaTypeKind::Integer { bits, signed }
}

#[test]
fn lowers_integer_literal_boundaries_without_runtime_overflow_checks_unit_comparison() {
    let unit = analyze(LITERAL_SOURCE, 12);
    for (name, bits, signed, value, returned) in [
        ("byteMin", 8, true, -128, "ret i8 -128"),
        ("byteMax", 8, true, 127, "ret i8 127"),
        ("ubyteMax", 8, false, 255, "ret i8 -1"),
        ("shortMin", 16, true, -32768, "ret i16 -32768"),
        ("shortMax", 16, true, 32767, "ret i16 32767"),
        ("ushortMax", 16, false, 65535, "ret i16 -1"),
        ("intMin", 32, true, -2147483648, "ret i32 -2147483648"),
        ("intMax", 32, true, 2147483647, "ret i32 2147483647"),
        ("uintMax", 32, false, 4294967295, "ret i32 -1"),
        (
            "longMin",
            64,
            true,
            -9223372036854775808,
            "ret i64 -9223372036854775808",
        ),
        (
            "longMax",
            64,
            true,
            9223372036854775807,
            "ret i64 9223372036854775807",
        ),
        (
            "ulongMax",
            64,
            false,
            18446744073709551615_i128,
            "ret i64 -1",
        ),
    ] {
        let (program, root, llvm) = lower_root(&unit, name);
        let module = program.module(root.module()).unwrap();
        let function = module.function(root).unwrap();
        assert!(function.blocks[0].parameters.is_empty());
        let [result_type] = function.return_types.as_slice() else {
            panic!("one original scalar result for {name}");
        };
        assert_eq!(module.type_kind(*result_type), Some(&integer(bits, signed)));
        let constants = function
            .instructions
            .iter()
            .filter_map(|instruction| {
                if let Operation::Constant(ScalarConstant::Integer(constant)) =
                    instruction.operation
                {
                    Some((constant, instruction.results.as_slice()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let [(constant, [EntityId::Value(result)])] = constants.as_slice() else {
            panic!("one exact original integer constant for {name}");
        };
        assert_eq!(*constant, value);
        assert_eq!(
            function.entity(EntityId::Value(*result)).unwrap().ty,
            EntityType::Value(*result_type)
        );
        let returns = function
            .blocks
            .iter()
            .filter_map(|block| {
                match &block.terminator.as_ref().expect("verified terminator").kind {
                    TerminatorKind::Return { values } => Some(values.as_slice()),
                    TerminatorKind::Abort => panic!("literal boundary must not abort"),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(returns, [std::slice::from_ref(result)]);
        assert!(
            !function
                .instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::CheckedArithmetic { .. }))
        );
        let ssa = render_program(&program);
        assert!(ssa.contains(&format!("const {value}")), "{name}: {ssa}");
        assert!(!ssa.contains("checked.sub"), "{ssa}");
        assert!(!ssa.contains("abort @source"), "{ssa}");
        let body = llvm_body(&llvm, function);
        assert!(
            body.contains(returned),
            "{name}: missing {returned} in {body}"
        );
        assert!(!llvm.contains("llvm.ssub.with.overflow"), "{llvm}");
        assert!(!llvm.contains("with.overflow"), "{llvm}");
        assert!(!llvm.contains("llvm.trap"), "{llvm}");
    }
}

fn assert_borrow_abi(
    module: &Module,
    function: &Function,
    bits: u16,
    signed: bool,
    count: usize,
    name: &str,
    llvm: &str,
) {
    let result_type = if bits == 1 {
        SsaTypeKind::Boolean
    } else {
        integer(bits, signed)
    };
    if bits == 0 {
        assert!(function.return_types.is_empty());
    } else {
        let [result] = function.return_types.as_slice() else {
            panic!("one scalar result")
        };
        assert_eq!(module.type_kind(*result), Some(&result_type));
    }
    assert!(function.receiver.is_none());
    assert!(function.borrow_return.is_none());
    let parameters = &function.blocks[0].parameters;
    assert_eq!(parameters.len(), count);
    let arguments = parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let EntityId::Loan(loan) = parameter else {
                panic!("unchanged default parameter remains Borrow")
            };
            let EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            } = function.entity(*parameter).unwrap().ty
            else {
                panic!("original parameter is a shared loan")
            };
            let expected = if (name == "choose" && index == 0) || name == "unit" {
                SsaTypeKind::Boolean
            } else {
                result_type.clone()
            };
            assert_eq!(module.type_kind(target), Some(&expected));
            format!("ptr %l{}", loan.index())
        })
        .collect::<Vec<_>>()
        .join(", ");
    let result = if bits == 0 {
        "void".to_owned()
    } else {
        format!("i{bits}")
    };
    assert!(
        llvm.contains(&format!(
            "define internal {result} @f{}.{}({arguments})",
            function.id.index(),
            function.name
        )),
        "{name}: {llvm}"
    );
}

#[test]
fn lowers_verified_frontend_ssa_to_deterministic_llvm_ir_unit_comparison() {
    let unit = analyze(ABI_SOURCE, 23);
    for (name, bits, signed, parameters) in [
        ("byte", 8, true, 1),
        ("ubyte", 8, false, 1),
        ("short", 16, true, 1),
        ("ushort", 16, false, 1),
        ("int", 32, true, 1),
        ("uint", 32, false, 1),
        ("long", 64, true, 1),
        ("ulong", 64, false, 1),
        ("boolean", 1, false, 1),
        ("addSigned", 32, true, 2),
        ("addUnsigned", 32, false, 2),
        ("subtractUnsigned", 32, false, 2),
        ("multiplyUnsigned", 32, false, 2),
        ("subtract", 32, true, 2),
        ("multiply", 32, true, 2),
        ("divide", 32, true, 2),
        ("remainder", 32, true, 2),
        ("divideUnsigned", 32, false, 2),
        ("remainderUnsigned", 32, false, 2),
        ("choose", 32, true, 3),
        ("increment", 32, true, 1),
        ("invoke", 32, true, 2),
        ("unit", 0, false, 1),
    ] {
        let (program, root, llvm) = lower_root(&unit, name);
        let module = program.module(root.module()).unwrap();
        let function = module.function(root).unwrap();
        let body = llvm_body(&llvm, function);
        assert_borrow_abi(module, function, bits, signed, parameters, name, &llvm);
        assert!(llvm.contains(&format!(
            "target triple = \"{}\"",
            crate::llvm::native_target_triple().unwrap()
        )));
        assert!(!llvm.contains("invoke "), "{llvm}");
        assert!(!llvm.contains("landingpad"), "{llvm}");
        let expected = match name {
            "addSigned" | "increment" => Some((
                CheckedArithmeticOperator::Add,
                "llvm.sadd.with.overflow.i32",
            )),
            "addUnsigned" => Some((
                CheckedArithmeticOperator::Add,
                "llvm.uadd.with.overflow.i32",
            )),
            "subtractUnsigned" => Some((
                CheckedArithmeticOperator::Subtract,
                "llvm.usub.with.overflow.i32",
            )),
            "multiplyUnsigned" => Some((
                CheckedArithmeticOperator::Multiply,
                "llvm.umul.with.overflow.i32",
            )),
            "subtract" => Some((
                CheckedArithmeticOperator::Subtract,
                "llvm.ssub.with.overflow.i32",
            )),
            "multiply" => Some((
                CheckedArithmeticOperator::Multiply,
                "llvm.smul.with.overflow.i32",
            )),
            "divide" => Some((CheckedArithmeticOperator::Divide, " sdiv i32 ")),
            "remainder" => Some((CheckedArithmeticOperator::Remainder, " srem i32 ")),
            "divideUnsigned" => Some((CheckedArithmeticOperator::Divide, " udiv i32 ")),
            "remainderUnsigned" => Some((CheckedArithmeticOperator::Remainder, " urem i32 ")),
            _ => None,
        };
        let checked = function
            .instructions
            .iter()
            .filter_map(|instruction| match instruction.operation {
                Operation::CheckedArithmetic { operator, .. } => Some(operator),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            checked,
            expected
                .iter()
                .map(|(operator, _)| *operator)
                .collect::<Vec<_>>(),
            "{name}"
        );
        if let Some((_, instruction)) = expected {
            assert!(body.contains(instruction), "{name}: {body}");
            assert!(body.contains("call void @koven.abort.report"), "{body}");
            assert!(body.contains("unreachable"), "{body}");
        }
        if matches!(name, "divide" | "remainder") {
            for fragment in [
                ".zero = icmp eq i32",
                ".min = icmp eq i32",
                ", -2147483648",
                ".minus_one = icmp eq i32",
                ", -1",
                ".signed_overflow = and i1",
                " = or i1 ",
                " = select i1 ",
                "i32 1",
            ] {
                assert!(
                    body.contains(fragment),
                    "{name}: missing {fragment} in {body}"
                );
            }
        }
        if matches!(name, "divideUnsigned" | "remainderUnsigned") {
            for fragment in [" = icmp eq i32 ", ", 0", " = select i1 ", "i32 1"] {
                assert!(
                    body.contains(fragment),
                    "{name}: missing {fragment} in {body}"
                );
            }
            assert!(!body.contains(".signed_overflow"), "{body}");
        }
        if matches!(name, "choose" | "increment") {
            assert!(body.contains("phi i32"), "{body}");
            assert!(body.contains("br i1"), "{body}");
        }
        if name == "invoke" {
            let add_origin = Origin::Source(
                unit.declaration_span(unit.declaration("addSigned", SymbolKind::Function)),
            );
            let targets = module
                .functions
                .iter()
                .filter(|callee| callee.origin == add_origin)
                .collect::<Vec<_>>();
            let [target] = targets.as_slice() else {
                panic!("one original addSigned target")
            };
            let calls = function
                .instructions
                .iter()
                .filter_map(|instruction| {
                    if let Operation::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    } = &instruction.operation
                    {
                        Some((callee, receiver, arguments))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let [(callee, receiver, arguments)] = calls.as_slice() else {
                panic!("one original direct call")
            };
            assert_eq!(**callee, target.id);
            assert!(receiver.is_none());
            assert_eq!(
                arguments.as_slice(),
                function.blocks[0].parameters.as_slice()
            );
            assert!(
                body.contains(&format!(
                    "call i32 @f{}.{}(",
                    target.id.index(),
                    target.name
                )),
                "{body}"
            );
            assert_borrow_abi(module, target, 32, true, 2, "addSigned", &llvm);
        }
    }
}

// Byte-for-byte legacy fixtures: do not add own, a package, wrappers, or main.
const LITERAL_SOURCE: &str = "fun byteMin(): Byte = -128\n\
         fun byteMax(): Byte = 127\n\
         fun ubyteMax(): UByte = 255u\n\
         fun shortMin(): Short = -32768\n\
         fun shortMax(): Short = 32767\n\
         fun ushortMax(): UShort = 65535u\n\
         fun intMin(): Int = -2147483648\n\
         fun intMax(): Int = 2147483647\n\
         fun uintMax(): UInt = 4294967295u\n\
         fun longMin(): Long = -9223372036854775808L\n\
         fun longMax(): Long = 9223372036854775807L\n\
         fun ulongMax(): ULong = 18446744073709551615uL";

const ABI_SOURCE: &str = "fun byte(input: Byte): Byte = input\n\
         fun ubyte(input: UByte): UByte = input\n\
         fun short(input: Short): Short = input\n\
         fun ushort(input: UShort): UShort = input\n\
         fun int(input: Int): Int = input\n\
         fun uint(input: UInt): UInt = input\n\
         fun long(input: Long): Long = input\n\
         fun ulong(input: ULong): ULong = input\n\
         fun boolean(input: Boolean): Boolean = !input\n\
         fun addSigned(left: Int, right: Int): Int = left + right\n\
         fun addUnsigned(left: UInt, right: UInt): UInt = left + right\n\
         fun subtractUnsigned(left: UInt, right: UInt): UInt = left - right\n\
         fun multiplyUnsigned(left: UInt, right: UInt): UInt = left * right\n\
         fun subtract(left: Int, right: Int): Int = left - right\n\
         fun multiply(left: Int, right: Int): Int = left * right\n\
         fun divide(left: Int, right: Int): Int = left / right\n\
         fun remainder(left: Int, right: Int): Int = left % right\n\
         fun divideUnsigned(left: UInt, right: UInt): UInt = left / right\n\
         fun remainderUnsigned(left: UInt, right: UInt): UInt = left % right\n\
         fun choose(flag: Boolean, left: Int, right: Int): Int =\n\
             if (flag) { left } else { right }\n\
         fun increment(limit: Int): Int {\n\
             var current: Int = 0\n\
             while (current < limit) { current += 1 }\n\
             return current\n\
         }\n\
         fun invoke(left: Int, right: Int): Int = addSigned(left, right)\n\
         fun unit(flag: Boolean): Unit { if (flag) { 1 } }";
