//! Non-vacuous publication gates; these do not replace the full Single event stream.
use super::*;

const PREFIX: &str = "fun read(text: String): Unit {}\nfun run(): Unit { read(\"before\")";

fn assert_successful_prefix() {
    let observed = observe(&format!("{PREFIX} }}"));
    assert_units_admitted(&observed);
    assert_fixed_ends(
        &observed,
        &[ExpectedEnd {
            call: (50, 64),
            argument: (55, 63),
            parameter: (9, 21),
            point: ExpectedPoint::CallReturn,
            target: ExpectedTarget::Temporary,
        }],
    );
    for unit in &observed.units {
        let [event] = unit.direct_call_loan_ends() else {
            panic!("one actual prefix event");
        };
        let [drop] = unit.drops() else {
            panic!("one original temporary owner cleanup");
        };
        assert_eq!(
            drop.target(),
            UnitDropTarget::Temporary(event.loan().argument())
        );
        assert_eq!(drop.point(), event.point());
        assert_eq!(event.preceding_drops(), 0);
    }
}

#[test]
fn later_ownership_error_revokes_a_proven_prefix_event_and_cleanup() {
    assert_successful_prefix();
    let observed = observe(&format!(
        "{PREFIX}; val source = \"held\"; val moved = source; println(source) }}"
    ));
    assert_fixed_ends(&observed, &[]);
    for unit in &observed.units {
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0131"),
            "{:?}",
            unit.diagnostics()
        );
        assert!(unit.drops().is_empty());
        assert!(unit.clone().validate().is_err());
    }
}

#[test]
fn later_deferred_work_revokes_a_proven_prefix_event_capability() {
    assert_successful_prefix();
    let observed = observe(&format!(
        "{PREFIX}; var callback: () -> Int = {{ 1 }}; val old = replace(&callback, {{ 2 }}) }}"
    ));
    assert_fixed_ends(&observed, &[]);
    for unit in &observed.units {
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
        assert!(!unit.deferred().is_empty());
        assert!(unit.clone().validate().is_err());
    }
}
