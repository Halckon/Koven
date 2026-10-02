//! SPEC-0245: named resource owners use lexical cleanup in both frontend entries.

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, DropPoint, DropTarget, OwnershipCheckedFile, UnitDropPoint,
        UnitDropTarget, check_compilation_unit_ownership, check_ownership,
    },
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

#[derive(Debug, PartialEq, Eq)]
struct NamedDrop {
    name: String,
    boundary: &'static str,
    text: String,
}

fn plans(text: &str) -> [Vec<NamedDrop>; 2] {
    inspected_plans(text, |_, _, _, _| {})
}

fn inspected_plans(
    text: &str,
    inspect: impl FnOnce(&SourceMap, &ParsedFile, &OwnershipCheckedFile, &CompilationUnitOwnership),
) -> [Vec<NamedDrop>; 2] {
    let mut sources = SourceMap::new();
    let source = sources.add_source("resource-drop.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (name_env, type_env) = standard_environments();
    let names = resolve_names(&sources, &parsed, &name_env).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &type_env).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single_owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(
        single_owned.diagnostics().is_empty(),
        "{:?}",
        single_owned.diagnostics()
    );
    assert!(
        single_owned.deferred().is_empty(),
        "{:?}",
        single_owned.deferred()
    );
    let single = single_owned
        .drops()
        .iter()
        .filter_map(|drop| {
            let DropTarget::Named(symbol) = drop.target() else {
                return None;
            };
            let name = names.symbols()[symbol.index()].name().to_string();
            let (boundary, text) = point(&sources, &parsed, drop.point());
            Some(NamedDrop {
                name,
                boundary,
                text,
            })
        })
        .collect();

    let inputs = [SourceUnitInput::new(
        "root",
        "resource-drop.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_env)
        .unwrap()
        .validate()
        .unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_env, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let unit = owned
        .drops()
        .iter()
        .filter_map(|drop| {
            let UnitDropTarget::Named(symbol) = drop.target() else {
                return None;
            };
            let name = names.names().source_units()[symbol.source_unit().index()]
                .resolution()
                .symbols()[symbol.symbol().index()]
            .name()
            .to_string();
            let point = match drop.point() {
                UnitDropPoint::AfterStatement(id) => DropPoint::AfterStatement(id.statement()),
                UnitDropPoint::AfterExpression(id) => DropPoint::AfterExpression(id.expression()),
                UnitDropPoint::ControlTransfer(id) => DropPoint::ControlTransfer(id.expression()),
                UnitDropPoint::FunctionEntry(id) => DropPoint::FunctionEntry(id.item()),
                UnitDropPoint::CallReturn(id) => DropPoint::CallReturn(id.expression()),
                UnitDropPoint::BranchExit { control, branch } => DropPoint::BranchExit {
                    control: control.expression(),
                    branch,
                },
                UnitDropPoint::LoopExit(id) => DropPoint::LoopExit(id.statement()),
                other => panic!("unexpected drop boundary: {other:?}"),
            };
            let (boundary, text) = point_text(&sources, &parsed, point);
            Some(NamedDrop {
                name,
                boundary,
                text,
            })
        })
        .collect();
    inspect(&sources, &parsed, &single_owned, &owned);
    [single, unit]
}

fn point(sources: &SourceMap, parsed: &ParsedFile, point: DropPoint) -> (&'static str, String) {
    point_text(sources, parsed, point)
}

fn point_text(
    sources: &SourceMap,
    parsed: &ParsedFile,
    point: DropPoint,
) -> (&'static str, String) {
    let (kind, span) = match point {
        DropPoint::AfterStatement(id) => (
            "statement",
            parsed.ast().statements().get(id).unwrap().span(),
        ),
        DropPoint::AfterExpression(id) => (
            "expression",
            parsed.ast().expressions().get(id).unwrap().span(),
        ),
        DropPoint::ControlTransfer(id) => (
            "transfer",
            parsed.ast().expressions().get(id).unwrap().span(),
        ),
        DropPoint::CallReturn(id) => ("call", parsed.ast().expressions().get(id).unwrap().span()),
        DropPoint::FunctionEntry(id) => ("entry", parsed.ast().items().get(id).unwrap().span()),
        DropPoint::BranchExit { control, .. } => (
            "branch",
            parsed.ast().expressions().get(control).unwrap().span(),
        ),
        DropPoint::LoopExit(id) => ("loop", parsed.ast().statements().get(id).unwrap().span()),
        other => panic!("unexpected drop boundary: {other:?}"),
    };
    (kind, sources.slice(span).unwrap().to_string())
}

const HEADER: &str = "class Guard { deinit() {} }\nclass Memory {}\nfun tick(): Unit {}\nfun inspect(value: Guard): Unit {}\n";

#[test]
fn unused_resources_wait_for_scope_and_reverse_declarations_while_memory_is_asap() {
    let body = "{ val first = Guard(); val memory = Memory(); val second = Guard(); tick() }";
    for drops in plans(&format!("{HEADER}fun run(): Unit {body}")) {
        assert_eq!(
            drops
                .iter()
                .map(|drop| drop.name.as_str())
                .collect::<Vec<_>>(),
            ["memory", "second", "first"]
        );
        assert!(drops[0].text.starts_with("val memory"), "{drops:?}");
        for drop in &drops[1..] {
            assert_eq!(drop.boundary, "statement");
            assert_eq!(drop.text, body);
        }
    }
}

#[test]
fn borrowing_a_resource_does_not_release_it_at_its_last_read() {
    let body = "{ val guard = Guard(); inspect(guard); tick() }";
    for drops in plans(&format!("{HEADER}fun run(): Unit {body}")) {
        assert_eq!(
            drops,
            [NamedDrop {
                name: "guard".into(),
                boundary: "statement",
                text: body.into()
            }]
        );
    }
}

#[test]
fn return_cleans_resources_inner_to_outer_in_reverse_order() {
    let text = format!(
        "{HEADER}fun run(flag: Boolean): Unit {{ val outer = Guard(); if (flag) {{ val first = Guard(); val second = Guard(); return }}; tick() }}"
    );
    for drops in plans(&text) {
        let returned = drops
            .iter()
            .filter(|drop| drop.boundary == "transfer")
            .map(|drop| drop.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(returned, ["second", "first", "outer"], "{drops:?}");
        let outer = drops
            .iter()
            .filter(|drop| drop.name == "outer")
            .collect::<Vec<_>>();
        assert_eq!(outer.len(), 2, "{drops:?}");
        assert_eq!(outer[1].boundary, "statement");
    }
}

#[test]
fn moving_a_resource_leaves_only_the_destination_cleanup() {
    let body = "{ val source = Guard(); val destination = source; tick() }";
    for drops in plans(&format!("{HEADER}fun run(): Unit {body}")) {
        assert_eq!(
            drops,
            [NamedDrop {
                name: "destination".into(),
                boundary: "statement",
                text: body.into()
            }]
        );
    }
}

#[test]
fn break_and_continue_cleanup_only_the_exited_scope() {
    for jump in ["break", "continue"] {
        let body = format!(
            "{{ val outer = Guard(); while (flag) {{ val inner = Guard(); {jump} }}; tick() }}"
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
fn owned_resource_parameter_is_lexical_and_borrow_parameter_has_no_cleanup() {
    let body = "{ tick() }";
    for drops in plans(&format!(
        "{HEADER}fun run(own owned: Guard, borrowed: Guard): Unit {body}"
    )) {
        assert_eq!(
            drops,
            [NamedDrop {
                name: "owned".into(),
                boundary: "statement",
                text: body.into()
            }]
        );
    }
}

#[test]
fn recursive_resource_fields_inherit_lexical_cleanup() {
    let body = "{ val holder = Holder(Guard()); tick() }";
    for drops in plans(&format!(
        "{HEADER}class Holder(val resource: Guard)\nfun run(): Unit {body}"
    )) {
        assert_eq!(
            drops,
            [NamedDrop {
                name: "holder".into(),
                boundary: "statement",
                text: body.into()
            }]
        );
    }
}

fn ownership_codes(text: &str) -> [Vec<String>; 2] {
    let mut sources = SourceMap::new();
    let source = sources.add_source("resource-readonly.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (name_env, type_env) = standard_environments();
    let names = resolve_names(&sources, &parsed, &name_env).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &type_env).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    let single = owned
        .diagnostics()
        .iter()
        .map(|diag| diag.code().to_string())
        .collect();
    assert!(owned.drops().is_empty(), "invalid body published drops");

    let inputs = [SourceUnitInput::new(
        "root",
        "resource-readonly.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_env)
        .unwrap()
        .validate()
        .unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_env, &typed).unwrap();
    let unit = owned
        .diagnostics()
        .iter()
        .map(|diag| diag.code().to_string())
        .collect();
    assert!(owned.drops().is_empty(), "invalid body published drops");
    [single, unit]
}

#[test]
fn deinit_cannot_consume_this_or_move_a_field() {
    for body in ["val stolen = this", "val stolen = this.payload"] {
        for codes in ownership_codes(&format!(
            "class Payload {{}}\nclass Guard(val payload: Payload) {{ deinit() {{ {body} }} }}"
        )) {
            assert!(
                codes.iter().any(|code| code == "L0133" || code == "L0132"),
                "body={body}, codes={codes:?}"
            );
        }
    }
}

#[test]
fn deinit_cannot_mutate_fields_or_establish_exclusive_reborrows() {
    for body in ["this.value = 3", "update(&this.value)"] {
        for (entry, codes) in ownership_codes(&format!(
            "fun update(inout value: Int): Unit {{}}\nclass Guard(var value: Int) {{ deinit() {{ {body} }} }}"
        )).into_iter().enumerate() {
            // Single-file field mutation uses shared-capability L0135. Unit receiver
            // validation uses its established L0134 capability diagnostic; & is L0134 in both.
            let expected = if entry == 0 && body.starts_with("this.") { "L0135" } else { "L0134" };
            assert_eq!(codes, [expected], "entry={entry}, body={body}");
        }
    }
}

#[test]
fn nested_scope_releases_its_resource_without_shortening_the_outer_scope() {
    let inner = "{ val inner = Guard(); tick() }";
    let body = format!("{{ val outer = Guard(); if (flag) {inner}; tick() }}");
    for drops in plans(&format!("{HEADER}fun run(flag: Boolean): Unit {body}")) {
        assert_eq!(
            drops,
            [
                NamedDrop {
                    name: "inner".into(),
                    boundary: "statement",
                    text: inner.into()
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

#[test]
fn replacement_releases_the_old_resource_then_keeps_declaration_cleanup_order() {
    let body = "{ var first = Guard(); val second = Guard(); first = Guard(); tick() }";
    for drops in plans(&format!("{HEADER}fun run(): Unit {body}")) {
        assert_eq!(
            drops
                .iter()
                .map(|drop| drop.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second", "first"]
        );
        assert_eq!(drops[0].boundary, "expression");
        assert_eq!(drops[0].text, "Guard()");
        assert_eq!(drops[1].text, body);
        assert_eq!(drops[2].text, body);
    }
}

fn resource_deferred(text: &str) -> [bool; 2] {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;
    let mut sources = SourceMap::new();
    let source = sources.add_source("resource-deferred.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (name_env, type_env) = standard_environments();
    let names = resolve_names(&sources, &parsed, &name_env).unwrap();
    let typed = check_types(&sources, &parsed, &names, &type_env).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let single = owned
        .deferred()
        .iter()
        .any(|fact| fact.reason() == OwnershipDeferredReason::ResourceLifetime);
    if single {
        assert!(owned.drops().is_empty());
    }
    let inputs = [SourceUnitInput::new(
        "root",
        "resource-deferred.ko",
        source,
        &parsed,
    )];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_env)
        .unwrap()
        .validate()
        .unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_env, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let unit = owned
        .deferred()
        .iter()
        .any(|fact| fact.reason() == OwnershipDeferredReason::ResourceLifetime);
    if unit {
        assert!(owned.drops().is_empty());
    }
    [single, unit]
}

#[test]
fn unit_conditional_resource_move_is_deferred_instead_of_released_early_or_lost() {
    let text = format!(
        "{HEADER}fun consume(own consumed: Guard): Unit {{}}\nfun run(flag: Boolean): Unit {{ val guard = Guard(); if (flag) {{ consume(guard) }}; tick() }}"
    );
    assert_eq!(resource_deferred(&text), [false, true]);
}

#[test]
fn loop_carried_resource_consumption_is_deferred_instead_of_dropped_twice() {
    let text = format!(
        "{HEADER}fun consume(own consumed: Guard): Unit {{}}\nfun run(flag: Boolean): Unit {{ val guard = Guard(); while (flag) {{ consume(guard); break }}; tick() }}"
    );
    assert_eq!(resource_deferred(&text), [true, true]);
}

#[test]
fn deinit_body_has_lexical_locals_without_owning_this() {
    let deinit_body = "{ val nested = Guard(); tick() }";
    let run_body = "{ val owner = Owner(); tick() }";
    for drops in plans(&format!(
        "{HEADER}class Owner {{ deinit() {deinit_body} }}\nfun run(): Unit {run_body}"
    )) {
        assert_eq!(
            drops,
            [
                NamedDrop {
                    name: "nested".into(),
                    boundary: "statement",
                    text: deinit_body.into()
                },
                NamedDrop {
                    name: "owner".into(),
                    boundary: "statement",
                    text: run_body.into()
                },
            ]
        );
    }
}

#[test]
fn unit_resource_fields_keep_their_lexical_lifetime_across_source_files() {
    let mut sources = SourceMap::new();
    let definitions = sources
        .add_source(
            "resources.ko",
            "class Guard { deinit() {} }\nclass Holder(val guard: Guard)",
        )
        .unwrap();
    let definitions_file = parse_file(&sources, &lex(&sources, definitions).unwrap()).unwrap();
    let body = "{ val holder = Holder(Guard()); tick() }";
    let consumer = sources
        .add_source(
            "consumer.ko",
            format!("fun tick(): Unit {{}}\nfun run(): Unit {body}"),
        )
        .unwrap();
    let consumer_file = parse_file(&sources, &lex(&sources, consumer).unwrap()).unwrap();
    let inputs = [
        SourceUnitInput::new("root", "resources.ko", definitions, &definitions_file),
        SourceUnitInput::new("root", "consumer.ko", consumer, &consumer_file),
    ];
    let (name_env, type_env) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_env)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_env)
        .unwrap()
        .validate()
        .unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_env, &typed).unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let drops = owned
        .drops()
        .iter()
        .filter(|drop| matches!(drop.target(), UnitDropTarget::Named(_)))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1, "{drops:?}");
    let UnitDropPoint::AfterStatement(statement) = drops[0].point() else {
        panic!("not a lexical drop: {drops:?}");
    };
    assert_eq!(
        sources
            .slice(
                consumer_file
                    .ast()
                    .statements()
                    .get(statement.statement())
                    .unwrap()
                    .span()
            )
            .unwrap(),
        body
    );
    assert_eq!(
        names.names().index().source_units()[statement.source_unit().index()].source_id(),
        consumer
    );
}

#[test]
fn known_memory_closures_remain_asap_when_an_unrelated_resource_is_declared() {
    for (initialization, read) in [
        ("val value = \"memory\"", "println(value)"),
        ("val value = listOf(\"memory\")", "inspectList(value)"),
    ] {
        let text = format!(
            "{HEADER}fun inspectList(value: List<String>): Unit {{}}\nfun run(): Unit {{ {initialization}; val callback: move () -> Unit = move {{ {read} }}; callback(); tick() }}"
        );
        for drops in plans(&text) {
            let callback = drops
                .iter()
                .filter(|drop| drop.name == "callback")
                .collect::<Vec<_>>();
            assert_eq!(callback.len(), 1, "{drops:?}");
            assert_eq!(callback[0].boundary, "call", "{drops:?}");
            assert_eq!(callback[0].text, "callback()");
        }
    }
}

#[test]
fn concrete_memory_generic_remains_asap_when_an_unrelated_resource_is_declared() {
    let text = format!(
        "{HEADER}class Holder<T>(val value: T)\nfun run(): Unit {{ val holder = Holder(\"memory\"); tick() }}"
    );
    for drops in plans(&text) {
        assert_eq!(
            drops,
            [NamedDrop {
                name: "holder".into(),
                boundary: "statement",
                text: "val holder = Holder(\"memory\")".into()
            }]
        );
    }
}

#[test]
fn owned_resource_capture_keeps_the_environment_until_its_lexical_end() {
    let body = "{ val guard = Guard(); val callback: move () -> Unit = move { inspect(guard) }; callback(); tick() }";
    for drops in plans(&format!("{HEADER}fun run(): Unit {body}")) {
        assert_eq!(
            drops,
            [NamedDrop {
                name: "callback".into(),
                boundary: "statement",
                text: body.into()
            }]
        );
    }
}

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
    inspected_plans(&text, |_, _, single, unit| {
        assert_eq!(single.ownership_primitives().len(), 1);
        assert_eq!(unit.ownership_primitives().len(), 1);
        assert_eq!(single.drops().len(), 1);
        assert_eq!(unit.drops().len(), 1);
        assert!(matches!(single.drops()[0].target(), DropTarget::Named(_)));
        assert!(matches!(unit.drops()[0].target(), UnitDropTarget::Named(_)));
        assert!(matches!(
            single.drops()[0].point(),
            DropPoint::ControlTransfer(_)
        ));
        assert!(matches!(
            unit.drops()[0].point(),
            UnitDropPoint::ControlTransfer(_)
        ));
    });
}
