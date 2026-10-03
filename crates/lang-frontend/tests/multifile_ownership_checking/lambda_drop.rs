use super::*;

#[test]
fn lambda_tail_consumes_result_and_drops_only_body_owned_inputs() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nfun make(): String = \"made\"",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         val topAction: move () -> String = move {\n\
             val result = \"owned\"\n\
             return result\n\
         }\n\
         fun entry(): Unit {\n\
             val action: move () -> String = move { \"left\" + p.make() }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed_inputs = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    let consumer_unit = source_unit(&names, consumer_source);
    let tail = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "\"left\" + p.make()"),
    );
    let left = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "\"left\""),
    );
    let call = UnitExpressionId::new(
        consumer_unit,
        expression_with_text(&sources, &consumer, "p.make()"),
    );
    let operand_drops = ownership
        .drops()
        .iter()
        .filter_map(|fact| {
            (fact.point() == UnitDropPoint::AfterBinaryOperands(tail)).then_some(fact.target())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        operand_drops,
        [
            UnitDropTarget::Temporary(call),
            UnitDropTarget::Temporary(left)
        ],
        "lambda tail operands drop in reverse evaluation order"
    );
    assert!(
        !ownership
            .drops()
            .iter()
            .any(|fact| fact.target() == UnitDropTarget::Temporary(tail)),
        "implicit lambda result transfers to the caller instead of being dropped"
    );
    let result = names.names().source_units()[consumer_unit.index()]
        .resolution()
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "result")
        .expect("lambda body local exists")
        .id();
    assert!(
        !ownership.drops().iter().any(|fact| {
            matches!(
                fact.target(),
                UnitDropTarget::Named(target)
                    if target.source_unit() == consumer_unit && target.symbol() == result
            )
        }),
        "a body-local owner returned from a top-level initializer lambda remains live until transfer"
    );
    ownership
        .clone()
        .validate()
        .expect("complete lambda drop facts validate");
    let reversed_names = validated_names(&sources, &reversed_inputs, &name_environment);
    let reversed_typed = validated_types(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
    );
    let reversed = check_compilation_unit_ownership(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
        &reversed_typed,
    )
    .expect("reversed unit ownership product");
    assert_eq!(ownership.drops(), reversed.drops());
}

#[test]
fn lambda_value_parameters_publish_entry_read_and_transfer_drop_facts() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun inspect(message: String): Unit {}\n\
         fun consume(own message: String): Unit {}",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val unused: move (own String, own String) -> String = move { first, second -> \"unused-a\" + \"unused-b\" }\n\
             val read: move (own String) -> Unit = move { observed -> p.inspect(observed) }\n\
             val consumed: move (own String) -> Unit = move { delivered -> p.consume(delivered) }\n\
             val implicitAction: move (own String) -> String = move { transferred -> transferred }\n\
             val explicitAction: move (own String) -> String = move { explicitValue -> return explicitValue }\n\
             val borrowed: move (borrow String) -> String = move { item -> \"borrow-a\" + \"borrow-b\" }\n\
             val copied: move (own Int) -> String = move { item -> \"copy-a\" + \"copy-b\" }\n\
             val implicitUnused: move (own String) -> String = move { \"implicit-a\" + \"implicit-b\" }\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed_inputs = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = validated_types(&sources, &inputs, &names, &type_environment);
    let ownership =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .expect("unit ownership product");

    assert!(ownership.diagnostics().is_empty());
    let source_unit = source_unit(&names, consumer_source);
    let unused_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { first, second -> \"unused-a\" + \"unused-b\" }",
        ),
    );
    let read_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { observed -> p.inspect(observed) }",
        ),
    );
    let returned_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(&sources, &consumer, "move { transferred -> transferred }"),
    );
    let consumed_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { delivered -> p.consume(delivered) }",
        ),
    );
    let explicit_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { explicitValue -> return explicitValue }",
        ),
    );
    let borrowed_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { item -> \"borrow-a\" + \"borrow-b\" }",
        ),
    );
    let copied_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { item -> \"copy-a\" + \"copy-b\" }",
        ),
    );
    let implicit_unused_lambda = UnitExpressionId::new(
        source_unit,
        expression_with_text(
            &sources,
            &consumer,
            "move { \"implicit-a\" + \"implicit-b\" }",
        ),
    );
    let first = symbol_named(&ownership, &names, source_unit, "first");
    let second = symbol_named(&ownership, &names, source_unit, "second");
    let observed = symbol_named(&ownership, &names, source_unit, "observed");
    let entry_targets = ownership
        .drops()
        .iter()
        .filter_map(|fact| {
            (fact.point() == UnitDropPoint::LambdaEntry(unused_lambda)).then_some(fact.target())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        entry_targets,
        [UnitDropTarget::Named(second), UnitDropTarget::Named(first)],
        "unused MoveOnly Value parameters drop at lambda entry in reverse declaration order"
    );
    let implicit_it = symbol_named(&ownership, &names, source_unit, "it");
    assert!(ownership.drops().iter().any(|fact| {
        fact.point() == UnitDropPoint::LambdaEntry(implicit_unused_lambda)
            && fact.target() == UnitDropTarget::Named(implicit_it)
    }));
    assert!(
        ownership.drops().iter().any(|fact| {
            fact.target() == UnitDropTarget::Named(observed)
                && matches!(fact.point(), UnitDropPoint::CallReturn(_))
        }),
        "a borrowed read drops the owned parameter after the synchronous call returns"
    );
    let transferred_parameter_drops = ownership
        .drops()
        .iter()
        .filter_map(|fact| {
            let origin = sources.slice(fact.value_origin()).ok()?;
            (origin == "delivered" || origin == "transferred" || origin == "explicitValue")
                .then_some((origin, fact))
        })
        .collect::<Vec<_>>();
    assert!(
        transferred_parameter_drops.is_empty(),
        "Value calls and lambda returns transfer their MoveOnly Value parameters without dropping them: {transferred_parameter_drops:?}"
    );
    for lambda in [
        read_lambda,
        consumed_lambda,
        returned_lambda,
        explicit_lambda,
        borrowed_lambda,
        copied_lambda,
    ] {
        assert!(
            !ownership
                .drops()
                .iter()
                .any(|fact| fact.point() == UnitDropPoint::LambdaEntry(lambda)),
            "only unused MoveOnly Value parameters produce lambda-entry drops"
        );
    }
    let unused_tail = UnitExpressionId::new(
        source_unit,
        expression_with_text(&sources, &consumer, "\"unused-a\" + \"unused-b\""),
    );
    assert!(
        ownership
            .drops()
            .iter()
            .any(|fact| fact.point() == UnitDropPoint::AfterBinaryOperands(unused_tail)),
        "MoveOnly Value parameters no longer defer lambda body drop planning"
    );
    ownership
        .clone()
        .validate()
        .expect("lambda parameter drop facts validate");
    let reversed_names = validated_names(&sources, &reversed_inputs, &name_environment);
    let reversed_typed = validated_types(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
    );
    let reversed = check_compilation_unit_ownership(
        &sources,
        &reversed_inputs,
        &reversed_names,
        &type_environment,
        &reversed_typed,
    )
    .expect("reversed unit ownership product");
    assert_eq!(ownership.drops(), reversed.drops());
    reversed
        .validate()
        .expect("reversed lambda parameter drop facts validate");
}
