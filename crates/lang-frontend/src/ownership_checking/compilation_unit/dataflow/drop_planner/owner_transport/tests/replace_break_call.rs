//! Real checked targets and actual pending handoff, independent of the B bind/drop suffix.
use super::*;
use crate::ownership_checking::{
    CompilationUnitOwnership, UnitDropTarget, UnitOwnerReplaceBreakCallActionV1 as A,
};
use crate::type_checking::UnitCallTarget;

const SINK: &str = "fun sink(own value: String) {}";
const RUN: &str = r#"fun run(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; sink(held) }"#;
fn boundary(
    owned: &CompilationUnitOwnership,
) -> &crate::ownership_checking::UnitOwnerReplaceBreakObservationV1 {
    let found: Vec<_> = owned
        .owner_observations()
        .iter()
        .filter_map(|o| o.replace_break_observation_v1())
        .collect();
    assert_eq!(found.len(), 1);
    found[0]
}
fn check(f: &Fixture, callee_file: usize) {
    let owned = f.shared();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.clone().validate().is_ok());
    let boundary = boundary(&owned);
    let consumer = boundary
        .suffix_call_consumer_v1()
        .expect("actual pending Value transfer must seal the P/Q call consumer");
    assert!(boundary.suffix_consumer_v1().is_none());
    assert_eq!(
        consumer.incoming(),
        &[
            boundary.condition_false_exit().clone(),
            boundary.break_boundary().clone()
        ]
    );
    let p = consumer.incoming()[0].owner();
    let q = consumer.incoming()[1].owner();
    assert_ne!(p, q);
    assert!(boundary.input_producers().definition(p.clone()).is_some());
    assert!(boundary.body_producers().definition(q.clone()).is_some());
    assert_eq!(boundary.break_boundary(), boundary.body_completion());
    assert_eq!(boundary.entry(), boundary.condition_false_exit());
    let call = UnitExpressionId::new(f.source(0), f.expression(0, "sink(held)"));
    let crate::parser::Expression::Call { arguments, .. } = f.parsed[0]
        .ast()
        .expressions()
        .get(call.expression())
        .unwrap()
        .payload()
    else {
        panic!("real sink call")
    };
    let argument = UnitExpressionId::new(f.source(0), arguments[0].value);
    let target = f.symbol(callee_file, "sink");
    assert_eq!(
        (consumer.call(), consumer.argument(), consumer.target()),
        (call, argument, target)
    );
    assert_eq!(
        consumer.actions(),
        &[
            A::Take(argument, f.local(0, "held")),
            A::Call {
                call,
                argument,
                target
            }
        ]
    );
    let UnitCallTarget::Declaration(declaration) = f.typed.types().call(call).unwrap().target()
    else {
        panic!("resolved declaration")
    };
    assert_eq!(
        f.typed
            .types()
            .signatures()
            .declaration(declaration)
            .unwrap()
            .symbol(),
        target
    );
    let previous = f.local(0, "previous");
    let value = f.symbol(callee_file, "value");
    let drops: Vec<_> = owned.drops().iter().map(|d| d.target()).collect();
    assert_eq!(drops.len(), 2);
    assert_eq!(
        drops
            .iter()
            .filter(|d| **d == UnitDropTarget::Named(previous))
            .count(),
        1
    );
    assert_eq!(
        drops
            .iter()
            .filter(|d| **d == UnitDropTarget::Named(value))
            .count(),
        1
    );
    assert!(!drops.contains(&UnitDropTarget::Named(f.local(0, "held"))));
    assert!(!drops.contains(&UnitDropTarget::Temporary(argument)));
    compare(f, &f.parts(&owned));
}
#[test]
fn replace_break_call_consumes_real_pending_value() {
    for explicit in [false, true] {
        for same_text in [false, true] {
            let mut run = RUN.to_owned();
            if explicit {
                run = run.replace("flag: Boolean)", "flag: Boolean): Unit");
            }
            if same_text {
                run = run.replace("\"next\"", "\"outer\"");
            }
            check(&Fixture::new(&[("root.ko", &format!("{SINK}\n{run}"))]), 0);
        }
    }
}
#[test]
fn replace_break_call_cross_source_target_is_resolved() {
    check(&Fixture::new(&[("a.ko", RUN), ("b.ko", SINK)]), 1);
}
#[test]
fn replace_break_call_declines_neighbor_shapes() {
    let cases = [
        format!("{}\n{RUN}", SINK.replace("own value", "value")),
        format!("{SINK}\n{}", RUN.replace("sink(held)", "sink((held))")),
        format!(
            "{SINK}\n{}",
            RUN.replace("sink(held)", "sink(value = held)")
        ),
        format!(
            "{SINK}\n{}",
            RUN.replace("sink(held)", "sink(held); val after = 1")
        ),
        format!(
            "{SINK}\n{}",
            RUN.replace("while(flag)", "while((flag == true))")
        ),
        format!(
            "{SINK}\n{}",
            RUN.replace("while(flag)", "while(flag && flag)")
        ),
        format!("{SINK}\n{}", RUN.replace("break", "continue")),
        format!("fun sink(own value: String): Int = 1\n{RUN}"),
        format!(
            "fun sink(own value: String, other: Int) {{}}\n{}",
            RUN.replace("sink(held)", "sink(held, 1)")
        ),
    ];
    for text in cases {
        let f = Fixture::new(&[("negative.ko", &text)]);
        let owned = f.shared();
        assert!(
            owned.clone().validate().is_ok(),
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned
                .owner_observations()
                .iter()
                .all(|o| o.replace_break_observation_v1().is_none()),
            "{text}"
        );
        compare(&f, &f.parts(&owned));
    }
}

// Deliberately damaged products/deferred publication are compile-only locally.
#[test]
fn ci_only_replace_break_call_rejects_damaged_or_deferred_products() {
    use crate::ownership_checking::OwnershipDeferredReason;
    let text = format!("{SINK}\n{RUN}");
    let f = Fixture::new(&[("root.ko", &text)]);
    let owned = f.shared();
    let original = boundary(&owned);
    assert!(
        original
            .input_producers()
            .definition(original.break_boundary().owner())
            .is_none()
    );
    assert!(
        original
            .body_producers()
            .definition(original.entry().owner())
            .is_none()
    );
    for variant in 0..4 {
        let mut damaged = original.clone();
        let consumer = damaged.call_consumer.as_mut().unwrap();
        match variant {
            0 => {
                consumer.actions.pop();
            }
            1 => consumer.incoming.swap(0, 1),
            2 => consumer.target.target = f.local(0, "held"),
            _ => consumer.incoming[1] = consumer.incoming[0].clone(),
        }
        assert!(!damaged.is_consistent());
        assert!(damaged.suffix_call_consumer_v1().is_none());
    }
    let parts = f.parts(&owned);
    let mut raw = f.dataflow(&parts);
    assert!(raw.owner_observations.iter().any(|o| {
        o.replace_break_observation_v1()
            .and_then(|b| b.suffix_call_consumer_v1())
            .is_some()
    }));
    raw.deferred.push(UnitOwnershipDeferredFact::new(
        UnitExpressionId::new(f.source(0), f.expression(0, "flag")),
        OwnershipDeferredReason::MemberReceiver,
    ));
    let withheld = CompilationUnitOwnership::new(
        f.typed.types(),
        parts.bindings.into_values().collect(),
        parts.arguments,
        parts.receivers,
        parts.capture,
        raw,
    );
    assert!(withheld.clone().validate().is_err());
    assert!(
        withheld
            .owner_observations()
            .iter()
            .all(|o| o.replace_break_value.is_none())
    );
    let bad = Fixture::new(&[(
        "root.ko",
        &format!("{text}\nfun bad(input: String) {{ val moved = input }}"),
    )]);
    assert!(bad.shared().owner_observations().is_empty());
}
