//! Preserve intrinsic whole-call L0091, never an Internal or successful admission.
use super::*;
use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
    name_resolution::NameEnvironment,
    type_checking::{
        FinalTypeAdmissionError, IntrinsicCallable, TypeEnvironment,
        complete_compilation_unit_types, complete_file_types,
    },
};

fn rejected(result: Result<(), FinalTypeAdmissionError>) -> Vec<Diagnostic> {
    match result {
        Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => diagnostics,
        other => panic!("expected original source rejection, never Internal or success: {other:?}"),
    }
}

fn assert_rejection(
    text: &str,
    call: &str,
    intrinsic: &str,
    environment: &NameEnvironment,
    types: &TypeEnvironment,
    extra_errors: &[(&str, &str)],
) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "primitive-arity.ko", text);
    let call_id = expression(&sources, &file, call);
    let start = text.find(call).unwrap();
    let span = sources.span(source, start, start + call.len()).unwrap();
    let catalog = DiagnosticCodeCatalog::try_new(&["L0091"]).unwrap();
    let expected = Diagnostic::new(
        &sources,
        Severity::Error,
        catalog.resolve("L0091").unwrap(),
        format!("intrinsic '{intrinsic}' accepts at most one type argument"),
        span,
    )
    .unwrap();
    let assert_diagnostics = |raw: &[Diagnostic], complete: Vec<Diagnostic>| {
        assert_eq!(
            complete, raw,
            "retain every raw diagnostic including details"
        );
        assert_eq!(raw.len(), 1 + extra_errors.len(), "{raw:?}");
        let arity = raw
            .iter()
            .filter(|d| d.code().to_string() == "L0091")
            .collect::<Vec<_>>();
        assert_eq!(arity, [&expected]);
        let others = raw
            .iter()
            .filter(|d| d.code().to_string() != "L0091")
            .collect::<Vec<_>>();
        assert_eq!(others.len(), extra_errors.len());
        for (diagnostic, (code, token)) in others.iter().zip(extra_errors) {
            assert_eq!(diagnostic.code().to_string(), *code);
            assert_eq!(sources.slice(diagnostic.primary_span()), Ok(*token));
        }
    };
    let names = resolve_names(&sources, &file, environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &file, &names, types).unwrap();
    assert!(typed.call(call_id).is_none());
    assert!(typed.ownership_primitives().is_empty());
    assert_eq!(
        typed
            .expression_type(call_id)
            .and_then(|ty| typed.types().get(ty)),
        Some(&TypeKind::Error)
    );
    let complete =
        rejected(complete_file_types(&sources, &file, &names, types, &typed).map(|_| ()));
    assert_diagnostics(typed.diagnostics(), complete);
    for input in [
        SourceUnitInput::new("root", "primitive-arity.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, environment)
            .unwrap()
            .validate()
            .unwrap();
        let unit = names.names().index().source_units()[0].id();
        assert_eq!(names.names().index().source_units()[0].source_id(), source);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, types).unwrap();
        let call_id = UnitExpressionId::new(unit, call_id);
        assert!(typed.call(call_id).is_none());
        assert!(typed.ownership_primitives().is_empty());
        assert_eq!(
            typed
                .expression_type(call_id)
                .and_then(|ty| typed.types().get(ty)),
            Some(&UnitTypeKind::Error)
        );
        let complete = rejected(
            complete_compilation_unit_types(&sources, &inputs, &names, types, &typed).map(|_| ()),
        );
        assert_diagnostics(typed.diagnostics(), complete);
    }
}

#[test]
fn original_excess_type_arguments_preserve_exact_whole_call_diagnostics() {
    let (environment, types) = standard_environments();
    for (intrinsic, call) in [
        ("replace", "replace<Int, Int>(&a, 2)"),
        ("swap", "swap<Int, Int>(&a, &b)"),
        ("replace", "replace<Int, Int, Int>(&a, 2)"),
        ("swap", "swap<Int, Int, Int>(&a, &b)"),
    ] {
        // First two are the exact original type_ownership_primitives fixture bytes.
        let text = format!("fun run(): Unit {{\nvar a = 1\nvar b = 2\nvar flag = true\n{call}\n}}");
        assert_rejection(&text, call, intrinsic, &environment, &types, &[]);
    }
}

#[test]
fn binding_identity_not_replace_or_swap_spelling_controls_recovery() {
    let mut names = NameEnvironment::new();
    let int = names.declare_type("Int").unwrap();
    let unit = names.declare_type("Unit").unwrap();
    let exchange = names.declare_function("exchange").unwrap();
    let trade = names.declare_function("trade").unwrap();
    let mut types = TypeEnvironment::new(&names);
    types.bind_builtin(int, BuiltinType::Int).unwrap();
    types.bind_builtin(unit, BuiltinType::Unit).unwrap();
    types
        .bind_intrinsic_callable(exchange, IntrinsicCallable::Replace)
        .unwrap();
    types
        .bind_intrinsic_callable(trade, IntrinsicCallable::Swap)
        .unwrap();
    for (intrinsic, call) in [
        ("replace", "exchange<Int, Int>(&a, 2)"),
        ("swap", "trade<Int, Int>(&a, &b)"),
    ] {
        let text = format!("fun run(): Unit {{ var a = 1; var b = 2; {call} }}");
        assert_rejection(&text, call, intrinsic, &names, &types, &[]);
    }
}

#[test]
fn rejected_primitive_still_checks_value_arguments_and_independent_errors() {
    let (environment, types) = standard_environments();
    let call = "replace<Int, Int>(&a, true + false)";
    let text = format!("fun run(): Unit {{ var a = 1; {call}; val other = true + false }}");
    assert_rejection(
        &text,
        call,
        "replace",
        &environment,
        &types,
        &[("L0085", "+"), ("L0085", "+")],
    );
}

#[test]
fn source_functions_named_like_intrinsics_and_valid_primitives_still_complete() {
    let (environment, types) = standard_environments();
    for text in [
        "fun <A, B> replace(value: Int): Int = value\nfun <A, B> swap(value: Int): Int = value\nfun run(): Int = replace<Int, Int>(swap<Int, Int>(1))",
        "fun run(): Unit { var a = 1; var b = 2; replace<Int>(&a, 3); swap<Int>(&a, &b) }",
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "healthy.ko", text);
        let names = resolve_names(&sources, &file, &environment).unwrap();
        let typed = check_types(&sources, &file, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let complete = complete_file_types(&sources, &file, &names, &types, &typed).unwrap();
        assert!(complete.is_same_analysis(&typed));
        let count = if text.starts_with("fun <") { 0 } else { 2 };
        assert_eq!(typed.ownership_primitives().len(), count);
        for input in [
            SourceUnitInput::new("root", "healthy.ko", source, &file),
            SourceUnitInput::memory_document(source, &file),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let complete =
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).unwrap();
            assert!(complete.is_same_analysis(&typed));
            assert_eq!(typed.ownership_primitives().len(), count);
        }
    }
}

#[test]
fn source_generic_arity_retains_its_original_callee_span_contract() {
    let text = "fun <T> replace(value: Int): Int = value\nfun run(): Int = replace<Int, Int>(1)";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "source-call.ko", text);
    let (environment, types) = standard_environments();
    let check = |raw: &[Diagnostic], complete: Vec<Diagnostic>| {
        assert_eq!(complete, raw);
        assert_eq!(raw.len(), 1, "{raw:?}");
        assert_eq!(raw[0].code().to_string(), "L0091");
        assert_eq!(sources.slice(raw[0].primary_span()), Ok("replace"));
        assert_eq!(
            raw[0].primary_span().start(),
            text.rfind("replace").unwrap()
        );
    };
    let names = resolve_names(&sources, &file, &environment).unwrap();
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    let complete =
        rejected(complete_file_types(&sources, &file, &names, &types, &typed).map(|_| ()));
    check(typed.diagnostics(), complete);
    for input in [
        SourceUnitInput::new("root", "source-call.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        let complete = rejected(
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed).map(|_| ()),
        );
        check(typed.diagnostics(), complete);
    }
}

#[test]
fn first_explicit_type_error_is_not_hidden_by_excess_type_arguments() {
    let (environment, types) = standard_environments();
    let call = "replace<Int<Boolean>, Int>(&a, 2)";
    let text = format!("fun run(): Unit {{ var a = 1; {call} }}");
    assert_rejection(
        &text,
        call,
        "replace",
        &environment,
        &types,
        &[("L0082", "Boolean")],
    );
}

fn ordinary_external() -> lang_frontend::type_checking::EnvironmentFunction {
    use lang_frontend::type_checking::{
        EnvironmentFunction, EnvironmentParameter, EnvironmentType, ParameterMode,
    };
    EnvironmentFunction {
        parameters: vec![EnvironmentParameter {
            mode: ParameterMode::Value,
            ty: EnvironmentType::Builtin(BuiltinType::Int),
        }],
        return_type: EnvironmentType::Builtin(BuiltinType::Int),
        effects: Vec::new(),
    }
}

#[test]
fn same_spelling_non_intrinsic_external_keeps_normal_callee_arity_rejection() {
    let mut environment = NameEnvironment::new();
    let int = environment.declare_type("Int").unwrap();
    let replace = environment.declare_function("replace").unwrap();
    let mut types = TypeEnvironment::new(&environment);
    types.bind_builtin(int, BuiltinType::Int).unwrap();
    types.bind_function(replace, ordinary_external()).unwrap();
    let text = "fun run(): Int = replace<Int, Int>(1)";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "external.ko", text);
    let check = |raw: &[Diagnostic], complete: Vec<Diagnostic>| {
        assert_eq!(complete, raw);
        assert_eq!(raw.len(), 1, "{raw:?}");
        assert_eq!(raw[0].code().to_string(), "L0091");
        assert_eq!(sources.slice(raw[0].primary_span()), Ok("replace"));
        assert_eq!(raw[0].primary_span().start(), text.find("replace").unwrap());
        assert_eq!(
            raw[0].message(),
            "callable does not accept these type arguments"
        );
    };
    let names = resolve_names(&sources, &file, &environment).unwrap();
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(typed.ownership_primitives().is_empty());
    check(
        typed.diagnostics(),
        rejected(complete_file_types(&sources, &file, &names, &types, &typed).map(|_| ())),
    );
    for input in [
        SourceUnitInput::new("root", "external.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.ownership_primitives().is_empty());
        check(
            typed.diagnostics(),
            rejected(
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                    .map(|_| ()),
            ),
        );
    }
}

#[test]
fn external_overload_set_keeps_the_real_intrinsic_binding() {
    use lang_frontend::name_resolution::{ReferenceTarget, UnitReferenceTarget};
    let mut environment = NameEnvironment::new();
    let int = environment.declare_type("Int").unwrap();
    let unit = environment.declare_type("Unit").unwrap();
    let ordinary = environment.declare_function("exchangeSet").unwrap();
    let intrinsic = environment.declare_function("exchangeSet").unwrap();
    let mut types = TypeEnvironment::new(&environment);
    types.bind_builtin(int, BuiltinType::Int).unwrap();
    types.bind_builtin(unit, BuiltinType::Unit).unwrap();
    types.bind_function(ordinary, ordinary_external()).unwrap();
    types
        .bind_intrinsic_callable(intrinsic, IntrinsicCallable::Replace)
        .unwrap();
    let call = "exchangeSet<Int, Int>(&a, 2)";
    let text = format!("fun run(): Unit {{ var a = 1; {call} }}");
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "overload.ko", &text);
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(
        names
            .references()
            .iter()
            .any(|reference| matches!(reference.target(),
        ReferenceTarget::ExternalOverloadSet(ids) if ids.as_slice() == [ordinary, intrinsic]))
    );
    for input in [
        SourceUnitInput::new("root", "overload.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        assert!(names.names().references().iter().any(|reference| matches!(reference.target(),
            UnitReferenceTarget::ExternalOverloadSet(ids) if ids.as_slice() == [ordinary, intrinsic])));
    }
    assert_rejection(&text, call, "replace", &environment, &types, &[]);
}
