//! Real argument plans use an exact-size bucket scoped to each concrete caller.
use super::*;
use std::fmt::Write;

#[test]
fn borrowed_arguments_remain_local_to_each_caller_at_scale() {
    for count in [8, 16] {
        let mut client = String::from("package p\nimport q.accept\n");
        for i in 0..count {
            writeln!(
                client,
                "fun client{i}(): Unit {{ accept({{ n -> n + {i} }})\naccept({{ n -> n - {i} }}) }}"
            )
            .unwrap();
        }
        client.push_str("fun entry(): Unit {\n");
        for i in 0..count {
            writeln!(client, "client{i}()").unwrap();
        }
        client.push('}');
        let fixture = Fixture::new(
            "package q\nfun accept(callback: (Int) -> Int): Unit {}",
            &client,
        );
        let plan = fixture.plan();
        assert_eq!(fixture.counts(&plan, &["accept"]), [1]);
        assert_eq!(plan, fixture.reversed_plan());
        let callables = plan.callable_plan();
        let accept = fixture.target("accept");
        let mut all = std::collections::BTreeSet::new();
        for i in 0..count {
            let target = fixture.target(&format!("client{i}"));
            let instance = plan
                .iter()
                .find(|instance| instance.key().target() == target)
                .unwrap();
            let owner = instance.source_token();
            let mut routes = callables.routes_for(owner);
            assert_eq!(routes.len(), 2, "both calls stay in this caller bucket");
            assert!(routes.all(|route| {
                route
                    .dispatch_key()
                    .is_some_and(|key| key.target() == accept)
            }));
            let bucket = callables.borrowed_arguments(owner);
            assert_eq!(bucket.len(), 2, "do not walk other caller buckets");
            let mut expressions = Vec::new();
            for token in bucket {
                let Some(CallableKey::Lambda {
                    owner: actual,
                    expression,
                }) = callables.callable(token)
                else {
                    panic!("one actual lambda per Borrow argument");
                };
                assert_eq!(*actual, owner);
                assert_eq!(callables.borrowed_argument(owner, *expression), Some(token));
                assert!(
                    all.insert(token),
                    "separate caller scopes cannot share this lambda"
                );
                expressions.push(*expression);
            }
            assert!(
                expressions[0] < expressions[1],
                "preserve exact source operand order"
            );
        }
        for name in ["accept", "entry"] {
            let target = fixture.target(name);
            let source = plan
                .iter()
                .find(|instance| instance.key().target() == target)
                .unwrap()
                .source_token();
            assert_eq!(
                callables.routes_for(source).len(),
                if name == "entry" { count } else { 0 }
            );
            assert_eq!(callables.borrowed_arguments(source).len(), 0);
        }
        assert_eq!(all.len(), 2 * count);
    }
}
