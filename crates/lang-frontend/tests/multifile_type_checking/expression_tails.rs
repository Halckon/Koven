use super::*;

#[test]
fn interpolated_strings_traverse_cross_file_expressions_in_source_order() {
    let mut sources = SourceMap::new();
    let (declarations_source, declarations) = parsed(
        &mut sources,
        "interpolation-declarations.ko",
        "package p\nfun label(input: Int): Int = input",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "interpolation-uses.ko",
        "package p\nfun render(input: Int): String = \"before ${label(input)} after\"",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new(
            "root",
            "p/declarations.ko",
            declarations_source,
            &declarations,
        ),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("interpolation traverses its nested expression");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed interpolation inputs type check");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.calls(), reverse.calls());
    assert_eq!(typed.calls().len(), 1);
    let unit = source_unit(&names, uses_source);
    let string = expression_with_text(&sources, &uses, "\"before ${label(input)} after\"");
    let call = expression_with_text(&sources, &uses, "label(input)");
    assert!(matches!(
        typed.types().get(
            typed
                .expression_type(UnitExpressionId::new(unit, string))
                .expect("interpolated string has a type")
        ),
        Some(UnitTypeKind::Builtin(BuiltinType::String))
    ));
    assert_eq!(
        typed
            .call(UnitExpressionId::new(unit, call))
            .map(|call| call.target()),
        Some(UnitCallTarget::Declaration(declaration(&names, "label")))
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn invalid_interpolation_recovers_the_outer_string_and_later_body() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-interpolation.ko",
        "fun invalid(): String = \"bad ${1 + true}\"\n\
         fun later(): String = \"ok ${42}\"",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-interpolation.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid nested interpolation remains recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0085"]
    );
    let unit = source_unit(&names, source);
    for text in ["\"bad ${1 + true}\"", "\"ok ${42}\""] {
        let expression = expression_with_text(&sources, &file, text);
        let ty = typed
            .expression_type(UnitExpressionId::new(unit, expression))
            .expect("outer string keeps its String type");
        assert!(matches!(
            typed.types().get(ty),
            Some(UnitTypeKind::Builtin(BuiltinType::String))
        ));
    }
    let literal = expression_with_text(&sources, &file, "42");
    assert!(
        typed
            .expression_type(UnitExpressionId::new(unit, literal))
            .is_some()
    );
    assert!(typed.validate().is_err());
}

#[test]
fn remaining_expression_tails_publish_stable_types_and_traverse_cross_file_children() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "expression-tail-provider.ko",
        "package p\n\
         interface Parent { fun ping(input: Int): Unit {} }\n\
         fun produce(): Int = 1\n\
         fun target(input: Int): Int = input",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "expression-tail-consumer.ko",
        "package p\n\
         class Child : Parent {\n\
             override fun ping(input: Int): Unit {\n\
                 super<Parent>.ping(produce())\n\
                 val nullable: Int? = null\n\
                 val asserted: Int = nullable!!\n\
                 val casted = produce() as Long\n\
                 val propagated = produce()?\n\
                 val callable = ::target\n\
                 val bound = this::ping\n\
                 val closed = produce()..produce()\n\
                 val open = produce()..<produce()\n\
                 val pair = produce() to produce()\n\
                 val contains = produce() in produce()\n\
                 val missing = produce() !in produce()\n\
                 val fallback: Int = nullable ?: produce()\n\
             }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a-provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "p/b-consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("remaining expression tails preserve a typed recovery product");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed expression-tail inputs preserve the same product");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(typed.type_ref_types(), reverse.type_ref_types());
    assert_eq!(typed.calls(), reverse.calls());
    let unit = source_unit(&names, consumer_source);
    let expression_kind = |text| {
        typed
            .expression_type(UnitExpressionId::new(
                unit,
                expression_with_text(&sources, &consumer, text),
            ))
            .and_then(|ty| typed.types().get(ty))
    };
    for text in ["nullable!!", "nullable ?: produce()"] {
        assert!(matches!(
            expression_kind(text),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
    }
    for (text, reason) in [
        ("produce() as Long", DeferredReason::CastOrTypeTest),
        ("produce()?", DeferredReason::ErrorPropagation),
        ("::target", DeferredReason::OverloadSelection),
        ("this::ping", DeferredReason::OverloadSelection),
        ("produce()..produce()", DeferredReason::Call),
        ("produce()..<produce()", DeferredReason::Call),
        ("produce() to produce()", DeferredReason::Call),
        ("produce() in produce()", DeferredReason::Call),
        ("produce() !in produce()", DeferredReason::Call),
    ] {
        assert!(matches!(
            expression_kind(text),
            Some(UnitTypeKind::Deferred(actual)) if *actual == reason
        ));
    }
    assert!(matches!(
        expression_kind("super<Parent>.ping(produce())"),
        Some(UnitTypeKind::Builtin(BuiltinType::Unit))
    ));
    for expression in expressions_with_text(&sources, &consumer, "produce()") {
        assert!(matches!(
            typed
                .expression_type(UnitExpressionId::new(unit, expression))
                .and_then(|ty| typed.types().get(ty)),
            Some(UnitTypeKind::Builtin(BuiltinType::Int))
        ));
    }
    assert!(
        type_refs_with_text(&sources, &consumer, "Parent")
            .into_iter()
            .all(|type_ref| typed
                .type_ref_type(UnitTypeRefId::new(unit, type_ref))
                .is_some())
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn invalid_expression_tails_keep_single_file_diagnostics_and_later_recovery() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-expression-tails.ko",
        "fun consume(input: Int): Int = input\n\
         fun invalid(): Unit {\n\
             val asserted = 1!!\n\
             val fallback = 1 ?: consume(true)\n\
             val later: Int = consume(2)\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-expression-tails.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid expression tails remain recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0085", "L0085", "L0084"]
    );
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| sources
                .slice(diagnostic.primary_span())
                .expect("diagnostic span"))
            .collect::<Vec<_>>(),
        ["!!", "?:", "true"]
    );
    let unit = source_unit(&names, source);
    let later = expression_with_text(&sources, &file, "consume(2)");
    assert!(matches!(
        typed
            .expression_type(UnitExpressionId::new(unit, later))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(typed.validate().is_err());
}

#[test]
fn expression_tail_fallthrough_is_stable_under_input_permutation() {
    let mut sources = SourceMap::new();
    let (tails_source, tails) = parsed(
        &mut sources,
        "expression-tail-flow.ko",
        "package p\n\
         fun missing(input: Int?): Int { input ?: return 0 }\n\
         fun closed(input: Nothing?): Int { input ?: return 0 }\n\
         fun casted(): Int { (return 1) as Int }\n\
         fun propagated(): Int { (return 1)? }\n\
         fun referenced(): Int { (return 1)::next }",
    );
    let (other_source, other) = parsed(
        &mut sources,
        "expression-tail-other.ko",
        "package p\nfun unaffected(): Int = 2",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/a-tails.ko", tails_source, &tails),
        SourceUnitInput::new("root", "p/b-other.ko", other_source, &other),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("expression-tail flow diagnostics remain recoverable");
    let reverse_inputs = [inputs[1], inputs[0]];
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed expression-tail flow inputs remain recoverable");

    assert_eq!(typed.body_diagnostics(), reverse.body_diagnostics());
    assert_eq!(typed.expression_types(), reverse.expression_types());
    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0088"]
    );
    let unit = source_unit(&names, other_source);
    let unaffected = expression_with_text(&sources, &other, "2");
    assert!(matches!(
        typed
            .expression_type(UnitExpressionId::new(unit, unaffected))
            .and_then(|ty| typed.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(typed.validate().is_err());
}
