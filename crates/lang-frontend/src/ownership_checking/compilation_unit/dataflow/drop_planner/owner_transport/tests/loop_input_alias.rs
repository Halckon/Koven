//! The invariant owner enters the loop through one real named move, not a new producer.
use super::*;
const SOURCE: &str = r#"fun run(flag: Boolean) { val seed = "outer"; val held = seed; while(flag) { val local = "inner" }; val retired = held }"#;
#[test]
fn actual_named_move_supplies_the_loop_input() {
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    assert!(owned.clone().validate().is_ok());
    check(&fixture, 0, &owned, "");
    let boundary = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    let outer = boundary
        .post_loop_transport_v1()
        .expect("actual incoming move and final consumption");
    assert!(outer.is_consistent());
    assert_eq!(outer.prefix().len(), 7);
    assert_eq!(outer.producers().definitions().len(), 1);
    compare(&fixture, &fixture.parts(&owned));
}

fn check(
    fixture: &Fixture,
    index: usize,
    owned: &crate::ownership_checking::CompilationUnitOwnership,
    jump: &str,
) {
    use crate::ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitOwnerOrigin, UnitOwnerValueEventV1 as Event,
        UnitOwnerValueV1 as Value,
    };
    use crate::type_checking::UnitStatementId;
    let observation = &owned.owner_observations()[index];
    let (entry, body, outer) = if jump == "break" {
        super::invariant_break::check_break(fixture, index, owned);
        let b = observation.invariant_break_observation_v1().unwrap();
        (
            b.entry(),
            b.body_transport(),
            b.post_loop_transport_v1().unwrap(),
        )
    } else {
        super::invariant_while::check(
            fixture,
            index,
            owned,
            (!jump.is_empty()).then(|| fixture.expression(index, jump)),
        );
        let b = observation.invariant_loop_observation_v1().unwrap();
        (
            b.entry(),
            b.body_transport(),
            b.post_loop_transport_v1().unwrap(),
        )
    };
    let source = fixture.source(index);
    let initializer = UnitExpressionId::new(source, fixture.expression(index, "\"outer\""));
    let alias = UnitExpressionId::new(source, fixture.expression(index, "seed"));
    let use_expression = UnitExpressionId::new(source, fixture.expression(index, "held"));
    let seed = fixture.local(index, "seed");
    let held = fixture.local(index, "held");
    let retired = fixture.local(index, "retired");
    let suffix = fixture.parsed[index]
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| {
            fixture
                .sources
                .slice(node.span())
                .unwrap()
                .starts_with("val retired")
                .then_some(UnitStatementId::new(source, id))
        })
        .unwrap();
    let [
        Event::Create(created, owner),
        Event::Bind(first, bound, Value::Definition(first_owner)),
        Event::Move(moved_at, moved_from, Value::Definition(moved)),
        Event::Bind(input, received, Value::Definition(input_owner)),
        Event::Move(used_at, used_from, Value::Definition(used)),
        Event::Bind(output, installed_at, Value::Definition(installed)),
        Event::Drop(point, dropped, Value::Definition(consumed)),
    ] = outer.prefix()
    else {
        panic!("exact real incoming and outgoing moves")
    };
    assert!(outer.is_consistent() && outer.choice().is_none() && outer.suffix().is_empty());
    assert_eq!(outer.producers().definitions().len(), 1);
    assert_eq!((*created, *first, *bound), (initializer, seed, initializer));
    assert_eq!(
        (*moved_at, *moved_from, *input, *received),
        (alias, seed, held, alias)
    );
    assert_eq!(
        (*used_at, *used_from, *output, *installed_at),
        (use_expression, held, retired, use_expression)
    );
    assert_eq!(
        (*point, *dropped),
        (UnitDropPoint::AfterStatement(suffix), retired)
    );
    for token in [
        owner,
        first_owner,
        moved,
        input_owner,
        used,
        installed,
        consumed,
    ] {
        assert_eq!(*token, entry.owner());
        assert_eq!(
            outer
                .producers()
                .definition(token.clone())
                .unwrap()
                .origin(),
            UnitOwnerOrigin::String(initializer)
        );
        assert!(body.producers().definition(token.clone()).is_none());
    }
    assert!(
        body.producers()
            .definitions()
            .iter()
            .all(|d| outer.producers().definition(d.id()).is_none())
    );
    assert_eq!(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(retired))
            .count(),
        1
    );
    assert!(
        owned
            .drops()
            .iter()
            .any(|d| d.target() == UnitDropTarget::Named(retired) && d.point() == *point)
    );
    for moved in [seed, held] {
        assert!(
            owned
                .drops()
                .iter()
                .all(|d| d.target() != UnitDropTarget::Named(moved))
        );
    }
}

#[test]
fn named_input_preserves_actual_control_and_source_identities() {
    for condition in ["flag", "(flag)", "!flag", "(!flag)", "!(!flag)"] {
        for result in ["", ": Unit"] {
            for body in [
                r#"val local = "inner""#,
                r#"val local = "inner"; val alias = (local)"#,
                r#"var root = "old"; val incoming = "next"; val old = replace(&root, incoming)"#,
                r#"var left = "left"; var right = "right"; val old = replace(&left, replace(&right, "next"))"#,
            ] {
                for jump in ["", "continue", "break"] {
                    let tail = if jump.is_empty() {
                        String::new()
                    } else {
                        format!("; {jump}")
                    };
                    let text = format!(
                        r#"fun run(flag: Boolean){result} {{ val seed = "outer"; val held = seed; while({condition}) {{ {body}{tail} }}; val retired = held }}"#
                    );
                    let fixture = Fixture::new(&[("root.ko", &text)]);
                    let owned = fixture.shared();
                    assert!(owned.clone().validate().is_ok(), "{text}");
                    check(&fixture, 0, &owned, jump);
                    compare(&fixture, &fixture.parts(&owned));
                }
            }
        }
    }
    for jump in ["", "continue", "break"] {
        let text = if jump.is_empty() {
            SOURCE.to_string()
        } else {
            SOURCE.replace("\"inner\"", &format!("\"inner\"; {jump}"))
        };
        let second = text.replace("run", "second");
        let fixture = Fixture::new(&[("a.ko", &text), ("b.ko", &second)]);
        let owned = fixture.shared();
        let again = fixture.shared();
        assert!(owned.clone().validate().is_ok() && again.clone().validate().is_ok());
        for index in 0..2 {
            check(&fixture, index, &owned, jump);
            check(&fixture, index, &again, jump);
            let graph = |unit: &crate::ownership_checking::CompilationUnitOwnership,
                         index: usize| {
                let o = &unit.owner_observations()[index];
                if jump == "break" {
                    o.invariant_break_observation_v1()
                        .unwrap()
                        .input_producers()
                        .clone()
                } else {
                    o.invariant_loop_observation_v1()
                        .unwrap()
                        .input_producers()
                        .clone()
                }
            };
            let local = graph(&owned, index);
            for foreign in [graph(&owned, 1 - index), graph(&again, index)] {
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
fn named_input_keeps_exact_admission_and_failed_publication_boundaries() {
    for text in [
        SOURCE.replace("val seed", "var seed"),
        SOURCE.replace("val held", "var held"),
        SOURCE.replace("val retired", "var retired"),
        SOURCE.replace("val seed =", "val seed: String? ="),
        SOURCE.replace("val held =", "val held: String? ="),
        SOURCE.replace("val retired =", "val retired: String? ="),
        SOURCE.replace("= \"outer\"", "= (\"outer\")"),
        SOURCE.replace("= seed", "= (seed)"),
        SOURCE.replace("\"outer\"", "\"${flag}\""),
        format!(
            "{}\nfun make(): String = \"outer\"",
            SOURCE.replace("\"outer\"", "make()")
        ),
        SOURCE.replace("= held", "= (held)"),
        SOURCE.replace("val held = seed", "val middle = seed; val held = middle"),
        SOURCE.replace(
            "val retired = held",
            "val retired = held; val extra = \"extra\"",
        ),
        SOURCE.replace("val local = \"inner\"", ""),
        SOURCE.replace("while(flag)", "while(false)"),
        SOURCE.replace("while(flag)", "while(flag == true)"),
        SOURCE.replace("val local = \"inner\"", "val local = \"inner\"; (continue)"),
        SOURCE.replace(
            "val local = \"inner\"",
            "val local = \"inner\"; break; val dead = \"dead\"",
        ),
        SOURCE.replace(
            "val local = \"inner\"",
            "val local = \"inner\"; while(flag) { break }",
        ),
    ] {
        let fixture = Fixture::new(&[("negative.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{text}");
        let observed = &owned.owner_observations()[0];
        assert!(
            observed.invariant_loop_observation_v1().is_none()
                && observed.invariant_break_observation_v1().is_none(),
            "{text}"
        );
        compare(&fixture, &fixture.parts(&owned));
    }
    for (left, right) in [
        ("", ""),
        ("continue", "continue"),
        ("continue", "break"),
        ("break", "continue"),
        ("break", "break"),
        ("", "continue"),
        ("continue", ""),
        ("", "break"),
        ("break", ""),
    ] {
        let text = SOURCE
            .replace("flag: Boolean", "flag: Boolean, other: Boolean")
            .replace(
                "val local = \"inner\"",
                &format!("val local = \"inner\"; if(other) {{ {left} }} else {{ {right} }}"),
            );
        for direct in [false, true] {
            let text = if direct {
                text.replace(
                    "val seed = \"outer\"; val held = seed",
                    "val held = \"outer\"",
                )
            } else {
                text.clone()
            };
            let fixture = Fixture::new(&[("conditional.ko", &text)]);
            let owned = fixture.shared();
            assert!(owned.clone().validate().is_ok());
            let o = &owned.owner_observations()[0];
            assert!(
                o.invariant_normal_join_observation_v1().is_some()
                    || o.invariant_mixed_join_observation_v1().is_some()
                    || o.invariant_split_continue_observation_v1().is_some()
                    || o.invariant_split_exit_observation_v1().is_some(),
                "{text}"
            );
            if !direct {
                super::loop_input_alias_conditional::check(&fixture, 0, &owned, left, right);
            }
            assert!(
                o.invariant_loop_observation_v1().is_none()
                    && o.invariant_break_observation_v1().is_none()
            );
            compare(&fixture, &fixture.parts(&owned));
        }
    }
    for text in [
        SOURCE.replace("val local = \"inner\"", "val local = held"),
        SOURCE.replace("val local = \"inner\"", "val local = seed"),
        format!("{SOURCE}\nfun bad(input: String) {{ val moved = input }}"),
    ] {
        let fixture = Fixture::new(&[("failed.ko", &text)]);
        let failed = fixture.shared();
        assert!(failed.clone().validate().is_err());
        assert!(failed.owner_observations().is_empty());
    }
}

// Deliberately damaged seals compile locally; execute only in formal CI.
#[test]
fn named_input_seal_rejects_erased_moves_and_substituted_origins() {
    use crate::ownership_checking::UnitOwnerValueEventV1 as Event;
    let fixture = Fixture::new(&[("root.ko", SOURCE)]);
    let owned = fixture.shared();
    let original = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap();
    for variant in 0..4 {
        let mut damaged = original.clone();
        let prefix = &mut damaged.post_loop.as_mut().unwrap().transport.prefix;
        match variant {
            0 => {
                prefix.drain(2..4);
                if let Event::Bind(symbol, ..) = &mut prefix[1] {
                    *symbol = fixture.local(0, "held");
                }
            }
            1 => {
                for index in [1, 2] {
                    match &mut prefix[index] {
                        Event::Bind(symbol, ..) | Event::Move(_, symbol, _) => {
                            *symbol = fixture.local(0, "retired")
                        }
                        _ => unreachable!(),
                    }
                }
            }
            2 => {
                let foreign =
                    UnitExpressionId::new(fixture.source(0), fixture.expression(0, "held"));
                if let Event::Move(expression, ..) = &mut prefix[2] {
                    *expression = foreign;
                }
                if let Event::Bind(_, expression, _) = &mut prefix[3] {
                    *expression = foreign;
                }
            }
            _ => {
                prefix.remove(2);
            }
        }
        assert!(damaged.is_consistent());
        if variant < 3 {
            assert!(
                damaged
                    .post_loop
                    .as_ref()
                    .unwrap()
                    .transport
                    .is_consistent()
            );
        }
        assert!(damaged.post_loop_transport_v1().is_none());
    }
    let direct = SOURCE.replace(
        "val seed = \"outer\"; val held = seed",
        "val held = \"outer\"",
    );
    let fixture = Fixture::new(&[("direct.ko", &direct)]);
    let owned = fixture.shared();
    let mut damaged = owned.owner_observations()[0]
        .invariant_loop_observation_v1()
        .unwrap()
        .clone();
    let token = damaged.entry().owner();
    let prefix = &mut damaged.post_loop.as_mut().unwrap().transport.prefix;
    let seed = fixture.local(0, "retired");
    let alias = UnitExpressionId::new(fixture.source(0), fixture.expression(0, "held"));
    if let Event::Bind(symbol, ..) = &mut prefix[1] {
        *symbol = seed;
    }
    prefix.insert(
        2,
        Event::Move(
            alias,
            seed,
            crate::ownership_checking::UnitOwnerValueV1::Definition(token.clone()),
        ),
    );
    prefix.insert(
        3,
        Event::Bind(
            fixture.local(0, "held"),
            alias,
            crate::ownership_checking::UnitOwnerValueV1::Definition(token),
        ),
    );
    assert!(
        damaged.is_consistent()
            && damaged
                .post_loop
                .as_ref()
                .unwrap()
                .transport
                .is_consistent()
    );
    assert!(damaged.post_loop_transport_v1().is_none());
}
