//! Branches must not be flattened into the straight-line identity replay.
use super::*;

// The same admitted sources also exercise observation-disabled planner equivalence.
pub(in crate::ownership_checking::compilation_unit) const BRANCHES: &[&str] = &[
    r#"if (flag) "left" else "right""#,
    r#"if (flag) { if (flag) "left" else "middle" } else "right""#,
    r#"if (flag) replace(&a, "left") else replace(&a, "right")"#,
    r#"when (flag) { true -> "left"; else -> "right" }"#,
    r#"when { flag -> replace(&a, "left"); else -> replace(&a, "right") }"#,
    r#"if (flag) { return } else { val local = "right" }"#,
    r#"if (flag) { val local = "left" }"#,
];

pub(in crate::ownership_checking::compilation_unit) fn source(control: &str) -> String {
    format!(
        r#"fun run(flag: Boolean): Unit {{
var a = "old"
val prefix = replace(&a, "new")
{control}
}}
fun independent(): Unit {{
var root = "kept"
val old = replace(&root, "replacement")
}}"#
    )
}

#[test]
fn each_branch_shape_rejects_at_its_control_before_any_path_is_published() {
    for control in BRANCHES {
        let text = source(control);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{control}");
        let [rejected, independent] = owned.owner_observations() else {
            panic!("both ordinary functions must have observations: {control}");
        };
        assert_eq!(
            rejected.status(),
            UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterExpression(UnitExpressionId::new(
                    fixture.source(0),
                    fixture.expression(0, control),
                )),
                reason: UnitOwnerUnsupportedReason::Syntax,
            },
            "{control}"
        );
        assert!(rejected.definitions().is_empty(), "{control}");
        assert!(rejected.events().is_empty(), "{control}");
        assert!(rejected.is_consistent(), "{control}");
        assert_eq!(independent.status(), UnitOwnerObservationStatus::Complete);
        assert!(independent.is_consistent());
        assert_eq!(independent.definitions().len(), 2);
        assert!(independent.events().iter().any(|event| matches!(
            event,
            UnitOwnerTransportEvent::Replace { previous, incoming, .. }
                if previous != incoming
        )));
    }
}

#[test]
fn unsupported_branch_cannot_publish_prefix_or_sibling_lineage() {
    let text = source(BRANCHES[2]);
    let fixture = Fixture::new(&[("root.ko", &text)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let rejected = &owned.owner_observations()[0];
    let sibling = &owned.owner_observations()[1];
    assert!(sibling.is_consistent());
    assert!(!sibling.definitions().is_empty());
    assert!(!sibling.events().is_empty());
    let mut definitions = rejected.clone();
    definitions
        .definitions
        .extend(sibling.definitions().iter().cloned());
    assert!(!definitions.is_consistent());
    let mut events = rejected.clone();
    events.events.extend(sibling.events().iter().cloned());
    assert!(!events.is_consistent());
    for definition in sibling.definitions() {
        assert!(rejected.definition(definition.id()).is_none());
    }
}
