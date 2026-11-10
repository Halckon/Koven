//! The real held owner is observed after an incoming move on every actual If path.
use super::*;
const SOURCE: &str = r#"fun run(flag: Boolean, other: Boolean) { val seed = "outer"; val held = seed; while(flag) { val local = "inner"; if(other) {} else {} }; val retired = held }"#;
#[test]
fn real_named_input_reaches_the_actual_normal_join() {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    let joined = owned.owner_observations()[0]
        .invariant_normal_join_observation_v1()
        .expect("actual named input through both normal arms");
    assert!(joined.is_consistent());
    assert_eq!(joined.entry(), joined.join_boundary());
    assert_eq!(joined.entry(), joined.backedge());
    check(&fixture, 0, &owned, "", "");
    compare(&fixture, &fixture.parts(&owned));
}

const EXITS: [(&str, &str); 9] = [
    ("", ""),
    ("continue", "continue"),
    ("continue", "break"),
    ("break", "continue"),
    ("break", "break"),
    ("", "continue"),
    ("continue", ""),
    ("", "break"),
    ("break", ""),
];
fn catalog(
    observation: &crate::ownership_checking::UnitOwnerObservation,
) -> &crate::ownership_checking::UnitOwnerProducerCatalogV1 {
    use crate::ownership_checking::compilation_unit::owner_transport::InvariantLoopValueV1 as I;
    match observation.invariant_loop_value.as_deref().unwrap() {
        I::NormalJoin(v) => v.input_producers(),
        I::MixedJoin(v) => v.input_producers(),
        I::SplitContinue(v) => v.input_producers(),
        I::SplitExit(v) => v.input_producers(),
        _ => panic!("only conditional input evidence"),
    }
}
pub(super) fn check(
    fixture: &Fixture,
    index: usize,
    owned: &crate::ownership_checking::CompilationUnitOwnership,
    left: &str,
    right: &str,
) {
    match (left, right) {
        ("", "") => super::invariant_normal_join::check(fixture, index, owned),
        ("continue", "continue") => super::invariant_split_continue::check(fixture, index, owned),
        ("", _) | (_, "") => super::invariant_mixed_join::check(fixture, index, owned),
        _ => super::invariant_split_exits::check(fixture, index, owned),
    }
    let observation = &owned.owner_observations()[index];
    assert!(
        observation
            .invariant_loop_value
            .as_deref()
            .unwrap()
            .suffix_boundaries()
            .is_none()
    );
    let seed = fixture.local(index, "seed");
    assert!(
        owned
            .drops()
            .iter()
            .all(|d| d.target() != crate::ownership_checking::UnitDropTarget::Named(seed))
    );
}
#[test]
fn named_input_preserves_each_real_conditional_path() {
    for (condition, other) in [
        ("flag", "other"),
        ("(flag)", "(other)"),
        ("!flag", "other"),
        ("flag", "!other"),
        ("!(!flag)", "(!other)"),
    ] {
        for result in ["", ": Unit"] {
            for body in [
                r#"val local = "inner""#,
                r#"val local = "inner"; val alias = (local)"#,
                r#"var root = "old"; val incoming = "next"; val old = replace(&root, incoming)"#,
                r#"var left = "left"; var right = "right"; val old = replace(&left, replace(&right, "next"))"#,
            ] {
                for (left, right) in EXITS {
                    let text = format!(
                        r#"fun run(flag: Boolean, other: Boolean){result} {{ val seed = "outer"; val held = seed; while({condition}) {{ {body}; if({other}) {{ {left} }} else {{ {right} }} }}; val retired = held }}"#
                    );
                    let fixture = Fixture::new(&[("root.ko", &text)]);
                    let owned = fixture.shared();
                    assert!(owned.clone().validate().is_ok(), "{text}");
                    check(&fixture, 0, &owned, left, right);
                    compare(&fixture, &fixture.parts(&owned));
                }
            }
        }
    }
    for (left, right) in EXITS {
        let text = SOURCE.replace(
            "if(other) {} else {}",
            &format!("if(other) {{ {left} }} else {{ {right} }}"),
        );
        let second = text.replace("run", "second");
        let fixture = Fixture::new(&[("a.ko", &text), ("b.ko", &second)]);
        let owned = fixture.shared();
        let again = fixture.shared();
        assert!(owned.clone().validate().is_ok() && again.clone().validate().is_ok());
        assert_eq!(fixture.expression(0, "seed"), fixture.expression(1, "seed"));
        for index in 0..2 {
            check(&fixture, index, &owned, left, right);
            check(&fixture, index, &again, left, right);
            let local = catalog(&owned.owner_observations()[index]);
            for foreign in [
                catalog(&owned.owner_observations()[1 - index]),
                catalog(&again.owner_observations()[index]),
            ] {
                assert!(
                    foreign
                        .definitions()
                        .iter()
                        .all(|d| local.definition(d.id()).is_none())
                );
            }
        }
    }
}
#[test]
fn conditional_named_input_keeps_unsupported_states_and_atomic_failure() {
    for text in [
        SOURCE.replace("val seed", "var seed"),
        SOURCE.replace("val held", "var held"),
        SOURCE.replace("val retired", "var retired"),
        SOURCE.replace("val seed =", "val seed: String? ="),
        SOURCE.replace("val held =", "val held: String? ="),
        SOURCE.replace("val retired =", "val retired: String? ="),
        SOURCE.replace("= \"outer\"", "= (\"outer\")"),
        SOURCE.replace("= seed", "= (seed)"),
        SOURCE.replace("= held", "= (held)"),
        SOURCE.replace("val held = seed", "val middle = seed; val held = middle"),
        SOURCE.replace("= \"outer\"", "= \"${flag}\""),
        format!(
            "{}\nfun make(): String = \"outer\"",
            SOURCE.replace("\"outer\"", "make()")
        ),
        SOURCE.replace(
            "val retired = held",
            "val retired = held; val extra = \"extra\"",
        ),
        SOURCE.replace("val retired = held", "val retired = \"other\""),
        SOURCE.replace("if(other)", "if(flag)"),
        SOURCE.replace("while(flag)", "while(false)"),
        SOURCE.replace("if(other)", "if(false)"),
        SOURCE.replace("if(other)", "if(other == true)"),
        format!(
            "{}\nfun predicate(value: Boolean): Boolean = value",
            SOURCE.replace("if(other)", "if(predicate(other))")
        ),
        SOURCE.replace("if(other) {} else {}", "if(other) { (continue) } else {}"),
        SOURCE.replace(
            "if(other) {} else {}",
            "if(other) { if(other) {} else {} } else {}",
        ),
        SOURCE.replace("if(other) {} else {}", "if(other) {}"),
        SOURCE.replace(
            "if(other) {} else {}",
            "if(other) { val arm = \"arm\" } else {}",
        ),
        SOURCE.replace(
            "if(other) {} else {}",
            "if(other) {} else {}; val late = \"late\"",
        ),
    ] {
        let fixture = Fixture::new(&[("negative.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{text}");
        assert!(
            owned.owner_observations()[0].invariant_loop_value.is_none(),
            "{text}"
        );
        compare(&fixture, &fixture.parts(&owned));
    }
    for text in [
        SOURCE.replace("val local = \"inner\"", "val local = held"),
        SOURCE.replace("val local = \"inner\"", "val local = seed"),
        format!("{SOURCE}\nfun bad(input: String) {{ val moved = input }}"),
    ] {
        let fixture = Fixture::new(&[("failed.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err() && failed.owner_observations().is_empty());
    }
}
