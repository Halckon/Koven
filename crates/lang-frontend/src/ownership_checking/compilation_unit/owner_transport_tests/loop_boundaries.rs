//! Static rejection coverage only: no bounded replay is evidence of loop termination.
use super::*;

pub(in crate::ownership_checking::compilation_unit) const LOOPS: &[&str] = &[
    // Even a known zero-iteration edge must not grant loop admission.
    r#"while (false) { val local = "zero body" }"#,
    // The condition is unchanged: these are syntax/back-edge boundaries, not executions.
    r#"while (flag) { var local = "body"; val old = replace(&local, "next") }"#,
    r#"while (true) { val local = "unbounded body" }"#,
    r#"while (flag) { val local = "return body"; return }"#,
    r#"while (flag) { val local = replace(&root, "carried") }"#,
    r#"while (flag) { val local = "continue body"; continue }"#,
    r#"while (flag) { val local = "break body"; break }"#,
    r#"loop { val local = "explicit break"; break }"#,
    r#"loop { val local = "explicit continue"; continue }"#,
    r#"while (flag) { while (flag) { val local = "nested"; break }; break }"#,
];

pub(in crate::ownership_checking::compilation_unit) fn source(control: &str) -> String {
    format!(
        r#"fun run(flag: Boolean): Unit {{
var root = "prefix root"
val previous = replace(&root, "prefix replacement")
{control}
}}
fun independent(): Unit {{
var sibling_root = "sibling root"
val sibling_old = replace(&sibling_root, "sibling replacement")
}}"#
    )
}

#[test]
fn loops_reject_at_outer_statement_and_atomically_discard_observed_prefix() {
    for control in LOOPS {
        let text = source(control);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{control}");
        // The rejected observation must not erase the real loop-local cleanup product.
        let local = fixture.local(0, "local");
        assert!(
            owned
                .drops()
                .iter()
                .any(|fact| { fact.target() == UnitDropTarget::Named(local) }),
            "loop-local cleanup remains present: {control}"
        );
        let [rejected, sibling] = owned.owner_observations() else {
            panic!("two ordinary callable observations: {control}");
        };
        let statements = fixture.parsed[0]
            .ast()
            .statements()
            .iter()
            .filter_map(|(id, node)| {
                (fixture.sources.slice(node.span()).unwrap() == *control).then_some(id)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            statements.len(),
            1,
            "unique outer loop statement: {control}"
        );
        assert_eq!(
            rejected.status(),
            UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterStatement(UnitStatementId::new(
                    fixture.source(0),
                    statements[0],
                )),
                reason: UnitOwnerUnsupportedReason::Syntax,
            },
            "{control}"
        );
        assert!(rejected.definitions().is_empty(), "{control}");
        assert!(rejected.events().is_empty(), "{control}");
        assert!(rejected.value_observation_v1().is_none(), "{control}");
        assert!(rejected.is_consistent(), "{control}");
        assert_eq!(sibling.status(), UnitOwnerObservationStatus::Complete);
        assert!(sibling.is_consistent());
        assert_eq!(sibling.definitions().len(), 2);
        assert!(sibling.events().iter().any(|event| matches!(
            event,
            UnitOwnerTransportEvent::Replace { previous, incoming, .. }
                if previous != incoming
        )));
        // Unsupported cannot be made to carry even otherwise-valid sibling facts.
        let mut polluted = rejected.clone();
        polluted
            .definitions
            .extend(sibling.definitions().iter().cloned());
        assert!(!polluted.is_consistent());
        let mut polluted = rejected.clone();
        polluted.events.extend(sibling.events().iter().cloned());
        assert!(!polluted.is_consistent());
    }
}

#[test]
fn loop_fixture_prefix_is_independently_admitted_before_the_boundary() {
    let text = source("");
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let prefix = &owned.owner_observations()[0];
    assert_eq!(prefix.status(), UnitOwnerObservationStatus::Complete);
    assert!(prefix.is_consistent());
    assert_eq!(prefix.definitions().len(), 2);
    assert!(prefix.events().iter().any(|event| matches!(
        event,
        UnitOwnerTransportEvent::Replace { previous, incoming, .. }
            if previous != incoming
    )));
}
