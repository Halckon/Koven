use super::super::origin_probe::Event;
use super::*;

#[test]
fn ordinary_literal_binding_and_applied_replace_trace_has_no_legacy_token_dependency() {
    let source = r#"fun run(count: Int) {
var a = "initial"
var b = "kept"
var counter = count
while (counter > 0) {
val old = replace(&a, "next")
counter = counter - 1
}
println(b)
}"#;
    let fixture = Fixture::new(&[("origins.ko", source)]);
    let owned = fixture.shared();
    let a = fixture.local(0, "a");
    let b = fixture.local(0, "b");
    let old = fixture.local(0, "old");
    let expression = |text| {
        crate::type_checking::UnitExpressionId::new(fixture.source(0), fixture.expression(0, text))
    };
    let initial = expression("\"initial\"");
    let kept = expression("\"kept\"");
    let next = expression("\"next\"");
    let call = expression("replace(&a, \"next\")");
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let make = || {
            super::super::super::DropPlanner::new(
                checker,
                super::super::super::liveness::build(checker, true).unwrap(),
            )
        };
        let baseline = make().run().unwrap();
        let mut planner = make();
        planner.origin_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        assert_eq!(baseline.facts, observed.facts);
        assert_eq!(
            baseline.conditional_receiver_facts,
            observed.conditional_receiver_facts
        );
        assert_eq!(baseline.owner_observations, observed.owner_observations);
        assert_eq!(
            observed
                .origin_probe
                .as_ref()
                .unwrap()
                .iter()
                .filter_map(|event| match event {
                    Event::Dropped(fact) => Some(*fact),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            baseline
                .facts
                .iter()
                .map(|fact| fact.into_unit(fixture.source(0)))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            observed
                .origin_probe
                .as_ref()
                .unwrap()
                .iter()
                .filter(|event| !matches!(event, Event::Dropped(_)))
                .cloned()
                .collect::<Vec<_>>(),
            vec![
                Event::Literal(initial),
                Event::Bound(a, initial),
                Event::Literal(kept),
                Event::Bound(b, kept),
                Event::Literal(next),
                Event::Applied {
                    call,
                    root: a,
                    incoming: next
                },
                Event::Bound(old, call),
            ]
        );
    });
}

#[test]
fn ordinary_move_and_drop_trace_records_actual_removal_and_publication() {
    let fixture = Fixture::new(&[("move.ko", "fun run() { val a = \"source\"; val b = a }")]);
    let owned = fixture.shared();
    let a = fixture.local(0, "a");
    let b = fixture.local(0, "b");
    compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
        let mut planner = super::super::super::DropPlanner::new(
            checker,
            super::super::super::liveness::build(checker, true).unwrap(),
        );
        planner.origin_probe = Some(Vec::new());
        let observed = planner.run().unwrap();
        let events = observed.origin_probe.unwrap();
        let [
            Event::Literal(literal),
            Event::Bound(first, initial),
            Event::Moved { expression, source },
            Event::Bound(second, value),
            Event::Dropped(drop),
        ] = events.as_slice()
        else {
            panic!("exact real move/bind/drop sequence: {events:?}")
        };
        assert_eq!(*first, a);
        assert_eq!(*source, a);
        assert_eq!(*second, b);
        assert_eq!(literal, initial);
        assert_eq!(expression, value);
        assert_eq!(
            drop.target(),
            crate::ownership_checking::UnitDropTarget::Named(b)
        );
        assert_eq!(owned.drops(), &[*drop]);
    });
}
