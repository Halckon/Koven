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

    let block = analyze("fun repeat(input: Int): Unit { while (true) { return } }");
    let error = lower_scalar_file(
        &block.sources,
        &block.parsed,
        &block.names,
        &block.typed,
        &block.owned,
    )
    .err()
    .expect("loop lowering belongs to the next control-flow slice");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}
