//! SPEC-0247: recovery must not manufacture runtime layout capabilities.
use super::*;
use lang_frontend::{
    name_resolution::resolve_names,
    type_checking::{TypeKind, check_types},
};

#[test]
fn unbound_field_recipes_recover_without_partial_layouts_or_constructions() {
    for field_type in [
        "Opaque",
        "List<Opaque>?",
        "List<Opaque>",
        "Wrapper<Opaque>?",
        "(Opaque) -> Int",
        "() -> Opaque",
    ] {
        let mut sources = SourceMap::new();
        let (source, file) = parsed(
            &mut sources,
            "recovery.ko",
            &format!(
                "class Wrapper<T>(val item: T)\n\
                 class Broken(val first: Int, val poison: {field_type})\n\
                 class Complete(val first: Int, val second: Long)\n\
                 fun inspect(value: Broken): Unit {{}}\n\
                 fun create(): Complete = Complete(1, 2L)"
            ),
        );
        let (mut ne, te) = standard_environments();
        ne.declare_type("Opaque").unwrap();
        if field_type == "Opaque" {
            // The direct unbound-field fixture has the same diagnostic-free recovery
            // contract in both entries; composite capability recovery is independently staged.
            let single_names = resolve_names(&sources, &file, &ne).unwrap();
            assert!(single_names.diagnostics().is_empty());
            let single = check_types(&sources, &file, &single_names, &te).unwrap();
            assert!(
                single.diagnostics().is_empty(),
                "{:?}",
                single.diagnostics()
            );
            assert_eq!(single.constructions().len(), 1);
        }
        let inputs = [SourceUnitInput::new("root", "recovery.ko", source, &file)];
        let names = validated_names(&sources, &inputs, &ne);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te)
            .expect("non-concrete field stays a recovery type, not an internal error");
        assert!(
            typed.diagnostics().is_empty(),
            "{field_type}: {:?}",
            typed.diagnostics()
        );
        let complete = declaration(&names, "Complete");
        assert_eq!(typed.runtime_field_layouts().len(), 1, "{field_type}");
        let layout = &typed.runtime_field_layouts()[0];
        assert_eq!(layout.declaration(), complete);
        assert_eq!(layout.fields().len(), 2);
        for (field, builtin) in layout
            .fields()
            .iter()
            .zip([BuiltinType::Int, BuiltinType::Long])
        {
            assert_eq!(
                typed.types().get(field.concrete_type()),
                Some(&UnitTypeKind::Builtin(builtin))
            );
        }
        assert_eq!(typed.constructions().len(), 1);
        assert_eq!(
            typed.constructions()[0].target(),
            UnitConstructionTarget::Nominal(complete)
        );
    }
}

#[test]
fn unbound_layout_recovery_is_source_qualified_and_input_order_independent() {
    let mut sources = SourceMap::new();
    let (bad_source, bad) = parsed(
        &mut sources,
        "bad.ko",
        "package a\nclass Broken(val field: Opaque)",
    );
    let (good_source, good) = parsed(
        &mut sources,
        "good.ko",
        "package b\nclass Complete(val field: Int)",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package c\nimport a.Broken\nimport b.Complete\nfun inspect(bad: Broken, good: Complete): Unit {}\nfun create(): Complete = Complete(1)",
    );
    let inputs = [
        SourceUnitInput::new("root", "a/bad.ko", bad_source, &bad),
        SourceUnitInput::new("root", "b/good.ko", good_source, &good),
        SourceUnitInput::new("root", "c/uses.ko", uses_source, &uses),
    ];
    let (mut ne, te) = standard_environments();
    ne.declare_type("Opaque").unwrap();
    let mut prior = None;
    for inputs in [inputs, [inputs[2], inputs[1], inputs[0]]] {
        let names = validated_names(&sources, &inputs, &ne);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &te)
            .expect("one poisoned owner must not abort the entire compilation unit");
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let broken = declaration(&names, "Broken");
        let complete = declaration(&names, "Complete");
        let field = |id| {
            typed
                .signatures()
                .declaration(id)
                .unwrap()
                .nominal()
                .unwrap()
                .fields()[0]
                .symbol()
        };
        assert_eq!(
            field(broken).symbol(),
            field(complete).symbol(),
            "fixture must collide in file-local SymbolId"
        );
        assert_ne!(field(broken).source_unit(), field(complete).source_unit());
        assert_eq!(typed.runtime_field_layouts().len(), 1);
        let layout = &typed.runtime_field_layouts()[0];
        assert_eq!(layout.declaration(), complete);
        assert_eq!(layout.fields()[0].symbol(), field(complete));
        assert_eq!(typed.constructions().len(), 1);
        assert_eq!(
            typed.constructions()[0].target(),
            UnitConstructionTarget::Nominal(complete)
        );
        let facts = (
            typed.runtime_field_layouts().to_vec(),
            typed.constructions().to_vec(),
            typed.expression_types().clone(),
            typed.diagnostics().to_vec(),
        );
        if let Some(prior) = &prior {
            assert_eq!(&facts, prior);
        }
        prior = Some(facts);
    }
}

#[test]
fn implicit_unused_lambda_parameter_and_known_when_join_match_single_file() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "parity.ko",
        "fun done(): Unit {}\nfun parity(flag: Boolean): Unit {\nval unused: (borrow Int) -> Int = { 1 }\nval joined = when (flag) { true -> done(); false -> 0 }\n}",
    );
    let (ne, te) = standard_environments();
    let names = resolve_names(&sources, &file, &ne).unwrap();
    let single = check_types(&sources, &file, &names, &te).unwrap();
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    let inputs = [SourceUnitInput::new("root", "parity.ko", source, &file)];
    let unit_names = validated_names(&sources, &inputs, &ne);
    let unit = source_unit(&unit_names, source);
    let typed = check_compilation_unit_types(&sources, &inputs, &unit_names, &te).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let lambda = expression_with_text(&sources, &file, "{ 1 }");
    assert!(
        matches!(single.types().get(single.expression_type(lambda).unwrap()), Some(TypeKind::Function { parameters, return_type, .. }) if parameters.len() == 1 && parameters[0].mode == ParameterMode::Borrow && single.types().get(parameters[0].ty) == Some(&TypeKind::Builtin(BuiltinType::Int)) && single.types().get(*return_type) == Some(&TypeKind::Builtin(BuiltinType::Int)))
    );
    assert!(
        matches!(typed.types().get(typed.expression_type(UnitExpressionId::new(unit, lambda)).unwrap()), Some(UnitTypeKind::Function { parameters, return_type, .. }) if parameters.len() == 1 && parameters[0].mode() == ParameterMode::Borrow && typed.types().get(parameters[0].ty()) == Some(&UnitTypeKind::Builtin(BuiltinType::Int)) && typed.types().get(*return_type) == Some(&UnitTypeKind::Builtin(BuiltinType::Int)))
    );
    let implicit = unit_names.names().source_units()[unit.index()]
        .resolution()
        .symbols()
        .iter()
        .find(|s| s.name() == "it" && s.kind() == SymbolKind::LambdaParameter)
        .unwrap();
    let implicit = *typed
        .body_parameter_modes()
        .keys()
        .find(|key| key.source_unit() == unit && key.symbol() == implicit.id())
        .unwrap();
    assert_eq!(
        typed.body_parameter_mode(implicit),
        Some(ParameterMode::Borrow)
    );
    assert_eq!(
        typed.types().get(typed.symbol_type(implicit).unwrap()),
        Some(&UnitTypeKind::Builtin(BuiltinType::Int))
    );
    let when = when_expressions(&file)[0];
    assert_eq!(
        single.types().get(single.expression_type(when).unwrap()),
        Some(&TypeKind::Builtin(BuiltinType::Any))
    );
    assert_eq!(
        typed.types().get(
            typed
                .expression_type(UnitExpressionId::new(unit, when))
                .unwrap()
        ),
        Some(&UnitTypeKind::Builtin(BuiltinType::Any))
    );
    assert!(typed.validate().is_ok());
}
