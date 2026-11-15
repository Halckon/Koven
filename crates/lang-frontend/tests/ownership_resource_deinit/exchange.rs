//! Resource replace/swap transfers, interruptions and unique lexical cleanup.
use super::*;

#[test]
fn root_resource_swap_keeps_both_owners_until_scope_exit_in_binding_order() {
    for call in ["swap(&first, &second)", "swap(&second, &first)"] {
        let body = format!("{{ var first = Guard(); var second = Guard(); {call}; tick() }}");
        for drops in plans(&format!("{HEADER}fun run(): Unit {body}")) {
            assert_eq!(
                drops,
                [
                    NamedDrop {
                        name: "second".into(),
                        boundary: "statement",
                        text: body.clone()
                    },
                    NamedDrop {
                        name: "first".into(),
                        boundary: "statement",
                        text: body.clone()
                    },
                ],
                "{call}"
            );
        }
    }
}

#[test]
fn root_resource_replace_transfers_old_owner_once_and_preserves_binding_order() {
    for new in ["Guard()", "incoming"] {
        let incoming = if new == "incoming" {
            "val incoming = Guard(); "
        } else {
            ""
        };
        let body = format!(
            "{{ var first = Guard(); val second = Guard(); {incoming}val old = replace(&first, {new}); tick() }}"
        );
        for drops in plans(&format!("{HEADER}fun run(): Unit {body}")) {
            assert_eq!(
                drops,
                [
                    NamedDrop {
                        name: "old".into(),
                        boundary: "statement",
                        text: body.clone()
                    },
                    NamedDrop {
                        name: "second".into(),
                        boundary: "statement",
                        text: body.clone()
                    },
                    NamedDrop {
                        name: "first".into(),
                        boundary: "statement",
                        text: body.clone()
                    },
                ],
                "replacement={new}"
            );
        }
    }
}

#[test]
fn resource_exchange_coalesces_complete_branch_values_without_asap_release() {
    for initialization in ["Guard()", "if (flag) Guard() else Guard()"] {
        for operation in [
            "val old = replace(&first, if (flag) Guard() else Guard())",
            "swap(&first, &second)",
        ] {
            let body = format!(
                "{{ var first = {initialization}; var second = Guard(); {operation}; tick() }}"
            );
            for drops in plans(&format!("{HEADER}fun run(flag: Boolean): Unit {body}")) {
                let expected = if operation.starts_with("val old") {
                    vec!["old", "second", "first"]
                } else {
                    vec!["second", "first"]
                };
                assert_eq!(
                    drops
                        .iter()
                        .map(|drop| drop.name.as_str())
                        .collect::<Vec<_>>(),
                    expected
                );
                assert!(
                    drops
                        .iter()
                        .all(|drop| drop.boundary == "statement" && drop.text == body),
                    "{drops:?}"
                );
            }
        }
    }
}

#[test]
fn interrupted_resource_replacement_returns_cleanup_to_the_exited_scope() {
    let body = "{ var first = Guard(); val second = Guard(); replace(&first, return) }";
    for drops in plans(&format!("{HEADER}fun run(): Unit {body}")) {
        assert_eq!(
            drops,
            [
                NamedDrop {
                    name: "second".into(),
                    boundary: "transfer",
                    text: "return".into()
                },
                NamedDrop {
                    name: "first".into(),
                    boundary: "transfer",
                    text: "return".into()
                },
            ]
        );
    }
    for jump in ["break", "continue"] {
        let body = format!(
            "{{ var outer = Guard(); while (flag) {{ val inner = Guard(); replace(&outer, {jump}) }}; tick() }}"
        );
        for drops in plans(&format!("{HEADER}fun run(flag: Boolean): Unit {body}")) {
            assert_eq!(
                drops,
                [
                    NamedDrop {
                        name: "inner".into(),
                        boundary: "transfer",
                        text: jump.into()
                    },
                    NamedDrop {
                        name: "outer".into(),
                        boundary: "statement",
                        text: body.clone()
                    },
                ]
            );
        }
    }
}

#[test]
fn aborting_resource_replacement_does_not_unwind_lexical_owners() {
    for drops in plans(&format!(
        "{HEADER}fun run(): Unit {{ var first = Guard(); val second = Guard(); replace(&first, error(\"stop\")) }}"
    )) {
        assert!(drops.is_empty(), "{drops:?}");
    }
}

#[test]
fn resource_replace_and_swap_transport_distinct_owner_definitions_once() {
    use lang_frontend::ownership_checking::CleanupOwnerValue;
    let text = "class Resource(val id: Int) { deinit() {} }\nfun tick(): Unit {}\nfun run(): Unit { var first = Resource(1); var second = Resource(2); val old = replace(&first, Resource(3)); swap(&first, &second); tick() }";
    inspected_plans(text, |sources, parsed, single, unit| {
        assert_eq!(single.ownership_primitives().len(), 2);
        assert_eq!(unit.ownership_primitives().len(), 2);
        assert_eq!(
            single.drops().len(),
            3,
            "no hidden temporary or old-place drop"
        );
        assert_eq!(
            unit.drops().len(),
            3,
            "no hidden temporary or old-place drop"
        );
        let origins = single
            .drops()
            .iter()
            .map(|drop| {
                assert!(drop.condition().is_none());
                let owner = drop.owner().expect("named resource has an owner identity");
                let Some(CleanupOwnerValue::Expression { expression, .. }) =
                    single.cleanup_conditions().owner_value(owner)
                else {
                    panic!("resource must keep its original expression identity");
                };
                sources
                    .slice(parsed.ast().expressions().get(*expression).unwrap().span())
                    .unwrap()
            })
            .collect::<Vec<_>>();
        // old receives Resource(1); swap sends Resource(3) to second and Resource(2) to first.
        assert_eq!(origins, ["Resource(1)", "Resource(3)", "Resource(2)"]);
    });
}

#[test]
fn abandoned_outer_call_cleans_replaced_old_value_and_current_resource_once() {
    for exit in ["return", "error(\"stop\")"] {
        let text = format!(
            "{HEADER}fun accept(own first: Guard, last: Int): Unit {{}}\nfun run(): Unit {{ var root = Guard(); accept(replace(&root, Guard()), {exit}) }}"
        );
        inspected_plans(&text, |sources, parsed, single, unit| {
            assert_eq!(single.ownership_primitives().len(), 1);
            assert_eq!(unit.ownership_primitives().len(), 1);
            // accept's owned parameter has its own lexical drop in its body.
            let single_drops = single
                .drops()
                .iter()
                .filter(|drop| !matches!(drop.point(), DropPoint::AfterStatement(_)))
                .collect::<Vec<_>>();
            let unit_drops = unit
                .drops()
                .iter()
                .filter(|drop| !matches!(drop.point(), UnitDropPoint::AfterStatement(_)))
                .collect::<Vec<_>>();
            if exit == "return" {
                assert_eq!(single_drops.len(), 2, "{single_drops:?}");
                assert_eq!(unit_drops.len(), 2, "{unit_drops:?}");
                assert!(matches!(single_drops[0].target(), DropTarget::Temporary(_)));
                assert!(matches!(single_drops[1].target(), DropTarget::Named(_)));
                assert!(matches!(
                    unit_drops[0].target(),
                    UnitDropTarget::Temporary(_)
                ));
                assert!(matches!(unit_drops[1].target(), UnitDropTarget::Named(_)));
                assert_ne!(
                    single_drops[0].owner(),
                    single_drops[1].owner(),
                    "old and new must remain distinct owners"
                );
                for drop in single_drops {
                    let DropPoint::ControlTransfer(jump) = drop.point() else {
                        panic!("{drop:?}");
                    };
                    assert_eq!(
                        sources
                            .slice(parsed.ast().expressions().get(jump).unwrap().span())
                            .unwrap(),
                        "return"
                    );
                }
            } else {
                assert!(single_drops.is_empty(), "{single_drops:?}");
                assert!(unit_drops.is_empty(), "{unit_drops:?}");
            }
        });
    }
}

#[test]
fn returning_replaced_old_resource_transfers_it_without_a_second_drop() {
    let text = format!(
        "{HEADER}fun run(): Guard {{ var root = Guard(); return replace(&root, Guard()) }}"
    );
    unit_plans::inspected_plans(&text, |_, _, unit| {
        assert_eq!(unit.ownership_primitives().len(), 1);
        assert_eq!(unit.drops().len(), 1);
        assert!(matches!(unit.drops()[0].target(), UnitDropTarget::Named(_)));
        assert!(matches!(
            unit.drops()[0].point(),
            UnitDropPoint::ControlTransfer(_)
        ));
    });
}

#[test]
fn direct_field_resource_replace_keeps_parent_and_old_result_until_lexical_exit() {
    for binding in ["val", "var"] {
        for new in ["Guard()", "incoming"] {
            let incoming = if new == "incoming" {
                "val incoming = Guard(); "
            } else {
                ""
            };
            let body = format!(
                "{{ {binding} holder = Holder(Guard()); val sibling = Guard(); {incoming}val old = replace(&holder.state, {new}); inspect(holder.state); inspect(old); tick() }}"
            );
            let text = format!("{HEADER}class Holder(var state: Guard)\nfun run(): Unit {body}");
            let plans = inspected_plans(&text, |sources, parsed, single, unit| {
                assert_eq!(single.field_replacements().len(), 1);
                assert_eq!(unit.field_replacements().len(), 1);
                assert!(single.ownership_primitives().is_empty());
                assert!(unit.ownership_primitives().is_empty());
                assert_eq!(single.drops().len(), 3, "no old-field drop at commit");
                assert_eq!(unit.drops().len(), 3, "no old-field drop at commit");
                let parent = single.field_replacements()[0].place().root();
                let parent_drop = single
                    .drops()
                    .iter()
                    .find(|drop| drop.target() == DropTarget::Named(parent))
                    .unwrap();
                let owner = parent_drop
                    .owner()
                    .expect("parent retains its owner identity");
                let Some(lang_frontend::ownership_checking::CleanupOwnerValue::Expression {
                    expression,
                    ..
                }) = single.cleanup_conditions().owner_value(owner)
                else {
                    panic!("parent must retain its constructor owner definition");
                };
                assert_eq!(
                    sources
                        .slice(parsed.ast().expressions().get(*expression).unwrap().span())
                        .unwrap(),
                    "Holder(Guard())",
                    "field exchange does not replace the parent owner"
                );
                assert_ne!(
                    single.drops()[0].owner(),
                    parent_drop.owner(),
                    "old field result has an independent owner"
                );
                assert!(unit.clone().validate().is_ok());
            });
            for drops in plans {
                assert_eq!(
                    drops
                        .iter()
                        .map(|drop| drop.name.as_str())
                        .collect::<Vec<_>>(),
                    ["old", "sibling", "holder"]
                );
                assert!(
                    drops
                        .iter()
                        .all(|drop| drop.boundary == "statement" && drop.text == body),
                    "{drops:?}"
                );
            }
        }
    }
}

#[test]
fn direct_field_resource_return_transfers_old_and_cleans_only_complete_parent() {
    let text = format!(
        "{HEADER}class Holder(var state: Guard)\nfun run(): Guard {{ val holder = Holder(Guard()); return replace(&holder.state, Guard()) }}"
    );
    let plans = unit_plans::inspected_plans(&text, |_, _, unit| {
        assert_eq!(unit.field_replacements().len(), 1);
        assert_eq!(unit.drops().len(), 1);
        assert!(matches!(unit.drops()[0].target(), UnitDropTarget::Named(_)));
        assert!(unit.clone().validate().is_ok());
    });
    for drops in plans {
        assert_eq!(
            drops,
            [NamedDrop {
                name: "holder".into(),
                boundary: "transfer",
                text: "return replace(&holder.state, Guard())".into()
            }]
        );
    }
}

#[test]
fn direct_field_resource_interrupted_replacement_never_commits_or_unwinds_abort() {
    for exit in ["return", "error(\"stop\")"] {
        let text = format!(
            "{HEADER}class Holder(var state: Guard)\nfun run(): Unit {{ val holder = Holder(Guard()); val sibling = Guard(); replace(&holder.state, {exit}) }}"
        );
        let plans = unit_plans::inspected_plans(&text, |_, _, unit| {
            assert!(unit.field_replacements().is_empty());
            assert!(unit.ownership_primitives().is_empty());
            assert_eq!(unit.drops().len(), if exit == "return" { 2 } else { 0 });
        });
        for drops in plans {
            if exit == "return" {
                assert_eq!(
                    drops,
                    [
                        NamedDrop {
                            name: "sibling".into(),
                            boundary: "transfer",
                            text: "return".into()
                        },
                        NamedDrop {
                            name: "holder".into(),
                            boundary: "transfer",
                            text: "return".into()
                        }
                    ]
                );
            } else {
                assert!(drops.is_empty(), "{drops:?}");
            }
        }
    }
    for jump in ["break", "continue"] {
        let body = format!(
            "{{ val holder = Holder(Guard()); while (flag) {{ val inner = Guard(); replace(&holder.state, {jump}) }}; tick() }}"
        );
        let text =
            format!("{HEADER}class Holder(var state: Guard)\nfun run(flag: Boolean): Unit {body}");
        for drops in unit_plans::inspected_plans(&text, |_, _, unit| {
            assert!(unit.field_replacements().is_empty());
            assert_eq!(unit.drops().len(), 2);
        }) {
            assert_eq!(
                drops,
                [
                    NamedDrop {
                        name: "inner".into(),
                        boundary: "transfer",
                        text: jump.into()
                    },
                    NamedDrop {
                        name: "holder".into(),
                        boundary: "statement",
                        text: body.clone()
                    }
                ]
            );
        }
    }
}

#[test]
fn direct_field_resource_abandoned_result_and_parent_have_unique_cleanup() {
    for exit in ["return", "error(\"stop\")"] {
        let text = format!(
            "{HEADER}class Holder(var state: Guard)\nfun accept(own first: Guard, last: Int): Unit {{}}\nfun run(): Unit {{ val holder = Holder(Guard()); accept(replace(&holder.state, Guard()), {exit}) }}"
        );
        inspected_plans(&text, |_, _, single, unit| {
            assert_eq!(single.field_replacements().len(), 1);
            assert_eq!(unit.field_replacements().len(), 1);
            let single_drops = single
                .drops()
                .iter()
                .filter(|drop| !matches!(drop.point(), DropPoint::AfterStatement(_)))
                .collect::<Vec<_>>();
            let unit_drops = unit
                .drops()
                .iter()
                .filter(|drop| !matches!(drop.point(), UnitDropPoint::AfterStatement(_)))
                .collect::<Vec<_>>();
            if exit == "return" {
                assert_eq!(single_drops.len(), 2, "{single_drops:?}");
                assert_eq!(unit_drops.len(), 2, "{unit_drops:?}");
                assert!(matches!(single_drops[0].target(), DropTarget::Temporary(_)));
                assert!(matches!(single_drops[1].target(), DropTarget::Named(_)));
                assert!(matches!(
                    unit_drops[0].target(),
                    UnitDropTarget::Temporary(_)
                ));
                assert!(matches!(unit_drops[1].target(), UnitDropTarget::Named(_)));
                assert!(
                    single_drops
                        .iter()
                        .all(|drop| matches!(drop.point(), DropPoint::ControlTransfer(_)))
                );
                assert!(
                    unit_drops
                        .iter()
                        .all(|drop| matches!(drop.point(), UnitDropPoint::ControlTransfer(_)))
                );
                assert_ne!(single_drops[0].owner(), single_drops[1].owner());
            } else {
                assert!(single_drops.is_empty(), "{single_drops:?}");
                assert!(unit_drops.is_empty(), "{unit_drops:?}");
            }
        });
    }
}
