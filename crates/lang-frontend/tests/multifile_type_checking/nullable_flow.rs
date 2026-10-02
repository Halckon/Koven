use super::*;

#[test]
fn contextual_null_recovery_matches_single_file_rules() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "container-trial.ko",
        "fun nullable(): Int? = null\n\
         fun invalid(): Unit {\n\
             val missing = null\n\
             val mismatch: Int = null\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "container-trial.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("null recovery remains in the typed product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0083", "L0084"]
    );
    assert!(typed.container_constructions().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn null_comparisons_publish_stable_source_qualified_flow_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "p/models.ko",
        "package p\n\
         class Resource(val id: Int)\n\
         fun modelRead(input: Resource?): Int =\n\
             if (input != null) input.id else 0",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "p/uses.ko",
        "package p\n\
         fun read(input: Resource?): Int =\n\
             if (input != null) input.id else 0\n\
         fun mirrored(input: Resource?): Int =\n\
             if (null == input) 0 else input.id\n\
         fun shortCircuit(input: Resource?): Int =\n\
             if (input != null && input.id > 0) input.id else 0",
    );
    let forward_inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let reverse_inputs = [forward_inputs[1], forward_inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let forward_names = validated_names(&sources, &forward_inputs, &name_environment);
    let reverse_names = validated_names(&sources, &reverse_inputs, &name_environment);
    let forward =
        check_compilation_unit_types(&sources, &forward_inputs, &forward_names, &type_environment)
            .expect("null comparisons type check across files");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed null comparisons type check across files");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(forward.null_comparisons(), reverse.null_comparisons());
    assert_eq!(forward.non_null_uses(), reverse.non_null_uses());
    assert_eq!(forward.null_comparisons().len(), 4);
    assert_eq!(forward.non_null_uses().len(), 5);
    assert_eq!(
        forward
            .null_comparisons()
            .iter()
            .filter(|comparison| comparison.non_null_when_true())
            .count(),
        3
    );
    let model_unit = source_unit(&forward_names, models_source);
    let uses_unit = source_unit(&forward_names, uses_source);
    assert!(
        forward
            .null_comparisons()
            .iter()
            .any(|comparison| comparison.expression().source_unit() == model_unit)
    );
    assert!(
        forward
            .null_comparisons()
            .iter()
            .any(|comparison| comparison.expression().source_unit() == uses_unit)
    );
    let resource = forward
        .signatures()
        .declaration(declaration(&forward_names, "Resource"))
        .expect("Resource signature")
        .ty();
    for use_fact in forward.non_null_uses() {
        assert_eq!(
            use_fact.symbol().source_unit(),
            use_fact.expression().source_unit()
        );
        assert_eq!(use_fact.narrowed_type(), resource);
        assert_eq!(
            forward.types().get(use_fact.declared_type()),
            Some(&UnitTypeKind::Nullable(resource))
        );
        assert_eq!(
            forward.expression_type(use_fact.expression()),
            Some(use_fact.narrowed_type())
        );
        assert_eq!(forward.non_null_use(use_fact.expression()), Some(*use_fact));
    }
    for comparison in forward.null_comparisons() {
        assert_eq!(
            comparison.symbol().source_unit(),
            comparison.expression().source_unit()
        );
        assert_eq!(
            forward.types().get(comparison.nullable_type()),
            Some(&UnitTypeKind::Nullable(resource))
        );
        assert_eq!(
            forward.null_comparison(comparison.expression()),
            Some(*comparison)
        );
    }
    assert!(forward.validate().is_ok());
}

#[test]
fn invalid_null_comparisons_report_inference_errors_without_partial_facts() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "invalid-null-comparisons.ko",
        "fun nonNullable(input: Int): Boolean = input != null\n\
         fun bothNull(): Boolean = null == null",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "invalid-null-comparisons.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("invalid null comparisons remain recoverable");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0083", "L0083", "L0083"]
    );
    assert!(
        typed
            .body_diagnostics()
            .iter()
            .all(|diagnostic| { sources.slice(diagnostic.primary_span()) == Ok("null") })
    );
    assert!(typed.null_comparisons().is_empty());
    assert!(typed.non_null_uses().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn null_comparison_overload_trial_commits_only_the_unique_candidate_facts() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "nullable-trial.ko",
        "class Resource(val id: Int)\n\
         fun choose(action: () -> Int): Int\n\
         fun choose(action: () -> String): String\n\
         fun selected(input: Resource?): Int =\n\
             choose({ if (input != null) input.id else 0 })",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "nullable-trial.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("one nullable lambda candidate is valid");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.null_comparisons().len(), 1);
    assert_eq!(typed.non_null_uses().len(), 1);
    assert_eq!(typed.calls().len(), 1);
    assert!(typed.null_comparisons()[0].non_null_when_true());
    assert_eq!(
        typed.non_null_uses()[0].symbol(),
        typed.null_comparisons()[0].symbol()
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn nullable_lambda_parameters_are_stable_but_captured_mutable_locals_are_not() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "nullable-stability.ko",
        "class Resource(val id: Int)\n\
         fun apply(callback: (Resource?) -> Int): Int = 0\n\
         fun lambdaParameter(): Int =\n\
             apply({ input -> if (input != null) input.id else 0 })\n\
         fun captured(initial: Resource?): Boolean {\n\
             var current = initial\n\
             val capture = { current }\n\
             return current != null\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "nullable-stability.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("flow stability matches the single-file capture rules");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.null_comparisons().len(), 1);
    assert_eq!(typed.non_null_uses().len(), 1);
    let unit = source_unit(&names, source);
    let input = symbol_named(&typed, &names, unit, "input");
    let current = symbol_named(&typed, &names, unit, "current");
    assert_eq!(typed.null_comparisons()[0].symbol(), input);
    assert_eq!(typed.non_null_uses()[0].symbol(), input);
    assert!(
        typed
            .null_comparisons()
            .iter()
            .all(|comparison| comparison.symbol() != current)
    );
    assert!(typed.validate().is_ok());
}

#[test]
fn conflicting_short_circuit_facts_are_removed_instead_of_overwritten() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "nullable-conflict.ko",
        "enum class Shape { Circle(radius: Int), Point }\n\
         fun conflict(input: Shape?): Shape? =\n\
             if (input != null && input is Shape.Circle) input else input",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "nullable-conflict.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("conflicting flow facts use the conservative merge rule");

    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let unit = source_unit(&names, source);
    let shape = typed
        .signatures()
        .declaration(declaration(&names, "Shape"))
        .expect("Shape signature")
        .ty();
    let input_uses = expressions_with_text(&sources, &file, "input");
    assert_eq!(input_uses.len(), 4);
    let input_types = input_uses
        .iter()
        .map(|&expression| {
            typed
                .expression_type(UnitExpressionId::new(unit, expression))
                .expect("every input use has a type")
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        typed.types().get(input_types[0]),
        Some(UnitTypeKind::Nullable(inner)) if *inner == shape
    ));
    assert_eq!(input_types[1], shape);
    for index in [2, 3] {
        assert!(matches!(
            typed.types().get(input_types[index]),
            Some(UnitTypeKind::Nullable(inner)) if *inner == shape
        ));
    }
    assert_eq!(typed.null_comparisons().len(), 1);
    assert_eq!(typed.non_null_uses().len(), 1);
    assert!(typed.validate().is_ok());
}

#[test]
fn non_null_assertion_descriptors_are_source_qualified_and_keep_source_capabilities() {
    use lang_frontend::type_checking::{
        AssertionFailureEffect, Copyability, NullableWhenSubjectCategory as Category,
    };
    let mut sources = SourceMap::new();
    let (provider_id, provider) = parsed(
        &mut sources,
        "assert-provider.ko",
        "class Resource {}\nclass Holder(val item: Resource?) {}\nfun create(): Resource? = Resource()\nfun first(source: Int?): Int = source!!",
    );
    let (consumer_id, consumer) = parsed(
        &mut sources,
        "assert-consumer.ko",
        "fun second(source: Int?): Int = source!!\nfun extract(own source: Resource?): Resource = source!!\nfun borrowedSource(source: Resource?): Resource = source!!\nfun exclusive(inout source: Resource?): Resource = source!!\nfun field(source: Holder): Resource = source.item!!\nfun element(source: Array<Resource?>): Resource = source[0]!!\nfun temporary(): Resource = create()!!",
    );
    let inputs = [
        SourceUnitInput::new("root", "assert-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "assert-consumer.ko", consumer_id, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.non_null_assertions().len(), 8);
    for (source, file, expected) in [
        (provider_id, &provider, vec![Category::BorrowRoot]),
        (
            consumer_id,
            &consumer,
            vec![
                Category::BorrowRoot,
                Category::OwnedRoot,
                Category::BorrowRoot,
                Category::InoutRoot,
                Category::OrdinaryField,
                Category::ContainerElement,
                Category::Temporary,
            ],
        ),
    ] {
        let source = source_unit(&names, source);
        let facts = typed
            .non_null_assertions()
            .iter()
            .filter(|fact| fact.expression().source_unit() == source)
            .collect::<Vec<_>>();
        assert_eq!(
            facts
                .iter()
                .map(|fact| fact.source_category())
                .collect::<Vec<_>>(),
            expected
        );
        for fact in facts {
            assert_eq!(fact.operand().source_unit(), source);
            assert_eq!(
                typed.expression_type(fact.operand()),
                Some(fact.nullable_type())
            );
            assert_eq!(
                typed.expression_type(fact.expression()),
                Some(fact.inner_type())
            );
            assert_eq!(sources.slice(fact.operator_span()).unwrap(), "!!");
            assert!(
                matches!(file.ast().expressions().get(fact.expression().expression()).unwrap().payload(), Expression::NonNullAssert { operand, .. } if *operand == fact.operand().expression())
            );
            assert_eq!(fact.failure_effect(), AssertionFailureEffect::Abort);
            assert_eq!(typed.non_null_assertion(fact.expression()), Some(*fact));
        }
    }
    assert_eq!(
        typed
            .non_null_assertions()
            .iter()
            .filter(|fact| fact.copyability() == Copyability::Copyable)
            .count(),
        2
    );
    // Reordered inputs keep canonical source-qualified facts, even when local AST ids overlap.
    let again =
        check_compilation_unit_types(&sources, &[inputs[1], inputs[0]], &names, &type_environment)
            .unwrap();
    assert_eq!(typed.non_null_assertions(), again.non_null_assertions());
    assert!(typed.validate().is_ok());
}

#[test]
fn non_null_assertion_descriptor_trial_keeps_only_the_selected_candidate() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assert-trial.ko",
        "fun choose(action: () -> Int): Int\nfun choose(action: () -> String): String\nfun selected(input: Int?): Int = choose({ input!! })",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "assert-trial.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(typed.non_null_assertions().len(), 1);
    assert_eq!(typed.calls().len(), 1);
    assert!(typed.validate().is_ok());
}

#[test]
fn non_null_assertion_descriptor_rejects_non_nullable_operand() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assert-invalid.ko",
        "fun invalid(input: Int): Int = input!!",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "assert-invalid.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert_eq!(
        typed
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect::<Vec<_>>(),
        ["L0085"]
    );
    assert!(typed.non_null_assertions().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn non_null_assertion_descriptor_failed_trials_leave_no_facts() {
    // Each candidate reaches !! before its lambda return type is rejected.
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assert-failed-trial.ko",
        "fun choose(action: () -> Int): Int\nfun choose(action: () -> String): String\nfun rejected(input: Boolean?): Unit { val result = choose({ input!! }) }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "assert-failed-trial.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(!typed.diagnostics().is_empty());
    assert!(typed.non_null_assertions().is_empty());
    assert!(typed.validate().is_err());
}

#[test]
fn non_null_assertion_descriptor_member_parameter_group_and_implicit_field() {
    use lang_frontend::type_checking::NullableWhenSubjectCategory as Category;
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "assert-member.ko",
        "class Reader(val item: Int?) { fun read(source: Int?): Int = (source)!!\nfun readField(): Int = item!! }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "assert-member.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert_eq!(
        typed
            .non_null_assertions()
            .iter()
            .map(|fact| (fact.source_category(), fact.category()))
            .collect::<Vec<_>>(),
        [
            (Category::BorrowRoot, ExpressionCategory::Place),
            (Category::OrdinaryField, ExpressionCategory::Place)
        ]
    );
}
