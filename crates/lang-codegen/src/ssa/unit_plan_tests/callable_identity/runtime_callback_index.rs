//! Runtime constructor and map callbacks are admitted only with their checked loans.
use super::*;
use lang_frontend::type_checking::ContainerConstructionKind;

#[test]
fn checked_runtime_callbacks_keep_their_source_and_operand_identity() {
    let fixture = Fixture::new(
        "package q\nfun helper(): Unit {
            val items = List<Int>(2, { n -> n + 1 })
            val map = mapOf<String, String>()
            val present = map.withValue(\"key\", { value -> println(value) })
        }",
        "package p\nimport q.helper\nfun entry(): Unit {
            val items = Array<Int>(2, { n -> n + 2 })
            val map = mapOf<String, String>()
            val present = map.withValue(\"key\", { value -> println(value) })
            helper()
        }",
    );
    let plan = fixture.plan();
    assert_eq!(plan, fixture.reversed_plan());
    let lanes = ["helper", "entry"].map(|name| {
        plan.iter()
            .find(|instance| instance.key().target() == fixture.target(name))
            .unwrap()
            .source_token()
    });
    let typed = fixture.typed.types();
    let loans = fixture.owned.ownership().loans();
    let constructions = typed
        .container_constructions()
        .iter()
        .filter(|descriptor| descriptor.kind() == ContainerConstructionKind::RuntimeLength);
    let maps = typed.map_with_values();
    assert_eq!(maps.len(), 2);
    assert_eq!(constructions.clone().count(), 2);

    let mut tokens = std::collections::BTreeSet::new();
    for (call, action) in constructions
        .map(|descriptor| (descriptor.expression(), None))
        .chain(
            maps.iter()
                .map(|descriptor| (descriptor.expression(), Some(descriptor.action()))),
        )
    {
        assert!(
            loans.iter().any(|loan| loan.call() == call
                && action.is_none_or(|argument| loan.argument() == argument))
        );
        let [(_, token, source)] = lanes
            .iter()
            .filter_map(|&source| {
                plan.callable_plan()
                    .runtime_callback(source, call)
                    .map(|token| (call, token, source))
            })
            .collect::<Vec<_>>()[..]
        else {
            panic!("each callback belongs to exactly one caller: {call:?}");
        };
        assert!(tokens.insert(token));
        assert!(matches!(
            plan.callable_plan().callable(token),
            Some(CallableKey::Lambda { owner, expression })
                if *owner == source && expression.source_unit() == call.source_unit()
                    && action.is_none_or(|argument| *expression == argument)
        ));
    }
    assert_eq!(tokens.len(), 4);
}
