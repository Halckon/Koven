use lang_frontend::{
    lexer::lex,
    name_resolution::{NameEnvironment, NameResolution, resolve_names},
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{BuiltinType, TypeEnvironment, TypedFile, check_types},
};

use super::{
    lower_frontend::{LoweringErrorKind, orchestrate::lower_scalar_file},
    model::Operation,
    render::render_program,
};
use crate::llvm::render_verified_program;

const BUILTINS: [BuiltinType; 16] = [
    BuiltinType::Byte,
    BuiltinType::Short,
    BuiltinType::Int,
    BuiltinType::Long,
    BuiltinType::UByte,
    BuiltinType::UShort,
    BuiltinType::UInt,
    BuiltinType::ULong,
    BuiltinType::Float,
    BuiltinType::Double,
    BuiltinType::Boolean,
    BuiltinType::Char,
    BuiltinType::String,
    BuiltinType::Unit,
    BuiltinType::Nothing,
    BuiltinType::Any,
];

struct Analysis {
    sources: SourceMap,
    parsed: ParsedFile,
    names: NameResolution,
    types: TypeEnvironment,
    typed: TypedFile,
    owned: OwnershipCheckedFile,
}

fn environments() -> (NameEnvironment, TypeEnvironment) {
    let mut names = NameEnvironment::new();
    let declarations = BUILTINS.map(|builtin| {
        (
            names.declare_type(builtin.name()).expect("builtin name"),
            builtin,
        )
    });
    let mut types = TypeEnvironment::new(&names);
    for (symbol, builtin) in declarations {
        types
            .bind_builtin(symbol, builtin)
            .expect("builtin binding");
    }
    (names, types)
}

fn analyze(text: &str) -> Analysis {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("lowering.ko", text)
        .expect("source must be unique");
    let lexed = lex(&sources, source).expect("lexing must succeed internally");
    let parsed = parse_file(&sources, &lexed).expect("parsing must succeed internally");
    let (environment, types) = environments();
    let names =
        resolve_names(&sources, &parsed, &environment).expect("names must resolve internally");
    let typed =
        check_types(&sources, &parsed, &names, &types).expect("types must check internally");
    let owned = check_ownership(&sources, &parsed, &names, &typed)
        .expect("ownership must check internally");
    Analysis {
        sources,
        parsed,
        names,
        types,
        typed,
        owned,
    }
}

#[test]
fn lowers_real_scalar_expression_functions_through_verified_ssa() {
    let analysis = analyze(
        "fun add(left: Int, right: Int): Int = left + right\n\
         fun negate(input: Int): Int = -input\n\
         fun ordered(left: Int, right: Int): Boolean = !(left >= right)\n\
         fun constantOrder(): Boolean = 1 < 2\n\
         fun invoke(first: Int, second: Int): Int = add(right = second, left = first)",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("closed scalar subset must lower");
    let first = render_program(&program);
    let second = render_program(&program);
    assert_eq!(first, second);
    assert!(first.contains("func \"add\""));
    assert!(first.contains("func \"negate\""));
    assert!(first.contains("func \"ordered\""));
    assert!(first.contains("func \"constantOrder\""));
    assert!(first.contains("func \"invoke\""));
    assert!(first.contains("checked.add"));
    assert!(first.contains("checked.sub"));
    assert!(first.contains("cmp.ge"));
    assert!(first.contains("not %v"));
    assert!(first.contains("call @f0("));
    assert_eq!(first.matches("abort @source").count(), 2);
}

#[test]
fn lowers_integer_literal_boundaries_without_runtime_overflow_checks() {
    let analysis = analyze(
        "fun byteMin(): Byte = -128\n\
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
         fun ulongMax(): ULong = 18446744073709551615uL",
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("integer literal boundaries must lower through verified SSA");
    let ssa = render_program(&program);
    for constant in [
        "const -128",
        "const 127",
        "const 255",
        "const -32768",
        "const 32767",
        "const 65535",
        "const -2147483648",
        "const 2147483647",
        "const 4294967295",
        "const -9223372036854775808",
        "const 9223372036854775807",
        "const 18446744073709551615",
    ] {
        assert!(ssa.contains(constant), "missing {constant} in {ssa}");
    }
    assert!(!ssa.contains("checked.sub"));
    assert!(!ssa.contains("abort @source"));

    let llvm = render_verified_program(&program).expect("literal boundary SSA must lower to LLVM");
    for (function, instruction) in [
        ("byteMin", "ret i8 -128"),
        ("byteMax", "ret i8 127"),
        ("ubyteMax", "ret i8 -1"),
        ("shortMin", "ret i16 -32768"),
        ("shortMax", "ret i16 32767"),
        ("ushortMax", "ret i16 -1"),
        ("intMin", "ret i32 -2147483648"),
        ("intMax", "ret i32 2147483647"),
        ("uintMax", "ret i32 -1"),
        ("longMin", "ret i64 -9223372036854775808"),
        ("longMax", "ret i64 9223372036854775807"),
        ("ulongMax", "ret i64 -1"),
    ] {
        let body = llvm_function_body(&llvm, function);
        assert!(
            body.contains(instruction),
            "missing {instruction} in {body}"
        );
    }
    assert!(!llvm.contains("llvm.ssub.with.overflow"));
    assert!(!llvm.contains("llvm.trap"));

    let invalid = analyze(
        "fun byteTooLow(): Byte = -129\n\
         fun ulongTooHigh(): ULong = 18446744073709551616uL",
    );
    assert_eq!(invalid.typed.diagnostics().len(), 2);
    assert!(
        invalid
            .typed
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.code().to_string() == "L0090")
    );
    let error = lower_scalar_file(
        &invalid.sources,
        &invalid.parsed,
        &invalid.names,
        &invalid.typed,
        &invalid.owned,
    )
    .err()
    .expect("out-of-range literals must be rejected before SSA construction");
    assert_eq!(error.kind, LoweringErrorKind::FrontendDiagnostics);
}

#[test]
fn lowers_reachable_scalar_generic_instances_once_and_keeps_recursive_identity() {
    let analysis = analyze(
        "fun <T> identity(own input: T): T = input\n\
         fun <T> relay(own input: T): T = identity(input)\n\
         fun <T> recurse(own input: T): T = recurse(input)\n\
         fun <T> unused(own input: T): T = input\n\
         fun useInt(input: Int): Int = relay<Int>(input)\n\
         fun useIntAgain(input: Int): Int = relay(input)\n\
         fun useLong(input: Long): Long = relay(input)\n\
         fun recursive(input: Int): Int = recurse(input)",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("reachable concrete scalar generic instances must lower");
    let rendered = render_program(&program);
    let repeated = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("repeated instance planning must lower");
    assert_eq!(rendered, render_program(&repeated));
    assert_eq!(rendered.matches("func \"identity<Int>\"").count(), 1);
    assert_eq!(rendered.matches("func \"identity<Long>\"").count(), 1);
    assert_eq!(rendered.matches("func \"relay<Int>\"").count(), 1);
    assert_eq!(rendered.matches("func \"relay<Long>\"").count(), 1);
    assert_eq!(rendered.matches("func \"recurse<Int>\"").count(), 1);
    assert!(!rendered.contains("unused<"));

    let module = &program.modules[0];
    let recursive = module
        .functions
        .iter()
        .find(|function| function.name == "recurse<Int>")
        .expect("recursive generic instance must exist");
    let recursive_body = rendered
        .split("func \"recurse<Int>\"")
        .nth(1)
        .and_then(|body| body.split("\n\n  func").next())
        .expect("recursive instance body must render");
    assert!(recursive_body.contains(&format!("call @f{}(", recursive.id.index())));
}

#[test]
fn generic_overloads_with_the_same_type_arguments_keep_distinct_targets() {
    let analysis = analyze(
        "fun <T> choose(own input: T): T = input\n\
         fun <T> choose(own input: T, fallback: Boolean): T = input\n\
         fun one(input: Int): Int = choose(input)\n\
         fun two(input: Int): Int = choose(input, true)",
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("generic overload instances must lower independently");
    let module = &program.modules[0];
    let choices = module
        .functions
        .iter()
        .filter(|function| function.name == "choose<Int>")
        .map(|function| function.id)
        .collect::<Vec<_>>();
    assert_eq!(choices.len(), 2);
    let target_of = |name: &str| {
        let function = module
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("caller must exist");
        function
            .instructions
            .iter()
            .find_map(|instruction| match instruction.operation {
                Operation::DirectCall { callee, .. } => Some(callee),
                _ => None,
            })
            .expect("caller must contain one direct call")
    };
    let one = target_of("one");
    let two = target_of("two");
    assert_ne!(one, two);
    assert!(choices.contains(&one));
    assert!(choices.contains(&two));
    let llvm = render_verified_program(&program).expect("distinct overload instances must lower");
    assert_eq!(llvm.matches("define i32 @\"f0.choose<Int>\"").count(), 1);
    assert_eq!(llvm.matches("define i32 @\"f1.choose<Int>\"").count(), 1);
    assert!(llvm.contains("call i32 @\"f0.choose<Int>\""));
    assert!(llvm.contains("call i32 @\"f1.choose<Int>\""));
}

#[test]
fn lowers_verified_frontend_ssa_to_deterministic_llvm_ir() {
    let analysis = analyze(
        "fun byte(input: Byte): Byte = input\n\
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
         fun unit(flag: Boolean): Unit { if (flag) { 1 } }",
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("frontend scalar matrix must produce verified SSA");
    let first =
        render_verified_program(&program).expect("verified SSA must lower to verified LLVM");
    let second = render_verified_program(&program).expect("repeated LLVM lowering must succeed");
    assert_eq!(first, second);
    assert!(first.contains("target triple = \"aarch64-apple-darwin\""));
    assert!(first.contains("define i8 @f0.byte(i8 %v0)"));
    assert!(first.contains("define i16 @f2.short(i16 %v0)"));
    assert!(first.contains("define i32 @f4.int(i32 %v0)"));
    assert!(first.contains("define i64 @f6.long(i64 %v0)"));
    assert!(first.contains("define i1 @f8.boolean(i1 %v0)"));
    assert!(first.contains("llvm.sadd.with.overflow.i32"));
    assert!(first.contains("llvm.uadd.with.overflow.i32"));
    assert!(first.contains("llvm.usub.with.overflow.i32"));
    assert!(first.contains("llvm.umul.with.overflow.i32"));
    assert!(first.contains("llvm.ssub.with.overflow.i32"));
    assert!(first.contains("llvm.smul.with.overflow.i32"));
    assert!(first.contains(" sdiv i32 "));
    assert!(first.contains(" udiv i32 "));
    assert!(first.contains(" srem i32 "));
    assert!(first.contains(" urem i32 "));
    assert!(first.contains("phi i32"));
    assert!(first.contains("br i1"));
    assert!(first.contains("call i32 @f9.addSigned"));
    assert!(first.contains("call void @llvm.trap()"));
    assert!(first.contains("unreachable"));
    assert!(!first.contains("invoke "));
    assert!(!first.contains("landingpad"));

    for function in ["divide", "remainder"] {
        let body = llvm_function_body(&first, function);
        assert!(body.contains(".zero = icmp eq i32"), "{body}");
        assert!(body.contains(".min = icmp eq i32"), "{body}");
        assert!(body.contains(", -2147483648"), "{body}");
        assert!(body.contains(".minus_one = icmp eq i32"), "{body}");
        assert!(body.contains(", -1"), "{body}");
        assert!(body.contains(".signed_overflow = and i1"), "{body}");
        assert!(body.contains(" = or i1 "), "{body}");
        assert!(body.contains(" = select i1 "), "{body}");
        assert!(body.contains("i32 1"), "{body}");
    }
    for function in ["divideUnsigned", "remainderUnsigned"] {
        let body = llvm_function_body(&first, function);
        assert!(body.contains(" = icmp eq i32 "), "{body}");
        assert!(body.contains(", 0"), "{body}");
        assert!(body.contains(" = select i1 "), "{body}");
        assert!(body.contains("i32 1"), "{body}");
        assert!(!body.contains(".signed_overflow"), "{body}");
    }
}

#[test]
fn rejects_mixed_analysis_chains_before_constructing_ssa() {
    let first = analyze("fun value(input: Int): Int = input");
    let second = analyze("fun value(input: Int): Int = input");
    let error = lower_scalar_file(
        &first.sources,
        &first.parsed,
        &first.names,
        &first.typed,
        &second.owned,
    )
    .err()
    .expect("foreign ownership analysis must be rejected");
    assert_eq!(error.kind, LoweringErrorKind::MismatchedSource);

    let repeated_typed = check_types(&first.sources, &first.parsed, &first.names, &first.types)
        .expect("repeated type analysis must succeed");
    let repeated_owned =
        check_ownership(&first.sources, &first.parsed, &first.names, &repeated_typed)
            .expect("ownership analysis for repeated typed product must succeed");
    let error = lower_scalar_file(
        &first.sources,
        &first.parsed,
        &first.names,
        &first.typed,
        &repeated_owned,
    )
    .err()
    .expect("ownership from another typed product must not match the original typed chain");
    assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
}

#[test]
fn lowers_straight_line_blocks_locals_assignments_and_returns() {
    let analysis = analyze(
        "fun compute(input: Int): Int {\n\
             val doubled: Int = input + input\n\
             var total: Int = doubled\n\
             { total += 1 }\n\
             return total\n\
         }\n\
         fun observe(input: Int): Unit {\n\
             val local: Int = input\n\
         }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("straight-line scalar blocks must lower");
    let rendered = render_program(&program);
    assert!(rendered.contains("func \"compute\""));
    assert!(rendered.contains("func \"observe\""));
    assert_eq!(rendered.matches("checked.add").count(), 2);
    assert_eq!(rendered.matches("abort @source").count(), 2);
}

#[test]
fn lowers_if_short_circuit_and_branch_local_updates_as_cfg() {
    let analysis = analyze(
        "fun rhs(): Boolean = true\n\
         fun choose(flag: Boolean, left: Int, right: Int): Int =\n\
             if (flag) { left } else { right }\n\
         fun conjunction(left: Boolean): Boolean = left && rhs()\n\
         fun disjunction(left: Boolean): Boolean = left || rhs()\n\
         fun update(flag: Boolean, input: Int): Int {\n\
             var total: Int = input\n\
             if (flag) { total = total + 1 } else { total = total + 2 }\n\
             return total\n\
         }\n\
         fun same(flag: Boolean, input: Int, replacement: Int): Int {\n\
             var total: Int = input\n\
             if (flag) { total = replacement } else { total = replacement }\n\
             return total\n\
         }\n\
         fun discard(flag: Boolean): Unit {\n\
             if (flag) { 1 }\n\
             when { flag -> 2 }\n\
         }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("if and short-circuit expressions must lower through verified CFG");
    let rendered = render_program(&program);
    assert!(rendered.contains("func \"choose\""));
    assert!(rendered.contains("func \"conjunction\""));
    assert!(rendered.contains("func \"disjunction\""));
    assert!(rendered.contains("func \"update\""));
    assert!(rendered.contains("func \"same\""));
    assert!(rendered.contains("func \"discard\""));
    assert!(rendered.matches("cond %v").count() >= 4);
    assert!(rendered.matches("branch bb").count() >= 8);
    assert!(rendered.contains("call @f0()"));
    assert!(rendered.contains("bb3(%v"));
    let conjunction = rendered
        .split("func \"conjunction\"")
        .nth(1)
        .and_then(|body| body.split("\n\n  func").next())
        .expect("conjunction function must render");
    assert!(
        conjunction.find("cond %v").expect("short-circuit branch")
            < conjunction.find("call @f0()").expect("right-hand call")
    );
    let same = rendered
        .split("func \"same\"")
        .nth(1)
        .and_then(|body| body.split("\n\n  func").next())
        .expect("same function must render");
    assert!(same.contains("return %v2"));
}

#[test]
fn lowers_boolean_when_subjectless_chains_and_diverging_entries() {
    let analysis = analyze(
        "fun select(flag: Boolean): Int = when (flag) {\n\
             true -> 1\n\
             false -> 2\n\
         }\n\
         fun predicate(left: Boolean, right: Boolean): Int = when {\n\
             left && right -> 1\n\
             left -> 2\n\
             else -> 3\n\
         }\n\
         fun grouped(left: Boolean, right: Boolean): Int = when {\n\
             left, right -> 7\n\
             else -> 8\n\
         }\n\
         fun early(flag: Boolean): Int = when (flag) {\n\
             true -> return 4\n\
             false -> 5\n\
         }\n\
         fun update(flag: Boolean, input: Int): Int {\n\
             var total: Int = input\n\
             when { flag -> total = 6 }\n\
             return total\n\
         }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("Boolean when forms must lower through verified CFG");
    let rendered = render_program(&program);
    assert!(rendered.contains("func \"select\""));
    assert!(rendered.contains("func \"predicate\""));
    assert!(rendered.contains("func \"grouped\""));
    assert!(rendered.contains("func \"early\""));
    assert!(rendered.contains("func \"update\""));
    assert!(rendered.matches("cond %v").count() >= 6);
    assert!(rendered.matches("return %v").count() >= 4);
    assert!(rendered.contains("bb3(%v"));
}

#[test]
fn lowers_while_loop_break_continue_and_nested_loop_targets() {
    let analysis = analyze(
        "fun increment(limit: Int): Int {\n\
             var current: Int = 0\n\
             while (current < limit) { current += 1 }\n\
             return current\n\
         }\n\
         fun exits(flag: Boolean): Int {\n\
             var result: Int = 0\n\
             loop {\n\
                 if (flag) { { result = 1 } break } else { continue }\n\
             }\n\
             return result\n\
         }\n\
         fun nested(outer: Boolean, inner: Boolean): Int {\n\
             var total: Int = 0\n\
             loop {\n\
                 while (inner) { break }\n\
                 if (outer) { { total = 2 } break } else { continue }\n\
             }\n\
             return total\n\
         }\n\
         fun spin(): Unit { loop { continue } }",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("while/loop and nearest break/continue targets must lower through verified SSA");
    let rendered = render_program(&program);
    assert!(rendered.contains("func \"increment\""));
    assert!(rendered.contains("func \"exits\""));
    assert!(rendered.contains("func \"nested\""));
    assert!(rendered.contains("func \"spin\""));
    assert!(rendered.matches("branch bb1(").count() >= 6);
    assert!(rendered.matches("bb1(%v").count() >= 3);
    let spin = rendered
        .split("func \"spin\"")
        .nth(1)
        .and_then(|body| body.split("\n\n  func").next())
        .expect("spin function must render");
    assert!(!spin.contains("return"));
}

#[test]
fn diagnostics_and_unsupported_bodies_fail_without_partial_programs() {
    let diagnostic = analyze("fun broken(input: Int): Int = missing");
    let error = lower_scalar_file(
        &diagnostic.sources,
        &diagnostic.parsed,
        &diagnostic.names,
        &diagnostic.typed,
        &diagnostic.owned,
    )
    .err()
    .expect("frontend diagnostics must gate lowering");
    assert_eq!(error.kind, LoweringErrorKind::FrontendDiagnostics);

    let numeric_when = analyze(
        "fun numeric(input: Int): Int = when (input) {\n\
             1 -> 2\n\
             else -> 3\n\
         }",
    );
    assert!(numeric_when.typed.diagnostics().is_empty());
    let error = lower_scalar_file(
        &numeric_when.sources,
        &numeric_when.parsed,
        &numeric_when.names,
        &numeric_when.typed,
        &numeric_when.owned,
    )
    .err()
    .expect("non-Boolean when remains outside the scalar control-flow slice");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());

    let invalid_jump = analyze("fun invalid(): Unit { break }");
    assert_eq!(invalid_jump.typed.diagnostics().len(), 1);
    assert_eq!(
        invalid_jump.typed.diagnostics()[0].code().to_string(),
        "L0142"
    );
    let error = lower_scalar_file(
        &invalid_jump.sources,
        &invalid_jump.parsed,
        &invalid_jump.names,
        &invalid_jump.typed,
        &invalid_jump.owned,
    )
    .err()
    .expect("Phase 2 jump diagnostics must gate SSA construction");
    assert_eq!(error.kind, LoweringErrorKind::FrontendDiagnostics);

    let for_loop = analyze("fun iterate(): Unit { for (item in 1) {} }");
    assert!(for_loop.typed.diagnostics().is_empty());
    let error = lower_scalar_file(
        &for_loop.sources,
        &for_loop.parsed,
        &for_loop.names,
        &for_loop.typed,
        &for_loop.owned,
    )
    .err()
    .expect("for lowering must wait for iterable and binding typed facts");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());

    let non_scalar_instance = analyze(
        "fun <T> identity(own input: T): T = input\n\
         fun text(own input: String): String = identity(input)",
    );
    assert!(non_scalar_instance.typed.diagnostics().is_empty());
    assert!(non_scalar_instance.owned.diagnostics().is_empty());
    let error = lower_scalar_file(
        &non_scalar_instance.sources,
        &non_scalar_instance.parsed,
        &non_scalar_instance.names,
        &non_scalar_instance.typed,
        &non_scalar_instance.owned,
    )
    .err()
    .expect("non-scalar generic instances remain outside SPEC-0034");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

fn llvm_function_body<'a>(llvm: &'a str, source_name: &str) -> &'a str {
    let marker = format!(".{source_name}(");
    llvm.split("define ")
        .skip(1)
        .find(|definition| {
            definition
                .split_once('{')
                .is_some_and(|(header, _)| header.contains(&marker))
        })
        .and_then(|definition| definition.split("\n}").next())
        .expect("LLVM function definition")
}
