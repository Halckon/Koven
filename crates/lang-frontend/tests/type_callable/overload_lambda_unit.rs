//! 成功 overload trial 的原固定 oracle；双 Unit 入口各重复检查同一名称产物。
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
