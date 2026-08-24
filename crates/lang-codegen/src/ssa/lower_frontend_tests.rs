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
    assert!(invalid_jump.typed.diagnostics().is_empty());
    let error = lower_scalar_file(
        &invalid_jump.sources,
        &invalid_jump.parsed,
        &invalid_jump.names,
        &invalid_jump.typed,
        &invalid_jump.owned,
    )
    .err()
    .expect("a jump without a lexical loop must not construct SSA");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());

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
