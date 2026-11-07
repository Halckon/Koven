//! 普通 expected Function 接收 move literal，同时保留 canonical 类型与参数契约。

use super::*;
use lang_frontend::{
    name_resolution::{
        CompilationUnitNames, SourceUnitInput, SourceUnitKey, UnitSymbolId, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    type_checking::{
        CompilationUnitTypes, UnitExpressionId, UnitTypeKind, check_compilation_unit_types,
    },
};

fn checked_entries(
    text: &str,
) -> (
    SourceMap,
    ParsedFile,
    [(CompilationUnitNames, CompilationUnitTypes); 2],
) {
    let (sources, file) = parsed(text);
    let (environment, types) = environments();
    let entries = [false, true].map(|rooted| {
        let input = if rooted {
            SourceUnitInput::new("root", "callable.ko", file.source_id(), &file)
        } else {
            SourceUnitInput::memory_document(file.source_id(), &file)
        };
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        let [source] = index.source_units() else {
            panic!("one original source")
        };
        assert_eq!(source.source_id(), file.source_id());
        match (rooted, source.key()) {
            (false, SourceUnitKey::MemoryDocument) => {}
            (true, SourceUnitKey::Rooted { root, logical_path }) => {
                assert_eq!(root.as_str(), "root");
                assert_eq!(logical_path.as_str(), "callable.ko");
            }
            _ => panic!("requested entry identity"),
        }
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("unit names");
        assert!(names.diagnostics().is_empty());
        let names = names.validate().expect("valid names");
        let typed =
            check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit types");
        (names.into_names(), typed)
    });
    (sources, file, entries)
}

fn symbol_named(
    typed: &CompilationUnitTypes,
    names: &CompilationUnitNames,
    name: &str,
) -> UnitSymbolId {
    let [source] = names.source_units() else {
        panic!("one original names source")
    };
    let local = source
        .resolution()
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .expect("original symbol")
        .id();
    typed
        .body_symbol_types()
        .keys()
        .copied()
        .find(|symbol| symbol.source_unit() == source.source_unit() && symbol.symbol() == local)
        .expect("qualified original symbol")
}

#[test]
fn ordinary_expected_move_literals_keep_canonical_type_and_borrow_parameters() {
    let text = "fun apply(callback: (Int) -> Boolean): Unit {}\n\
                fun make(own returnedLabel: String): (Int) -> Boolean = move { returnedIndex -> returnedLabel == \"return\" && returnedIndex == 0 }\n\
                fun use(own localLabel: String, own argumentLabel: String): Unit {\n\
                    val contextual: (Int) -> Boolean = move { localIndex -> localLabel == \"local\" && localIndex == 0 }\n\
                    apply(move { argumentIndex -> argumentLabel == \"argument\" && argumentIndex == 0 })\n\
                }";
    let (_, parsed, entries) = checked_entries(text);
    for (resolution, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let contextual = symbol_named(&typed, &resolution, "contextual");
        let expected = typed
            .symbol_type(contextual)
            .expect("ordinary expected type");
        assert!(
            matches!(typed.types().get(expected), Some(UnitTypeKind::Function {
        move_only: false, parameters, return_type,
    }) if parameters.len() == 1 && parameters[0].mode() == ParameterMode::Borrow
        && typed.types().get(parameters[0].ty()) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        && typed.types().get(*return_type) == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean)))
        );
        let lambdas = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), Expression::Lambda { .. }).then_some(id)
            })
            .collect::<Vec<_>>();
        assert_eq!(lambdas.len(), 3);
        for lambda in lambdas {
            assert_eq!(
                typed.expression_type(UnitExpressionId::new(
                    resolution.source_units()[0].source_unit(),
                    lambda
                )),
                Some(expected),
                "local, argument and return literals adopt the same canonical ordinary Function"
            );
        }
        for name in ["returnedIndex", "localIndex", "argumentIndex"] {
            let symbol = symbol_named(&typed, &resolution, name);
            assert_eq!(
                typed.body_parameter_mode(symbol),
                Some(ParameterMode::Borrow)
            );
            assert_eq!(
                typed
                    .symbol_type(symbol)
                    .and_then(|ty| typed.types().get(ty)),
                Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            );
        }
    }
}

#[test]
fn contextual_move_literal_acceptance_preserves_named_identity_mode_and_arity_errors() {
    let text = "fun take(callback: (Int) -> Int): Unit {}\n\
                fun strong(callback: move (Int) -> Int): Unit {}\n\
                fun use(named: move (Int) -> Int, ordinary: (Int) -> Int): Unit {\n\
                    val rejected: (Int) -> Int = named\n\
                    take(named)\n\
                    val wrongMode: (own Int) -> Int = ordinary\n\
                    strong({ shared -> shared })\n\
                    val wrongArity: (Int) -> Int = move { first, second -> first }\n\
                    val inferred = move { 1 }\n\
                    val wrongInferred: () -> Int = inferred\n\
                }";
    let (sources, _, entries) = checked_entries(text);
    for (resolution, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0084"; 6]);
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("primary"))
                .collect::<Vec<_>>(),
            ["named", "named", "ordinary", "->", "->", "inferred"]
        );
        for name in ["shared", "first", "second"] {
            let symbol = symbol_named(&typed, &resolution, name);
            assert_eq!(
                typed.body_parameter_mode(symbol),
                None,
                "rejected lambda structure must not publish an adopted parameter mode"
            );
        }
        let inferred = symbol_named(&typed, &resolution, "inferred");
        assert!(
            matches!(typed.symbol_type(inferred).and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Function { move_only: true, parameters, return_type }) if parameters.is_empty()
            && typed.types().get(*return_type) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))),
            "a move literal without expected context retains its move Function identity"
        );
    }
}
