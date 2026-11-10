//! The real Return traversal consumes the path carrier, not the legacy scalar P projection.
use super::*;
use crate::ownership_checking::{
    CompilationUnitOwnership, UnitDropTarget, UnitOwnerCallable,
    UnitOwnerReplaceBreakReturnActionV1 as A,
};
use crate::parser::Expression;
use crate::type_checking::UnitItemId;

const RUN: &str = r#"fun run(flag: Boolean): String { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; return held }"#;
fn boundary(
    owned: &CompilationUnitOwnership,
    source: crate::name_resolution::SourceUnitId,
) -> &crate::ownership_checking::UnitOwnerReplaceBreakObservationV1 {
    let found: Vec<_> = owned.owner_observations().iter().filter_map(|o| o.replace_break_observation_v1()).filter(|b| matches!(b.callable(), UnitOwnerCallable::Function(function) if function.source_unit() == source)).collect();
    assert_eq!(found.len(), 1);
    found[0]
}
fn check(f: &Fixture, file: usize, owned: &CompilationUnitOwnership) {
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.clone().validate().is_ok());
    let boundary = boundary(owned, f.source(file));
    let consumer = boundary
        .suffix_return_consumer_v1()
        .expect("actual Return must receive the saved P/Q carrier");
    assert!(
        boundary.suffix_consumer_v1().is_none() && boundary.suffix_call_consumer_v1().is_none()
    );
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
    assert!(boundary.input_producers().definition(p).is_some());
    assert!(boundary.body_producers().definition(q).is_some());
    assert_eq!(boundary.entry(), boundary.condition_false_exit());
    assert_eq!(boundary.break_boundary(), boundary.body_completion());
    let expression = UnitExpressionId::new(f.source(file), f.expression(file, "return held"));
    let Expression::Return {
        value: Some(payload),
        ..
    } = f.parsed[file]
        .ast()
        .expressions()
        .get(expression.expression())
        .unwrap()
        .payload()
    else {
        panic!("real Return payload")
    };
    let payload = UnitExpressionId::new(f.source(file), *payload);
    assert_eq!(
        (consumer.return_expression(), consumer.payload()),
        (expression, payload)
    );
    assert_eq!(
        consumer.actions(),
        &[
            A::Take(payload, f.local(file, "held")),
            A::Return {
                expression,
                payload
            }
        ]
    );
    let previous = f.local(file, "previous");
    let drops: Vec<_> = owned
        .drops()
        .iter()
        .filter(
            |d| matches!(d.target(), UnitDropTarget::Named(s) if s.source_unit() == f.source(file)),
        )
        .map(|d| d.target())
        .collect();
    assert_eq!(drops, [UnitDropTarget::Named(previous)]);
    assert!(
        owned
            .drops()
            .iter()
            .all(|d| d.target() != UnitDropTarget::Temporary(payload))
    );
    let UnitOwnerCallable::Function(function) = boundary.callable() else {
        panic!("ordinary function")
    };
    assert_eq!(
        function,
        UnitItemId::new(f.source(file), f.parsed[file].roots()[0])
    );
}
#[test]
fn replace_break_return_consumes_real_payload() {
    for same_text in [false, true] {
        let text = if same_text {
            RUN.replace("\"next\"", "\"outer\"")
        } else {
            RUN.to_owned()
        };
        let f = Fixture::new(&[("root.ko", &text)]);
        let owned = f.shared();
        check(&f, 0, &owned);
        compare(&f, &f.parts(&owned));
    }
}
#[test]
fn replace_break_return_keeps_same_unit_callables_separate() {
    let second = RUN.replace("fun run(", "fun second(");
    let f = Fixture::new(&[("a.ko", RUN), ("b.ko", &second)]);
    let owned = f.shared();
    check(&f, 0, &owned);
    check(&f, 1, &owned);
    assert_ne!(
        boundary(&owned, f.source(0)).entry().owner(),
        boundary(&owned, f.source(1)).entry().owner()
    );
    compare(&f, &f.parts(&owned));
}
#[test]
fn replace_break_return_declines_neighbor_shapes() {
    let cases = [
        RUN.replace("return held", "return (held)"),
        RUN.replace("return held", "return \"other\""),
        RUN.replace("return held", "val retired = held; return retired"),
        RUN.replace("while(flag)", "while((flag == flag))"),
        RUN.replace("while(flag)", "while(flag && flag)"),
        RUN.replace("break", "continue"),
        RUN.replace("): String", "): String?"),
        RUN.replace("flag: Boolean)", "flag: Boolean, other: Boolean)"),
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

// Deliberately damaged products and raw deferred publication: compile locally, run only in CI.
#[test]
fn ci_only_replace_break_return_rejects_damaged_or_deferred_products() {
    use crate::ownership_checking::OwnershipDeferredReason;
    let f = Fixture::new(&[("root.ko", RUN)]);
    let owned = f.shared();
    let original = boundary(&owned, f.source(0));
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
        let consumer = damaged.return_consumer.as_mut().unwrap();
        match variant {
            0 => {
                consumer.actions.pop();
            }
            1 => consumer.incoming.swap(0, 1),
            2 => consumer.target.payload = consumer.target.expression,
            _ => consumer.incoming[1] = consumer.incoming[0].clone(),
        }
        assert!(!damaged.is_consistent());
        assert!(damaged.suffix_return_consumer_v1().is_none());
    }
    let parts = f.parts(&owned);
    let mut raw = f.dataflow(&parts);
    assert!(raw.owner_observations.iter().any(|o| {
        o.replace_break_observation_v1()
            .and_then(|b| b.suffix_return_consumer_v1())
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
    let text = format!("{RUN}\nfun bad(input: String) {{ val moved = input }}");
    let bad = Fixture::new(&[("root.ko", &text)]);
    assert!(bad.shared().owner_observations().is_empty());
}
