//! SPEC-0291: ordinary local receivers retain both concrete Map type arguments.

use lang_frontend::{
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    type_checking::{
        BuiltinType, IntrinsicTypeConstructor, TypeKind, UnitExpressionId, UnitTypeKind,
        check_compilation_unit_types, check_types, standard_environments,
    },
};

#[test]
fn mutable_map_consume_local_roots_preserve_exact_key_value_types() {
    let cases = [
        (
            "val-empty-int-int",
            "fun run(): Unit {\n\
             val source = mutableMapOf<Int, Int>()\n\
             val result = source.consume()\n\
             }",
            BuiltinType::Int,
            BuiltinType::Int,
        ),
        (
            "var-empty-string-string",
            "fun run(): Unit {\n\
             var source = mutableMapOf<String, String>()\n\
             val result = source.consume()\n\
             }",
            BuiltinType::String,
            BuiltinType::String,
        ),
        (
            "val-nonempty-string-int",
            "fun run(): Unit {\n\
             var seed = mutableMapOf<String, Int>()\n\
             seed.put(\"key\", 1)\n\
             val source = seed\n\
             val result = source.consume()\n\
             }",
            BuiltinType::String,
            BuiltinType::Int,
        ),
        (
            "var-nonempty-int-string",
            "fun run(): Unit {\n\
             var source = mutableMapOf<Int, String>()\n\
             source.put(1, \"value\")\n\
             val result = source.consume()\n\
             }",
            BuiltinType::Int,
            BuiltinType::String,
        ),
    ];
    let mut observations = Vec::with_capacity(cases.len() * 2);
    for (label, source, key, value) in cases {
        observations.extend(observe_both(label, source, key, value));
    }

    assert_eq!(observations.len(), 8, "all four sources need both entries");
    assert!(
        observations.iter().all(|(_, accepted)| *accepted),
        "all eight ordinary-source observations:\n{observations:#?}"
    );
}

fn observe_both(
    label: &str,
    text: &str,
    key: BuiltinType,
    value: BuiltinType,
) -> [(String, bool); 2] {
    let (sources, file) = super::parsed(text);
    let calls = file
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (sources.slice(node.span()) == Ok("source.consume()")).then_some(id)
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1, "{label}: one actual consume expression");
    let call = calls[0];
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).expect("single names");
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let single = check_types(&sources, &file, &names, &types).expect("single raw types");
    let single_expected = TypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::Map,
        arguments: vec![
            single.types().builtin(key).expect("standard key type"),
            single.types().builtin(value).expect("standard value type"),
        ],
    };
    let single_actual = single
        .expression_type(call)
        .and_then(|ty| single.types().get(ty));
    let single_observation = (
        format!(
            "{label}/single: expected {single_expected:?}, actual {single_actual:?}, diagnostics {:?}",
            single.diagnostics()
        ),
        single.diagnostics().is_empty() && single_actual == Some(&single_expected),
    );

    let inputs = [SourceUnitInput::new(
        "root",
        "map_test.ko",
        file.source_id(),
        &file,
    )];
    let index = index_compilation_unit(&sources, &inputs).expect("unit index");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .expect("unit names")
        .validate()
        .expect("valid unit names");
    let source_unit = names.names().index().source_units()[0].id();
    let unit =
        check_compilation_unit_types(&sources, &inputs, &names, &types).expect("unit raw types");
    let unit_expected = UnitTypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::Map,
        arguments: vec![
            unit.types().builtin(key).expect("standard key type"),
            unit.types().builtin(value).expect("standard value type"),
        ],
    };
    let unit_actual = unit
        .expression_type(UnitExpressionId::new(source_unit, call))
        .and_then(|ty| unit.types().get(ty));
    let unit_observation = (
        format!(
            "{label}/unit: expected {unit_expected:?}, actual {unit_actual:?}, diagnostics {:?}",
            unit.diagnostics()
        ),
        unit.diagnostics().is_empty() && unit_actual == Some(&unit_expected),
    );
    [single_observation, unit_observation]
}
