//! Pure Boolean wrappers change the selected edge, never the arm owner identity.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitOwnerObservationStatus, UnitOwnerUnsupportedReason,
};

#[test]
fn grouped_and_negated_parameter_conditions_keep_actual_choice_edges() {
    for condition in [
        "flag",
        "(flag)",
        "((flag))",
        "!flag",
        "(!flag)",
        "!(flag)",
        "!(!((flag)))",
    ] {
        for arms in [
            r#""left" else "right""#,
            r#"{ val left = "left"; (left) } else "right""#,
            r#""left" else { val right = "right"; (right) }"#,
            r#"{ val local = "left"; (local) } else { val local = "right"; ((local)) }"#,
        ] {
            let control = format!("if ({condition}) {arms}");
            let source = format!(
                "fun run(flag: Boolean): Unit {{ val chosen = {control} }}\nfun sibling(): Unit {{ val safe = \"safe\" }}"
            );
            let fixture = Fixture::new(&[("root.ko", &source)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty(), "{source}");
            assert!(owned.clone().validate().is_ok(), "{source}");
            let legacy = &owned.owner_observations()[0];
            assert_eq!(
                legacy.status(),
                UnitOwnerObservationStatus::Unsupported {
                    point: UnitDropPoint::AfterExpression(UnitExpressionId::new(
                        fixture.source(0),
                        fixture.expression(0, &control),
                    )),
                    reason: UnitOwnerUnsupportedReason::Syntax,
                },
                "the legacy protocol still rejects the String-valued If"
            );
            assert!(legacy.definitions().is_empty() && legacy.events().is_empty());
            let graph = legacy
                .value_observation_v1()
                .unwrap_or_else(|| panic!("real Choice for {source}"));
            // This helper derives true/false bodies, literal tokens, forwarding,
            // local moves and the final Bind/Drop point from this source's AST.
            // Negation must not swap graph inputs: they belong to If edge 0/1,
            // not to the Boolean parameter's un-negated value.
            choice_branch_local::assert_ast_graph(&fixture, &control, &graph);
            assert_eq!(owned.owner_observations().len(), 2);
            let sibling = &owned.owner_observations()[1];
            assert_eq!(sibling.status(), UnitOwnerObservationStatus::Complete);
            assert!(sibling.value_observation_v1().unwrap().choice().is_none());
            compare(&fixture, &fixture.parts(&owned));
        }
    }
}

#[test]
fn wrapped_conditions_do_not_admit_calls_constants_or_wider_boolean_trees() {
    for condition in [
        "true",
        "!false",
        "flag && flag",
        "!(flag || flag)",
        "flag == flag",
        "!(flag != flag)",
        "decision()",
        "!((decision()))",
    ] {
        let source = format!(
            "fun decision(): Boolean = true\nfun run(flag: Boolean): Unit {{ val chosen = if ({condition}) \"left\" else \"right\" }}\nfun sibling(): Unit {{ val safe = \"safe\" }}"
        );
        let fixture = Fixture::new(&[("root.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{source}");
        assert!(owned.clone().validate().is_ok(), "{source}");
        let rejected = &owned.owner_observations()[1];
        assert!(rejected.value_observation_v1().is_none(), "{source}");
        assert!(rejected.definitions().is_empty() && rejected.events().is_empty());
        assert_eq!(
            owned.owner_observations()[2].status(),
            UnitOwnerObservationStatus::Complete
        );
        compare(&fixture, &fixture.parts(&owned));
    }
}

#[test]
fn wrapped_choice_conditions_do_not_admit_loops_or_following_owner_uses() {
    for body in [
        r#"while (flag) { val chosen = if (!flag) "left" else "right"; break }"#,
        r#"loop { val chosen = if (!flag) "left" else "right"; break }"#,
        r#"val chosen = if (!flag) "left" else "right"; val callback = move { chosen }"#,
    ] {
        let source = format!("fun run(flag: Boolean): Unit {{ {body} }}");
        let fixture = Fixture::new(&[("root.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty(), "{source}");
        assert!(owned.clone().validate().is_ok(), "{source}");
        let rejected = &owned.owner_observations()[0];
        assert!(rejected.value_observation_v1().is_none(), "{source}");
        assert!(rejected.definitions().is_empty() && rejected.events().is_empty());
        compare(&fixture, &fixture.parts(&owned));
    }
}
