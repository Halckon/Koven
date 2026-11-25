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
        let declaration = declaration(&fixture.names, name);
        let target = UnitCallableTarget::Declaration(declaration);
        let token = plan
            .iter()
            .find(|instance| instance.key().target() == target)
            .unwrap()
            .source_token();
        let source =
            fixture.names.names().index().declarations()[declaration.index()].source_unit();
        (source, token)
    });
    assert_ne!(lanes[0].0, lanes[1].0);
    assert_ne!(lanes[0].1, lanes[1].1);
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
        let [(unit_a, token_a), (unit_b, token_b)] = lanes;
        let (owner, other) = match call.source_unit() {
            unit if unit == unit_a => (token_a, token_b),
            unit if unit == unit_b => (token_b, token_a),
            unit => panic!("unexpected callback source {unit:?} for {call:?}"),
        };
        let token = plan
            .callable_plan()
            .runtime_callback(owner, call)
            .unwrap_or_else(|| panic!("declaring caller {owner:?} must own {call:?}"));
        assert_eq!(
            plan.callable_plan().runtime_callback(other, call),
            None,
            "the other caller must not own {call:?}"
        );
        assert!(tokens.insert(token));
        assert!(matches!(
            plan.callable_plan().callable(token),
            Some(CallableKey::Lambda { owner: expected_owner, expression })
                if *expected_owner == owner && expression.source_unit() == call.source_unit()
                    && action.is_none_or(|argument| *expression == argument)
        ));
    }
    assert_eq!(tokens.len(), 4);
}
