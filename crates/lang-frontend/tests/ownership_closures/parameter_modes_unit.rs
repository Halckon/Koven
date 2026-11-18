//! Original expected-parameter and capture oracles, using source-qualified Unit facts.
use super::*;
use lang_frontend::{
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::check_compilation_unit_ownership,
    type_checking::{UnitExpressionId, check_compilation_unit_types},
};

pub(super) fn check(text: &str, expected: &[(&str, ParameterMode)], expected_codes: &[&str]) {
    for rooted in [false, true] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("closures.ko", text).unwrap();
        let parsed = parse_file_twice(&sources, source, "lambda parameter Unit");
        assert!(parsed.diagnostics().is_empty());
        assert_eq!(sources.source_text(source), Ok(text));
        let (environment, types) = environments();
        let input = if rooted {
            SourceUnitInput::new("root", "closures.ko", source, &parsed)
        } else {
            SourceUnitInput::memory_document(source, &parsed)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [entry] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(entry.source_id(), source);
        let unit = entry.id();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert!(names.diagnostics().is_empty());
        let names = names.validate().unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let resolution = names.names().source_units()[0].resolution();
        let modes = resolution
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
                    panic!("one actual adopted mode for this source parameter")
                };
                (symbol.name(), typed.body_parameter_mode(**qualified))
            })
            .collect::<Vec<_>>();
        let expected = expected
            .iter()
            .map(|(name, mode)| (*name, Some(*mode)))
            .collect::<Vec<_>>();
        assert_eq!(modes, expected);
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert!(owned.is_compatible_with(&typed));
        let lambdas = lambdas(&parsed);
        assert_eq!(lambdas.len(), expected.len());
        for lambda in lambdas {
            assert_eq!(
                owned
                    .captures_of(UnitExpressionId::new(unit, lambda))
                    .count(),
                0
            );
        }
        // On rejected input this retains the existing empty-publication oracle;
        // it does not independently prove a successful capture collection.
        assert_eq!(unit_codes(&owned), expected_codes);
    }
}
