//! New Guide-derived obligations, not a reconstruction/equivalence claim for historical active20/10.
#[path = "support/closure_safety.rs"]
mod support;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{SymbolKind, UnitReferenceTarget},
    ownership_checking::*,
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{UnitExpressionId, UnitStatementId},
};
use support::{Case, Facts, observe, observe_with};

fn rejected(body: String, code: &'static str, needle: &'static str, primary: &'static str) -> Case {
    Case {
        body,
        rejected: Some((code, needle, primary)),
    }
}
fn both(case: Case) {
    let results = [false, true].map(|unit| observe(&case, unit));
    assert!(
        results.iter().all(|r| matches!(r, Ok(true))),
        "source={}\n{results:#?}",
        case.body
    );
}

#[test]
fn key_and_value_closure_return_cannot_outlive_provider() {
    for (component, lambda) in [("key", "{ read(key) }"), ("value", "{ read(value) }")] {
        both(rejected(
            format!(
                "fun run(source: Map<String, String>): () -> Unit {{ for ((key,value) in source) {{ return ({lambda}) }}; return ({{}}) }}"
            ),
            "L0137",
            lambda,
            lambda,
        ));
        println!("MAP OBLIGATION return-escape component={component} single+unit");
    }
}

#[test]
fn key_and_value_closure_assignment_cannot_outlive_loop() {
    for (component, lambda) in [("key", "{ read(key) }"), ("value", "{ read(value) }")] {
        for transfer in ["", "break", "continue"] {
            both(rejected(
                format!(
                    "fun run(source: Map<String, String>): Unit {{ var saved: () -> Unit = {{}}; for ((key,value) in source) {{ saved = ({lambda}); {transfer} }}; saved() }}"
                ),
                "L0137",
                "in source)",
                "source",
            ));
            println!(
                "MAP OBLIGATION outer-local-escape component={component} transfer={transfer:?} single+unit"
            );
        }
    }
}

#[test]
fn active_source_rejects_mutation_relocation_move_and_drop() {
    for operation in [
        "source.put(\"k\".clone(), \"v\".clone())",
        "source.remove(\"k\")",
        "source.consume()",
        "release(source)",
    ] {
        both(rejected(
            format!(
                "fun release(own item: MutableMap<String,String>): Unit {{}}\nfun run(): Unit {{ var source = mutableMapOf<String,String>(); for ((key,value) in source) {{ val saved: () -> Unit = {{ read(value) }}; {operation}; saved() }} }}"
            ),
            "L0135",
            operation,
            "source",
        ));
        println!("MAP OBLIGATION active-source operation={operation} single+unit");
    }
}

#[test]
fn return_operand_must_be_delivered_before_source_release() {
    both(rejected("fun release(own item: MutableMap<String,String>): Int { return 7 }\nfun run(own source: MutableMap<String,String>): Int { for ((key,value) in source) { val saved: () -> Unit = { read(value) }; saved(); return release(source) }; return 0 }".into(), "L0135", "release(source)", "source"));
}

fn expression(sources: &SourceMap, file: &ParsedFile, text: &str) -> ExpressionId {
    let found = file
        .ast()
        .expressions()
        .iter()
        .filter(|(_, n)| sources.slice(n.span()).is_ok_and(|s| s == text))
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "unique actual expression {text}");
    found[0]
}

// The same logical assertions consume each entry's own qualified IDs, never compare bare Unit symbols.
macro_rules! inspect_exit {
    ($owned:expr, $new:expr, $old:expr, $key:expr, $value:expr, $source:path, $action:ident, $exit:ident, $point:ident, $capture:expr, $transfer:expr, $saved:expr, $old_point:expr, $call:expr, $drop:ident) => {{
        let (owned, new, old, key, value) = ($owned, $new, $old, $key, $value);
        assert_eq!(owned.captures().len(), 2);
        for (closure, root) in [(old, if $capture == "value" { key } else { value }), (new, if $capture == "value" { value } else { key })] {
            let captured = owned.captures_of(closure).collect::<Vec<_>>();
            assert!(matches!(captured.as_slice(), [c] if c.source() == $source(root) && c.mode() == ClosureCaptureMode::Shared && c.effect() == ClosureCaptureEffect::Borrow), "actual lambda/root capture identity {captured:?}");
        }
        let [plan] = owned.map_iterations() else { panic!("one actual provider") };
        let exits = plan.exits().iter().filter(|e| match e.kind() {
            $exit::Break(_) => $transfer == "break", $exit::Continue(_) => $transfer == "continue", $exit::Return(_) => $transfer == "return", _ => false
        }).collect::<Vec<_>>();
        let [exit] = exits.as_slice() else { panic!("one actual transfer exit: {exits:?}") };
        let jump = match exit.kind() { $exit::Break(e) | $exit::Continue(e) | $exit::Return(e) => e, _ => unreachable!() };
        assert_eq!(exit.point(), $point::ControlTransfer(jump));
        let drops = owned.drops().iter().filter(|d| d.target() == $drop::Named($saved)).map(|d| d.point()).collect::<Vec<_>>();
        let expected = [$old_point, $point::ControlTransfer(jump), $point::CallReturn($call)];
        assert_eq!(drops.len(), expected.len(), "no overlong or duplicate closure drops: {drops:?}");
        for point in expected { assert_eq!(drops.iter().filter(|p| **p == point).count(), 1, "ASAP closure point {point:?}: {drops:?}"); }
        let actions = exit.actions();
        let positions = |test: &dyn Fn(&$action) -> bool| actions.iter().enumerate().filter(|(_, a)| test(a)).map(|(i, _)| i).collect::<Vec<_>>();
        let captures = positions(&|a| matches!(a, $action::EndCaptureLoan { closure, source, .. } if *closure == new && *source == $source(if $capture == "value" {value} else {key})));
        let keys = positions(&|a| matches!(a, $action::EndBinding { symbol, statement } if *symbol == key && *statement == plan.descriptor().statement()));
        let values = positions(&|a| matches!(a, $action::EndBinding { symbol, statement } if *symbol == value && *statement == plan.descriptor().statement()));
        let elements = positions(&|a| matches!(a, $action::EndElement(s) if *s == plan.descriptor().statement()));
        assert!(matches!((captures.as_slice(), keys.as_slice(), values.as_slice()), ([c],[k],[v]) if c < v && c < k && v < k), "capture→value→key at original Map transfer: {actions:?}");
        assert!(elements.is_empty(), "Map components use EndBinding, not Sequential EndElement: {actions:?}");
        let finishes = positions(&|a| matches!(a, $action::FinishProvider(s) if *s == plan.descriptor().statement()));
        let sources = positions(&|a| matches!(a, $action::EndSource(s) if *s == plan.descriptor().statement()));
        if $transfer == "continue" { assert!(finishes.is_empty() && sources.is_empty()); }
        else { assert!(matches!((finishes.as_slice(), sources.as_slice()), ([f],[s]) if keys[0] < *f && f < s), "finish then source: {actions:?}"); }
    }};
}

fn old_declaration(sources: &SourceMap, file: &ParsedFile) -> lang_frontend::ast::StatementId {
    let found = file
        .ast()
        .statements()
        .iter()
        .filter(|(_, n)| {
            sources
                .slice(n.span())
                .is_ok_and(|t| t.starts_with("var saved:"))
        })
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let [statement] = found.as_slice() else {
        panic!("one original saved declaration")
    };
    *statement
}

// Single's published dynamic environment contract is checked without manufacturing a Unit analogue.
fn inspect_single_instances(
    owned: &OwnershipCheckedFile,
    old: ExpressionId,
    new: ExpressionId,
    old_root: ClosureCaptureSource,
    new_root: ClosureCaptureSource,
    old_point: DropPoint,
    call: ExpressionId,
) {
    let table = owned.cleanup_conditions();
    let ends=owned.cleanup_steps().iter().filter(|(_,a)|matches!(a,IterationCleanupAction::EndCaptureLoan {closure,..} if *closure==old || *closure==new)).collect::<Vec<_>>();
    assert_eq!(
        ends.len(),
        3,
        "old ASAP once plus new on two disjoint paths: {ends:?}"
    );
    for (closure, root, count) in [(old, old_root, 1), (new, new_root, 2)] {
        let creates = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(p, a)| match a {
                IterationCleanupAction::CreateClosureOwner { owner, closure: c }
                    if *c == closure =>
                {
                    Some((*p, *owner))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(formed_at, formed)] = creates.as_slice() else {
            panic!("one actual closure formation")
        };
        assert_eq!(*formed_at, DropPoint::AfterExpression(closure));
        assert!(
            matches!(table.owner_value(*formed),Some(CleanupOwnerValue::Closure{expression,..}) if *expression==closure)
        );
        let selected=ends.iter().filter(|(_,a)|matches!(a,IterationCleanupAction::EndCaptureLoan{closure:c,..} if *c==closure)).collect::<Vec<_>>();
        assert_eq!(selected.len(), count);
        for (point, action) in selected {
            let IterationCleanupAction::EndCaptureLoan {
                owner,
                instance_address,
                capture_slot,
                condition,
                source,
                value,
                ..
            } = action
            else {
                unreachable!()
            };
            assert_eq!(
                *condition, None,
                "these source branches use actual cleanup points, no conditional bypass"
            );
            assert_eq!(*owner, *formed);
            assert_eq!(*source, root);
            assert_eq!(*value, CleanupCaptureValue::Place(root));
            let address = table.instance_address(*instance_address).unwrap();
            assert_eq!(address.root(), *formed);
            assert!(address.capture_path().is_empty());
            assert_eq!(*capture_slot, table.capture_slot(*formed, root));
            let slot = table
                .capture_slot_value(capture_slot.expect("one real capture slot"))
                .unwrap();
            assert_eq!(slot.environment(), *formed);
            assert_eq!(slot.closure(), closure);
            assert_eq!(slot.source(), root);
            assert_eq!(slot.position(), 0);
            if closure == old {
                assert_eq!(*point, old_point);
            }
            let at_point = owned
                .cleanup_steps()
                .iter()
                .filter(|(p, _)| p == point)
                .map(|(_, a)| a)
                .collect::<Vec<_>>();
            let drop_at=at_point.iter().position(|a|matches!(a,IterationCleanupAction::Drop(d) if d.owner()==Some(*formed)&&d.condition().is_none())).expect("same current environment is dropped before its capture ends");
            let end_at = at_point
                .iter()
                .position(|a| std::ptr::eq(*a, action))
                .unwrap();
            assert!(drop_at < end_at);
        }
        if closure == new {
            assert_eq!(ends.iter().filter(|(p,a)|*p==DropPoint::CallReturn(call)&&matches!(a,IterationCleanupAction::EndCaptureLoan{closure:c,..} if *c==new)).count(),1);
        }
    }
}

fn inspect(
    sources: &SourceMap,
    facts: Facts<'_>,
    component: &str,
    transfer: &str,
) -> Result<(), String> {
    let new_text = if component == "value" {
        "{ read(value) }"
    } else {
        "{ read(key) }"
    };
    let old_text = if component == "value" {
        "{ read(key) }"
    } else {
        "{ read(value) }"
    };
    match facts {
        Facts::Single(file, names, owned) => {
            let symbol = |name| {
                let items = names
                    .symbols()
                    .iter()
                    .filter(|s| {
                        s.name() == name && (name == "saved" || s.kind() == SymbolKind::ForBinding)
                    })
                    .collect::<Vec<_>>();
                let [item] = items.as_slice() else {
                    panic!("unique real For binding {name}")
                };
                item.id()
            };
            let old = expression(sources, file, old_text);
            let new = expression(sources, file, new_text);
            let old_root = ClosureCaptureSource::Symbol(symbol(if component == "value" {
                "key"
            } else {
                "value"
            }));
            let new_root = ClosureCaptureSource::Symbol(symbol(component));
            inspect_single_instances(
                owned,
                old,
                new,
                old_root,
                new_root,
                DropPoint::AfterStatement(old_declaration(sources, file)),
                expression(sources, file, "saved()"),
            );
            inspect_exit!(
                owned,
                expression(sources, file, new_text),
                expression(sources, file, old_text),
                symbol("key"),
                symbol("value"),
                ClosureCaptureSource::Symbol,
                IterationCleanupAction,
                IterationExitKind,
                DropPoint,
                component,
                transfer,
                symbol("saved"),
                DropPoint::AfterStatement(old_declaration(sources, file)),
                expression(sources, file, "saved()"),
                DropTarget
            );
        }
        Facts::Unit(file, names, owned) => {
            let [unit] = names.names().source_units() else {
                panic!("one source unit")
            };
            let symbol = |name| {
                let items = unit
                    .resolution()
                    .symbols()
                    .iter()
                    .filter(|s| {
                        s.name() == name && (name == "saved" || s.kind() == SymbolKind::ForBinding)
                    })
                    .collect::<Vec<_>>();
                let [item] = items.as_slice() else {
                    panic!("unique real unit For binding {name}")
                };
                names
                    .names()
                    .references()
                    .iter()
                    .find_map(|r| match r.target() {
                        UnitReferenceTarget::Symbol(s)
                            if s.source_unit() == unit.source_unit() && s.symbol() == item.id() =>
                        {
                            Some(*s)
                        }
                        _ => None,
                    })
                    .expect("qualified symbol from actual reference")
            };
            inspect_exit!(
                owned,
                UnitExpressionId::new(unit.source_unit(), expression(sources, file, new_text)),
                UnitExpressionId::new(unit.source_unit(), expression(sources, file, old_text)),
                symbol("key"),
                symbol("value"),
                UnitClosureCaptureSource::Symbol,
                UnitIterationCleanupAction,
                UnitIterationExitKind,
                UnitDropPoint,
                component,
                transfer,
                symbol("saved"),
                UnitDropPoint::AfterStatement(UnitStatementId::new(
                    unit.source_unit(),
                    old_declaration(sources, file)
                )),
                UnitExpressionId::new(unit.source_unit(), expression(sources, file, "saved()")),
                UnitDropTarget
            );
        }
    }
    Ok(())
}

#[test]
fn replacement_capture_ends_before_components_on_break_continue_and_return() {
    for component in ["key", "value"] {
        for transfer in ["break", "continue", "return"] {
            let previous = if component == "key" { "value" } else { "key" };
            let case = Case {
                body: format!(
                    "fun run(source: Map<String,String>, flag: Boolean): Unit {{ for ((key,value) in source) {{ var saved: () -> Unit = {{ read({previous}) }}; saved = ({{ read({component}) }}); if (flag) {{ {transfer} }}; saved() }} }}"
                ),
                rejected: None,
            };
            let results = [false, true].map(|unit| {
                observe_with(&case, unit, |sources, facts| {
                    inspect(sources, facts, component, transfer)
                })
            });
            assert!(
                results.iter().all(|r| matches!(r, Ok(true))),
                "component={component} transfer={transfer}: {results:#?}"
            );
            println!(
                "MAP OBLIGATION exact-capture-and-exit component={component} transfer={transfer} single+unit"
            );
        }
    }
}

#[test]
fn last_use_and_break_restore_mutable_source_permissions() {
    both(Case { body:"fun run(): Unit { var source = mutableMapOf<String,String>(); for ((key,value) in source) { val saved: () -> Unit = { read(value) }; saved(); break }; source.put(\"later\".clone(), \"value\".clone()); source.remove(\"later\"); val result = source.consume() }".into(), rejected:None });
}

#[test]
fn move_outer_cannot_hide_key_or_value_borrow_on_return_or_value_delivery() {
    let mut results = Vec::new();
    for component in ["key", "value"] {
        for returning in [false, true] {
            let ending = if returning {
                "return outer"
            } else {
                "deliver(outer)"
            };
            let result = if returning { "move () -> Unit" } else { "Unit" };
            let tail = if returning { "return (move {})" } else { "" };
            let case = rejected(
                format!(
                    "fun deliver(own action: move () -> Unit): Unit {{}}\nfun run(source: Map<String,String>): {result} {{ for ((key,value) in source) {{ val inner: () -> Unit = {{ read({component}) }}; val outer: move () -> Unit = move {{ inner() }}; {ending} }}; {tail} }}"
                ),
                "L0137",
                if returning {
                    "return outer"
                } else {
                    "deliver(outer)"
                },
                "outer",
            );
            for unit in [false, true] {
                results.push((component, returning, unit, observe(&case, unit)));
            }
            println!(
                "MAP OBLIGATION hidden-borrow component={component} return={returning} single+unit"
            );
        }
    }
    assert_eq!(results.len(), 8);
    assert!(
        results.iter().all(|r| matches!(r.3, Ok(true))),
        "{results:#?}"
    );
}

#[test]
fn move_outer_with_component_borrow_can_finish_in_current_body() {
    for component in ["key", "value"] {
        let case = Case {
            body: format!(
                "fun run(source: Map<String,String>): Unit {{ for ((key,value) in source) {{ val inner: () -> Unit = {{ read({component}) }}; val outer: move () -> Unit = move {{ inner() }}; outer(); break }} }}"
            ),
            rejected: None,
        };
        both(case);
        println!("MAP OBLIGATION nested-same-body component={component} single+unit");
    }
}

#[test]
fn nested_outer_holds_source_until_its_last_use_then_releases_it() {
    both(rejected("fun run(own source: String): Unit { val inner: () -> Unit = { read(source) }; val outer: move () -> Unit = move { inner() }; take(source); outer() }".into(), "L0135", "take(source)", "source"));
    both(Case { body:"fun run(own source: String): Unit { val inner: () -> Unit = { read(source) }; val outer: move () -> Unit = move { inner() }; outer(); take(source) }".into(), rejected:None });
}

#[test]
fn branch_selected_nested_map_borrow_cannot_escape() {
    let mut results = Vec::new();
    for component in ["key", "value"] {
        let case = rejected(
            format!(
                "fun deliver(own action: move () -> Unit): Unit {{}}\nfun run(source: Map<String,String>, flag: Boolean): Unit {{ for ((key,value) in source) {{ val inner: () -> Unit = {{ read({component}) }}; var outer: move () -> Unit = move {{}}; if(flag) {{outer=(move {{inner()}})}} else {{outer=(move {{}})}}; deliver(outer) }} }}"
            ),
            "L0137",
            "deliver(outer)",
            "outer",
        );
        for unit in [false, true] {
            results.push((component, unit, observe(&case, unit)));
        }
    }
    assert!(
        results.iter().all(|r| matches!(r.2, Ok(true))),
        "{results:#?}"
    );
}

#[test]
fn captured_environment_is_frozen_before_source_rebinding() {
    for component in ["key", "value"] {
        both(rejected(
            format!(
                "fun deliver(own action: move () -> Unit): Unit {{}}\nfun run(source: Map<String,String>): Unit {{ for ((key,value) in source) {{var inner: () -> Unit = {{read({component})}}; val outer: move () -> Unit = move {{inner()}}; inner=({{}}); deliver(outer); inner()}} }}"
            ),
            "L0137",
            "deliver(outer)",
            "outer",
        ));
        both(Case {
            body: format!(
                "fun deliver(own action: move () -> Unit): Unit {{}}\nfun run(source: Map<String,String>): Unit {{ for ((key,value) in source) {{var inner: () -> Unit = {{}}; val outer: move () -> Unit = move {{inner()}}; inner=({{read({component})}}); deliver(outer); inner()}} }}"
            ),
            rejected: None,
        });
    }
}

#[test]
fn direct_and_three_level_environments_cannot_hide_map_borrows() {
    for component in ["key", "value"] {
        both(rejected(
            format!(
                "fun deliver(own action: move () -> Unit): Unit {{}}\nfun run(source: Map<String,String>): Unit {{ for ((key,value) in source) {{val inner: () -> Unit = {{read({component})}}; deliver(move {{inner()}})}} }}"
            ),
            "L0137",
            "move {inner()}",
            "move {inner()}",
        ));
        both(rejected(
            format!(
                "fun deliver(own action: move () -> Unit): Unit {{}}\nfun run(source: Map<String,String>): Unit {{ for ((key,value) in source) {{val inner: () -> Unit = {{read({component})}}; val middle: move () -> Unit = move {{inner()}}; val outer: move () -> Unit = move {{middle()}}; deliver(outer)}} }}"
            ),
            "L0137",
            "deliver(outer)",
            "outer",
        ));
    }
}

#[test]
fn conditional_initializer_keeps_all_origins_and_legal_last_use() {
    for component in ["key", "value"] {
        both(rejected(
            format!(
                "fun deliver(own action: move () -> Unit): Unit {{}}\nfun run(source: Map<String,String>, flag: Boolean): Unit {{for((key,value) in source) {{val inner: () -> Unit = {{read({component})}}; val outer: move () -> Unit = if(flag) move {{inner()}} else move {{}}; deliver(outer)}} }}"
            ),
            "L0137",
            "deliver(outer)",
            "outer",
        ));
        both(Case {
            body: format!(
                "fun run(source: Map<String,String>, flag: Boolean): Unit {{for((key,value) in source) {{val inner: () -> Unit = {{read({component})}}; val outer: move () -> Unit = if(flag) move {{inner()}} else move {{}}; outer(); break}} }}"
            ),
            rejected: None,
        });
    }
}

#[test]
fn live_move_outer_cannot_carry_component_borrow_over_iteration_boundaries() {
    for component in ["key", "value"] {
        for transfer in ["", "break", "continue"] {
            both(rejected(
                format!(
                    "fun run(source: Map<String,String>): Unit {{var escaped: move () -> Unit = move {{}}; for((key,value) in source) {{val inner: () -> Unit = {{read({component})}}; escaped=(move {{inner()}}); {transfer}}}; escaped()}}"
                ),
                "L0137",
                "in source)",
                "source",
            ));
        }
    }
}

#[test]
fn branch_selected_nested_owned_source_releases_after_call() {
    for initializer in [true, false] {
        let select = if initializer {
            "val outer: move () -> Unit = if(flag) move {inner()} else move {}"
        } else {
            "var outer: move () -> Unit = move {}; if(flag) {outer=(move {inner()})} else {outer=(move {})}"
        };
        both(Case {
            body: format!(
                "fun run(own source: String, flag: Boolean): Unit {{val inner: () -> Unit = {{read(source)}}; {select}; outer(); take(source)}}"
            ),
            rejected: None,
        });
    }
}

#[test]
fn control_tail_local_hands_environment_to_result_before_scope_end() {
    for control in [
        "if(flag) {val local: () -> Unit = {read(source)}; local} else ({})",
        "when {flag -> {val local: () -> Unit = {read(source)}; local}; else -> ({})}",
    ] {
        both(Case {
            body: format!(
                "fun run(own source: String, flag: Boolean): Unit {{val outer: () -> Unit = {control}; outer(); take(source)}}"
            ),
            rejected: None,
        });
    }
}

#[test]
fn evaluated_callee_keeps_capture_across_control_arguments() {
    for control in ["if(flag) 1 else 2", "when {flag -> 1; else -> 2}"] {
        both(rejected(
            format!(
                "fun release(own item: String): Int {{return 0}}\nfun run(own source: String, flag: Boolean): Unit {{val action: (Int,Int) -> Unit = {{a,b->read(source)}}; action({control},release(source))}}"
            ),
            "L0135",
            "release(source)",
            "source",
        ));
    }
}

#[test]
fn temporary_control_tail_borrow_argument_releases_after_call() {
    for control in [
        "if(flag) {val local: () -> Unit = {read(source)}; local} else ({})",
        "when {flag -> {val local: () -> Unit = {read(source)}; local}; else -> ({})}",
    ] {
        both(Case {
            body: format!(
                "fun apply(action: () -> Unit): Unit {{}}\nfun run(own source: String, flag: Boolean): Unit {{apply({control}); take(source)}}"
            ),
            rejected: None,
        });
    }
}
