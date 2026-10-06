//! Full-key routes must survive planning without relying on lowering's old symbol/type lookup.

use super::*;
use std::collections::BTreeSet;

fn source(plan: &FunctionInstancePlan, fixture: &Fixture, name: &str) -> SourceToken {
    let instances = plan
        .instances()
        .iter()
        .filter(|instance| instance.key.symbol() == fixture.symbol(name))
        .collect::<Vec<_>>();
    assert_eq!(instances.len(), 1, "one concrete source owner for {name}");
    instances[0].source
}

fn callback(
    plan: &FunctionInstancePlan,
    owner: SourceToken,
    expression: ExpressionId,
) -> CallableToken {
    let route = plan
        .call_site(owner, expression)
        .expect("one frozen route for the committed typed call");
    assert_eq!(route.callable_arguments().len(), 1);
    route.callable_arguments()[0].1
}

#[test]
fn single_callable_route_named_sparse_slots_preserve_complete_key_and_order() {
    let fixture = Fixture::analyze(
        "fun helper(first: (Int)->Int, size: Int, last: (Int)->Int): Unit {}\n\
         fun entry(): Unit { val a: (Int)->Int = { index -> index }\n\
         val b: (Int)->Int = { index -> index + 1 }\n\
         helper(a, 1, b)\nhelper(last = b, size = 2, first = a)\nhelper(b, 3, a) }",
    );
    let plan = fixture.plan().unwrap();
    let entry = source(&plan, &fixture, "entry");
    let ordinary = plan
        .call_site(entry, fixture.expression("helper(a, 1, b)"))
        .unwrap();
    let reordered = plan
        .call_site(
            entry,
            fixture.expression("helper(last = b, size = 2, first = a)"),
        )
        .unwrap();
    let swapped = plan
        .call_site(entry, fixture.expression("helper(b, 3, a)"))
        .unwrap();
    assert_eq!(
        ordinary, reordered,
        "argument source order must not become key slot order"
    );
    assert_ne!(
        ordinary, swapped,
        "the complete route retains both callback identities"
    );
    assert_eq!(
        ordinary
            .callable_arguments()
            .iter()
            .map(|(slot, _)| *slot)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert_eq!(
        swapped.callable_arguments()[0].1,
        ordinary.callable_arguments()[1].1
    );
    assert_eq!(
        swapped.callable_arguments()[1].1,
        ordinary.callable_arguments()[0].1
    );
    for key in [ordinary, swapped] {
        let instance = plan
            .instances()
            .iter()
            .find(|instance| &instance.key == key)
            .unwrap();
        assert_eq!(
            plan.source(instance.source),
            Some(key),
            "route is the full planned callee key"
        );
    }
}

#[test]
fn single_callable_route_forwarding_and_recursive_call_reuse_parameter_token() {
    let fixture = Fixture::analyze(
        "fun leaf(callback: (Int)->Int): Unit {}\n\
         fun relay(forwarded: (Int)->Int): Unit { leaf((forwarded))\nrelay(forwarded) }\n\
         fun entry(): Unit { relay({ index -> index }) }",
    );
    let plan = fixture.plan().unwrap();
    let entry = source(&plan, &fixture, "entry");
    let relay = source(&plan, &fixture, "relay");
    let token = callback(
        &plan,
        entry,
        fixture.expression("relay({ index -> index })"),
    );
    assert_eq!(
        callback(&plan, relay, fixture.expression("leaf((forwarded))")),
        token
    );
    assert_eq!(
        callback(&plan, relay, fixture.expression("relay(forwarded)")),
        token
    );
    assert_eq!(
        plan.source(relay).unwrap().callable_arguments(),
        [(0, token)]
    );
    assert_eq!(
        plan.call_site(relay, fixture.expression("relay(forwarded)")),
        plan.source(relay)
    );
    assert!(
        matches!(plan.callable(token), Some(CallableKey::Lambda { owner, expression }) if *owner == entry && expression.expression() == fixture.lambdas()[0])
    );
}

#[test]
fn single_callable_route_same_ast_lambda_has_distinct_concrete_owner_tokens() {
    let fixture = Fixture::analyze(
        "fun leaf(callback: (Int)->Int): Unit {}\n\
         fun <T> relay(own value: T): Unit { leaf({ index -> index }) }\n\
         fun entry(): Unit { relay<Int>(1)\nrelay<Boolean>(true) }",
    );
    let plan = fixture.plan().unwrap();
    let relays = plan
        .instances()
        .iter()
        .filter(|instance| instance.key.symbol() == fixture.symbol("relay"))
        .collect::<Vec<_>>();
    assert_eq!(relays.len(), 2);
    let call = fixture.expression("leaf({ index -> index })");
    let a = callback(&plan, relays[0].source, call);
    let b = callback(&plan, relays[1].source, call);
    assert_ne!(a, b);
    for (instance, token) in relays.into_iter().zip([a, b]) {
        assert!(
            matches!(plan.callable(token), Some(CallableKey::Lambda { owner, expression }) if *owner == instance.source && expression.expression() == fixture.lambdas()[0])
        );
    }
}

#[test]
fn single_callable_route_factory_memo_is_owned_by_factory_across_real_callers() {
    let fixture = Fixture::analyze(
        "fun leaf(callback: (Int)->Int): Unit {}\n\
         fun first(): Unit { val result = factory()\nval alias = (result)\nleaf(alias) }\n\
         fun second(): Unit { leaf(factory()) }\n\
         fun factory(): (Int)->Int = ({ index -> index })",
    );
    let plan = fixture.plan().unwrap();
    let factory = source(&plan, &fixture, "factory");
    let first = source(&plan, &fixture, "first");
    let second = source(&plan, &fixture, "second");
    let token = plan
        .pointer_return(factory)
        .expect("memo of one validated factory return");
    assert_eq!(
        callback(&plan, first, fixture.expression("leaf(alias)")),
        token
    );
    assert_eq!(
        callback(&plan, second, fixture.expression("leaf(factory())")),
        token
    );
    assert!(
        matches!(plan.callable(token), Some(CallableKey::Lambda { owner, expression }) if *owner == factory && expression.expression() == fixture.lambdas()[0])
    );
    assert!(plan.pointer_return(first).is_none());
    assert!(plan.pointer_return(second).is_none());
    assert_eq!(
        plan.call_site(first, fixture.expression("leaf(alias)")),
        plan.call_site(second, fixture.expression("leaf(factory())"))
    );
}

#[test]
fn single_callable_route_three_environments_point_to_three_planned_full_keys() {
    let fixture = Fixture::analyze(
        "fun leaf(callback: (Int)->Boolean): Unit {}\n\
         fun entry(label: String, own taken: String): Unit {\n\
         leaf({ index -> index == 0 })\n\
         leaf({ index -> label == \"shared\" && index == 0 })\n\
         leaf(move { index -> taken == \"owned\" && index == 0 }) }",
    );
    let plan = fixture.plan().unwrap();
    let entry = source(&plan, &fixture, "entry");
    let mut tokens = BTreeSet::new();
    for text in [
        "leaf({ index -> index == 0 })",
        "leaf({ index -> label == \"shared\" && index == 0 })",
        "leaf(move { index -> taken == \"owned\" && index == 0 })",
    ] {
        let route = plan.call_site(entry, fixture.expression(text)).unwrap();
        tokens.insert(callback(&plan, entry, fixture.expression(text)));
        assert!(
            plan.instances()
                .iter()
                .any(|instance| &instance.key == route)
        );
    }
    assert_eq!(tokens.len(), 3);
    let modes = fixture
        .lambdas()
        .into_iter()
        .map(|lambda| {
            fixture
                .owned
                .captures_of(lambda)
                .map(|capture| capture.mode())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        modes,
        [
            vec![],
            vec![ClosureCaptureMode::Shared],
            vec![ClosureCaptureMode::Owned]
        ]
    );
}
