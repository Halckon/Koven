//! 原 callable 参数模式 oracle，消费同一源码的两个真实 Unit 入口。
use super::*;
use lang_frontend::type_checking::UnitExpressionId;

pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    environment: &NameEnvironment,
    types: &TypeEnvironment,
) {
    for (names, typed) in unit_entries::checked_named_entries(sources, parsed, environment, types) {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let [source] = names.source_units() else {
            panic!("one original names source")
        };
        let unit = source.source_unit();
        let resolution = source.resolution();
        let named_modes = resolution
            .symbols()
            .iter()
            .filter(|symbol| symbol.kind() == SymbolKind::ValueParameter)
            .map(|symbol| {
                let parameters = typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter_map(|declaration| declaration.callable())
                    .flat_map(|callable| callable.parameters())
                    .filter(|parameter| {
                        parameter.symbol().is_some_and(|qualified| {
                            qualified.source_unit() == unit && qualified.symbol() == symbol.id()
                        })
                    })
                    .collect::<Vec<_>>();
                let [parameter] = parameters.as_slice() else {
                    panic!("one signature parameter for the exact source binding")
                };
                assert_eq!(parameter.name(), Some(symbol.name()));
                (symbol.name(), Some(parameter.mode()))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            named_modes,
            [
                ("input", Some(ParameterMode::Borrow)),
                ("input", Some(ParameterMode::Inout)),
                ("input", Some(ParameterMode::Value)),
                ("callback", Some(ParameterMode::Borrow)),
                ("callback", Some(ParameterMode::Borrow)),
                ("callback", Some(ParameterMode::Borrow)),
            ]
        );
        let lambda_modes = resolution
            .symbols()
            .iter()
            .filter(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
            .map(|symbol| {
                let qualified = typed
                    .body_parameter_modes()
                    .keys()
                    .filter(|candidate| {
                        candidate.source_unit() == unit && candidate.symbol() == symbol.id()
                    })
                    .collect::<Vec<_>>();
                let [qualified] = qualified.as_slice() else {
                    panic!("one adopted mode for the exact source lambda parameter")
                };
                typed.body_parameter_mode(**qualified)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            lambda_modes,
            [
                Some(ParameterMode::Borrow),
                Some(ParameterMode::Inout),
                Some(ParameterMode::Value),
                Some(ParameterMode::Borrow),
                Some(ParameterMode::Borrow),
                Some(ParameterMode::Value),
                Some(ParameterMode::Inout),
            ]
        );
        let lambda_type_modes = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), Expression::Lambda { .. }).then_some(id)
            })
            .map(|id| {
                match typed
                    .expression_type(UnitExpressionId::new(unit, id))
                    .and_then(|ty| typed.types().get(ty))
                {
                    Some(UnitTypeKind::Function { parameters, .. }) => {
                        assert_eq!(parameters.len(), 1);
                        parameters[0].mode()
                    }
                    other => panic!("expected typed lambda function, got {other:?}"),
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            lambda_type_modes,
            [
                ParameterMode::Borrow,
                ParameterMode::Inout,
                ParameterMode::Value,
                ParameterMode::Borrow,
                ParameterMode::Borrow,
                ParameterMode::Value,
                ParameterMode::Inout,
            ]
        );
    }
}
