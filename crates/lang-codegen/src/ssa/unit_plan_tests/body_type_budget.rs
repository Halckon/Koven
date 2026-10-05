//! SPEC-0276: planner budgets retain their own gate and recipe arbitration.
use super::*;

#[test]
fn unit_generic_body_budget_zero_one_two_keep_pure_limit_template_spans() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun <T> inner(own item: T): Int {\n\
             val values = listOf(item)\nreturn values.size\n}\n\
         fun <T> middle(own item: T): Int = inner(item)\n\
         fun <T> relay(own item: T): Int = middle(item)",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nfun entry(): Int = p.relay(1)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    assert_concrete_body_container(&typed, IntrinsicTypeConstructor::List);
    let arena_len = typed.types().types().len();
    let entry = declaration(&names, "entry");
    let targets = ["relay", "middle", "inner"].map(|name| declaration(&names, name));
    for (limit, target) in targets.into_iter().enumerate() {
        let span = provider
            .ast()
            .items()
            .get(names.names().index().declarations()[target.index()].root())
            .unwrap()
            .span();
        assert_eq!(span.source_id(), provider_source);
        for unit_inputs in [&inputs[..], &reversed[..]] {
            let error = plan_unit_instances_with_limit(
                &sources,
                unit_inputs,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
                limit,
            )
            .expect_err("the current specialized template retains its original budget gate");
            assert_eq!(error.kind, LoweringErrorKind::InstanceLimitExceeded);
            assert_eq!(error.span, Some(span));
            assert_eq!(typed.types().types().len(), arena_len);
        }
    }
    let complete = plan_unit_instances_with_limit(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
        3,
    )
    .expect("three distinct specializations complete the ordinary chain");
    assert_eq!(
        complete.len(),
        4,
        "entry plus exactly one Int instance of each relay"
    );
    let int = typed.types().types().builtin(BuiltinType::Int).unwrap();
    for target in targets {
        let instances = complete
            .iter()
            .filter(|instance| instance.key().target() == UnitCallableTarget::Declaration(target))
            .collect::<Vec<_>>();
        let [instance] = instances.as_slice() else {
            panic!("one instance for {target:?}")
        };
        assert_eq!(instance.key().type_arguments(), &[int]);
        assert_eq!(instance.substitutions().len(), 1);
        let symbol = typed
            .types()
            .signatures()
            .declaration(target)
            .unwrap()
            .callable()
            .unwrap()
            .type_parameters()[0];
        assert_eq!(instance.substitutions().get(&symbol), Some(&int));
    }
    assert_eq!(typed.types().types().len(), arena_len);
    let [construction] = typed.types().container_constructions() else {
        panic!("only inner constructs a source container")
    };
    assert!(
        matches!(
            typed.types().types().get(construction.element_type()),
            Some(UnitTypeKind::TypeParameter(_))
        ),
        "body facts stay symbolic after planning"
    );
}

#[test]
fn unit_generic_body_budget_later_sibling_recipe_precedes_zero_one_two_limit() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         interface Base<A> { fun readGrowth(): Int = 7 }\n\
         class GrowA<T>(val next: GrowB<List<T>>)\n\
         class GrowB<U>(val next: GrowA<U>)\n\
         class Host<T>: Base<GrowA<T>> {}\n\
         fun <T> cleanA(): Int {\n\
             val values: MutableList<T> = mutableListOf()\nreturn cleanB<T>()\n}\n\
         fun <T> cleanB(): Int {\n\
             val values: MutableList<T> = mutableListOf()\nreturn cleanC<T>()\n}\n\
         fun <T> cleanC(): Int {\n\
             val values: MutableList<T> = mutableListOf()\nreturn values.size\n}\n\
         fun <T> bad(host: Host<T>): Int = host.readGrowth()",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nimport p.Host\n\
         fun entry(host: Host<Int>): Int = p.cleanA<Int>() + p.bad(host)",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &environment);
    // MutableList<Int> appears only in these real bodies; the retained recipe
    // witness happens to use List<T>, so it cannot seed this concrete demand.
    assert_concrete_body_container(&typed, IntrinsicTypeConstructor::MutableList);
    let order = ["cleanA", "cleanB", "cleanC", "bad"].map(|name| declaration(&names, name));
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]));
    let witness = typed
        .types()
        .signatures()
        .declaration(declaration(&names, "GrowB"))
        .unwrap()
        .nominal()
        .unwrap()
        .fields()[0]
        .span();
    assert_eq!(witness.source_id(), provider_source);
    assert_eq!(sources.slice(witness), Ok("next"));
    let arena_len = typed.types().types().len();
    for limit in 0..=2 {
        for unit_inputs in [&inputs[..], &reversed[..]] {
            let error = plan_unit_instances_with_limit(
                &sources,
                unit_inputs,
                &names,
                &environment,
                &typed,
                &owned,
                declaration(&names, "entry"),
                limit,
            )
            .expect_err("the later pending bad recipe must win before the current limit");
            assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
            assert_eq!(error.span, Some(witness));
            assert_eq!(typed.types().types().len(), arena_len);
        }
    }
}

fn assert_concrete_body_container(
    typed: &ValidatedCompilationUnitTypes,
    constructor: IntrinsicTypeConstructor,
) {
    let arena = typed.types().types();
    let int = arena.builtin(BuiltinType::Int).unwrap();
    assert!(
        arena
            .find(&UnitTypeKind::Intrinsic {
                constructor,
                arguments: vec![int]
            })
            .is_some(),
        "the real frontend publishes the body-only concrete identity before planner arbitration"
    );
}
