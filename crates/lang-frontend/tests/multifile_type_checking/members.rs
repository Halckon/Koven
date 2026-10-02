use super::*;

#[test]
fn cross_file_member_bodies_calls_and_fields_publish_source_qualified_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         interface Named {\n\
             fun current(): Unit {\n\
                 val current = this\n\
             }\n\
             fun label(): String\n\
         }\n\
         class Holder<T>(val item: T) : Named {\n\
             override fun label(): String = \"holder\"\n\
             fun self(): Holder<T> = this\n\
             fun field(): T = this.item\n\
             fun <R> pick(own selected: R, fallback: T): R = selected\n\
             fun forwarded(): String = label()\n\
         }\n\
         value class Pair<T>(val first: T, val second: T)\n\
         class Tools { companion object { fun identity(input: Int): Int = input } }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun use(holder: Holder<Int>, pair: Pair<Long>): String {\n\
             val field = holder.item\n\
             val picked = holder.pick<String>(\"ok\", 1)\n\
             val component = pair.component1()\n\
             return holder.label()\n\
         }",
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
            .expect("cross-file member bodies and uses are supported");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed cross-file member bodies and uses are supported");

    assert!(
        forward.diagnostics().is_empty(),
        "{:?}",
        forward.diagnostics()
    );
    assert_eq!(forward.types(), reverse.types());
    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(
        forward.aggregate_projections(),
        reverse.aggregate_projections()
    );
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(forward.calls().len(), 4);
    let member_calls = forward
        .calls()
        .iter()
        .filter(|call| matches!(call.target(), UnitCallTarget::Symbol(_)))
        .collect::<Vec<_>>();
    assert_eq!(member_calls.len(), 3);
    let mut member_instance_arities = member_calls
        .iter()
        .map(|call| call.instance().type_arguments().len())
        .collect::<Vec<_>>();
    member_instance_arities.sort_unstable();
    assert_eq!(member_instance_arities, [1, 1, 2]);
    assert!(matches!(
        forward.calls()[2].target(),
        UnitCallTarget::StructuralComponent(_)
    ));
    assert_eq!(forward.aggregate_projections().len(), 3);
    assert!(forward.aggregate_projections().iter().all(|projection| {
        projection.field().source_unit() == source_unit(&forward_names, models_source)
    }));
    assert_eq!(
        forward
            .aggregate_projections()
            .iter()
            .filter(|projection| projection.kind() == UnitAggregateProjectionKind::Field)
            .count(),
        2
    );
    assert_eq!(
        forward
            .aggregate_projections()
            .iter()
            .filter(|projection| {
                projection.kind() == UnitAggregateProjectionKind::StructuralComponent
            })
            .count(),
        1
    );
    for projection in forward.aggregate_projections() {
        let expected = match projection.kind() {
            UnitAggregateProjectionKind::Field => ExpressionCategory::Place,
            UnitAggregateProjectionKind::StructuralComponent => ExpressionCategory::Temporary,
        };
        assert_eq!(
            forward.expression_category(projection.expression()),
            Some(expected)
        );
    }

    let model_unit = source_unit(&forward_names, models_source);
    let this_types = expressions_with_text(&sources, &models, "this")
        .into_iter()
        .map(|expression| {
            forward
                .expression_type(UnitExpressionId::new(model_unit, expression))
                .and_then(|ty| forward.types().get(ty))
                .expect("member this expression has a type")
        })
        .collect::<Vec<_>>();
    assert!(matches!(this_types[0], UnitTypeKind::StaticSelf(_)));
    let named = declaration(&forward_names, "Named");
    let named_signature = forward
        .signatures()
        .declaration(named)
        .and_then(|signature| signature.nominal())
        .expect("Named interface signature");
    let current_receiver = named_signature
        .members()
        .iter()
        .find(|member| member.name() == "current")
        .and_then(|member| member.receiver())
        .expect("interface default receiver contract");
    assert!(matches!(
        forward.types().get(current_receiver.ty()),
        Some(UnitTypeKind::StaticSelf(interface)) if *interface == named_signature.ty()
    ));
    assert!(
        this_types[1..]
            .iter()
            .all(|kind| matches!(kind, UnitTypeKind::Nominal { .. }))
    );
    let companion_input = expression_with_text(&sources, &models, "input");
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(model_unit, companion_input))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(forward.validate().is_ok());
}

#[test]
fn member_overload_trials_and_payload_errors_recover_without_leaking_facts() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         class Resolver {\n\
             fun choose(callback: (Int) -> Int): Int = 1\n\
             fun choose(callback: (String) -> String): String = \"text\"\n\
         }\n\
         enum class Maybe {\n\
             Some(item: Int), None;\n\
             fun payloadOrZero(): Int = when (this) {\n\
                 is Some -> item\n\
                 is None -> 0\n\
             }\n\
         }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun selected(resolver: Resolver): Int = resolver.choose({ input -> input + 1 })\n\
         fun mismatch(resolver: Resolver): String = resolver.choose({ input -> input + 1 })\n\
         fun payload(subject: Maybe): Int = subject.item\n\
         fun nullablePayload(subject: Maybe?): Int = subject.item",
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
            .expect("member errors stay in the recovery product");
    let reverse =
        check_compilation_unit_types(&sources, &reverse_inputs, &reverse_names, &type_environment)
            .expect("reversed member errors stay in the recovery product");

    assert_eq!(forward.expression_types(), reverse.expression_types());
    assert_eq!(forward.calls(), reverse.calls());
    assert_eq!(
        forward.aggregate_projections(),
        reverse.aggregate_projections()
    );
    assert_eq!(forward.diagnostics(), reverse.diagnostics());
    assert_eq!(
        forward
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0084", "L0113", "L0113"]
    );
    assert_eq!(forward.calls().len(), 2);
    assert!(
        forward
            .calls()
            .iter()
            .all(|call| matches!(call.target(), UnitCallTarget::Symbol(_)))
    );
    assert_eq!(forward.aggregate_projections().len(), 1);
    assert!(matches!(
        forward.aggregate_projections()[0].receiver(),
        UnitAggregateProjectionReceiver::This(_)
    ));
    let payload_item = expression_with_text(&sources, &models, "item");
    assert!(matches!(
        forward
            .expression_type(UnitExpressionId::new(
                source_unit(&forward_names, models_source),
                payload_item,
            ))
            .and_then(|ty| forward.types().get(ty)),
        Some(UnitTypeKind::Builtin(BuiltinType::Int))
    ));
    assert!(forward.validate().is_err());
}

#[test]
fn member_visibility_shapes_and_owner_dependent_bounds_are_preserved() {
    let mut sources = SourceMap::new();
    let (models_source, models) = parsed(
        &mut sources,
        "models.ko",
        "package p\n\
         interface Marker<T>\n\
         interface GenericBase { fun <T> id(input: T): T }\n\
         class Marked : Marker<Int>\n\
         class Host<T>(private val secret: T) : GenericBase {\n\
             private fun hidden(): T = secret\n\
             fun same(other: Host<T>): T = other.hidden()\n\
             fun sameField(other: Host<T>): T = other.secret\n\
             override fun <R> id(input: R): R = input\n\
             fun <R : Marker<T>> accept(input: R): R = input\n\
             fun <A> make(input: A): A = input\n\
             fun <A, B> make(first: A, second: B): B = second\n\
             fun <A> tag(input: Int): Int = input\n\
             fun <A, B> tag(input: Int): Int = input\n\
         }",
    );
    let (uses_source, uses) = parsed(
        &mut sources,
        "uses.ko",
        "package p\n\
         fun valid(host: Host<Int>, marked: Marked): Int {\n\
             val inferred = host.accept(marked)\n\
             val explicit = host.accept<Marked>(marked)\n\
             val one = host.make(1)\n\
             val two = host.make(1, \"ok\")\n\
             val tagOne = host.tag<String>(1)\n\
             val tagTwo = host.tag<String, Long>(1)\n\
             return host.id(1)\n\
         }\n\
         fun privateField(host: Host<Int>): Int = host.secret\n\
         fun privateCall(host: Host<Int>): Int = host.hidden()\n\
         fun privateSafeField(host: Host<Int>?): Int? = host?.secret\n\
         fun privateSafeCall(host: Host<Int>?): Int? = host?.hidden()\n\
         fun publicSafeCall(host: Host<Int>?): Int? = host?.sameField(host)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/uses.ko", uses_source, &uses),
        SourceUnitInput::new("root", "p/models.ko", models_source, &models),
    ];
    let (name_environment, type_environment) = standard_environments();
    let names = validated_names(&sources, &inputs, &name_environment);
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .expect("private access errors stay in the recovery product");

    assert_eq!(
        typed
            .body_diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>(),
        ["L0080", "L0080", "L0080", "L0080"]
    );
    assert_eq!(
        typed
            .calls()
            .iter()
            .filter(|call| matches!(call.target(), UnitCallTarget::Symbol(_)))
            .count(),
        8
    );
    assert_eq!(typed.aggregate_projections().len(), 2);
    assert!(
        typed
            .aggregate_projections()
            .iter()
            .any(|projection| matches!(
                projection.receiver(),
                UnitAggregateProjectionReceiver::This(_)
            ))
    );
    assert!(typed.validate().is_err());
}
