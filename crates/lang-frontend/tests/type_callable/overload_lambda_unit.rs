//! Overload trial 的双 Unit 见证，每个入口重复检查同一名称产物。
use super::*;
use lang_frontend::{
    name_resolution::{
        CompilationUnitNames, SourceUnitInput, SourceUnitKey, UnitSymbolId, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    type_checking::{CompilationUnitTypes, check_compilation_unit_types},
};

fn checked_entries(
    sources: &SourceMap,
    parsed: &ParsedFile,
    environment: &NameEnvironment,
    types: &TypeEnvironment,
) -> [(
    CompilationUnitNames,
    CompilationUnitTypes,
    CompilationUnitTypes,
); 2] {
    [false, true].map(|rooted| {
        let inputs = [if rooted {
            SourceUnitInput::new("root", "callable.ko", parsed.source_id(), parsed)
        } else {
            SourceUnitInput::memory_document(parsed.source_id(), parsed)
        }];
        let index = index_compilation_unit(sources, &inputs).expect("unit index");
        let [source] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(source.source_id(), parsed.source_id());
        match (rooted, source.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "callable.ko");
            }
            _ => panic!("requested entry identity"),
        }
        let names = resolve_compilation_unit_names(sources, &inputs, &index, environment)
            .expect("unit names");
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("valid unit names");
        let typed =
            check_compilation_unit_types(sources, &inputs, &names, types).expect("unit types");
        let repeated = check_compilation_unit_types(sources, &inputs, &names, types)
            .expect("repeated unit types with the same names");
        for result in [&typed, &repeated] {
            assert!(
                result
                    .calls()
                    .iter()
                    .all(|call| call.expression().source_unit() == source.id())
            );
        }
        (names.into_names(), typed, repeated)
    })
}

fn lambda_parameters<'a>(
    names: &'a CompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Vec<(&'a str, UnitSymbolId)> {
    let [source] = names.source_units() else {
        panic!("one original names source")
    };
    source
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
        .map(|symbol| {
            let matches = typed
                .body_parameter_modes()
                .keys()
                .filter(|candidate| {
                    candidate.source_unit() == source.source_unit()
                        && candidate.symbol() == symbol.id()
                })
                .copied()
                .collect::<Vec<_>>();
            let [qualified] = matches.as_slice() else {
                panic!("one committed mode for the exact original lambda binding")
            };
            (symbol.name(), *qualified)
        })
        .collect()
}

pub(super) fn unique_trial(
    sources: &SourceMap,
    parsed: &ParsedFile,
    environment: &NameEnvironment,
    types: &TypeEnvironment,
) {
    for (names, typed, repeated) in checked_entries(sources, parsed, environment, types) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.diagnostics(), repeated.diagnostics());
        assert_eq!(typed.calls(), repeated.calls());
        assert_eq!(
            typed.body_parameter_modes(),
            repeated.body_parameter_modes()
        );
        assert_eq!(typed.body_symbol_types(), repeated.body_symbol_types());
        let mut call_spans = typed
            .calls()
            .iter()
            .map(|call| {
                parsed
                    .ast()
                    .expressions()
                    .get(call.expression().expression())
                    .unwrap()
                    .span()
            })
            .collect::<Vec<_>>();
        call_spans.sort_by_key(|span| span.start());
        assert_eq!(
            call_spans
                .into_iter()
                .map(|span| sources.slice(span).unwrap())
                .collect::<Vec<_>>(),
            ["resolve { intResult(it) }", "intResult(it)"]
        );
        assert_eq!(typed.body_parameter_modes().len(), 1);
        assert!(matches!(
            typed.types().get(typed.calls()[0].return_type()),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
        assert!(matches!(
            typed.types().get(typed.calls()[1].return_type()),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
        let parameters = lambda_parameters(&names, &typed);
        let [("it", parameter)] = parameters.as_slice() else {
            panic!("one committed lambda parameter")
        };
        assert_eq!(
            typed.body_parameter_mode(*parameter),
            Some(ParameterMode::Borrow)
        );
        assert!(matches!(
            typed
                .symbol_type(*parameter)
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
    }
}

pub(super) fn generic_trial(
    sources: &SourceMap,
    parsed: &ParsedFile,
    environment: &NameEnvironment,
    types: &TypeEnvironment,
) {
    for (names, typed, repeated) in checked_entries(sources, parsed, environment, types) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        assert_eq!(typed.diagnostics(), repeated.diagnostics());
        assert_eq!(typed.calls(), repeated.calls());
        assert_eq!(
            typed.body_parameter_modes(),
            repeated.body_parameter_modes()
        );
        assert_eq!(typed.body_symbol_types(), repeated.body_symbol_types());
        let [call] = typed.calls() else {
            panic!("only the winning dispatch call publishes a descriptor")
        };
        assert_eq!(
            call.arguments()
                .iter()
                .map(|argument| argument.parameter_index())
                .collect::<Vec<_>>(),
            [2, 0, 1]
        );
        assert!(matches!(
            typed.types().get(call.return_type()),
            Some(UnitTypeKind::Builtin(BuiltinType::Long))
        ));
        assert_eq!(
            call.instance()
                .type_arguments()
                .iter()
                .map(|&argument| typed.types().get(argument))
                .collect::<Vec<_>>(),
            [
                Some(&UnitTypeKind::Builtin(BuiltinType::String)),
                Some(&UnitTypeKind::Builtin(BuiltinType::Int)),
            ]
        );
        assert_eq!(typed.body_parameter_modes().len(), 3);
        assert_eq!(
            lambda_parameters(&names, &typed)
                .into_iter()
                .map(|(name, parameter)| {
                    (
                        name,
                        typed.body_parameter_mode(parameter),
                        typed
                            .symbol_type(parameter)
                            .and_then(|ty| typed.types().get(ty)),
                    )
                })
                .collect::<Vec<_>>(),
            [
                (
                    "changed",
                    Some(ParameterMode::Inout),
                    Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                ),
                (
                    "borrowed",
                    Some(ParameterMode::Borrow),
                    Some(&UnitTypeKind::Builtin(BuiltinType::String))
                ),
                (
                    "owned",
                    Some(ParameterMode::Value),
                    Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                ),
            ]
        );
    }
}

pub(super) fn failed_trials(
    sources: &SourceMap,
    parsed: &ParsedFile,
    environment: &NameEnvironment,
    types: &TypeEnvironment,
) {
    let expressions = parsed.ast().expressions();
    let lambdas = expressions
        .iter()
        .filter_map(|(_, node)| {
            matches!(node.payload(), Expression::Lambda { .. }).then_some(node.span())
        })
        .collect::<Vec<_>>();
    let mut callees = expressions
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Expression::Call { callee, .. } => Some(expressions.get(*callee).unwrap().span()),
            _ => None,
        })
        .collect::<Vec<_>>();
    callees.sort_by_key(|span| span.start());
    assert_eq!(callees.len(), 2);
    assert!(
        callees
            .iter()
            .all(|&span| sources.slice(span) == Ok("resolve"))
    );
    for (names, typed, repeated) in checked_entries(sources, parsed, environment, types) {
        assert_eq!(codes(typed.diagnostics()), ["L0124", "L0123"]);
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|d| d.primary_span())
                .collect::<Vec<_>>(),
            callees
        );
        assert_eq!(typed.diagnostics(), repeated.diagnostics());
        assert_eq!(typed.types(), repeated.types());
        assert!(typed.calls().is_empty());
        assert_eq!(typed.calls(), repeated.calls());
        assert!(typed.body_parameter_modes().is_empty());
        assert_eq!(
            typed.body_parameter_modes(),
            repeated.body_parameter_modes()
        );
        assert_eq!(typed.body_symbol_types(), repeated.body_symbol_types());

        let [source] = names.source_units() else {
            panic!("one original source")
        };
        let parameters = source
            .resolution()
            .symbols()
            .iter()
            .filter(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
            .collect::<Vec<_>>();
        assert_eq!(parameters.len(), 2);
        for parameter in parameters {
            assert!(
                !typed.body_symbol_types().keys().any(|id| {
                    id.source_unit() == source.source_unit() && id.symbol() == parameter.id()
                }),
                "uncommitted lambda parameter must have no type fact"
            );
        }

        let mut observed = [0, 0];
        let mut withheld = 0;
        for (expression, node) in expressions.iter() {
            let id = UnitExpressionId::new(source.source_unit(), expression);
            assert_eq!(typed.expression_type(id), repeated.expression_type(id));
            assert_eq!(
                typed.expression_category(id),
                repeated.expression_category(id)
            );
            if lambdas.iter().any(|span| {
                span.source_id() == node.span().source_id()
                    && span.start() <= node.span().start()
                    && node.span().end() <= span.end()
            }) {
                withheld += 1;
                for analysis in [&typed, &repeated] {
                    // Query absence directly: an invalid TypeId must not masquerade as None.
                    assert_eq!(
                        analysis.expression_type(id),
                        None,
                        "trial type leaked at {:?}",
                        node.span()
                    );
                    assert_eq!(
                        analysis.expression_category(id),
                        None,
                        "trial category leaked at {:?}",
                        node.span()
                    );
                }
            }
            let expected = match node.payload() {
                Expression::Lambda { .. } => {
                    observed[0] += 1;
                    None
                }
                Expression::Call { .. } => {
                    observed[1] += 1;
                    Some(&UnitTypeKind::Error)
                }
                _ => continue,
            };
            for analysis in [&typed, &repeated] {
                let actual = analysis
                    .expression_type(id)
                    .and_then(|ty| analysis.types().get(ty));
                assert_eq!(actual, expected, "{:?}", sources.slice(node.span()));
            }
        }
        assert_eq!(observed, [2, 2]);
        assert_eq!(withheld, 4, "two original lambdas and their it/true bodies");
        // Rollback must preserve already checked, unrelated source bodies in this same pass.
        for (literal, builtin) in [("1", BuiltinType::Int), ("\"text\"", BuiltinType::String)] {
            let literals = expressions
                .iter()
                .filter(|(_, node)| sources.slice(node.span()) == Ok(literal))
                .map(|(local, _)| UnitExpressionId::new(source.source_unit(), local))
                .collect::<Vec<_>>();
            let [id] = literals.as_slice() else {
                panic!("one untouched original function body for {literal}")
            };
            for analysis in [&typed, &repeated] {
                assert_eq!(
                    analysis
                        .expression_type(*id)
                        .and_then(|ty| analysis.types().get(ty)),
                    Some(&UnitTypeKind::Builtin(builtin))
                );
            }
        }
    }
}
