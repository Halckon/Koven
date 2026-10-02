use super::*;

#[test]
fn top_level_initializer_is_checked_instead_of_silently_skipped() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "state.ko", "package p\nval state: Int = 1");
    let inputs = [SourceUnitInput::new("root", "p/state.ko", source, &file)];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);

    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("top-level initializer is supported");
    let unit = source_unit(&names, source);
    let state = symbol_named(&typed, &names, unit, "state");
    let initializer = expression_with_text(&sources, &file, "1");
    assert!(matches!(
        typed
            .types()
            .get(typed.symbol_type(state).expect("state symbol type")),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert_eq!(
        typed.expression_type(UnitExpressionId::new(unit, initializer)),
        typed.symbol_type(state)
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn top_level_initializers_publish_stable_cross_file_symbol_and_expression_types() {
    let mut sources = SourceMap::new();
    let (values_source, values) = parsed(
        &mut sources,
        "top-level-values.ko",
        "package p\n\
         val inferred = 1\n\
         val annotated: Long = 2L\n\
         const val text: String = \"ready\"\n\
         val forward: Boolean = later\n\
         val later: Boolean = true",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "top-level-uses.ko",
        "package p\n\
         fun number(): Int = inferred\n\
         fun wide(): Long = annotated\n\
         fun message(): String = text\n\
         fun flag(): Boolean = forward",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a-values.ko", values_source, &values),
        SourceUnitInput::new("root", "p/b-uses.ko", uses_source, &uses),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("top-level initializers type check");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed top-level inputs type check");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.body_symbol_types(), reverse.body_symbol_types());
    let values_unit = source_unit(&names, values_source);
    for (name, builtin) in [
        ("inferred", BuiltinType::Int),
        ("annotated", BuiltinType::Long),
        ("text", BuiltinType::String),
        ("forward", BuiltinType::Boolean),
        ("later", BuiltinType::Boolean),
    ] {
        let symbol = symbol_named(&typed, &names, values_unit, name);
        assert!(matches!(
            typed.types().get(typed.symbol_type(symbol).expect("top-level symbol type")),
            Some(UnitTypeKind::Builtin(actual)) if *actual == builtin
        ));
    }
    let uses_unit = source_unit(&names, uses_source);
    for name in ["inferred", "annotated", "text", "forward"] {
        let expression = expression_with_text(&sources, &uses, name);
        assert!(
            typed
                .expression_type(UnitExpressionId::new(uses_unit, expression))
                .is_some()
        );
    }
    // Guide05 §36.4: ordinary facts do not grant the separate constant capability.
    assert!(typed.clone().validate().is_err());
    assert!(reverse.clone().validate().is_err());
    let enabled = typed
        .validate_constants()
        .expect("complete constant capability");
    let reverse = reverse
        .validate_constants()
        .expect("reversed constant capability");
    assert_eq!(enabled.constants(), reverse.constants());
    let facts = enabled.constants();
    assert_eq!(facts.declarations().len(), 1);
    assert_eq!(facts.uses().len(), 1);
    let declaration = &facts.declarations()[0];
    assert_eq!(
        declaration.value(),
        &ConstValue::String(std::sync::Arc::from(&b"ready"[..]))
    );
    let usage = &facts.uses()[0];
    assert_eq!(usage.target(), declaration.symbol());
    assert_eq!(usage.ty(), declaration.ty());
    assert_eq!(usage.value(), declaration.value());
    assert!(declaration.dependencies().is_empty());
    assert_eq!(
        enabled.types().expression_type(usage.expression()),
        Some(usage.ty())
    );
    assert_eq!(
        enabled.types().expression_category(usage.expression()),
        Some(ExpressionCategory::Temporary)
    );
    assert!(enabled.into_types().validate().is_err());
}

#[test]
fn cross_file_unannotated_forward_use_preserves_the_single_file_deferred_boundary() {
    let mut sources = SourceMap::new();
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "top-level-consumer.ko",
        "package p\nfun before(): Int = inferred",
    );
    let (provider_source, provider) = parsed(
        &mut sources,
        "top-level-provider.ko",
        "package p\nval inferred = 1\nfun after(): Int = inferred",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a-consumer.ko", consumer_source, &consumer),
        SourceUnitInput::new("root", "p/z-provider.ko", provider_source, &provider),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("unannotated forward value remains a deterministic deferred boundary");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed unannotated forward inputs keep the same boundary");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    let before = expression_with_text(&sources, &consumer, "inferred");
    let after = expression_with_text(&sources, &provider, "inferred");
    assert!(matches!(
        typed.types().get(
            typed
                .expression_type(UnitExpressionId::new(
                    source_unit(&names, consumer_source),
                    before,
                ))
                .expect("forward use type")
        ),
        Some(UnitTypeKind::Deferred(DeferredReason::ForwardValueType))
    ));
    assert!(matches!(
        typed.types().get(
            typed
                .expression_type(UnitExpressionId::new(
                    source_unit(&names, provider_source),
                    after,
                ))
                .expect("later use type")
        ),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(typed.validate().is_ok());
}

#[test]
fn invalid_top_level_initializers_recover_and_reject_return_outside_callable() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-top-level.ko",
        "val mismatch: Int = true\n\
         val outside = return 1\n\
         const val invalidConst: String = 2\n\
         fun later(): Int = 3",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-top-level.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid top-level initializers remain recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084", "L0086", "L0084"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["true", "return", "2"]
    );
    let unit = source_unit(&names, source);
    let later = expression_with_text(&sources, &file, "3");
    assert!(matches!(
        typed.types().get(
            typed
                .expression_type(UnitExpressionId::new(unit, later))
                .expect("later function still checked")
        ),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    let outside = symbol_named(&typed, &names, unit, "outside");
    assert!(matches!(
        typed
            .types()
            .get(typed.symbol_type(outside).expect("outside symbol type")),
        Some(UnitTypeKind::Error)
    ));
    assert!(typed.validate().is_err());
}

#[test]
fn companion_constant_initializers_publish_stable_ordinary_typed_facts() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         val CODE = 9\n\
         class Service {\n\
             companion object {\n\
                 fun version(): Int = VERSION\n\
                 const val VERSION: Int = 7\n\
             }\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "p/consumer.ko",
        "package q\n\
         fun readVersion(): Int = p.Service.version()\n\
         fun readCode(): Int = p.CODE",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a-provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("companion constant initializer type checks");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed companion constant inputs type check");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.types(), reverse.types());
    assert_eq!(typed.calls(), reverse.calls());
    assert_eq!(typed.diagnostics(), reverse.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.body_symbol_types(), reverse.body_symbol_types());
    let provider_unit = source_unit(&names, provider_source);
    let version = symbol_named(&typed, &names, provider_unit, "VERSION");
    assert!(matches!(
        typed
            .types()
            .get(typed.symbol_type(version).expect("companion constant type")),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    let initializer = expression_with_text(&sources, &provider, "7");
    assert!(matches!(
        typed.types().get(
            typed
                .expression_type(UnitExpressionId::new(provider_unit, initializer))
                .expect("companion initializer expression type")
        ),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(
        type_refs_with_text(&sources, &provider, "Int")
            .into_iter()
            .all(|type_ref| typed
                .type_ref_type(UnitTypeRefId::new(provider_unit, type_ref))
                .is_some())
    );
    let consumer_unit = source_unit(&names, consumer_source);
    for text in ["p.Service.version()", "p.CODE"] {
        let expression = expression_with_text(&sources, &consumer, text);
        assert!(matches!(
            typed.types().get(
                typed
                    .expression_type(UnitExpressionId::new(consumer_unit, expression))
                    .expect("qualified expression type")
            ),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
    }
    let qualified_code = expression_with_text(&sources, &consumer, "p.CODE");
    assert_eq!(
        typed.expression_category(UnitExpressionId::new(consumer_unit, qualified_code)),
        Some(ExpressionCategory::Temporary)
    );
    let version_call = expression_with_text(&sources, &consumer, "p.Service.version()");
    assert!(matches!(
        typed
            .call(UnitExpressionId::new(consumer_unit, version_call))
            .expect("qualified companion call descriptor")
            .target(),
        UnitCallTarget::Symbol(_)
    ));
    // Guide05 §36.4: ordinary facts do not grant the separate constant capability.
    assert!(typed.clone().validate().is_err());
    assert!(reverse.clone().validate().is_err());
    let enabled = typed
        .validate_constants()
        .expect("complete constant capability");
    let reverse = reverse
        .validate_constants()
        .expect("reversed constant capability");
    assert_eq!(enabled.constants(), reverse.constants());
    let facts = enabled.constants();
    assert_eq!(facts.declarations().len(), 1);
    assert_eq!(facts.uses().len(), 1);
    let declaration = &facts.declarations()[0];
    assert_eq!(
        declaration.value(),
        &ConstValue::Integer {
            ty: BuiltinType::Int,
            value: 7
        }
    );
    let usage = &facts.uses()[0];
    assert_eq!(usage.target(), declaration.symbol());
    assert_eq!(usage.ty(), declaration.ty());
    assert_eq!(usage.value(), declaration.value());
    assert!(declaration.dependencies().is_empty());
    assert_eq!(
        enabled.types().expression_type(usage.expression()),
        Some(usage.ty())
    );
    assert_eq!(
        enabled.types().expression_category(usage.expression()),
        Some(ExpressionCategory::Temporary)
    );
    assert!(enabled.into_types().validate().is_err());
}

#[test]
fn invalid_companion_constant_initializer_recovers_and_checks_later_members() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-companion-constant.ko",
        "class Service {\n\
             companion object {\n\
                 const val INVALID: String = 1\n\
                 fun later(): Int = 2\n\
             }\n\
         }\n\
         fun use(): Int = Service.later()",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-companion-constant.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid companion constant initializer remains recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084"]
    );
    assert_eq!(
        sources.slice(typed.body_diagnostics()[0].primary_span()),
        Ok("1")
    );
    let unit = source_unit(&names, source);
    for text in ["2", "Service.later()"] {
        let expression = expression_with_text(&sources, &file, text);
        assert!(matches!(
            typed.types().get(
                typed
                    .expression_type(UnitExpressionId::new(unit, expression))
                    .expect("later expression still checked")
            ),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
    }
    assert!(typed.validate().is_err());
}
