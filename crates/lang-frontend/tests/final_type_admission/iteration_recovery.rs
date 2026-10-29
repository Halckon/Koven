//! SPEC-0341 D10/E2: retain raw witnesses while closing rejected iteration roots.
use super::*;
use lang_frontend::{
    diagnostic::Diagnostic,
    name_resolution::{
        NameResolution, Namespace, ReferenceTarget, SymbolKind, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        CallableTarget, FinalTypeAdmissionError, IntrinsicTypeConstructor, IterationProvider,
        ParameterMode, SequentialContainerKind, SequentialIterationBinding, UnitCallTarget,
        UnitSequentialIterationBinding, complete_compilation_unit_types, complete_file_types,
    },
};

const TRIAL: &str = "fun choose(callback: (Int) -> Int): Int = 1
        fun choose(callback: (String) -> String): String = \"text\"
        fun needInt(n: Int): Int = n
        fun source(): List<Int> = listOf(1)
        fun run() { val chosen = choose { for (n in source()) { val x: Int = n } needInt(it) } }";

fn rejected(result: Result<(), FinalTypeAdmissionError>) -> Vec<Diagnostic> {
    match result {
        Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => diagnostics,
        other => panic!("expected source diagnostics, never Internal or success: {other:?}"),
    }
}

// Use one SourceMap and ParsedFile for all three entries so complete diagnostics,
// including labels and source identity, can be compared without normalization.
fn observe(text: &str, raw_code: Option<&str>, selected: bool) -> (SourceMap, Vec<Diagnostic>) {
    observe_with_member(text, raw_code, selected, None)
}

fn observe_with_member(
    text: &str,
    raw_code: Option<&str>,
    selected: bool,
    final_member: Option<&str>,
) -> (SourceMap, Vec<Diagnostic>) {
    let mut sources = SourceMap::new();
    sources
        .add_source("unselected.ko", "fun bad() = missing")
        .unwrap();
    let (source, file) = parsed(&mut sources, "iteration.ko", text);
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let single = check_types(&sources, &file, &names, &types).unwrap();
    assert_raw(single.diagnostics(), raw_code);
    assert_eq!(single.sequential_iterations().len(), usize::from(selected));
    if raw_code.is_some() || selected {
        assert_single_trial(&file, &names, &single, selected);
    }
    let expression_types = || {
        file.ast()
            .expressions()
            .iter()
            .map(|(id, _)| single.expression_type(id))
            .collect::<Vec<_>>()
    };
    let before = expression_types();
    let calls = single.calls().to_vec();
    let plans = single.sequential_iterations().to_vec();
    let result = complete_file_types(&sources, &file, &names, &types, &single).map(|complete| {
        assert!(std::ptr::eq(complete.types(), &single));
        assert!(complete.is_same_analysis(&single));
    });
    assert_eq!(expression_types(), before);
    assert_eq!(single.calls(), calls);
    assert_eq!(single.sequential_iterations(), plans);
    if raw_code.is_some() || selected {
        assert_single_trial(&file, &names, &single, selected);
    }
    let expected = if selected {
        result.expect("selected iteration completes");
        Vec::new()
    } else {
        rejected(result)
    };
    if raw_code.is_some() {
        assert_eq!(
            &expected[..1],
            single.diagnostics(),
            "keep the original full diagnostic"
        );
        if let Some(member) = final_member {
            assert_eq!(expected.len(), 2, "{expected:?}");
            assert_eq!(expected[1].code().to_string(), "L0080");
            assert_eq!(sources.slice(expected[1].primary_span()), Ok(member));
        } else {
            assert_eq!(expected.len(), 1, "{expected:?}");
        }
    }
    if selected {
        let plan = &single.sequential_iterations()[0];
        assert_eq!(
            single
                .calls()
                .iter()
                .filter(|call| call.expression() == plan.source())
                .count(),
            1
        );
    }
    for input in [
        SourceUnitInput::new("root", "iteration.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let unit_source = names.names().index().source_units()[0].id();
        let unit = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert_raw(unit.diagnostics(), raw_code);
        assert_eq!(unit.diagnostics(), single.diagnostics());
        assert_eq!(unit.sequential_iterations().len(), usize::from(selected));
        if raw_code.is_some() || selected {
            assert_unit_trial(&file, &names, &unit, unit_source, selected);
        }
        let before = unit.clone();
        let result = complete_compilation_unit_types(&sources, &inputs, &names, &types, &unit).map(
            |complete| {
                assert!(std::ptr::eq(complete.types(), &unit));
                assert!(complete.is_same_analysis(&unit));
            },
        );
        assert_eq!(
            unit, before,
            "admission must preserve raw facts and rollback"
        );
        assert!(unit.is_same_analysis(&before));
        if selected {
            result.expect("selected Unit iteration completes");
            let plan = &unit.sequential_iterations()[0];
            assert_eq!(plan.source().source_unit(), unit_source);
            assert_eq!(
                unit.calls()
                    .iter()
                    .filter(|call| call.expression() == plan.source())
                    .count(),
                1
            );
        } else {
            assert_eq!(
                rejected(result),
                expected,
                "full Single/rooted/MemoryDocument diagnostics"
            );
        }
    }
    (sources, expected)
}

fn assert_raw(diagnostics: &[Diagnostic], code: Option<&str>) {
    match code {
        Some(code) => {
            assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
            assert_eq!(diagnostics[0].code().to_string(), code);
        }
        None => assert!(diagnostics.is_empty(), "{diagnostics:?}"),
    }
}

#[test]
fn deferred_iteration_source_reports_initializer_without_static_callee_cascade() {
    for binding in ["n", "(n, flag)"] {
        let text = format!(
            "fun identity(n: Int): Int = n\nfun run() {{ val reference = ::identity\n for ({binding} in listOf(reference(1))) {{}} }}"
        );
        let (sources, diagnostics) = observe(&text, None, false);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code().to_string(), "L0165");
        assert_eq!(
            diagnostic.message(),
            "type checking could not complete: overload selection is unresolved"
        );
        assert_eq!(sources.slice(diagnostic.primary_span()), Ok("::identity"));
        assert_eq!(
            (
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ),
            (58, 68)
        );
        assert!(diagnostic.details().is_empty());
    }
}

#[test]
fn rejected_iteration_trials_preserve_ambiguity_and_no_match_diagnostics() {
    for (tail, code) in [("it", "L0124"), ("true", "L0123")] {
        let text = TRIAL.replace("needInt(it)", tail);
        let (sources, diagnostics) = observe(&text, Some(code), false);
        assert_eq!(sources.slice(diagnostics[0].primary_span()), Ok("choose"));
    }
}

#[test]
fn selected_iteration_trial_still_closes_committed_body_and_source_call() {
    observe(TRIAL, None, true);
}

#[test]
fn ambiguous_iteration_does_not_hide_independent_final_roots() {
    let text = format!(
        "{}\nfun separate(): Int {{ val n = 1; return n.fooBar() }}",
        TRIAL.replace("needInt(it)", "it")
    );
    observe_with_member(&text, Some("L0124"), false, Some("fooBar"));
}

fn within(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn trial_lambda(file: &ParsedFile) -> (ExpressionId, Span) {
    let lambdas = file
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Expression::Lambda { .. }))
        .map(|(id, node)| (id, node.span()))
        .collect::<Vec<_>>();
    assert_eq!(lambdas.len(), 1, "one actual contextual lambda");
    lambdas[0]
}

fn assert_single_trial(
    file: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    selected: bool,
) {
    let (lambda, span) = trial_lambda(file);
    let parameters = names
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.kind() == SymbolKind::LambdaParameter && within(span, symbol.span())
        })
        .collect::<Vec<_>>();
    assert_eq!(parameters.len(), 1, "real implicit it binding");
    let parameter = parameters[0].id();
    if !selected {
        assert_eq!(
            typed.parameter_mode(parameter),
            None,
            "trial mode rolled back"
        );
        assert_eq!(
            typed
                .symbol_type(parameter)
                .and_then(|ty| typed.types().get(ty)),
            Some(&TypeKind::Error),
            "trial parameter type rolled back"
        );
        assert_eq!(
            typed
                .expression_type(lambda)
                .and_then(|ty| typed.types().get(ty)),
            Some(&TypeKind::Error)
        );
        let nested_calls = file
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| {
                within(span, node.span()) && matches!(node.payload(), Expression::Call { .. })
            })
            .collect::<Vec<_>>();
        assert!(
            !nested_calls.is_empty(),
            "fixture contains the nested iteration source() call"
        );
        for (id, node) in nested_calls {
            let Expression::Call { arguments, .. } = node.payload() else {
                unreachable!()
            };
            for argument in arguments {
                assert_eq!(
                    typed
                        .expression_type(argument.value)
                        .and_then(|ty| typed.types().get(ty)),
                    Some(&TypeKind::Error),
                    "nested argument type rolled back"
                );
            }
            assert!(
                typed.call(id).is_none(),
                "nested call/argument mapping must roll back"
            );
            assert_eq!(
                typed
                    .expression_type(id)
                    .and_then(|ty| typed.types().get(ty)),
                Some(&TypeKind::Error)
            );
        }
        assert!(typed.calls().iter().all(|call| {
            !within(
                span,
                file.ast()
                    .expressions()
                    .get(call.expression())
                    .unwrap()
                    .span(),
            )
        }));
        return;
    }
    let int = typed.types().builtin(BuiltinType::Int).unwrap();
    assert_eq!(typed.symbol_type(parameter), Some(int));
    assert_eq!(typed.parameter_mode(parameter), Some(ParameterMode::Borrow));
    let TypeKind::Function {
        parameters,
        return_type,
        ..
    } = typed
        .types()
        .get(typed.expression_type(lambda).unwrap())
        .unwrap()
    else {
        panic!("committed lambda type")
    };
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].ty, int);
    assert_eq!(parameters[0].mode, ParameterMode::Borrow);
    assert_eq!(*return_type, int);
    let plan = &typed.sequential_iterations()[0];
    assert_eq!(
        plan.provider(),
        IterationProvider::Sequential(SequentialContainerKind::List)
    );
    assert_eq!(plan.delivery(), ParameterMode::Borrow);
    assert_eq!(plan.element_type(), int);
    assert!(
        matches!(typed.types().get(plan.source_type()), Some(TypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::List, arguments }) if arguments.as_slice() == [int])
    );
    let SequentialIterationBinding::Name(binding) = plan.binding() else {
        panic!("loop name binding")
    };
    assert_eq!(names.symbols()[binding.index()].name(), "n");
    assert_eq!(typed.symbol_type(*binding), Some(int));
    for (id, node) in file.ast().expressions().iter().filter(|(_, node)| {
        within(span, node.span()) && matches!(node.payload(), Expression::Call { .. })
    }) {
        let call = typed.call(id).expect("committed nested call");
        let Expression::Call { arguments, .. } = node.payload() else {
            unreachable!()
        };
        assert_eq!(call.arguments().len(), arguments.len());
        for (mapped, argument) in call.arguments().iter().zip(arguments) {
            assert_eq!(mapped.parameter_type(), int);
            assert_eq!(mapped.mode(), ParameterMode::Borrow);
            assert_eq!(typed.expression_type(argument.value), Some(int));
        }
    }
}

fn assert_unit_trial(
    file: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    source: SourceUnitId,
    selected: bool,
) {
    let (lambda, span) = trial_lambda(file);
    let local_names = names
        .names()
        .source_units()
        .iter()
        .find(|unit| unit.source_unit() == source)
        .unwrap()
        .resolution();
    let parameters = local_names
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.kind() == SymbolKind::LambdaParameter && within(span, symbol.span())
        })
        .collect::<Vec<_>>();
    assert_eq!(parameters.len(), 1);
    let is_parameter = |candidate: &UnitSymbolId| {
        candidate.source_unit() == source && candidate.symbol() == parameters[0].id()
    };
    let id = |local| UnitExpressionId::new(source, local);
    if !selected {
        assert!(
            !typed.body_parameter_modes().keys().any(is_parameter),
            "trial mode not published for the actual source-qualified lambda parameter"
        );
        assert!(
            !typed.body_symbol_types().keys().any(is_parameter),
            "trial parameter type not published"
        );
        assert_eq!(
            typed.expression_type(id(lambda)),
            None,
            "trial lambda not published"
        );
        let nested_calls = file
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| {
                within(span, node.span()) && matches!(node.payload(), Expression::Call { .. })
            })
            .collect::<Vec<_>>();
        assert!(!nested_calls.is_empty());
        for (local, node) in nested_calls {
            let Expression::Call { arguments, .. } = node.payload() else {
                unreachable!()
            };
            for argument in arguments {
                assert_eq!(
                    typed.expression_type(id(argument.value)),
                    None,
                    "nested argument type not published"
                );
            }
            assert!(
                typed.call(id(local)).is_none(),
                "no nested call/argument mapping published"
            );
            assert_eq!(typed.expression_type(id(local)), None);
        }
        assert!(typed.calls().iter().all(|call| {
            call.expression().source_unit() != source
                || !within(
                    span,
                    file.ast()
                        .expressions()
                        .get(call.expression().expression())
                        .unwrap()
                        .span(),
                )
        }));
        return;
    }
    let parameter = *typed
        .body_parameter_modes()
        .keys()
        .find(|candidate| is_parameter(candidate))
        .expect("committed parameter identity comes from the actual Unit producer");
    let int = typed.types().builtin(BuiltinType::Int).unwrap();
    assert_eq!(typed.symbol_type(parameter), Some(int));
    assert_eq!(
        typed.body_parameter_mode(parameter),
        Some(ParameterMode::Borrow)
    );
    let UnitTypeKind::Function {
        parameters,
        return_type,
        ..
    } = typed
        .types()
        .get(typed.expression_type(id(lambda)).unwrap())
        .unwrap()
    else {
        panic!("committed unit lambda type")
    };
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].ty(), int);
    assert_eq!(parameters[0].mode(), ParameterMode::Borrow);
    assert_eq!(*return_type, int);
    let plan = &typed.sequential_iterations()[0];
    assert_eq!(
        plan.provider(),
        IterationProvider::Sequential(SequentialContainerKind::List)
    );
    assert_eq!(plan.delivery(), ParameterMode::Borrow);
    assert_eq!(plan.element_type(), int);
    assert!(
        matches!(typed.types().get(plan.source_type()), Some(UnitTypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::List, arguments }) if arguments.as_slice() == [int])
    );
    let UnitSequentialIterationBinding::Name(binding) = plan.binding() else {
        panic!("unit loop name binding")
    };
    assert_eq!(binding.source_unit(), source);
    assert_eq!(local_names.symbols()[binding.symbol().index()].name(), "n");
    assert_eq!(typed.symbol_type(*binding), Some(int));
    for (local, node) in file.ast().expressions().iter().filter(|(_, node)| {
        within(span, node.span()) && matches!(node.payload(), Expression::Call { .. })
    }) {
        let call = typed.call(id(local)).expect("committed unit nested call");
        let Expression::Call { arguments, .. } = node.payload() else {
            unreachable!()
        };
        assert_eq!(call.arguments().len(), arguments.len());
        for (mapped, argument) in call.arguments().iter().zip(arguments) {
            assert_eq!(mapped.parameter_type(), int);
            assert_eq!(mapped.mode(), ParameterMode::Borrow);
            assert_eq!(typed.expression_type(id(argument.value)), Some(int));
        }
    }
}

#[test]
fn nested_trial_argument_contracts_commit_or_roll_back_with_the_lambda() {
    for (tail, code, selected) in [
        ("it", Some("L0124"), false),
        ("true", Some("L0123"), false),
        ("needInt(it)", None, true),
    ] {
        let text = TRIAL
            .replace("val x: Int = n", "val x: Int = needInt(n)")
            .replace("needInt(it)", tail);
        observe(&text, code, selected);
    }
}

#[test]
fn ambiguous_iteration_keeps_final_only_error_in_ordinary_argument_of_same_call() {
    let text = TRIAL
        .replace("choose(callback:", "choose(tag: Int, callback:")
        .replace(
            "choose {",
            "choose(if (true) { val n = 1; val bad = n.fooBar(); 1 } else { 1 }, {",
        )
        .replace("needInt(it) } }", "it }) }");
    let (sources, diagnostics) = observe_with_member(&text, Some("L0124"), false, Some("fooBar"));
    let source = diagnostics[0].primary_span().source_id();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty());
    let ordinary_argument = file
        .ast()
        .expressions()
        .iter()
        .find_map(|(_, node)| {
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                return None;
            };
            if file.ast().expressions().get(*callee).unwrap().span()
                != diagnostics[0].primary_span()
            {
                return None;
            }
            assert_eq!(arguments.len(), 2);
            Some(
                file.ast()
                    .expressions()
                    .get(arguments[0].value)
                    .unwrap()
                    .span(),
            )
        })
        .expect("the exact diagnosed call has an ordinary argument");
    assert!(within(ordinary_argument, diagnostics[1].primary_span()));
    assert!(
        !within(trial_lambda(&file).1, diagnostics[1].primary_span()),
        "the independent root is outside the rolled-back lambda"
    );
}

#[test]
fn grouped_contextual_lambda_preserves_rejection_and_committed_iteration() {
    for (tail, code, selected) in [
        ("it", Some("L0124"), false),
        ("true", Some("L0123"), false),
        ("needInt(it)", None, true),
    ] {
        let text = TRIAL
            .replace("choose {", "choose(({")
            .replace("needInt(it) } }", &format!("{tail} }})) }}"));
        observe(&text, code, selected);
    }
}

#[test]
fn local_callee_shadowing_overloads_keeps_all_independent_deferred_arguments() {
    let text = "fun identity(n: Int): Int = n
fun choose(callback: (Int) -> Int): Int = 1
fun choose(callback: (String) -> String): String = \"text\"
fun run() { val choose = ::identity; choose(::identity, ::identity) }";
    let mut probe_sources = SourceMap::new();
    let (_, file) = parsed(&mut probe_sources, "local-callee.ko", text);
    let (environment, _) = standard_environments();
    let names = resolve_names(&probe_sources, &file, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let callee = expression(&probe_sources, &file, "choose");
    let span = file.ast().expressions().get(callee).unwrap().span();
    let target = names
        .references()
        .iter()
        .find(|reference| reference.span() == span && reference.namespace() == Namespace::Value)
        .unwrap()
        .target();
    let ReferenceTarget::Symbol(symbol) = target else {
        panic!("local callee must shadow the overload set")
    };
    assert_eq!(names.symbols()[symbol.index()].kind(), SymbolKind::Variable);
    let (sources, diagnostics) = observe(text, None, false);
    let spans = text
        .match_indices("::identity")
        .map(|(start, value)| (start, start + value.len()))
        .collect::<Vec<_>>();
    assert_eq!(spans.len(), 3);
    assert_eq!(
        diagnostics.len(),
        spans.len(),
        "initializer and both ordinary arguments remain independent roots"
    );
    for (diagnostic, span) in diagnostics.iter().zip(spans) {
        assert_eq!(diagnostic.code().to_string(), "L0165");
        assert_eq!(
            diagnostic.message(),
            "type checking could not complete: overload selection is unresolved"
        );
        assert_eq!(
            (
                diagnostic.primary_span().start(),
                diagnostic.primary_span().end()
            ),
            span
        );
        assert_eq!(sources.slice(diagnostic.primary_span()), Ok("::identity"));
        assert!(diagnostic.details().is_empty());
    }
}

#[test]
fn selected_function_value_callee_is_checked_with_its_ordinary_argument() {
    let text = "fun choose(callback: (Int) -> Int): Int = 1
fun choose(callback: (String) -> String): String = \"text\"
fun run(choose: (Int) -> Int): Int = choose(if (true) { val n = 1; val bad = n.fooBar(); 1 } else { 1 })";
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "function-value.ko", text);
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let call = expression(
        &sources,
        &file,
        "choose(if (true) { val n = 1; val bad = n.fooBar(); 1 } else { 1 })",
    );
    let callee = expression(&sources, &file, "choose");
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(
        typed.call(call).unwrap().target(),
        CallableTarget::FunctionValue
    );
    assert!(matches!(
        typed.types().get(typed.expression_type(callee).unwrap()),
        Some(TypeKind::Function { .. })
    ));
    let expected =
        rejected(complete_file_types(&sources, &file, &names, &types, &typed).map(|_| ()));
    assert_eq!(expected.len(), 1, "{expected:?}");
    assert_eq!(expected[0].code().to_string(), "L0080");
    assert_eq!(sources.slice(expected[0].primary_span()), Ok("fooBar"));
    for input in [
        SourceUnitInput::new("root", "function-value.ko", source, &file),
        SourceUnitInput::memory_document(source, &file),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let unit_source = names.names().index().source_units()[0].id();
        let unit_id = |local| UnitExpressionId::new(unit_source, local);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(
            typed.call(unit_id(call)).unwrap().target(),
            UnitCallTarget::FunctionValue
        );
        assert!(matches!(
            typed
                .types()
                .get(typed.expression_type(unit_id(callee)).unwrap()),
            Some(UnitTypeKind::Function { .. })
        ));
        assert_eq!(
            rejected(
                complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                    .map(|_| ())
            ),
            expected
        );
    }
}
