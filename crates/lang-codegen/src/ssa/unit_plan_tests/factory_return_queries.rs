//! A direct constructor still needs the reachable factory's frozen return ABI.

use super::*;

#[test]
fn unit_callable_return_query_direct_constructor_freezes_one_factory_owned_pointer() {
    let fixture = Fixture::new(
        "package q\nfun factory(): (Int)->Int { println(\"factory\")\nreturn ({ index -> index }) }",
        "package p\nimport q.factory\nfun entry(): Unit {\n\
         val first = List<Int>(1, factory())\n\
         val second = List<Int>(3, factory()) }",
    );
    let summary = fixture
        .owned
        .ownership()
        .pointer_callable_return(fixture.target("factory"))
        .expect("the factory has a sealed single pointer return");
    let UnitPointerCallableReturnOrigin::Lambda(lambda) = summary.origin() else {
        panic!("the actual returned value is a pointer lambda")
    };
    assert_eq!(fixture.owned.ownership().captures_of(lambda).count(), 0);
    let plan = fixture.plan();
    assert_eq!(fixture.counts(&plan, &["factory"]), [1]);
    let factory = plan
        .iter()
        .find(|instance| instance.key().target() == fixture.target("factory"))
        .unwrap();
    let returned = plan
        .callable_plan()
        .callable_return(factory.source_token())
        .expect("a reachable factory needs its return ABI even without a source helper Fn slot");
    assert_eq!(returned.function_type(), summary.function_type());
    assert!(matches!(
        plan.callable_plan().callable(returned.callable()),
        Some(CallableKey::Lambda { owner, expression })
            if *owner == factory.source_token() && *expression == lambda
    ));
    let factory_calls = fixture
        .typed
        .types()
        .calls()
        .iter()
        .filter(|call| {
            call.target() == UnitCallTarget::Declaration(declaration(&fixture.names, "factory"))
        })
        .collect::<Vec<_>>();
    assert_eq!(factory_calls.len(), 2, "two real initializer evaluations");
    let entry = plan
        .iter()
        .find(|instance| instance.key().target() == fixture.target("entry"))
        .unwrap();
    for call in factory_calls {
        assert_eq!(
            plan.callable_plan()
                .call_site(entry.source_token(), call.expression())
                .unwrap()
                .key(),
            factory.key(),
            "each use keeps its own call route while sharing one factory return token"
        );
    }
}

#[test]
fn unit_callable_return_query_direct_constructor_demands_returned_named_source_body() {
    let fixture = Fixture::new(
        "package q\nfun identity(index: Int): Int = index\n\
         fun factory(): (Int)->Int = identity",
        "package p\nimport q.factory\nfun entry(): Unit {\n\
         val items = List<Int>(3, factory()) }",
    );
    let summary = fixture
        .owned
        .ownership()
        .pointer_callable_return(fixture.target("factory"))
        .expect("the actual non-Deferred return selects one named function");
    assert_eq!(
        summary.origin(),
        UnitPointerCallableReturnOrigin::KnownFunction(fixture.target("identity"))
    );
    let plan = fixture.plan();
    assert_eq!(
        fixture.counts(&plan, &["factory", "identity"]),
        [1, 1],
        "the returned named pointer demands its body without any direct identity() call"
    );
    let factory = plan
        .iter()
        .find(|instance| instance.key().target() == fixture.target("factory"))
        .unwrap();
    let identity = plan
        .iter()
        .find(|instance| instance.key().target() == fixture.target("identity"))
        .unwrap();
    let returned = plan
        .callable_plan()
        .callable_return(factory.source_token())
        .expect("the same frozen return plan supplies the source signature");
    assert_eq!(returned.function_type(), summary.function_type());
    assert_eq!(
        plan.callable_plan().callable(returned.callable()),
        Some(&CallableKey::KnownFunction {
            function: identity.source_token()
        })
    );
}
