//! SPEC-0210: associated selection crosses files, without granting a runtime capability.
use lang_frontend::{
    lexer::lex,
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    parser::parse_file,
    source::SourceMap,
    type_checking::{
        BuiltinType, ConstValue, ExpressionCategory, UnitExpressionId, UnitTypeKind,
        check_compilation_unit_types, standard_environments,
    },
};

#[test]
fn imported_and_absolute_constant_targets_keep_exact_type_and_temporary_category() {
    for declaration in [
        "object Config { const val VALUE: Int = 7 }",
        "class Config { companion object { const val VALUE: Int = 7 } }",
        "value class Config(val x: Int) { companion object { const val VALUE: Int = 7 } }",
        "interface Config { companion object { const val VALUE: Int = 7 } }",
        "enum class Config { One; companion object { const val VALUE: Int = 7 } }",
    ] {
        for access in ["Config.VALUE", "p.Config.VALUE"] {
            check(declaration, access, &[]);
        }
    }
}

#[test]
fn private_constant_access_reaches_type_phase_with_l0154() {
    for declaration in [
        "object Config { private const val VALUE: Int = 7 }",
        "class Config { companion object { private const val VALUE: Int = 7 } }",
    ] {
        for access in ["Config.VALUE", "p.Config.VALUE"] {
            check(declaration, access, &["L0154"]);
        }
    }
}

fn check(declaration: &str, access: &str, expected_codes: &[&str]) {
    let mut sources = SourceMap::new();
    let provider = sources
        .add_source("p/provider.ko", format!("package p\n{declaration}"))
        .unwrap();
    let consumer = sources
        .add_source(
            "q/consumer.ko",
            format!("package q\nimport p.Config\nfun read(): Int = {access}"),
        )
        .unwrap();
    let provider_file = parse_file(&sources, &lex(&sources, provider).unwrap()).unwrap();
    let consumer_file = parse_file(&sources, &lex(&sources, consumer).unwrap()).unwrap();
    assert!(
        provider_file.diagnostics().is_empty(),
        "{:?}",
        provider_file.diagnostics()
    );
    assert!(
        consumer_file.diagnostics().is_empty(),
        "{:?}",
        consumer_file.diagnostics()
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider, &provider_file),
        SourceUnitInput::new("root", "q/consumer.ko", consumer, &consumer_file),
    ];
    let (name_environment, type_environment) = standard_environments();
    for inputs in [inputs, [inputs[1], inputs[0]]] {
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment).unwrap();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().unwrap();
        let typed =
            check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
        let codes = typed
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(codes, expected_codes, "{declaration}: {access}");
        if expected_codes.is_empty() {
            let source = names
                .names()
                .index()
                .source_units()
                .iter()
                .find(|unit| unit.source_id() == consumer)
                .unwrap()
                .id();
            let expression = consumer_file
                .ast()
                .expressions()
                .iter()
                .find(|(_, node)| sources.slice(node.span()) == Ok(access))
                .unwrap()
                .0;
            let expression = UnitExpressionId::new(source, expression);
            assert_eq!(
                typed
                    .types()
                    .get(typed.expression_type(expression).unwrap()),
                Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            );
            assert_eq!(
                typed.expression_category(expression),
                Some(ExpressionCategory::Temporary)
            );
            assert!(
                typed
                    .aggregate_projections()
                    .iter()
                    .all(|fact| fact.expression() != expression)
            );
            let enabled = typed.clone().validate_constants().unwrap();
            let facts = enabled.constants();
            let usage = facts
                .uses()
                .iter()
                .find(|usage| usage.expression() == expression)
                .unwrap();
            let declaration = facts
                .declarations()
                .iter()
                .find(|declaration| declaration.symbol() == usage.target())
                .unwrap();
            assert_eq!(usage.ty(), declaration.ty());
            assert_eq!(
                usage.value(),
                &ConstValue::Integer {
                    ty: BuiltinType::Int,
                    value: 7
                }
            );
            assert_eq!(usage.value(), declaration.value());
            assert!(
                typed.validate().is_err(),
                "selection alone cannot grant base ownership/native capability"
            );
        } else {
            assert!(typed.constants().is_none());
            assert!(typed.clone().validate_constants().is_err());
            assert_eq!(
                sources
                    .slice(typed.diagnostics()[0].primary_span())
                    .unwrap(),
                "VALUE"
            );
        }
    }
}

#[test]
fn private_constant_is_visible_inside_its_recorded_owner() {
    check(
        "class Config { companion object {\nprivate const val SECRET: Int = 3\nconst val VALUE: Int = 7\nfun read(): Int = p.Config.SECRET\n} }",
        "Config.VALUE",
        &[],
    );
}

#[test]
fn associated_member_import_and_invisible_type_keep_name_phase_diagnostics() {
    for (declaration, import, expected) in [
        (
            "class Config { companion object { const val VALUE: Int = 7 } }",
            "p.Config.VALUE",
            "L0148",
        ),
        (
            "private class Config { companion object { const val VALUE: Int = 7 } }",
            "p.Config",
            "L0149",
        ),
    ] {
        let mut sources = SourceMap::new();
        let provider = sources
            .add_source("provider.ko", format!("package p\n{declaration}"))
            .unwrap();
        let consumer = sources
            .add_source(
                "consumer.ko",
                format!("package q\nimport {import}\nfun entry(): Unit {{}}"),
            )
            .unwrap();
        let provider_file = parse_file(&sources, &lex(&sources, provider).unwrap()).unwrap();
        let consumer_file = parse_file(&sources, &lex(&sources, consumer).unwrap()).unwrap();
        assert!(provider_file.diagnostics().is_empty());
        assert!(consumer_file.diagnostics().is_empty());
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider, &provider_file),
            SourceUnitInput::new("root", "q/consumer.ko", consumer, &consumer_file),
        ];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let (environment, _) = standard_environments();
        let names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &environment).unwrap();
        assert_eq!(
            names
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_string())
                .collect::<Vec<_>>(),
            [expected]
        );
        assert!(
            names.validate().is_err(),
            "invalid imports must stop before unit type checking"
        );
    }
}

#[test]
fn missing_local_constant_member_stops_before_capability_publication() {
    for declaration in [
        "object A { const val X = 1 }",
        "class A { companion object { const val X = 1 } }",
    ] {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "a.ko",
                format!("package a\n{declaration}\nfun read(): Int = A.MISSING"),
            )
            .unwrap();
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let inputs = [SourceUnitInput::new("root", "a/source.ko", source, &parsed)];
        let (ne, _) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne).unwrap();
        assert_eq!(
            names
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0080"]
        );
        assert_eq!(
            sources
                .slice(names.diagnostics()[0].primary_span())
                .unwrap(),
            "MISSING"
        );
        assert!(names.validate().is_err());
    }
}
