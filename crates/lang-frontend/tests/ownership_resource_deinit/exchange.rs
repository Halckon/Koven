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
fn resource_replace_and_swap_keep_three_named_lexical_drops() {
    let text = include_str!("fixtures/root-resource-exchange.ko");
    // Exact instance identity is tested on the same source by the Unit SSA/native consumer.
    let plans = unit_plans::inspected_plans(text, |_, _, unit| {
        assert_eq!(unit.ownership_primitives().len(), 2);
        assert_eq!(
            unit.drops().len(),
            3,
            "no hidden temporary or old-place drop"
        );
        assert!(
            unit.drops()
                .iter()
                .all(|drop| !drop.is_conditional_temporary())
        );
        assert!(unit.clone().validate().is_ok());
    });
    for drops in plans {
        assert_eq!(
            drops
                .iter()
                .map(|drop| drop.name.as_str())
                .collect::<Vec<_>>(),
            ["old", "second", "first"]
        );
        assert!(
            drops
                .iter()
                .all(|drop| drop.boundary == "statement" && text.ends_with(&drop.text))
        );
    }
}

#[test]
fn abandoned_outer_call_cleans_replaced_old_value_and_current_resource_once() {
    let fixture = include_str!("fixtures/abandoned-root-resource-exchange.ko.in");
    for exit in ["return", "error(\"stop\")"] {
        unit_plans::inspected_plans(&fixture.replace("$EXIT", exit), |sources, parsed, unit| {
            let [exchange] = unit.ownership_primitives() else {
                panic!("one committed root exchange");
            };
            assert!(unit.field_replacements().is_empty());
            let [place] = exchange.places() else {
                panic!("one root place");
            };
            assert!(place.is_root());
            assert_abandoned_drops(
                sources,
                parsed,
                unit,
                exchange.descriptor().expression(),
                place.root(),
                exit,
            );
        });
    }
}

fn assert_abandoned_drops(
    sources: &SourceMap,
    parsed: &ParsedFile,
    unit: &CompilationUnitOwnership,
    exchange: lang_frontend::type_checking::UnitExpressionId,
    parent: lang_frontend::name_resolution::UnitSymbolId,
    exit: &str,
) {
    // The unused owned parameter of accept has its own independent lexical drop.
    let deferred_call_drops = unit
        .drops()
        .iter()
        .filter(|drop| !matches!(drop.point(), UnitDropPoint::AfterStatement(_)))
        .collect::<Vec<_>>();
    if exit != "return" {
        assert!(deferred_call_drops.is_empty(), "{deferred_call_drops:?}");
        return;
    }
    let [old, live] = deferred_call_drops.as_slice() else {
        panic!("two abandon-time cleanup obligations: {deferred_call_drops:?}");
    };
    assert_eq!(old.target(), UnitDropTarget::Temporary(exchange));
    assert_eq!(live.target(), UnitDropTarget::Named(parent));
    let UnitDropPoint::ControlTransfer(transfer) = old.point() else {
        panic!("the original return relinquishes the pending argument");
    };
    assert_eq!(transfer.source_unit(), exchange.source_unit());
    assert_eq!(
        sources.slice(
            parsed
                .ast()
                .expressions()
                .get(transfer.expression())
                .unwrap()
                .span()
        ),
        Ok("return"),
    );
    assert_eq!(live.point(), old.point());
    assert!(!old.is_conditional_temporary());
    // The identical original source is independently checked against actual Unit SSA owners.
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
    let fixture = include_str!("fixtures/lexical-field-resource-exchange.ko.in");
    for binding in ["val", "var"] {
        for new in ["Guard()", "incoming"] {
            let incoming = if new == "incoming" {
                "val incoming = Guard(); "
            } else {
                ""
            };
            let text = fixture
                .replace("$BINDING", binding)
                .replace("$INCOMING", incoming)
                .replace("$NEW", new);
            let plans = unit_plans::inspected_plans(&text, |_, _, unit| {
                let [exchange] = unit.field_replacements() else {
                    panic!("one committed single field exchange");
                };
                assert_eq!(exchange.place().fields().len(), 1);
                assert!(unit.ownership_primitives().is_empty());
                let [_, _, parent] = unit.drops() else {
                    panic!("no old-field drop at commit");
                };
                assert_eq!(
                    parent.target(),
                    UnitDropTarget::Named(exchange.place().root())
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
                let body = text.rsplit_once("fun run(): Unit ").unwrap().1;
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
    let fixture = include_str!("fixtures/abandoned-field-resource-exchange.ko.in");
    for exit in ["return", "error(\"stop\")"] {
        unit_plans::inspected_plans(&fixture.replace("$EXIT", exit), |sources, parsed, unit| {
            let [exchange] = unit.field_replacements() else {
                panic!("one committed field exchange");
            };
            assert!(unit.ownership_primitives().is_empty());
            assert_eq!(exchange.place().fields().len(), 1);
            assert_abandoned_drops(
                sources,
                parsed,
                unit,
                exchange.descriptor().expression(),
                exchange.place().root(),
                exit,
            );
        });
    }
}
