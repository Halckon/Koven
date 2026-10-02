//! SPEC-0211：循环 provider 的 source 与 element 必须保持借用能力。
use lang_frontend::{
    name_resolution::resolve_names,
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::ParsedFile,
    source::SourceMap,
    type_checking::{check_types, standard_environments},
};

#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;

fn checked(text: &str) -> (SourceMap, ParsedFile, OwnershipCheckedFile) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("iteration.ko", text).unwrap();
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "iteration ownership");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(
        owned.drops().iter().all(|fact| !matches!(
            fact.target(),
            lang_frontend::ownership_checking::DropTarget::Named(_)
        ) || fact.owner().is_some()),
        "every named cleanup must identify its value definition"
    );
    (sources, parsed, owned)
}

#[test]
fn loop_carried_nullable_closure_can_be_taken_called_and_replaced() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};

    // flags 为 [true, false] 时：第一轮保存环境；第二轮先形成新环境，
    // 再调用旧环境并补回 saved。
    // 不能因为同一静态 lambda 同时存在多个实例而拒绝合法源码。
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}
fun <T> none(): T? { return null }
fun run(flags: List<Boolean>, xs: List<Int>, ys: List<Int>) {
    var saved = none<move () -> Unit>()
    for (flag in flags) {
        val chosen: () -> Unit = if (flag) ({ read(xs) }) else ({ read(ys) })
        val current: move () -> Unit = move {
            val inner: move () -> Unit = move { val used = chosen() }
            val used = inner()
        }
        if (flag) { saved = current } else {
            val old = saved!!
            { val used = old() }
            saved = current
        }
    }
}",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let old_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("old()")).then_some(id))
        .unwrap();
    let old_owner = owned
        .drops()
        .iter()
        .find(|fact| {
            fact.point() == DropPoint::CallReturn(old_call)
                && fact.owner().is_some()
                && sources.slice(fact.value_origin()) == Ok("old")
        })
        .expect("the extracted old value needs a return-point cleanup")
        .owner();
    let old_drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.owner() == old_owner)
        .collect::<Vec<_>>();
    assert_eq!(
        old_drops.len(),
        1,
        "the old owner must not also be released before or after its call: {old_drops:?}"
    );
    assert!(owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(old_call)
            && matches!(action, IterationCleanupAction::PassClosureEnvironment { callee, closure: None }
                if Some(*callee) == old_owner)
    }), "the stable evaluated old value must provide the called environment");
}

#[test]
fn call_entry_transports_owned_opaque_function_without_guessing_a_lambda() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};
    for own in ["own ", ""] {
        let (sources, parsed, owned) = checked(&format!(
            "fun run({own}cb: move () -> Unit) {{ val used = cb() }}"
        ));
        assert!(owned.diagnostics().is_empty());
        assert!(owned.deferred().is_empty());
        let call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()) == Ok("cb()")).then_some(id))
            .unwrap();
        let passes = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::PassClosureEnvironment { callee, closure }
                    if *point == DropPoint::CallEntry(call) =>
                {
                    Some((*callee, *closure))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if own.is_empty() {
            assert!(
                passes.is_empty(),
                "borrowed parameters have no callee-owned value slot"
            );
        } else {
            let [(callee, closure)] = passes.as_slice() else {
                panic!("one owned callee environment");
            };
            assert!(
                closure.is_none(),
                "an opaque function is not a known file lambda"
            );
            assert!(
                owned
                    .drops()
                    .iter()
                    .any(|fact| fact.owner() == Some(*callee)
                        && fact.point() == DropPoint::CallReturn(call))
            );
        }
    }
}

#[test]
fn elvis_closure_result_keeps_capture_until_the_selected_call() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val none: Nothing? = null
            val f: () -> Unit = none ?: ({ read(xs) })
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let source = owned
        .captures()
        .iter()
        .find_map(|capture| match capture.source() {
            ClosureCaptureSource::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .unwrap();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == DropTarget::Named(source))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1, "{drops:?}");
    assert_eq!(
        drops[0].point(),
        DropPoint::CallReturn(call),
        "the Elvis result owns the capture loan"
    );
}

#[test]
fn elvis_return_does_not_hide_the_non_null_continuation() {
    let (_, _, owned) = checked(
        "fun take(own xs: List<Int>) {}\nfun run(own xs: List<Int>, maybe: Int?) {
            val moved = take(xs)
            val selected = maybe ?: return
            val reused = take(xs)
        }",
    );
    let codes = owned
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        ["L0131"],
        "non-null skips the return and reaches the invalid reuse"
    );
}

#[test]
fn elvis_borrowed_closure_cannot_escape_through_the_result() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>): () -> Unit {
            val none: Nothing? = null
            return none ?: ({ read(xs) })
        }",
    );
    let codes = owned
        .diagnostics()
        .iter()
        .map(|d| d.code().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        ["L0137"],
        "Elvis must preserve the selected closure's escape restriction"
    );
}

#[test]
fn elvis_nothing_nullable_has_no_non_null_successor() {
    let (_, _, owned) = checked(
        "fun take(own xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val moved = take(xs)
            val none: Nothing? = null
            val selected = none ?: return
            val reused = take(xs)
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn elvis_move_only_source_transfers_only_on_the_non_null_edge() {
    for (text, expected) in [
        (
            "class Node {}\nfun fallback(x: Node?): Node = Node()\nfun run(own x: Node?): Node = x ?: fallback(x)",
            vec![],
        ),
        (
            "class Node {}\nfun run(own x: Node?) { val selected = x ?: Node() val reused = x }",
            vec!["L0131"],
        ),
        (
            "class Node {}\nfun run(x: Node?): Node = x ?: Node()",
            vec!["L0133"],
        ),
        (
            "class Node {}\nfun run(inout x: Node?): Node = x ?: Node()",
            vec!["L0133"],
        ),
        (
            "class Node {}\nclass Holder(val item: Node?)\nfun run(own h: Holder): Node = h.item ?: Node()",
            vec!["L0132"],
        ),
        (
            "class Node {}\nfun run(own xs: List<Node?>): Node = xs[0] ?: Node()",
            vec!["L0136"],
        ),
    ] {
        let (_, _, owned) = checked(text);
        let codes = owned
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect::<Vec<_>>();
        assert_eq!(codes, expected, "{text}");
    }
}

#[test]
fn elvis_return_keeps_named_owner_live_on_the_non_null_successor() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, maybe: Int?) {
            val selected = maybe ?: return
            val used = read(xs)
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), DropTarget::Named(_)))
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        2,
        "each path releases xs exactly once: {drops:?}"
    );
    assert!(
        drops
            .iter()
            .any(|fact| matches!(fact.point(), DropPoint::ControlTransfer(_)))
    );
    assert!(drops.iter().any(|fact| matches!(fact.point(), DropPoint::CallReturn(id)
        if sources.slice(parsed.ast().expressions().get(id).unwrap().span()).unwrap() == "read(xs)")));
}

#[test]
fn elvis_temporary_source_is_transferred_or_cleaned_on_return() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget};
    let (sources, parsed, owned) = checked(
        "class Node {}\nfun make(): Node? = Node()\nfun read(x: Node) {}\nfun run() {
            val selected = make() ?: return
            val used = read(selected)
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let source = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "make()").then_some(id))
        .unwrap();
    let drops = owned.drops().iter().filter(|fact| fact.owner().is_some_and(|owner|
        matches!(owned.cleanup_conditions().owner_value(owner), Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == source))).collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        2,
        "the same evaluated wrapper reaches one of two cleanup paths: {drops:?}"
    );
    assert!(
        drops
            .iter()
            .any(|fact| fact.target() == DropTarget::Temporary(source)
                && matches!(fact.point(), DropPoint::ControlTransfer(_)))
    );
    assert!(
        drops
            .iter()
            .any(|fact| matches!(fact.target(), DropTarget::Named(_))
                && matches!(fact.point(), DropPoint::CallReturn(_)))
    );
}

#[test]
fn elvis_copyable_element_keeps_its_temporary_container_cleanup() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    for (ty, left) in [
        ("List<Int?>", "pass(xs)[0]"),
        ("List<List<Int?>>", "(pass(xs)[0])[0]"),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun pass(own xs: {ty}): {ty} = xs\nfun run(own xs: {ty}) {{
            val selected = {left} ?: return
        }}",
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let source = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                (sources.slice(node.span()).unwrap() == "pass(xs)").then_some(id)
            })
            .unwrap();
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Temporary(source))
            .collect::<Vec<_>>();
        assert_eq!(
            drops.len(),
            2,
            "both paths must clean the container, not the Copyable element: {drops:?}"
        );
        assert!(
            drops
                .iter()
                .any(|fact| matches!(fact.point(), DropPoint::ControlTransfer(_)))
        );
        assert!(
            drops
                .iter()
                .any(|fact| matches!(fact.point(), DropPoint::BranchExit { branch: 0, .. }))
        );
        assert!(
            owned
                .drops()
                .iter()
                .filter(|fact| matches!(fact.target(), DropTarget::Temporary(_)))
                .all(|fact| fact.target() == DropTarget::Temporary(source)),
            "an indexed element is not a second owner"
        );
    }
}

#[test]
fn branch_replaced_capture_source_keeps_distinct_owner_versions() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue, DropPoint,
        DropTarget,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flag: Boolean) {
            var xs = listOf(1)
            if (flag) { xs = listOf(2) } else { xs = listOf(3) }
            val f: () -> Unit = { read(xs) }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let drops = owned.drops().iter().filter(|fact| {
        matches!(fact.target(), DropTarget::Named(_))
            && matches!(fact.point(), DropPoint::CallReturn(call)
                if sources.slice(parsed.ast().expressions().get(call).unwrap().span()).unwrap() == "f()")
            && sources.slice(fact.value_origin()).unwrap().starts_with("xs =")
    }).collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        2,
        "each replacement is a distinct source owner"
    );
    assert!(drops.iter().all(|fact| fact.condition().is_some()));
    assert_ne!(drops[0].owner(), drops[1].owner());
    for fact in &drops {
        let CleanupOwnerValue::Expression { expression, .. } = owned
            .cleanup_conditions()
            .owner_value(fact.owner().expect("source definition"))
            .unwrap()
        else {
            panic!("replacement must retain its evaluated value identity");
        };
        assert!(
            ["listOf(2)", "listOf(3)"].contains(
                &sources
                    .slice(parsed.ast().expressions().get(*expression).unwrap().span())
                    .unwrap()
            )
        );
    }
    fn selected(table: &CleanupConditions, id: CleanupConditionId, arm: usize) -> bool {
        match table.get(id).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { branches, .. } => selected(table, branches[arm], arm),
        }
    }
    for arm in 0..2 {
        assert_eq!(
            drops
                .iter()
                .filter(|fact| selected(owned.cleanup_conditions(), fact.condition().unwrap(), arm))
                .count(),
            1,
            "only the selected source version is destroyed"
        );
    }
}

#[test]
fn moving_a_source_binding_preserves_its_parameter_owner_identity() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropTarget};
    let (sources, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val ys = xs
            val f: () -> Unit = { read(ys) }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let fact = owned
        .drops()
        .iter()
        .find(|fact| {
            matches!(fact.target(), DropTarget::Named(_))
                && sources.slice(fact.value_origin()).unwrap() == "ys"
        })
        .unwrap();
    let CleanupOwnerValue::Parameter { origin, .. } = owned
        .cleanup_conditions()
        .owner_value(fact.owner().expect("moved source owner"))
        .unwrap()
    else {
        panic!("binding transfer must not fabricate a new value");
    };
    assert_eq!(sources.slice(*origin).unwrap(), "xs");
}

#[test]
fn source_owner_versions_survive_pending_value_argument_return() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget};
    let (_, parsed, owned) = checked(
        "fun consume(own xs: List<Int>, done: Unit) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) {
            consume(if (flag) xs else ys, return)
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let drops = owned.drops().iter().filter(|fact|
        matches!(fact.point(), DropPoint::ControlTransfer(_))
        && matches!(fact.target(), DropTarget::Temporary(expression)
            if matches!(parsed.ast().expressions().get(expression).unwrap().payload(), lang_frontend::parser::Expression::If { .. })))
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        2,
        "each incoming consumed parameter remains a pending obligation"
    );
    assert_ne!(drops[0].owner(), drops[1].owner());
    for fact in drops {
        assert!(fact.condition().is_some());
        assert!(matches!(
            owned
                .cleanup_conditions()
                .owner_value(fact.owner().unwrap()),
            Some(CleanupOwnerValue::Parameter { .. })
        ));
    }
}

#[test]
fn source_owner_identity_survives_non_null_assertion() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropTarget};
    let (sources, _, owned) =
        checked("class Resource {}\nfun run(own source: Resource?) { val extracted = source!! }");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let fact = owned
        .drops()
        .iter()
        .find(|fact| matches!(fact.target(), DropTarget::Named(_)))
        .unwrap();
    let CleanupOwnerValue::Parameter { origin, .. } = owned
        .cleanup_conditions()
        .owner_value(fact.owner().unwrap())
        .unwrap()
    else {
        panic!("extraction must transport the consumed owner");
    };
    assert_eq!(sources.slice(*origin).unwrap(), "source");
}

#[test]
fn constant_temporary_normalization_preserves_source_owner_identity() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget};
    for text in [
        "const val TEXT = \"hi\"\nfun view(text: String): Unit {}\nfun run(): Unit { val used = view((TEXT)) }",
        "const val TEXT = \"hi\"\nfun consume(own text: String, done: Unit): Unit {}\nfun run(): Unit { consume((TEXT), return) }",
    ] {
        let (_, _, owned) = checked(text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let fact = owned
            .drops()
            .iter()
            .find(|fact| {
                matches!(fact.target(), DropTarget::Temporary(_))
                    && matches!(
                        fact.point(),
                        DropPoint::CallReturn(_) | DropPoint::ControlTransfer(_)
                    )
            })
            .unwrap();
        assert!(matches!(
            owned.cleanup_conditions().owner_value(
                fact.owner()
                    .expect("normalization must preserve the value identity")
            ),
            Some(CleanupOwnerValue::Expression { .. })
        ));
    }
}

#[test]
fn moved_capture_keeps_the_source_value_after_its_binding_is_replaced() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropTarget};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run() {
            var xs = listOf(1)
            val f: move () -> Unit = move { read(xs) }
            xs = listOf(2)
            val g: move () -> Unit = move { read(xs) }
            val first = f()
            val second = g()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let captures = owned
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), DropTarget::Captured { .. }))
        .collect::<Vec<_>>();
    assert_eq!(captures.len(), 2);
    let mut definitions = Vec::new();
    for fact in captures {
        let CleanupOwnerValue::Expression { expression, .. } = owned
            .cleanup_conditions()
            .owner_value(
                fact.owner()
                    .expect("owned capture must retain its source value identity"),
            )
            .unwrap()
        else {
            panic!("capture must consume the evaluated source, not its later binding");
        };
        definitions.push(
            sources
                .slice(parsed.ast().expressions().get(*expression).unwrap().span())
                .unwrap(),
        );
    }
    assert_eq!(definitions, ["listOf(1)", "listOf(2)"]);
}

#[test]
fn nested_capture_refers_to_the_immediate_environment_slot() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupOwnerValue, IterationCleanupAction,
    };
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val outer: move () -> Unit = move {
                val inner: () -> Unit = { read(xs) }
                val used = inner()
            }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let creations = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, .. } => Some(*owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(creations.len(), 2);
    let mut nested = None;
    for owner in creations {
        let CleanupOwnerValue::Closure { inputs, .. } =
            owned.cleanup_conditions().owner_value(owner).unwrap()
        else {
            panic!("created environment");
        };
        assert_eq!(inputs.len(), 1);
        if let CleanupCaptureValue::Environment {
            owner: enclosing,
            source,
            slot,
        } = inputs[0].value()
        {
            assert_eq!(
                owned.cleanup_conditions().capture_slot(enclosing, source),
                Some(slot)
            );
            let layout = owned.cleanup_conditions().capture_slot_value(slot).unwrap();
            assert_eq!(layout.environment(), enclosing);
            assert_eq!(layout.source(), source);
            assert_eq!(layout.position(), 0);
            let inner_slot = owned
                .cleanup_conditions()
                .capture_slot(owner, source)
                .expect("the inner environment has its own capture slot");
            assert_ne!(inner_slot, slot);
            assert_eq!(
                owned
                    .cleanup_conditions()
                    .capture_slot_value(inner_slot)
                    .unwrap()
                    .position(),
                0
            );
            let CleanupOwnerValue::Closure { inputs: outer, .. } =
                owned.cleanup_conditions().owner_value(enclosing).unwrap()
            else {
                panic!("immediate environment");
            };
            assert_eq!(outer[0].source(), source);
            assert!(matches!(outer[0].value(), CleanupCaptureValue::Owner(_)));
            nested = Some((owner, inputs[0].value()));
        }
    }
    let (owner, value) = nested.expect("inner capture must use the outer environment");
    assert!(owned.cleanup_steps().iter().any(|(_, action)|
        matches!(action, IterationCleanupAction::EndCaptureLoan { owner: actual, value: actual_value, .. }
            if *actual == owner && *actual_value == value)));
}

#[test]
fn owned_nested_capture_facts_keep_the_moved_slot_identity() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupOwnerValue, ClosureCaptureEffect,
        ClosureCaptureMode, DropPoint, DropTarget, IterationCleanupAction,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val outer: move () -> Unit = move {
                val inner: move () -> Unit = move { read(xs) }
                val used = inner()
            }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let xs = names
        .symbols()
        .iter()
        .rfind(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let captured = owned
        .drops()
        .iter()
        .filter(|fact| match fact.target() {
            DropTarget::Captured {
                source: lang_frontend::ownership_checking::ClosureCaptureSource::Symbol(source),
                ..
            } => source == xs,
            _ => false,
        })
        .collect::<Vec<_>>();
    assert_eq!(captured.len(), 2, "both call exits have a candidate drop");
    let mut outer = None;
    let mut inner = None;
    for fact in &captured {
        let DropTarget::Captured {
            owner,
            source,
            value,
            ..
        } = fact.target()
        else {
            unreachable!();
        };
        let slot = owned
            .cleanup_conditions()
            .capture_slot(owner, source)
            .expect("created closure has its capture slot");
        assert_eq!(fact.capture_slot(), Some(slot));
        match value {
            CleanupCaptureValue::Owner(_) => outer = Some(slot),
            CleanupCaptureValue::Environment { slot: source, .. } => {
                inner = Some((owner, slot, source));
            }
            CleanupCaptureValue::Place(_) => panic!("owned capture needs an owner"),
        }
    }
    let outer = outer.unwrap();
    let (inner_owner, inner, source) = inner.unwrap();
    assert_eq!(
        source, outer,
        "inner formation reads the immediate outer slot"
    );
    assert!(owned.cleanup_steps().iter().any(|(_, action)| matches!(
        action,
        IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == inner_owner
    )));
    let Some(CleanupOwnerValue::Closure { inputs, .. }) =
        owned.cleanup_conditions().owner_value(inner_owner)
    else {
        panic!("the inner value is a formed closure");
    };
    assert_eq!(inputs.len(), 1);
    let edges = owned
        .cleanup_conditions()
        .closure_capture_edges(inner_owner)
        .expect("the formed environment has capture edges");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].target(), inner);
    assert_eq!(edges[0].input(), inputs[0]);
    let steps = owned.cleanup_steps();
    let creation = steps
        .iter()
        .position(|(_, action)| matches!(action, IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == inner_owner))
        .unwrap();
    assert!(matches!(
        steps[creation + 1],
        (point, IterationCleanupAction::SaveClosureCapture { owner, target, input })
            if point == steps[creation].0 && owner == inner_owner && target == inner && input == inputs[0]
    ));
    assert!(matches!(
        inputs[0].value(),
        CleanupCaptureValue::Environment { slot, .. } if slot == outer
    ));
    assert_eq!(inputs[0].effect(), ClosureCaptureEffect::Move);
    let outer_owner = owned
        .cleanup_conditions()
        .capture_slot_value(outer)
        .unwrap()
        .environment();
    let capture_write = |target| {
        steps
            .iter()
            .find_map(|(_, action)| match action {
                IterationCleanupAction::SaveClosureCapture {
                    owner,
                    target: actual,
                    input,
                } if *actual == target => Some((*owner, *input)),
                _ => None,
            })
            .unwrap()
    };
    let (written_outer, outer_input) = capture_write(outer);
    let (written_inner, inner_input) = capture_write(inner);
    assert_eq!(written_outer, outer_owner);
    assert_eq!(written_inner, inner_owner);
    let CleanupCaptureValue::Owner(xs_owner) = outer_input.value() else {
        panic!("outer formation must read the current xs owner");
    };
    assert_eq!(
        inner_input.value(),
        CleanupCaptureValue::Environment {
            owner: outer_owner,
            source: outer_input.source(),
            slot: source,
        }
    );
    for input in [outer_input, inner_input] {
        assert_eq!(input.mode(), ClosureCaptureMode::Owned);
        assert_eq!(input.effect(), ClosureCaptureEffect::Move);
        assert_eq!(
            owned.cleanup_conditions().get(input.condition()),
            Some(&CleanupCondition::Always)
        );
    }
    let mut owners = std::collections::BTreeMap::new();
    let mut slots = std::collections::BTreeMap::new();
    let mut instances = Vec::new();
    for round in 0..2_u32 {
        let xs_instance = 100 + round;
        let outer_instance = 200 + round;
        let inner_instance = 300 + round;
        owners.insert(xs_owner, xs_instance);
        owners.insert(outer_owner, outer_instance);
        let moved = owners.remove(&xs_owner).unwrap();
        assert!(
            slots
                .insert((owners[&written_outer], outer), moved)
                .is_none()
        );
        owners.insert(inner_owner, inner_instance);
        let CleanupCaptureValue::Environment { owner, slot, .. } = inner_input.value() else {
            unreachable!();
        };
        let moved = slots.remove(&(owners[&owner], slot)).unwrap();
        assert!(
            slots
                .insert((owners[&written_inner], inner), moved)
                .is_none()
        );
        instances.push((outer_instance, inner_instance, xs_instance));
    }
    assert_eq!(slots[&(instances[0].1, inner)], instances[0].2);
    assert_eq!(slots[&(instances[1].1, inner)], instances[1].2);
    let call = |text| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == text).then_some(id))
            .unwrap()
    };
    let call_order = [call("inner()"), call("outer()")];
    let drop_index = |call| {
        steps
            .iter()
            .position(|(point, action)| {
                *point == DropPoint::CallReturn(call)
                    && matches!(action, IterationCleanupAction::Drop(fact) if captured.contains(&fact))
            })
            .unwrap()
    };
    assert!(drop_index(call_order[0]) < drop_index(call_order[1]));
    let mut released = Vec::new();
    for (outer_instance, inner_instance, _) in instances {
        for (call, expected_owner, expected_slot, instance, input) in [
            (
                call_order[0],
                inner_owner,
                inner,
                inner_instance,
                inner_input,
            ),
            (
                call_order[1],
                outer_owner,
                outer,
                outer_instance,
                outer_input,
            ),
        ] {
            let actions = steps
                .iter()
                .filter_map(|(point, action)| match action {
                    IterationCleanupAction::Drop(fact)
                        if *point == DropPoint::CallReturn(call) && captured.contains(&fact) =>
                    {
                        Some(fact)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(actions.len(), 1, "each call needs its captured drop action");
            let fact = actions[0];
            assert_eq!(fact.point(), DropPoint::CallReturn(call));
            assert_eq!(fact.capture_slot(), Some(expected_slot));
            assert!(fact.condition().is_none_or(|condition| {
                owned.cleanup_conditions().get(condition) == Some(&CleanupCondition::Always)
            }));
            assert!(
                matches!(fact.target(), DropTarget::Captured { owner, value, .. }
                if owner == expected_owner && value == input.value())
            );
            if let Some(value) = slots.remove(&(instance, expected_slot)) {
                released.push(value);
            }
        }
    }
    assert_eq!(released, [100, 101], "each invocation releases its own xs");
    assert!(slots.is_empty(), "the moved outer slots stay empty");
}

#[test]
fn recursively_released_closure_keeps_its_capture_slot() {
    use lang_frontend::ownership_checking::DropTarget;
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val base: move () -> Unit = move { read(xs) }
            val outer: move () -> Unit = move { base() }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let captured = owned
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), DropTarget::Captured { .. }))
        .collect::<Vec<_>>();
    assert_eq!(captured.len(), 2, "outer owns base, base owns xs");
    for fact in captured {
        let DropTarget::Captured {
            closure, source, ..
        } = fact.target()
        else {
            unreachable!();
        };
        let slot = fact
            .capture_slot()
            .expect("recursive release keeps the slot identity");
        let layout = owned.cleanup_conditions().capture_slot_value(slot).unwrap();
        assert_eq!(layout.source(), source);
        assert!(matches!(
            owned.cleanup_conditions().owner_value(layout.environment()),
            Some(lang_frontend::ownership_checking::CleanupOwnerValue::Closure {
                expression,
                ..
            }) if *expression == closure
        ));
    }
}

#[test]
fn capture_inputs_keep_creation_conditions_separate_from_transport_conditions() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupOwnerValue, CleanupSelectorId, DropTarget, IterationCleanupAction,
    };
    use std::collections::{BTreeMap, BTreeSet};
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flag: Boolean) {
            var xs = listOf(0)
            if (flag) { xs = listOf(1) } else { xs = listOf(2) }
            val f: move () -> Unit = move { read(xs) }
            val g = f
            val used = g()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let table = owned.cleanup_conditions();
    fn enabled(
        table: &CleanupConditions,
        id: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(id).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => enabled(
                table,
                branches[*choices
                    .get(selector)
                    .expect("only initialized selections may be read")],
                choices,
            ),
        }
    }
    let copies = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, .. } => {
                Some(table.owner_snapshot(*owner).unwrap())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        copies.len(),
        2,
        "both initialization and moving to g save independent selections"
    );
    let targets = copies
        .iter()
        .flat_map(|snapshot| snapshot.copies().iter().map(|copy| copy.target()))
        .collect::<BTreeSet<_>>();
    let (environment, initial) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, .. } => {
                match table.owner_value(*owner).unwrap() {
                    CleanupOwnerValue::Closure { inputs, .. } => Some((*owner, inputs)),
                    _ => None,
                }
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(initial.len(), 2);
    assert_eq!(initial[0].source(), initial[1].source());
    let slot = table
        .capture_slot(environment, initial[0].source())
        .expect("both conditional source versions share one capture slot");
    let edges = table.closure_capture_edges(environment).unwrap();
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0].target(), slot);
    assert_eq!(edges[1].target(), slot);
    assert_eq!(edges[0].input(), initial[0]);
    assert_eq!(edges[1].input(), initial[1]);
    let steps = owned.cleanup_steps();
    let creation = steps
        .iter()
        .position(|(_, action)| matches!(action, IterationCleanupAction::CreateClosureOwner { owner, .. } if *owner == environment))
        .unwrap();
    for (offset, edge) in edges.iter().enumerate() {
        assert!(matches!(
            steps[creation + offset + 1],
            (point, IterationCleanupAction::SaveClosureCapture { owner, target, input })
                if point == steps[creation].0 && owner == environment && target == edge.target() && input == edge.input()
        ));
    }
    assert_eq!(
        table.capture_slot_value(slot).unwrap().environment(),
        environment
    );
    assert_eq!(table.capture_slot_value(slot).unwrap().position(), 0);
    let mut direct = None;
    for input in initial {
        let CleanupCondition::Choice { selector, .. } = table.get(input.condition()).unwrap()
        else {
            panic!("source version choice");
        };
        assert!(
            !targets.contains(selector),
            "creation cannot read a later Save target"
        );
        assert!(direct.is_none_or(|prior| prior == *selector));
        direct = Some(*selector);
    }
    let direct = direct.unwrap();
    for arm in 0..2 {
        let mut choices = BTreeMap::from([(direct, arm)]);
        let selected = initial
            .iter()
            .filter(|input| enabled(table, input.condition(), &choices))
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1);
        let source = selected[0].value();
        for snapshot in &copies {
            let before = choices.clone();
            for copy in snapshot.copies() {
                if enabled(table, copy.when(), &before) {
                    choices.insert(copy.target(), before[&copy.source()]);
                }
            }
        }
        // The source control can be evaluated again; g must retain its saved source choice.
        choices.insert(direct, 1 - arm);
        let released = owned
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.target(), DropTarget::Captured { .. })
                    && fact
                        .condition()
                        .is_none_or(|condition| enabled(table, condition, &choices))
            })
            .collect::<Vec<_>>();
        assert_eq!(released.len(), 1);
        let DropTarget::Captured { owner, value, .. } = released[0].target() else {
            unreachable!();
        };
        assert_eq!(owner, copies.last().unwrap().owner());
        assert_eq!(
            value, source,
            "moving the environment must retain its concrete captured value"
        );
        assert_eq!(
            value,
            CleanupCaptureValue::Owner(released[0].owner().unwrap())
        );
    }
}

#[test]
fn loop_carried_hidden_sources_keep_separate_cleanup_for_prior_and_current_environments() {
    use lang_frontend::ownership_checking::DropPoint;
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            for (_ in flags) {
                val prior = f
                val xs = listOf(1)
                { g = prior }
                { f = ({ read(xs) }) }
            }
            val first = g()
            val second = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let mut owners = Vec::new();
    for call in ["g()", "f()"] {
        let released = owned.drops().iter().filter(|fact|
            sources.slice(fact.value_origin()).unwrap() == "xs"
            && matches!(fact.point(), DropPoint::CallReturn(id)
                if sources.slice(parsed.ast().expressions().get(id).unwrap().span()).unwrap() == call))
            .collect::<Vec<_>>();
        assert!(
            !released.is_empty(),
            "{call}: hidden source must survive its lexical scope until the corresponding environment is released"
        );
        assert!(
            released.iter().all(|fact| fact.condition().is_some()),
            "zero iterations have no hidden source"
        );
        owners.push(
            released
                .iter()
                .map(|fact| fact.owner().expect("transported source owner"))
                .collect::<std::collections::BTreeSet<_>>(),
        );
    }
    assert!(
        owners[0].is_disjoint(&owners[1]),
        "the same local definition creates different live source instances in prior and current environments"
    );
}

#[test]
fn source_loan_prevents_move_replacement_and_exclusive_access() {
    for (parameter, body, expected) in [
        ("own xs: List<Int>", "consume(xs)\nbreak", "xs"),
        ("xs: List<Int>", "consume(xs)\nbreak", "xs"),
        ("inout xs: List<Int>", "replace(&xs)\nbreak", "&"),
        ("inout xs: List<Int>", "xs = listOf(2)\nbreak", "xs"),
    ] {
        let (sources, _, owned) = checked(&format!(
            "fun consume(own xs: List<Int>) {{}}\nfun replace(inout xs: List<Int>) {{}}\nfun run({parameter}) {{ for (_ in xs) {{ {body} }} }}"
        ));
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{body}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135", "{body}");
        assert_eq!(
            sources
                .slice(owned.diagnostics()[0].primary_span())
                .unwrap(),
            expected
        );
        assert!(owned.drops().is_empty(), "invalid plans must not escape");
    }
}

#[test]
fn loop_origin_fixed_point_reaches_prior_bindings_after_multiple_backedges() {
    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            var h: () -> Unit = {}
            for (_ in flags) {
                { h = g }
                { g = f }
                { f = ({ val marker = 7 }) }
            }
            val first = h()
            val second = g()
            val third = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let plan = &owned.iterations()[0];
    for name in ["f", "g", "h"] {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id();
        for bindings in [plan.closure_flow().header(), plan.closure_flow().exit()] {
            assert!(
                bindings.iter().any(|binding| binding.symbol() == symbol
                    && binding.origins().iter().any(|&id| sources
                        .slice(parsed.ast().expressions().get(id).unwrap().span())
                        .unwrap()
                        .contains("marker"))),
                "{name}: the last lambda reaches every carried binding; one simulated iteration is insufficient"
            );
        }
    }
}

#[test]
fn loop_origin_fixed_point_keeps_jump_edges_and_unreachable_tail_separate() {
    for (transfer, in_header, in_exit) in [
        ("continue", true, true),
        ("break", false, true),
        ("return", false, false),
        ("error(\"stop\")", false, false),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun run(flags: List<Boolean>) {{
                var f: () -> Unit = {{}}
                for (_ in flags) {{
                    {{ f = ({{ val selected = 1 }}) }}
                    {transfer}
                    {{ f = ({{ val unreachable = 2 }}) }}
                }}
                val used = f()
            }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{transfer}: {:?}",
            owned.diagnostics()
        );
        let flow = owned.iterations()[0].closure_flow();
        for (rows, expected) in [(flow.header(), in_header), (flow.exit(), in_exit)] {
            let values = rows
                .iter()
                .flat_map(|row| row.origins())
                .map(|&id| {
                    sources
                        .slice(parsed.ast().expressions().get(id).unwrap().span())
                        .unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(
                values.iter().any(|text| text.contains("selected")),
                expected,
                "{transfer}: {values:?}"
            );
            assert!(
                !values.iter().any(|text| text.contains("unreachable")),
                "{transfer}: dead tail cannot add an origin"
            );
        }
    }
}

#[test]
fn loop_origin_fixed_point_preserves_elvis_value_and_non_null_continuation() {
    {
        let (sources, parsed, owned) = checked(
            "fun run(flags: List<Boolean>) {
                var f: () -> Unit = {}
                val none: Nothing? = null
                for (_ in flags) { { f = (none ?: ({ val selected = 1 })) }\nbreak }
                val used = f()
            }",
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(
            owned.iterations()[0]
                .closure_flow()
                .exit()
                .iter()
                .flat_map(|row| row.origins())
                .any(|&id| sources
                    .slice(parsed.ast().expressions().get(id).unwrap().span())
                    .unwrap()
                    .contains("selected")),
            "Elvis delivers its selected lambda to the binding"
        );
    }
    for (operand, continues) in [("maybe", true), ("none", false)] {
        let (sources, parsed, owned) = checked(&format!(
            "fun run(flags: List<Boolean>, maybe: Int?) {{
                var f: () -> Unit = {{}}
                val none: Nothing? = null
                for (_ in flags) {{
                    val observed = {operand} ?: return
                    {{ f = ({{ val selected = 1 }}) }}
                }}
                val used = f()
            }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let flow = owned.iterations()[0].closure_flow();
        for rows in [flow.header(), flow.exit()] {
            assert_eq!(
                rows.iter().flat_map(|row| row.origins()).any(|&id| sources
                    .slice(parsed.ast().expressions().get(id).unwrap().span())
                    .unwrap()
                    .contains("selected")),
                continues,
                "only the nullable non-null path bypasses the returning RHS"
            );
        }
    }
}

#[test]
fn loop_origin_fixed_point_propagates_outer_backedges_into_nested_and_later_loops() {
    let (sources, parsed, owned) = checked(
        "fun run(outer: List<Boolean>, inner: List<Boolean>, after: List<Boolean>) {
            var f: () -> Unit = {}
            for (_ in outer) {
                for (_ in inner) {
                    val g = f
                    { f = g }
                }
                { f = ({ val later = 1 }) }
            }
            for (_ in after) { val used = f() }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 3);
    for plan in owned.iterations() {
        let rows = plan.closure_flow().header();
        assert!(
            rows.iter().flat_map(|row| row.origins()).any(|&id| sources
                .slice(parsed.ast().expressions().get(id).unwrap().span())
                .unwrap()
                .contains("later")),
            "every loop, including the inner and following loops, sees the outer backedge source"
        );
        assert!(
            rows.iter().all(|row| row
                .origins()
                .windows(2)
                .all(|pair| pair[0].index() < pair[1].index())),
            "origins are unique and deterministically ordered"
        );
    }
}

#[test]
fn loop_origin_fixed_point_consumes_inner_jumps_and_drops_lexical_aliases() {
    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            for (_ in flags) {
                val alias = f
                { f = alias }
                loop { break }
                { f = ({ val reached = 1 }) }
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let alias = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "alias")
        .unwrap()
        .id();
    let flow = owned.iterations()[0].closure_flow();
    for rows in [flow.header(), flow.exit()] {
        assert!(
            rows.iter().all(|row| row.symbol() != alias),
            "dead local alias cannot become a carried owner"
        );
        assert!(
            rows.iter().flat_map(|row| row.origins()).any(|&id| sources
                .slice(parsed.ast().expressions().get(id).unwrap().span())
                .unwrap()
                .contains("reached")),
            "the inner break does not terminate the outer body"
        );
    }
}

#[test]
fn loop_origin_fixed_point_isolates_nested_callable_bindings_and_jumps() {
    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            for (_ in flags) {
                val action: () -> Unit = { for (_ in flags) { break } }
                val invoked = action()
                { f = ({ val continued = 1 }) }
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 2);
    let outer = owned
        .iterations()
        .iter()
        .find(|plan| {
            sources
                .slice(
                    parsed
                        .ast()
                        .statements()
                        .get(plan.descriptor().statement())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                .contains("action")
        })
        .unwrap();
    let inner = owned
        .iterations()
        .iter()
        .find(|plan| plan.descriptor().statement() != outer.descriptor().statement())
        .unwrap();
    assert!(
        inner.closure_flow().header().is_empty(),
        "callable state includes captures, not arbitrary creating-scope bindings"
    );
    assert!(
        outer
            .closure_flow()
            .header()
            .iter()
            .flat_map(|row| row.origins())
            .any(|&id| sources
                .slice(parsed.ast().expressions().get(id).unwrap().span())
                .unwrap()
                .contains("continued")),
        "break in the nested callable cannot terminate the creating callable's loop"
    );
}

#[test]
fn loop_origin_fixed_point_keeps_while_condition_effects_on_exhaustion() {
    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Boolean>, flag: Boolean) {
            var f: () -> Unit = {}
            while (if (flag) { { f = ({ val selected = 1 }) }\nfalse } else false) { return }
            for (_ in flags) { val invoked = f() }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 1);
    assert!(
        owned.iterations()[0]
            .closure_flow()
            .header()
            .iter()
            .flat_map(|row| row.origins())
            .any(|&id| sources
                .slice(parsed.ast().expressions().get(id).unwrap().span())
                .unwrap()
                .contains("selected")),
        "while has no backedge; the following loop receives the condition's completed state, not the entry header"
    );
}

#[test]
fn loop_phi_layout_uses_distinct_statement_sources_before_body_choices() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupOwnerValue, CleanupSelectorSource, IterationPhiBoundary,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>) {
            var f: () -> Unit = {}
            for (flag in flags) { f = if (flag) ({ read(xs) }) else ({ read(ys) }) }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.iterations()[0];
    let statement = plan.descriptor().statement();
    let table = owned.cleanup_conditions();
    let body_choice = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            sources
                .slice(node.span())
                .unwrap()
                .starts_with("if (flag)")
                .then_some(id)
        })
        .unwrap();
    let body_selector = table
        .selectors()
        .iter()
        .position(|selector| selector.control() == Some(body_choice))
        .unwrap();
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let phis = plan
        .closure_phis()
        .iter()
        .filter(|phi| phi.symbol() == f)
        .collect::<Vec<_>>();
    assert_eq!(phis.len(), 2, "header and exit each own a separate f value");
    assert_eq!(phis[0].boundary(), IterationPhiBoundary::Header);
    assert_eq!(phis[1].boundary(), IterationPhiBoundary::Exit);
    assert_eq!(phis[0].symbol(), phis[1].symbol());
    assert_ne!(phis[0].owner(), phis[1].owner());
    for phi in phis {
        assert!(
            matches!(table.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
            statement: owner_loop, boundary, symbol, ..
        }) if *owner_loop == statement && *boundary == phi.boundary() && *symbol == phi.symbol())
        );
        let expected = match phi.boundary() {
            IterationPhiBoundary::Header => plan.closure_flow().header(),
            IterationPhiBoundary::Exit => plan.closure_flow().exit(),
        }
        .iter()
        .find(|row| row.symbol() == phi.symbol())
        .unwrap()
        .origins();
        assert_eq!(
            phi.origins()
                .iter()
                .map(|origin| origin.closure())
                .collect::<Vec<_>>(),
            expected
        );
        for origin in phi.origins() {
            let selector = table.selector(origin.selector()).unwrap();
            assert!(
                matches!(selector.source(), CleanupSelectorSource::IterationPhi {
                statement: owner_loop, boundary, ..
            } if owner_loop == statement && boundary == phi.boundary())
            );
            assert_eq!(
                selector.control(),
                None,
                "phi may not masquerade as a source expression"
            );
            assert!(
                origin.selector().index() < body_selector,
                "header and exit fields must exist before planning the body"
            );
            let Some(CleanupCondition::Choice { branches, .. }) = table.get(origin.condition())
            else {
                panic!("presence is a saved boolean choice")
            };
            assert_eq!(table.get(branches[0]), Some(&CleanupCondition::Never));
            assert_eq!(table.get(branches[1]), Some(&CleanupCondition::Always));
        }
    }
}

#[test]
fn loop_phi_layout_keeps_opaque_owned_parameters_and_bindings() {
    use lang_frontend::ownership_checking::IterationPhiBoundary;
    let (sources, parsed, owned) = checked(
        "fun run(own first: move () -> Unit, own second: move () -> Unit, flags: List<Boolean>) {
            var f = first
            var g = second
            for (_ in flags) {
                val saved = f
                { f = g }
                { g = saved }
            }
            val a = f()
            val b = g()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let plan = &owned.iterations()[0];
    for name in ["f", "g"] {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id();
        for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
            assert!(
                plan.closure_phis().iter().any(|phi| {
                    phi.boundary() == boundary && phi.symbol() == symbol && phi.origins().is_empty()
                }),
                "opaque {name} needs an owner slot at {boundary:?}"
            );
        }
    }
}

#[test]
fn loop_phi_layout_keeps_uncaptured_owned_parameter_in_header() {
    use lang_frontend::ownership_checking::IterationPhiBoundary;
    let (sources, parsed, owned) = checked(
        "fun run(own action: move () -> Unit, flags: List<Boolean>) {
            for (_ in flags) { val marker = 1 }
            val used = action()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let symbol = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "action")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    assert!(plan.closure_phis().iter().any(|phi| {
        phi.boundary() == IterationPhiBoundary::Header
            && phi.symbol() == symbol
            && phi.origins().is_empty()
    }));
}

#[test]
fn loop_phi_layout_keeps_implicit_owned_lambda_parameter() {
    use lang_frontend::name_resolution::SymbolKind;
    use lang_frontend::ownership_checking::IterationPhiBoundary;
    for (parameter, body) in [
        ("it", "{ for (_ in flags) {}\nread(it) }"),
        ("arg", "{ arg -> for (_ in flags) {}\nread(arg) }"),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flags: List<Boolean>) {{
                val action: (own List<Int>) -> Unit = {body}
                val invoked = action(xs)
            }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{parameter}: {:?}",
            owned.diagnostics()
        );
        let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
            .unwrap()
            .id();
        let plan = &owned.iterations()[0];
        for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
            assert!(
                plan.closure_phis().iter().any(|phi| {
                    phi.boundary() == boundary && phi.symbol() == symbol && phi.origins().is_empty()
                }),
                "{parameter} lacks {boundary:?} owner slot"
            );
        }
    }
}

#[test]
fn loop_phi_layout_does_not_own_shared_lambda_capture() {
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) {
            val action: () -> Unit = { for (_ in flags) {}\nread(xs) }
            val invoked = action()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let xs = names
        .symbols()
        .iter()
        .rev()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    assert!(
        plan.closure_phis().iter().all(|phi| phi.symbol() != xs),
        "shared capture only borrows the outer owner"
    );
}

#[test]
fn phi_incoming_does_not_allocate_a_slot_for_copyable_capture() {
    use lang_frontend::ownership_checking::ClosureCaptureSource;

    let (sources, parsed, owned) = checked(
        "fun read(n: Int) {}\nfun run(flags: List<Int>) { val n = 1\nvar f: () -> Unit = { read(n) }\nfor (_ in flags) {}\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let n = names
        .symbols()
        .iter()
        .rfind(|symbol| sources.slice(symbol.span()).unwrap() == "n")
        .unwrap()
        .id();
    let inputs = owned
        .iterations()
        .iter()
        .flat_map(|plan| plan.closure_phi_incomings())
        .flat_map(|incoming| incoming.bindings())
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .filter(|input| input.input().source() == ClosureCaptureSource::Symbol(n))
        .collect::<Vec<_>>();
    assert!(
        !inputs.is_empty(),
        "the carried closure must retain its checked capture"
    );
    assert!(
        inputs
            .iter()
            .all(|input| input.target().is_none() && input.capture_slot().is_none())
    );
}

#[test]
fn loop_phi_layout_keeps_owned_root_in_elvis_null_rhs() {
    use lang_frontend::ownership_checking::IterationPhiBoundary;
    let (sources, parsed, owned) = checked(
        "class Node {}\nfun read(x: Node?) {}\nfun run(own x: Node?, flags: List<Boolean>, flag: Boolean) {
            val selected = x ?: if (flag) {
                for (_ in flags) { val used = read(x) }
                Node()
            } else Node()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let x = names
        .symbols()
        .iter()
        .rev()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "x")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
        assert!(
            plan.closure_phis().iter().any(|phi| {
                phi.boundary() == boundary && phi.symbol() == x && phi.origins().is_empty()
            }),
            "Elvis null RHS loses x at {boundary:?}"
        );
    }
}

#[test]
fn loop_phi_layout_separates_captured_owner_slots_for_prior_and_current_environments() {
    use lang_frontend::ownership_checking::{
        CleanupOwnerValue, ClosureCaptureEffect, ClosureCaptureMode, ClosureCaptureSource,
        IterationPhiBoundary,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            for (_ in flags) {
                val prior = f
                val xs = listOf(1)
                { g = prior }
                { f = ({ read(xs) }) }
            }
            val first = g()
            val second = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let plan = &owned.iterations()[0];
    let capture = owned
        .captures()
        .iter()
        .find(|capture| sources.slice(capture.reference_span()).unwrap() == "xs")
        .unwrap();
    let source = capture.source();
    assert!(matches!(source, ClosureCaptureSource::Symbol(_)));
    let mut owners = std::collections::BTreeSet::new();
    for name in ["f", "g"] {
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id();
        for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
            let phi = plan
                .closure_phis()
                .iter()
                .find(|phi| phi.symbol() == symbol && phi.boundary() == boundary)
                .unwrap();
            let origin = phi
                .origins()
                .iter()
                .find(|origin| origin.closure() == capture.lambda())
                .unwrap();
            let slot = origin
                .sources()
                .iter()
                .find(|slot| slot.source() == source)
                .unwrap();
            assert_eq!(slot.mode(), ClosureCaptureMode::Shared);
            assert_eq!(slot.effect(), ClosureCaptureEffect::Borrow);
            assert!(matches!(
                owned.cleanup_conditions().owner_value(slot.owner()),
                Some(CleanupOwnerValue::IterationPhiSourceOwner {
                    environment,
                    closure,
                    source: captured,
                    ..
                }) if *environment == phi.owner() && *closure == capture.lambda() && *captured == source
            ));
            assert!(
                owners.insert(slot.owner()),
                "{name} {boundary:?} shares a captured owner slot"
            );
        }
    }
    assert_eq!(owners.len(), 4);
}

#[test]
fn loop_phi_layout_does_not_own_a_borrowed_outer_element_capture() {
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(xs: List<List<Int>>, flags: List<Boolean>) {
            for (n in xs) {
                var f: () -> Unit = {}
                for (_ in flags) { f = ({ read(n) }) }
                val used = f()
            }
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let inner = owned
        .iterations()
        .iter()
        .find(|plan| {
            sources
                .slice(
                    parsed
                        .ast()
                        .statements()
                        .get(plan.descriptor().statement())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                .starts_with("for (_ in flags)")
        })
        .unwrap();
    let capture = owned
        .captures()
        .iter()
        .find(|capture| sources.slice(capture.reference_span()).unwrap() == "n")
        .unwrap();
    let origins = inner
        .closure_phis()
        .iter()
        .flat_map(|phi| phi.origins())
        .filter(|origin| origin.closure() == capture.lambda())
        .collect::<Vec<_>>();
    assert!(!origins.is_empty());
    assert!(
        origins.iter().all(|origin| origin.sources().is_empty()),
        "Borrow element has no owner to transport"
    );
}

#[test]
fn loop_phi_layout_distinguishes_owned_move_capture_source() {
    use lang_frontend::ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode};
    let (sources, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) {
            var f: move () -> Unit = move { read(xs) }
            for (_ in flags) { val invoked = f() }
            val after = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let capture = owned
        .captures()
        .iter()
        .find(|capture| sources.slice(capture.reference_span()).unwrap() == "xs")
        .unwrap();
    let plan = &owned.iterations()[0];
    let origins = plan
        .closure_phis()
        .iter()
        .flat_map(|phi| phi.origins())
        .filter(|origin| origin.closure() == capture.lambda())
        .collect::<Vec<_>>();
    assert_eq!(origins.len(), 2, "header and exit keep separate sources");
    for origin in origins {
        let slot = origin
            .sources()
            .iter()
            .find(|slot| slot.source() == capture.source())
            .unwrap();
        assert_eq!(slot.mode(), ClosureCaptureMode::Owned);
        assert_eq!(slot.effect(), ClosureCaptureEffect::Move);
    }
}

#[test]
fn loop_phi_source_slots_keep_checked_capture_order() {
    use lang_frontend::ownership_checking::ClosureCaptureSource;
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, label: Int, flags: List<Boolean>) {
            var f: () -> Unit = { val seen = label\nval first = read(ys)\nval second = read(xs) }
            for (_ in flags) { val invoked = f() }
            val after = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let plan = &owned.iterations()[0];
    let (phi, origin) = plan
        .closure_phis()
        .iter()
        .find_map(|phi| {
            phi.origins()
                .iter()
                .find(|origin| origin.sources().len() == 2)
                .map(|origin| (phi, origin))
        })
        .unwrap();
    assert_eq!(
        plan.capture_graph().nodes()[origin.node()]
            .sources()
            .iter()
            .map(|source| source.position())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        origin
            .sources()
            .iter()
            .map(|slot| match slot.source() {
                ClosureCaptureSource::Symbol(symbol) => {
                    sources
                        .slice(names.symbols()[symbol.index()].span())
                        .unwrap()
                }
                ClosureCaptureSource::This => "this",
            })
            .collect::<Vec<_>>(),
        ["ys", "xs"]
    );
    let environment = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::CreateClosureOwner {
                owner,
                closure,
            } if *closure == origin.closure() => Some(*owner),
            _ => None,
        })
        .expect("the phi source is a created closure environment");
    let label = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "label")
        .unwrap()
        .id();
    let label_slot = owned
        .cleanup_conditions()
        .capture_slot(environment, ClosureCaptureSource::Symbol(label))
        .expect("the non-phi capture still occupies its environment slot");
    assert_eq!(
        owned
            .cleanup_conditions()
            .capture_slot_value(label_slot)
            .unwrap()
            .position(),
        0
    );
    for (position, source) in origin.sources().iter().enumerate() {
        let slot = owned
            .cleanup_conditions()
            .capture_slot(environment, source.source())
            .expect("every checked capture has a static environment slot");
        assert_eq!(
            owned
                .cleanup_conditions()
                .capture_slot_value(slot)
                .unwrap()
                .position(),
            position + 1
        );
        let phi_slot = owned
            .cleanup_conditions()
            .phi_capture_slot(phi.owner(), origin.closure(), source.source())
            .expect("phi capture uses the full checked capture position");
        let mapped = phi
            .capture_layout()
            .iter()
            .find(|mapped| mapped.node() == origin.node() && mapped.position() == position + 1)
            .expect("the finite graph maps the original capture position");
        assert_eq!(mapped.slot(), phi_slot);
        assert_eq!(
            owned
                .cleanup_conditions()
                .capture_slot_value(phi_slot)
                .unwrap()
                .position(),
            position + 1
        );
    }
}

#[test]
fn phi_capture_slots_distinguish_same_source_in_alternative_lambdas() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureSource, DropPoint, DropTarget, IterationPhiBoundary, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean, flags: List<Boolean>) {
            var f: move () -> Unit = if (flag) (move { read(xs) }) else (move { read(xs) })
            for (_ in flags) { val marker = 1 }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let xs = names
        .symbols()
        .iter()
        .rfind(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let source = ClosureCaptureSource::Symbol(xs);
    let plan = &owned.iterations()[0];
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == f)
        .unwrap();
    assert_eq!(exit.origins().len(), 2);
    let table = owned.cleanup_conditions();
    let slots = exit
        .origins()
        .iter()
        .map(|origin| {
            let slot = table
                .phi_capture_slot(exit.owner(), origin.closure(), source)
                .expect("each lambda candidate has its own phi capture slot");
            let layout = table.capture_slot_value(slot).unwrap();
            assert_eq!(layout.environment(), exit.owner());
            assert_eq!(layout.closure(), origin.closure());
            assert_eq!(layout.source(), source);
            slot
        })
        .collect::<Vec<_>>();
    assert_ne!(slots[0], slots[1]);
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let carried = exhaustion
        .bindings()
        .iter()
        .find(|binding| binding.target() == exit.owner())
        .unwrap();
    assert_eq!(carried.origins().len(), slots.len());
    for (index, origin) in carried.origins().iter().enumerate() {
        let incoming_slots = origin
            .environments()
            .iter()
            .flat_map(|environment| environment.sources())
            .filter(|input| input.input().source() == source)
            .map(|input| input.capture_slot())
            .collect::<Vec<_>>();
        assert_eq!(incoming_slots, [Some(slots[index])]);
    }
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, expression)| {
            (sources.slice(expression.span()).unwrap() == "f()").then_some(id)
        })
        .unwrap();
    let released = owned
        .drops()
        .iter()
        .filter(|fact| {
            fact.point() == DropPoint::CallReturn(call)
                && matches!(fact.target(), DropTarget::Captured { owner, source: actual, .. }
                    if owner == exit.owner() && actual == source)
        })
        .collect::<Vec<_>>();
    assert_eq!(released.len(), 2);
    for fact in released {
        let DropTarget::Captured { closure, .. } = fact.target() else {
            unreachable!();
        };
        assert_eq!(
            fact.capture_slot(),
            table.phi_capture_slot(exit.owner(), closure, source)
        );
    }
}

#[test]
fn nested_phi_capture_drop_uses_the_root_layout_slot() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, ClosureCaptureSource, DropPoint, DropTarget, IterationPhiBoundary,
        IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>) {
            var f: move () -> Unit = move {}
            for (_ in flags) { val xs = listOf(1)
                val g: move () -> Unit = move { read(xs) }
                { f = move { g() } } }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let xs = names
        .symbols()
        .iter()
        .rfind(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let source = ClosureCaptureSource::Symbol(xs);
    let plan = &owned.iterations()[0];
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == f)
        .unwrap();
    let header = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == f)
        .unwrap();
    let inner = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, expression)| {
            (sources.slice(expression.span()).unwrap() == "move { read(xs) }").then_some(id)
        })
        .unwrap();
    let slot = owned
        .cleanup_conditions()
        .phi_capture_slot(exit.owner(), inner, source)
        .expect("the nested source shares the root phi's finite layout");
    let layout = owned.cleanup_conditions().capture_slot_value(slot).unwrap();
    assert_eq!(layout.environment(), exit.owner());
    assert_eq!(layout.closure(), inner);
    let source_slot = owned
        .cleanup_conditions()
        .phi_capture_slot(header.owner(), inner, source)
        .expect("nested forwarding reads the header environment layout");
    for incoming in plan.closure_phi_incomings() {
        for binding in incoming.bindings() {
            let layout = plan
                .closure_phis()
                .iter()
                .find(|layout| layout.owner() == binding.target())
                .unwrap();
            assert_eq!(binding.selector_writes().len(), layout.origins().len());
            for (write, node) in binding.selector_writes().iter().zip(layout.origins()) {
                assert_eq!(
                    (write.node(), write.target()),
                    (node.node(), node.selector())
                );
            }
        }
    }
    let entry = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let header_input = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == header.owner())
        .unwrap();
    let inner_node = header
        .origins()
        .iter()
        .find(|origin| origin.closure() == inner)
        .unwrap();
    assert_eq!(header_input.selector_writes().len(), header.origins().len());
    assert_eq!(
        owned.cleanup_conditions().get(
            header_input
                .selector_writes()
                .iter()
                .find(|write| write.node() == inner_node.node())
                .unwrap()
                .condition()
        ),
        Some(&CleanupCondition::Never),
        "zero-round entry must clear the not-yet-formed child presence"
    );
    let incoming_slots = plan
        .closure_phi_incomings()
        .iter()
        .filter(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .flat_map(|incoming| incoming.bindings())
        .filter(|binding| binding.target() == exit.owner())
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .flat_map(|source| source.captured())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .filter(|input| input.input().source() == source)
        .map(|input| (input.capture_slot(), input.source_capture_slot()))
        .collect::<Vec<_>>();
    assert!(
        incoming_slots.contains(&(Some(slot), Some(source_slot))),
        "{incoming_slots:?}"
    );
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, expression)| {
            (sources.slice(expression.span()).unwrap() == "f()").then_some(id)
        })
        .unwrap();
    assert!(owned.drops().iter().any(|fact| {
        fact.point() == DropPoint::CallReturn(call)
            && matches!(fact.target(), DropTarget::Captured { owner, closure, source: actual, .. }
                if owner != exit.owner() && closure == inner && actual == source)
            && fact.capture_slot() == Some(slot)
    }), "nested captured drop must use its root phi layout slot: {:?}", owned.drops());
}

#[test]
fn loop_phi_entry_writes_actual_owner_and_initializes_absent_origins() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupOwnerValue, IterationPhiBoundary,
        IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>) {
            var f: () -> Unit = { read(xs) }
            for (_ in flags) { f = ({ read(ys) }) }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let phi = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == f)
        .unwrap();
    let entry = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    assert_eq!(entry.boundary(), IterationPhiBoundary::Header);
    let binding = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == phi.owner())
        .unwrap();
    assert_eq!(binding.availability_selector(), phi.availability_selector());
    assert_eq!(
        owned.cleanup_conditions().get(binding.available_when()),
        Some(&CleanupCondition::Always)
    );
    let value = binding.values().first().unwrap();
    assert_ne!(value.source(), phi.owner());
    assert!(matches!(
        owned.cleanup_conditions().owner_value(value.source()),
        Some(CleanupOwnerValue::Closure { .. })
    ));
    let mut present = 0;
    let mut absent = 0;
    for origin in phi.origins() {
        let input = binding
            .origins()
            .iter()
            .find(|input| input.target() == origin.selector())
            .unwrap();
        let text = sources
            .slice(
                parsed
                    .ast()
                    .expressions()
                    .get(origin.closure())
                    .unwrap()
                    .span(),
            )
            .unwrap();
        if text.contains("read(xs)") {
            assert_eq!(
                owned.cleanup_conditions().get(input.condition()),
                Some(&CleanupCondition::Always)
            );
            assert_eq!(input.environments().len(), 1);
            let source = &input.environments()[0].sources()[0];
            assert_eq!(source.target(), Some(origin.sources()[0].owner()));
            assert!(matches!(source.value(), CleanupCaptureValue::Owner(_)));
            present += 1;
        } else {
            assert_eq!(
                owned.cleanup_conditions().get(input.condition()),
                Some(&CleanupCondition::Never)
            );
            assert!(input.environments().is_empty());
            absent += 1;
        }
    }
    assert_eq!((present, absent), (1, 1));

    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let exit = plan
        .closure_phis()
        .iter()
        .find(|candidate| {
            candidate.boundary() == IterationPhiBoundary::Exit && candidate.symbol() == f
        })
        .unwrap();
    let exit_input = exhaustion
        .bindings()
        .iter()
        .find(|binding| binding.target() == exit.owner())
        .unwrap();
    assert_eq!(exit_input.values()[0].source(), phi.owner());
    assert_eq!(exit_input.available_when(), phi.availability_condition());
    for origin in exit.origins() {
        let prior = phi
            .origins()
            .iter()
            .find(|prior| prior.closure() == origin.closure())
            .unwrap();
        let input = exit_input
            .origins()
            .iter()
            .find(|input| input.target() == origin.selector())
            .unwrap();
        assert_eq!(input.condition(), prior.condition());
        assert_eq!(input.environments()[0].owner(), phi.owner());
        let source = &input.environments()[0].sources()[0];
        assert_eq!(source.target(), Some(origin.sources()[0].owner()));
        assert_eq!(
            source.value(),
            CleanupCaptureValue::Owner(prior.sources()[0].owner())
        );
    }
}

#[test]
fn loop_phi_completed_body_edges_use_the_replacement_environment() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, IterationPhiBoundary, IterationPhiIncomingKind,
    };
    for (transfer, boundary) in [
        ("", IterationPhiBoundary::Header),
        ("continue", IterationPhiBoundary::Header),
        ("break", IterationPhiBoundary::Exit),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>) {{
                var f: () -> Unit = {{ read(xs) }}
                for (_ in flags) {{ f = ({{ read(ys) }})\n{transfer} }}
                val used = f()
            }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
        let f = names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
            .unwrap()
            .id();
        let plan = &owned.iterations()[0];
        let incoming = plan
            .closure_phi_incomings()
            .iter()
            .find(|incoming| {
                incoming.boundary() == boundary
                    && matches!(
                        (transfer, incoming.kind()),
                        ("", IterationPhiIncomingKind::Fallthrough)
                            | ("continue", IterationPhiIncomingKind::Continue(_))
                            | ("break", IterationPhiIncomingKind::Break(_))
                    )
            })
            .unwrap();
        let phi = plan
            .closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.symbol() == f)
            .unwrap();
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == phi.owner())
            .unwrap();
        assert_eq!(
            owned.cleanup_conditions().get(binding.available_when()),
            Some(&CleanupCondition::Always)
        );
        let mut selected = 0;
        for origin in phi.origins() {
            let input = binding
                .origins()
                .iter()
                .find(|input| input.target() == origin.selector())
                .unwrap();
            let text = sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(origin.closure())
                        .unwrap()
                        .span(),
                )
                .unwrap();
            if text.contains("read(ys)") {
                assert_eq!(
                    owned.cleanup_conditions().get(input.condition()),
                    Some(&CleanupCondition::Always)
                );
                assert_eq!(input.environments().len(), 1);
                let source = &input.environments()[0].sources()[0];
                assert_eq!(source.target(), Some(origin.sources()[0].owner()));
                assert!(matches!(source.value(), CleanupCaptureValue::Owner(_)));
                selected += 1;
            } else {
                assert_eq!(
                    owned.cleanup_conditions().get(input.condition()),
                    Some(&CleanupCondition::Never)
                );
            }
        }
        assert_eq!(selected, 1, "{transfer}");
    }
}

#[test]
fn loop_phi_entry_keeps_conditional_capture_sources_separate() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, IterationPhiBoundary, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean, flags: List<Boolean>) {
            var f: () -> Unit = if (flag) ({ read(xs) }) else ({ read(ys) })
            for (_ in flags) { val invoked = f() }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let phi = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == f)
        .unwrap();
    let entry = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let binding = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == phi.owner())
        .unwrap();
    assert_eq!(binding.origins().len(), 2);
    let mut sources_by_branch = Vec::new();
    for input in binding.origins() {
        let CleanupCondition::Choice { selector, branches } =
            owned.cleanup_conditions().get(input.condition()).unwrap()
        else {
            panic!("conditional origin must retain a choice")
        };
        assert_eq!(input.environments().len(), 1);
        let source = &input.environments()[0].sources()[0];
        let CleanupCaptureValue::Owner(owner) = source.value() else {
            panic!("MoveOnly captured source must keep its owner")
        };
        sources_by_branch.push((*selector, branches.clone(), owner));
    }
    assert_eq!(sources_by_branch[0].0, sources_by_branch[1].0);
    assert_ne!(sources_by_branch[0].1, sources_by_branch[1].1);
    assert_ne!(sources_by_branch[0].2, sources_by_branch[1].2);
}

#[test]
fn loop_phi_exhaustion_does_not_transport_an_owner_dropped_at_loop_exit() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, DropPoint, DropTarget, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) {
            for (_ in flags) { val used = read(xs) }
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let xs = names
        .symbols()
        .iter()
        .rev()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let loop_id = plan.descriptor().statement();
    assert!(owned.drops().iter().any(|fact| {
        fact.point() == DropPoint::LoopExit(loop_id) && fact.target() == DropTarget::Named(xs)
    }));
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| {
            phi.symbol() == xs
                && phi.boundary() == lang_frontend::ownership_checking::IterationPhiBoundary::Exit
        })
        .unwrap();
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let input = exhaustion
        .bindings()
        .iter()
        .find(|input| input.target() == exit.owner())
        .unwrap();
    assert_eq!(
        owned.cleanup_conditions().get(input.available_when()),
        Some(&CleanupCondition::Never)
    );
    assert!(input.values().is_empty());
}

#[test]
fn loop_phi_exit_transports_a_held_source_only_through_its_closure() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, IterationPhiBoundary, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) {
            var f: () -> Unit = { read(xs) }
            for (_ in flags) { break }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let xs = owned
        .captures()
        .iter()
        .find_map(|capture| {
            (sources.slice(capture.reference_span()).unwrap() == "xs").then_some(capture.source())
        })
        .unwrap();
    let lang_frontend::ownership_checking::ClosureCaptureSource::Symbol(xs) = xs else {
        panic!("xs is a named owner")
    };
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let xs_exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == xs)
        .unwrap();
    let f_exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == f)
        .unwrap();
    for incoming in plan.closure_phi_incomings().iter().filter(|incoming| {
        matches!(
            incoming.kind(),
            IterationPhiIncomingKind::Break(_) | IterationPhiIncomingKind::Exhaustion
        )
    }) {
        let source_binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == xs_exit.owner())
            .unwrap();
        assert_eq!(
            owned
                .cleanup_conditions()
                .get(source_binding.available_when()),
            Some(&CleanupCondition::Never)
        );
        assert!(source_binding.values().is_empty());
        let closure_binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == f_exit.owner())
            .unwrap();
        assert!(closure_binding.origins().iter().any(|origin| {
            origin.environments().iter().any(|environment| {
                environment
                    .sources()
                    .iter()
                    .any(|source| matches!(source.value(), CleanupCaptureValue::Owner(_)))
            })
        }));
    }
}

#[test]
fn loop_phi_exhaustion_preserves_an_outer_pending_call_loan() {
    use lang_frontend::ownership_checking::{IterationPhiBoundary, IterationPhiIncomingKind};
    let (sources, parsed, owned) = checked(
        "fun use(xs: List<Int>, n: Int) {}\nfun run(own xs: List<Int>, flags: List<Boolean>) {
            use(xs, if (true) { for (_ in flags) {}\n0 } else 0)
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let xs = names
        .symbols()
        .iter()
        .rev()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "xs")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == xs)
        .unwrap();
    let header = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == xs)
        .unwrap();
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let input = exhaustion
        .bindings()
        .iter()
        .find(|binding| binding.target() == exit.owner())
        .unwrap();
    assert_eq!(input.available_when(), header.availability_condition());
    assert_eq!(input.values().len(), 1);
}

#[test]
fn loop_phi_exhaustion_preserves_the_old_owner_during_replacement_rhs() {
    use lang_frontend::ownership_checking::{IterationPhiBoundary, IterationPhiIncomingKind};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var target = listOf(1)
            { target = if (true) { val observed = read(target)\nfor (_ in flags) {}\nlistOf(2) } else listOf(3) }
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let target = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "target")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let header = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == target)
        .unwrap();
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == target)
        .unwrap();
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let input = exhaustion
        .bindings()
        .iter()
        .find(|binding| binding.target() == exit.owner())
        .unwrap();
    assert_eq!(input.available_when(), header.availability_condition());
    assert_eq!(input.values()[0].source(), header.owner());
}

#[test]
fn loop_phi_backedge_reads_the_previous_header_environment() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupOwnerValue, IterationPhiBoundary, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            for (_ in flags) {
                val prior = f
                { g = prior }
                { f = ({ val marker = 1 }) }
            }
            val first = g()
            val second = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let g = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "g")
        .unwrap()
        .id();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let f_header = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == f)
        .unwrap();
    let phi = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == g)
        .unwrap();
    let carried = phi
        .origins()
        .iter()
        .find(|origin| {
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(origin.closure())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                .contains("marker")
        })
        .unwrap();
    let fallthrough = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
        .unwrap();
    let binding = fallthrough
        .bindings()
        .iter()
        .find(|binding| binding.target() == phi.owner())
        .unwrap();
    let input = binding
        .origins()
        .iter()
        .find(|input| input.target() == carried.selector())
        .unwrap();
    assert_ne!(
        owned.cleanup_conditions().get(input.condition()),
        Some(&CleanupCondition::Never),
        "after the first round, g receives f's prior header environment"
    );
    let owner = input.environments()[0].owner();
    let mut pending = vec![owner];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !seen.insert(current) {
            continue;
        }
        if let Some(CleanupOwnerValue::Snapshot(snapshot)) =
            owned.cleanup_conditions().owner_value(current)
        {
            pending.extend(snapshot.capture_inputs().iter().map(|input| input.owner()));
        }
    }
    assert!(
        seen.contains(&f_header.owner()),
        "g must read header f through saved values, not the zero-iteration entry: {seen:?}"
    );
}

#[test]
fn loop_phi_backedge_copies_the_prior_environment_source_relation() {
    assert_loop_phi_source_replay("");
}

#[test]
fn loop_phi_continue_copies_the_prior_environment_source_relation() {
    assert_loop_phi_source_replay("continue");
}

fn assert_loop_phi_source_replay(transfer: &str) {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupOwnerValueId, CleanupSelection, CleanupSelectorId, CleanupSelectorSource,
        ClosureCaptureSource, DropFact, DropPoint, DropTarget, IterationCleanupAction,
        IterationPhiBoundary, IterationPhiIncoming, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(&format!(
        "fun read(xs: List<Int>) {{}}\nfun run(flags: List<Boolean>) {{
            var f: () -> Unit = {{}}
            var g: () -> Unit = {{}}
            for (_ in flags) {{
                val prior = f
                val xs = listOf(1)
                {{ g = prior }}
                {{ f = ({{ read(xs) }}) }}
                {transfer}
            }}
            val first = g()
            val second = f()
        }}"
    ));
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let symbol = |name| {
        names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id()
    };
    let plan = &owned.iterations()[0];
    let f = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == symbol("f"))
        .unwrap();
    let g = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == symbol("g"))
        .unwrap();
    let lambda = f
        .origins()
        .iter()
        .find(|origin| {
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(origin.closure())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                .contains("read(xs)")
        })
        .unwrap();
    let carried = g
        .origins()
        .iter()
        .find(|origin| origin.closure() == lambda.closure())
        .unwrap();
    let backedge = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| match incoming.kind() {
            IterationPhiIncomingKind::Fallthrough => transfer.is_empty(),
            IterationPhiIncomingKind::Continue(_) => !transfer.is_empty(),
            _ => false,
        })
        .unwrap();
    let input = backedge
        .bindings()
        .iter()
        .find(|binding| binding.target() == g.owner())
        .unwrap()
        .origins()
        .iter()
        .find(|input| input.target() == carried.selector())
        .unwrap();
    assert_ne!(
        owned.cleanup_conditions().get(input.condition()),
        Some(&CleanupCondition::Never)
    );
    assert!(input.environments().iter().any(|environment| {
        environment.sources().iter().any(|source| {
            source.target() == Some(carried.sources()[0].owner())
                && source.value() == CleanupCaptureValue::Owner(lambda.sources()[0].owner())
        })
    }));
    let source_input = |binding, selector| {
        backedge
            .bindings()
            .iter()
            .find(|incoming| incoming.target() == binding)
            .unwrap()
            .origins()
            .iter()
            .find(|origin| origin.target() == selector)
            .unwrap()
            .environments()
            .iter()
            .flat_map(|environment| environment.sources())
            .find_map(|source| match (source.target(), source.value()) {
                (Some(target), CleanupCaptureValue::Owner(actual)) => Some((target, actual)),
                _ => None,
            })
            .unwrap()
    };
    let f_source = source_input(f.owner(), lambda.selector());
    let g_source = source_input(g.owner(), carried.selector());
    assert_eq!(g_source.1, f_source.0, "g reads the previous f source");
    assert_ne!(f_source.1, f_source.0, "f gets this round's fresh xs");
    fn selected(
        table: &CleanupConditions,
        id: CleanupConditionId,
        choices: &std::collections::BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(id).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                selected(table, branches[choices[selector]], choices)
            }
        }
    }
    fn apply_incoming(
        table: &CleanupConditions,
        incoming: &IterationPhiIncoming,
        choices: &mut std::collections::BTreeMap<CleanupSelectorId, usize>,
        owners: &mut std::collections::BTreeMap<CleanupOwnerValueId, u32>,
        environments: &mut std::collections::BTreeMap<CleanupOwnerValueId, u32>,
        fresh_source: Option<(CleanupOwnerValueId, u32)>,
    ) {
        let before_choices = choices.clone();
        let before_owners = owners.clone();
        let before_environments = environments.clone();
        assert!(selected(table, incoming.condition(), &before_choices));
        let mut choice_writes = Vec::new();
        let mut owner_writes = Vec::new();
        let mut environment_writes = Vec::new();
        for binding in incoming.bindings() {
            let available = selected(table, binding.available_when(), &before_choices);
            choice_writes.push((binding.availability_selector(), usize::from(available)));
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert_eq!(
                values.len(),
                usize::from(available),
                "an available phi must copy exactly one environment value"
            );
            let environment = values.first().map(|value| {
                *before_environments
                    .get(&value.source())
                    .expect("incoming must read an initialized environment value")
            });
            if let Some(environment) = environment {
                environment_writes.push((binding.target(), environment));
            }
            let mut selected_environments = Vec::new();
            for write in binding.selector_writes() {
                choice_writes.push((
                    write.target(),
                    usize::from(selected(table, write.condition(), &before_choices)),
                ));
            }
            for origin in binding.origins() {
                let present = selected(table, origin.condition(), &before_choices);
                if !present {
                    continue;
                }
                for environment in origin.environments() {
                    if !selected(table, environment.condition(), &before_choices) {
                        continue;
                    }
                    selected_environments.push(
                        *before_environments
                            .get(&environment.owner())
                            .expect("selected origin needs its actual environment"),
                    );
                    for source in environment.sources() {
                        if !selected(table, source.input().condition(), &before_choices) {
                            continue;
                        }
                        if let (Some(target), CleanupCaptureValue::Owner(value)) =
                            (source.target(), source.value())
                        {
                            let actual = before_owners.get(&value).copied().or_else(|| {
                                fresh_source
                                    .and_then(|(owner, round)| (value == owner).then_some(round))
                            });
                            let actual = actual.expect("incoming must read an initialized owner");
                            owner_writes.push((target, actual));
                        }
                    }
                }
            }
            if let Some(environment) = environment {
                assert_eq!(
                    selected_environments,
                    [environment],
                    "the selected capture origin belongs to the copied environment value"
                );
            } else {
                assert!(selected_environments.is_empty());
            }
        }
        choices.extend(choice_writes);
        owners.extend(owner_writes);
        environments.extend(environment_writes);
    }
    let table = owned.cleanup_conditions();
    let snapshots = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot {
                condition,
                owner,
                value,
            } => Some((
                *point,
                *condition,
                *value,
                table.owner_snapshot(*owner).unwrap(),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(snapshots.len(), 3, "the body saves prior, g and f once");
    assert!(
        snapshots
            .iter()
            .all(|(_, _, value, snapshot)| snapshot.value() == *value),
        "the saved snapshot must describe the evaluated action value"
    );
    let fresh_closure = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::CreateClosureOwner { owner, closure }
                if *closure == lambda.closure() =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .expect("the body creates the captured lambda environment");
    let fresh_source = match table.owner_value(fresh_closure).unwrap() {
        lang_frontend::ownership_checking::CleanupOwnerValue::Closure { inputs, .. } => {
            assert_eq!(inputs.len(), 1, "the fresh lambda captures this round's xs");
            let CleanupCaptureValue::Owner(source) = inputs[0].value() else {
                panic!("the fresh lambda must capture an owned source");
            };
            source
        }
        _ => unreachable!(),
    };
    assert_eq!(
        f_source.1, fresh_source,
        "backedge f must copy the lambda's actual captured xs owner"
    );
    let ClosureCaptureSource::Symbol(local_xs) = lambda.sources()[0].source() else {
        panic!("this fixture captures its body-local xs");
    };
    let source_owners = plan
        .closure_phis()
        .iter()
        .flat_map(|phi| phi.origins())
        .filter(|origin| origin.closure() == lambda.closure())
        .flat_map(|origin| origin.sources().iter().map(|source| source.owner()))
        .chain(std::iter::once(fresh_source))
        .collect::<std::collections::BTreeSet<_>>();
    let is_source_drop = |fact: &DropFact| {
        fact.owner()
            .is_some_and(|owner| source_owners.contains(&owner))
            || matches!(fact.target(), DropTarget::RetainedSource(_))
            || fact.target() == DropTarget::Named(local_xs)
            || matches!(fact.target(), DropTarget::Captured { source: ClosureCaptureSource::Symbol(symbol), .. } if symbol == local_xs)
            || sources.slice(fact.value_origin()).unwrap() == "listOf(1)"
    };
    assert_eq!(
        snapshots[2]
            .3
            .capture_inputs()
            .iter()
            .map(|input| input.owner())
            .collect::<Vec<_>>(),
        [fresh_closure],
        "f's saved value must come from the new lambda environment"
    );
    assert_eq!(
        snapshots[2]
            .3
            .value_inputs()
            .iter()
            .map(|input| input.owner())
            .collect::<Vec<_>>(),
        [fresh_closure],
        "the public snapshot keeps the evaluated RHS owner for handle transport"
    );
    assert_eq!(
        snapshots
            .iter()
            .map(|(_, _, value, _)| {
                sources
                    .slice(parsed.ast().expressions().get(*value).unwrap().span())
                    .unwrap()
            })
            .collect::<Vec<_>>(),
        ["f", "prior", "({ read(xs) })"],
        "replay models the fixture's evaluated values in order"
    );
    assert!(
        snapshots
            .iter()
            .all(|(point, _, value, _)| { *point == DropPoint::AfterExpression(*value) }),
        "each snapshot must execute immediately after its evaluated value"
    );
    let f_commit = owned
        .cleanup_steps()
        .iter()
        .enumerate()
        .find_map(|(index, (point, action))| match action {
            IterationCleanupAction::CommitOwnerSnapshot { owner, target }
                if *target == symbol("f") =>
            {
                Some((index, *point, *owner))
            }
            _ => None,
        })
        .expect("f replacement commits its saved environment");
    let f_save = owned
        .cleanup_steps()
        .iter()
        .enumerate()
        .find_map(|(index, (point, action))| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, .. }
                if *owner == f_commit.2 && *point == f_commit.1 =>
            {
                Some(index)
            }
            _ => None,
        })
        .expect("f replacement saves its RHS at the commit point");
    assert!(
        f_save < f_commit.0,
        "save must precede f replacement commit"
    );
    let entry = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let mut choices = std::collections::BTreeMap::new();
    let mut owners = std::collections::BTreeMap::new();
    let mut environments = entry
        .bindings()
        .iter()
        .flat_map(|binding| {
            let instance = if binding.target() == f.owner() {
                100
            } else if binding.target() == g.owner() {
                101
            } else {
                panic!("unexpected entry binding")
            };
            binding
                .values()
                .iter()
                .map(move |value| (value.source(), instance))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    apply_incoming(
        table,
        entry,
        &mut choices,
        &mut owners,
        &mut environments,
        None,
    );
    let zero_round = (choices.clone(), owners.clone(), environments.clone());
    let loop_exit = DropPoint::LoopExit(plan.descriptor().statement());
    let mut modeled_early_points = snapshots
        .iter()
        .map(|(point, ..)| *point)
        .collect::<Vec<_>>();
    modeled_early_points.push(loop_exit);
    let assert_no_early_drop =
        |point, current: &std::collections::BTreeMap<CleanupSelectorId, usize>| {
            for (at, action) in owned.cleanup_steps() {
                let IterationCleanupAction::Drop(fact) = action else {
                    continue;
                };
                if *at == point && is_source_drop(fact) {
                    assert!(
                        !fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, current)),
                        "a live source cannot drop at {point:?}: {fact:?}"
                    );
                }
            }
        };
    for round in 1..=2 {
        for (index, (point, condition, _, snapshot)) in snapshots.iter().enumerate() {
            let prior = choices.clone();
            assert!(
                condition.is_none_or(|guard| selected(table, guard, &prior)),
                "round {round}: the fixture must execute every saved value"
            );
            let mut writes = Vec::new();
            for copy in snapshot.copies() {
                if selected(table, copy.when(), &prior) {
                    writes.push((copy.target(), prior[&copy.source()]));
                }
            }
            choices.extend(writes);
            let instance = if index == 2 {
                let input = &snapshot.capture_inputs()[0];
                assert!(selected(table, input.condition(), &prior));
                environments.insert(fresh_closure, round);
                environments[&input.owner()]
            } else if index == 0 {
                environments[&f.owner()]
            } else {
                environments[&snapshots[0].3.owner()]
            };
            environments.insert(snapshot.owner(), instance);
            assert_no_early_drop(*point, &choices);
        }
        apply_incoming(
            table,
            backedge,
            &mut choices,
            &mut owners,
            &mut environments,
            Some((fresh_source, round)),
        );
    }
    assert_eq!(environments[&g.owner()], 1, "g keeps round one's closure");
    assert_eq!(environments[&f.owner()], 2, "f keeps round two's closure");
    assert_eq!(owners[&g_source.0], 1, "g retains the earlier xs instance");
    assert_eq!(owners[&f_source.0], 2, "f owns this round's xs instance");
    assert_no_early_drop(loop_exit, &choices);
    apply_incoming(
        table,
        exhaustion,
        &mut choices,
        &mut owners,
        &mut environments,
        None,
    );
    let mut zero_choices = zero_round.0;
    let mut zero_owners = zero_round.1;
    let mut zero_environments = zero_round.2;
    assert_no_early_drop(loop_exit, &zero_choices);
    apply_incoming(
        table,
        exhaustion,
        &mut zero_choices,
        &mut zero_owners,
        &mut zero_environments,
        None,
    );
    for (name, expected) in [("f", 2), ("g", 1)] {
        let phi = plan
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == symbol(name)
            })
            .unwrap();
        let origin = phi
            .origins()
            .iter()
            .find(|origin| origin.closure() == lambda.closure())
            .unwrap();
        let slot = origin.sources()[0].owner();
        assert_eq!(
            choices[&origin.selector()],
            1,
            "{name}: two-round exit origin"
        );
        assert_eq!(owners[&slot], expected, "{name}: exit source instance");
        assert_eq!(
            environments[&phi.owner()],
            expected,
            "{name}: exit closure instance"
        );
        assert_eq!(
            zero_choices[&origin.selector()],
            0,
            "{name}: zero-round exit"
        );
        assert!(!zero_owners.contains_key(&slot));
        assert_eq!(
            zero_environments[&phi.owner()],
            if name == "f" { 100 } else { 101 },
            "{name}: zero-round exit keeps the initial closure"
        );
    }
    let call_points = ["g()", "f()"].map(|text| {
        let call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == text).then_some(id))
            .unwrap();
        DropPoint::CallReturn(call)
    });
    for (_, action) in owned.cleanup_steps() {
        if let IterationCleanupAction::Drop(fact) = action
            && is_source_drop(fact)
        {
            assert!(
                modeled_early_points.contains(&fact.point()) || call_points.contains(&fact.point()),
                "unreplayed captured-source cleanup point: {fact:?}"
            );
        }
    }
    let mut live_loans = std::collections::BTreeMap::from([(1, 1_usize), (2, 1_usize)]);
    let mut source_drops = std::collections::BTreeMap::<u32, usize>::new();
    for (index, (name, expected)) in [("g", 1), ("f", 2)].into_iter().enumerate() {
        let phi = plan
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == symbol(name)
            })
            .unwrap();
        let origin = phi
            .origins()
            .iter()
            .find(|origin| origin.closure() == lambda.closure())
            .unwrap();
        let slot = origin.sources()[0].owner();
        assert_eq!(owners[&slot], expected);
        let point = call_points[index];
        let retained = owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == point && is_source_drop(fact))
            .collect::<Vec<_>>();
        assert!(
            !retained.is_empty(),
            "{name}: retained-source cleanup exists"
        );
        for fact in &retained {
            assert!(
                !selected(table, fact.condition().unwrap(), &zero_choices),
                "{name}: zero-round call cannot release a source from an unexecuted body"
            );
        }
        let drop = retained
            .iter()
            .find(|fact| fact.owner() == Some(slot))
            .unwrap();
        let actions = owned
            .cleanup_steps()
            .iter()
            .filter(|(at, _)| *at == point)
            .map(|(_, action)| action)
            .collect::<Vec<_>>();
        let ended = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                IterationCleanupAction::EndCaptureLoan {
                    owner,
                    closure,
                    value: CleanupCaptureValue::Owner(source),
                    condition,
                    ..
                } if *owner == phi.owner() && *closure == origin.closure() && *source == slot => {
                    assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
                    Some(index)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(ended.len(), 1, "{name}: end exactly one source loan");
        let (tested, selector) = actions
            .iter()
            .enumerate()
            .find_map(|(index, action)| match action {
                IterationCleanupAction::TestLastCaptureLoan {
                    owner,
                    selector,
                    condition,
                    ..
                } if *owner == slot => {
                    assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
                    Some((index, *selector))
                }
                _ => None,
            })
            .unwrap();
        let dropped = actions
            .iter()
            .position(
                |action| matches!(action, IterationCleanupAction::Drop(fact) if fact == *drop),
            )
            .unwrap();
        assert!(ended[0] < tested && tested < dropped);
        let selector_info = table.selector(selector).unwrap();
        assert_eq!(selector_info.selection(), CleanupSelection::LastCaptureLoan);
        assert_eq!(
            selector_info.source(),
            CleanupSelectorSource::CaptureLoan { owner: slot }
        );
        let remaining = live_loans.get_mut(&expected).unwrap();
        *remaining -= ended.len();
        choices.insert(selector, usize::from(*remaining == 0));
        let executed = actions
            .iter()
            .filter_map(|action| match action {
                IterationCleanupAction::Drop(fact)
                    if is_source_drop(fact)
                        && fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, &choices)) =>
                {
                    Some(fact)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            executed.len(),
            1,
            "{name}: exactly one retained source drop"
        );
        assert_eq!(executed[0].owner(), Some(slot));
        let actual = owners[&executed[0].owner().unwrap()];
        *source_drops.entry(actual).or_insert(0) += 1;
    }
    assert_eq!(
        source_drops,
        std::collections::BTreeMap::from([(1, 1), (2, 1)])
    );
}

#[test]
fn loop_carried_sibling_closures_gate_shared_source_drop_on_last_loan() {
    assert_loop_carried_sibling_release(["f", "g"]);
}

#[test]
fn loop_carried_sibling_closures_release_in_reverse_order() {
    assert_loop_carried_sibling_release(["g", "f"]);
}

fn assert_loop_carried_sibling_release(call_order: [&str; 2]) {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupSelection, CleanupSelectorId, CleanupSelectorSource, ClosureCaptureSource,
        DropPoint, DropTarget, IterationCleanupAction, IterationPhiBoundary,
        IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(&format!(
        "fun read(xs: List<Int>) {{}}\nfun run(flags: List<Boolean>) {{
            var f: () -> Unit = {{}}
            var g: () -> Unit = {{}}
            for (_ in flags) {{
                val xs = listOf(1)
                {{ f = ({{ read(xs) }}) }}
                {{ g = ({{ read(xs) }}) }}
                break
            }}
            val first = {}()
            val second = {}()
        }}",
        call_order[0], call_order[1]
    ));
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let break_incoming = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| matches!(incoming.kind(), IterationPhiIncomingKind::Break(_)))
        .unwrap();
    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &std::collections::BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                selected(table, branches[choices[selector]], choices)
            }
        }
    }
    let table = owned.cleanup_conditions();
    let mut choices = std::collections::BTreeMap::new();
    assert!(selected(table, break_incoming.condition(), &choices));
    let before = choices.clone();
    let mut writes = Vec::new();
    for binding in break_incoming.bindings() {
        let available = selected(table, binding.available_when(), &before);
        writes.push((binding.availability_selector(), usize::from(available)));
        for write in binding.selector_writes() {
            writes.push((
                write.target(),
                usize::from(selected(table, write.condition(), &before)),
            ));
        }
    }
    choices.extend(writes);
    let sources_by_slot = break_incoming
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .filter_map(|source| match (source.target(), source.value()) {
            (Some(slot), CleanupCaptureValue::Owner(actual)) => Some((slot, actual)),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(sources_by_slot.len(), 2, "f/g need distinct source slots");
    let mut live_loans = std::collections::BTreeMap::new();
    for actual in sources_by_slot.values() {
        *live_loans.entry(*actual).or_insert(0_usize) += 1;
    }
    assert_eq!(live_loans.len(), 1, "f/g borrow one actual xs instance");
    let actual = *live_loans.keys().next().unwrap();
    let early = owned
        .drops()
        .iter()
        .filter(|fact| fact.owner() == Some(actual))
        .collect::<Vec<_>>();
    assert!(
        early.is_empty(),
        "both closures must keep xs live: {early:?}"
    );
    let premature = owned
        .drops()
        .iter()
        .filter(|fact| {
            matches!(fact.target(), DropTarget::RetainedSource(_))
                && !matches!(fact.point(), DropPoint::CallReturn(_))
        })
        .collect::<Vec<_>>();
    assert!(
        premature.is_empty(),
        "xs must survive the break: {premature:?}"
    );
    let call = |name| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, expression)| {
                (sources.slice(expression.span()).unwrap() == name).then_some(id)
            })
            .unwrap()
    };
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let symbol = |name| {
        names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id()
    };
    let source_drops = |call| {
        owned
            .drops()
            .iter()
            .filter(|fact| {
                fact.point() == DropPoint::CallReturn(call)
                    && matches!(
                        fact.target(),
                        DropTarget::RetainedSource(ClosureCaptureSource::Symbol(_))
                    )
            })
            .collect::<Vec<_>>()
    };
    fn contains_selector(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        target: lang_frontend::ownership_checking::CleanupSelectorId,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always | CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                *selector == target
                    || branches
                        .iter()
                        .any(|branch| contains_selector(table, *branch, target))
            }
        }
    }
    let mut executed_source_drops = 0;
    for (index, binding_name) in call_order.into_iter().enumerate() {
        let name = if binding_name == "f" { "f()" } else { "g()" };
        let call = call(name);
        let drops = source_drops(call);
        assert_eq!(drops.len(), 1, "{name}: one retained source obligation");
        let actions = owned
            .cleanup_steps()
            .iter()
            .filter(|(point, _)| *point == DropPoint::CallReturn(call))
            .map(|(_, action)| action)
            .collect::<Vec<_>>();
        let drop = drops[0];
        let owner = drop.owner().unwrap();
        let phi = owned.iterations()[0]
            .closure_phis()
            .iter()
            .find(|phi| {
                phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == symbol(binding_name)
            })
            .unwrap();
        let (origin, source) = phi
            .origins()
            .iter()
            .flat_map(|origin| origin.sources().iter().map(move |source| (origin, source)))
            .find(|(_, source)| source.owner() == owner)
            .expect("call must release this binding's exit source slot");
        assert_eq!(
            choices[&origin.selector()],
            1,
            "{name}: selected exit origin"
        );
        let (test_index, selector) = actions
            .iter()
            .enumerate()
            .find_map(|(index, action)| match action {
                IterationCleanupAction::TestLastCaptureLoan {
                    owner: actual,
                    selector,
                    ..
                } if *actual == owner => Some((index, *selector)),
                _ => None,
            })
            .expect("retained source must query the last active loan");
        let ended = actions
            .iter()
            .enumerate()
            .filter(|(_, action)| {
                matches!(action, IterationCleanupAction::EndCaptureLoan {
                    owner: environment_owner,
                    closure,
                    source: capture_source,
                    value: CleanupCaptureValue::Owner(actual),
                    ..
                } if *environment_owner == phi.owner()
                    && *closure == origin.closure()
                    && *capture_source == source.source()
                    && *actual == owner)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(
            ended.len(),
            1,
            "{name}: end exactly this environment's loan"
        );
        assert_eq!(
            actions.iter().filter(|action| matches!(action,
                IterationCleanupAction::EndCaptureLoan { value: CleanupCaptureValue::Owner(actual), .. }
                    if *actual == owner)).count(),
            1,
            "{name}: no other environment may consume this source slot"
        );
        assert!(ended.iter().all(|index| *index < test_index));
        for index in &ended {
            let IterationCleanupAction::EndCaptureLoan { condition, .. } = actions[*index] else {
                unreachable!();
            };
            assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
        }
        let IterationCleanupAction::TestLastCaptureLoan { condition, .. } = actions[test_index]
        else {
            unreachable!();
        };
        assert!(condition.is_none_or(|guard| selected(table, guard, &choices)));
        let drop_index = actions
            .iter()
            .position(
                |action| matches!(action, IterationCleanupAction::Drop(actual) if actual == drop),
            )
            .unwrap();
        assert!(
            test_index < drop_index,
            "query must precede the physical drop"
        );
        let selector_info = owned.cleanup_conditions().selector(selector).unwrap();
        assert_eq!(selector_info.selection(), CleanupSelection::LastCaptureLoan);
        assert_eq!(
            selector_info.source(),
            CleanupSelectorSource::CaptureLoan { owner }
        );
        let actual = sources_by_slot[&owner];
        let remaining = live_loans.get_mut(&actual).unwrap();
        *remaining -= ended.len();
        assert_eq!(*remaining == 0, index == 1, "{name}: last-loan choice");
        choices.insert(selector, usize::from(*remaining == 0));
        assert!(
            contains_selector(
                owned.cleanup_conditions(),
                drop.condition().unwrap(),
                selector
            ),
            "{name}: release is conditional on the actual source instance's last loan"
        );
        let executes = selected(table, drop.condition().unwrap(), &choices);
        assert_eq!(executes, index == 1, "{name}: guarded source drop");
        executed_source_drops += usize::from(executes);
    }
    assert_eq!(executed_source_drops, 1, "shared xs drops exactly once");
}

#[test]
fn element_and_component_bindings_cannot_transfer_move_only_owners() {
    for (element, binding, prefix) in [
        ("Node", "n", ""),
        (
            "Pair",
            "(n, _)",
            "value class Pair(val node: Node, val flag: Boolean)",
        ),
    ] {
        let (_, _, owned) = checked(&format!(
            "class Node {{}}\n{prefix}\nfun consume(own n: Node) {{}}\nfun run(xs: List<{element}>) {{ for ({binding} in xs) {{ consume(n)\nbreak }} }}"
        ));
        assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0133");
    }
}

#[test]
fn source_owner_is_not_dropped_until_iteration_exits() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget};
    use lang_frontend::parser::Statement;
    for (text, temporary) in [
        ("fun run(own xs: List<Int>) { for (_ in xs) {} }", false),
        ("fun run() { for (_ in listOf(1)) {} }", true),
    ] {
        let (_, parsed, owned) = checked(text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let (statement, source) = parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, n)| match n.payload() {
                Statement::For { source, .. } => Some((id, *source)),
                _ => None,
            })
            .unwrap();
        let drops = owned
            .drops()
            .iter()
            .filter(|drop| {
                if temporary {
                    drop.target() == DropTarget::Temporary(source)
                } else {
                    matches!(drop.target(), DropTarget::Named(_))
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].point(), DropPoint::LoopExit(statement));
        if temporary {
            let owner = drops[0]
                .owner()
                .expect("direct source has an owner identity");
            assert!(matches!(
                owned.cleanup_conditions().owner_value(owner),
                Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == source
            ));
        }
    }
}

#[test]
fn returns_and_nested_loops_keep_the_outer_provider_loan_active() {
    for body in [
        "return xs",
        "for (_ in xs) { break }\nreturn xs",
        "read(xs)\nreturn xs",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>): List<Int> {{ for (_ in xs) {{ {body} }} return xs }}"
        );
        let (sources, _, owned) = checked(&text);
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{body}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
        let operand_start = text.find("return xs").unwrap() + "return ".len();
        assert_eq!(
            owned.diagnostics()[0].primary_span().start(),
            operand_start,
            "the diagnostic must point to the return operand inside the loop"
        );
        assert_eq!(
            sources.slice(owned.diagnostics()[0].primary_span()),
            Ok("xs")
        );
    }
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>): List<Int> { for (n in xs) { read(xs)\nfor (m in xs) { val x: Int = m } } return xs }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn borrowed_iteration_binding_preserves_copy_inout_and_capture_rules() {
    for (text, code) in [
        (
            "fun change(inout n: Int) {}\nfun run(xs: List<Int>) { for (n in xs) { change(&n) } }",
            "L0134",
        ),
        (
            "class Node(var field: Int) {}\nfun run(xs: List<Node>) { for (n in xs) { n.field = 2 } }",
            "L0135",
        ),
        (
            "class Node {}\nfun consume(n: Node) {}\nfun run(xs: List<Node>) { for (n in xs) { val capture = move { consume(n) } } }",
            "L0138",
        ),
        (
            "class Node {}\nfun run(xs: List<Node>): Node { for (n in xs) { return n } return Node() }",
            "L0133",
        ),
    ] {
        let (_, _, owned) = checked(text);
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{text}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), code);
    }
    let (_, _, owned) =
        checked("fun run(xs: List<Int>): Int { for (n in xs) { return n } return 0 }");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn copyable_fields_are_reads_through_the_borrowed_element() {
    for declaration in [
        "value class Point(val x: Int)",
        "class Point(val x: Int) {}",
    ] {
        let (_, _, owned) = checked(&format!(
            "{declaration}\nfun run(xs: List<Point>) {{ for (p in xs) {{ val n: Int = p.x }} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    }
}

#[test]
fn temporary_sources_follow_control_transfer_boundaries() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    use lang_frontend::parser::{Expression, Statement};
    for (jump, releases) in [("break", true), ("continue", false), ("return", true)] {
        let (_, parsed, owned) = checked(&format!(
            "fun run() {{ for (_ in listOf(1)) {{ {jump} }} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let source = parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(_, node)| {
                if let Statement::For { source, .. } = node.payload() {
                    Some(*source)
                } else {
                    None
                }
            })
            .unwrap();
        let transfer = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                matches!(
                    node.payload(),
                    Expression::Break { .. }
                        | Expression::Continue { .. }
                        | Expression::Return { .. }
                )
                .then_some(id)
            })
            .unwrap();
        assert_eq!(
            owned
                .drops()
                .iter()
                .filter(|fact| {
                    fact.target() == DropTarget::Temporary(source)
                        && fact.point() == DropPoint::ControlTransfer(transfer)
                })
                .count(),
            usize::from(releases),
            "{jump}"
        );
    }
}

#[test]
fn conditional_temporary_source_survives_until_each_provider_exit() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget, LoanTarget};
    use lang_frontend::parser::{Expression, Statement};
    for text in [
        "fun run(flag: Boolean) { for (_ in if (flag) listOf(1) else listOf(2)) { break } }",
        "fun run(flag: Boolean) { for (_ in when (flag) { true -> listOf(1)\nelse -> listOf(2) }) { break } }",
    ] {
        let (sources, parsed, owned) = checked(text);
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let (statement, source) = parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| match node.payload() {
                Statement::For { source, .. } => Some((id, *source)),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            owned.iteration(statement).unwrap().source(),
            &LoanTarget::Temporary(source)
        );
        let transfer = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| matches!(node.payload(), Expression::Break { .. }).then_some(id))
            .unwrap();
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Temporary(source))
            .collect::<Vec<_>>();
        assert_eq!(
            drops.len(),
            2,
            "zero-round exhaustion and break each release the source"
        );
        let owner = drops[0]
            .owner()
            .expect("the selected source has an owner identity");
        assert_eq!(drops[1].owner(), Some(owner));
        assert!(matches!(
            owned.cleanup_conditions().owner_value(owner),
            Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == source
        ));
        assert!(
            drops
                .iter()
                .any(|fact| fact.point() == DropPoint::LoopExit(statement))
        );
        assert!(
            drops
                .iter()
                .any(|fact| fact.point() == DropPoint::ControlTransfer(transfer))
        );
        assert!(
            owned.drops().iter().all(|fact| match fact.target() {
                DropTarget::Temporary(value) =>
                    sources
                        .slice(parsed.ast().expressions().get(value).unwrap().span())
                        .unwrap()
                        != "listOf(1)"
                        && sources
                            .slice(parsed.ast().expressions().get(value).unwrap().span())
                            .unwrap()
                            != "listOf(2)",
                _ => true,
            }),
            "selected branch temporary transfers into the provider source"
        );
    }
}

#[test]
fn conditional_temporary_source_replays_each_executed_exit_once() {
    use lang_frontend::ownership_checking::{
        DropTarget, IterationCleanupAction as Action, IterationExitKind, IterationExitPlan,
        LoanTarget,
    };
    use lang_frontend::parser::Statement;

    fn replay(
        exit: &IterationExitPlan,
        source: lang_frontend::ast::ExpressionId,
        element_live: &mut bool,
        provider_live: &mut bool,
        source_loan_live: &mut bool,
        source_owned: &mut bool,
        drops: &mut usize,
    ) {
        for action in exit.actions() {
            match action {
                Action::EndElement(_) => {
                    assert!(*element_live, "element ends only after NextPlace");
                    *element_live = false;
                }
                Action::FinishProvider(_) => {
                    assert!(!*element_live && *provider_live);
                    *provider_live = false;
                }
                Action::EndSource(_) => {
                    assert!(!*provider_live && *source_loan_live);
                    *source_loan_live = false;
                }
                Action::Drop(fact) if fact.target() == DropTarget::Temporary(source) => {
                    assert!(!*source_loan_live && *source_owned);
                    assert!(fact.condition().is_none());
                    *source_owned = false;
                    *drops += 1;
                }
                _ => {}
            }
        }
    }

    for source_text in [
        "if (flag) listOf<Int>() else listOf(1, 2)",
        "when (flag) { true -> listOf<Int>()\nelse -> listOf(1, 2) }",
    ] {
        for (body, rounds, end) in [
            ("", 0, "exhaustion"),
            ("", 2, "exhaustion"),
            ("continue", 2, "exhaustion"),
            ("break", 1, "break"),
            ("return", 1, "return"),
        ] {
            let (_, parsed, owned) = checked(&format!(
                "fun run(flag: Boolean) {{ for (_ in {source_text}) {{ {body} }} }}"
            ));
            assert!(
                owned.diagnostics().is_empty(),
                "{source_text}, {body}: {:?}",
                owned.diagnostics()
            );
            let (statement, source) = parsed
                .ast()
                .statements()
                .iter()
                .find_map(|(id, node)| match node.payload() {
                    Statement::For { source, .. } => Some((id, *source)),
                    _ => None,
                })
                .unwrap();
            let plan = owned.iteration(statement).unwrap();
            assert_eq!(plan.source(), &LoanTarget::Temporary(source));
            let mut element_live = false;
            let mut provider_live = true;
            let mut source_loan_live = true;
            let mut source_owned = true;
            let mut drops = 0;
            for _ in 0..rounds {
                element_live = true;
                let kind = match body {
                    "continue" => "continue",
                    "break" => "break",
                    "return" => "return",
                    _ => "fallthrough",
                };
                let exit = plan
                    .exits()
                    .iter()
                    .find(|exit| {
                        matches!(
                            (kind, exit.kind()),
                            ("fallthrough", IterationExitKind::Fallthrough)
                                | ("continue", IterationExitKind::Continue(_))
                                | ("break", IterationExitKind::Break(_))
                                | ("return", IterationExitKind::Return(_))
                        )
                    })
                    .unwrap();
                replay(
                    exit,
                    source,
                    &mut element_live,
                    &mut provider_live,
                    &mut source_loan_live,
                    &mut source_owned,
                    &mut drops,
                );
                if matches!(
                    exit.kind(),
                    IterationExitKind::Fallthrough | IterationExitKind::Continue(_)
                ) {
                    assert!(provider_live && source_loan_live && source_owned);
                    assert_eq!(drops, 0);
                } else {
                    break;
                }
            }
            if end == "exhaustion" {
                let exit = plan
                    .exits()
                    .iter()
                    .find(|exit| exit.kind() == IterationExitKind::Exhaustion)
                    .unwrap();
                replay(
                    exit,
                    source,
                    &mut element_live,
                    &mut provider_live,
                    &mut source_loan_live,
                    &mut source_owned,
                    &mut drops,
                );
            }
            assert!(!element_live && !provider_live && !source_loan_live && !source_owned);
            assert_eq!(drops, 1, "{source_text}, {body}, {end}");
        }
    }
}

#[test]
fn nested_jumps_release_only_the_sources_they_leave() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    use lang_frontend::parser::Expression;
    for (jump, expected) in [
        ("break", vec!["listOf(2)"]),
        ("continue", vec![]),
        ("return", vec!["listOf(2)", "listOf(1)"]),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun run() {{ for (_ in listOf(1)) {{ for (_ in listOf(2)) {{ {jump} }} }} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let transfer = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                matches!(
                    node.payload(),
                    Expression::Break { .. }
                        | Expression::Continue { .. }
                        | Expression::Return { .. }
                )
                .then_some(id)
            })
            .unwrap();
        let actual = owned
            .drops()
            .iter()
            .filter_map(|fact| {
                if fact.point() == DropPoint::ControlTransfer(transfer)
                    && let DropTarget::Temporary(source) = fact.target()
                {
                    Some(
                        sources
                            .slice(parsed.ast().expressions().get(source).unwrap().span())
                            .unwrap(),
                    )
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{jump}");
    }
}

#[test]
fn inner_while_break_keeps_the_outer_temporary_source() {
    use lang_frontend::ownership_checking::DropPoint;
    use lang_frontend::parser::Expression;
    let (_, parsed, owned) =
        checked("fun run(flag: Boolean) { for (_ in listOf(1)) { while (flag) { break } } }");
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let jump = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| matches!(node.payload(), Expression::Break { .. }).then_some(id))
        .unwrap();
    assert!(
        !owned
            .drops()
            .iter()
            .any(|fact| fact.point() == DropPoint::ControlTransfer(jump))
    );
}

#[test]
fn source_evaluation_exit_never_creates_a_provider_owner() {
    for condition in ["return", "error(\"stop\")"] {
        let (_, _, owned) = checked(&format!(
            "fun run() {{ for (_ in if ({condition}) listOf(1) else listOf(2)) {{}} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.drops().is_empty(), "{condition}: {:?}", owned.drops());
    }
}

#[test]
fn when_source_return_does_not_clean_up_an_uncreated_provider() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    use lang_frontend::parser::{Expression, Statement};
    let (_, parsed, owned) = checked(
        "fun run(flag: Boolean) { for (_ in when (flag) { true -> return\nelse -> listOf(2) }) {} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let (statement, source) = parsed
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| match node.payload() {
            Statement::For { source, .. } => Some((id, *source)),
            _ => None,
        })
        .unwrap();
    let transfer = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| matches!(node.payload(), Expression::Return { .. }).then_some(id))
        .unwrap();
    assert!(
        owned.iteration(statement).is_some(),
        "else builds a provider"
    );
    assert!(owned.drops().iter().any(|fact| {
        fact.target() == DropTarget::Temporary(source)
            && fact.point() == DropPoint::LoopExit(statement)
    }));
    assert!(
        owned
            .drops()
            .iter()
            .all(|fact| fact.point() != DropPoint::ControlTransfer(transfer)),
        "the return edge owns neither the provider nor its selected source"
    );
}

#[test]
fn skipped_outer_branch_does_not_read_uninitialized_loop_phi_selectors() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupSelection, DropPoint,
        IterationPhiIncomingKind,
    };
    use lang_frontend::parser::Expression;

    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &std::collections::BTreeMap<usize, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                let branch = choices
                    .get(&selector.index())
                    .expect("a skipped loop cannot read its uninitialized phi selector");
                selected(table, branches[*branch], choices)
            }
        }
    }

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean, flags: List<Boolean>) {
            var f: () -> Unit = { read(xs) }
            if (flag) { for (_ in flags) { f = ({ read(ys) }) } }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let outer = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (matches!(node.payload(), Expression::If { .. })
                && sources.slice(node.span()).unwrap().starts_with("if (flag)"))
            .then_some(id)
        })
        .unwrap();
    let copied = owned
        .cleanup_steps()
        .iter()
        .flat_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::SaveOwnerSnapshot {
                owner,
                ..
            } => owned
                .cleanup_conditions()
                .owner_snapshot(*owner)
                .unwrap()
                .copies()
                .iter()
                .map(|copy| copy.target().index())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect::<std::collections::BTreeSet<_>>();
    let selectors = owned
        .cleanup_conditions()
        .selectors()
        .iter()
        .enumerate()
        .filter_map(|(index, selector)| {
            (selector.control() == Some(outer)
                && selector.selection() == CleanupSelection::Branch
                && !copied.contains(&index))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(selectors.len(), 1);
    let choices = std::collections::BTreeMap::from([(selectors[0], 1)]);
    let plan = &owned.iterations()[0];
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    assert!(
        !selected(owned.cleanup_conditions(), exhaustion.condition(), &choices),
        "an outer false branch never enters the loop's exhaustion edge"
    );
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let call_drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.point() == DropPoint::CallReturn(call))
        .collect::<Vec<_>>();
    assert!(
        !call_drops.is_empty(),
        "the final call has cleanup to check"
    );
    for fact in call_drops {
        if let Some(condition) = fact.condition() {
            selected(owned.cleanup_conditions(), condition, &choices);
        }
    }
    let capture_actions = owned
        .cleanup_steps()
        .iter()
        .filter(|(point, _)| *point == DropPoint::CallReturn(call))
        .filter_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::EndCaptureLoan {
                condition,
                ..
            }
            | lang_frontend::ownership_checking::IterationCleanupAction::TestLastCaptureLoan {
                condition,
                ..
            } => Some(*condition),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !capture_actions.is_empty(),
        "the call releases a capture loan"
    );
    for condition in capture_actions.into_iter().flatten() {
        selected(owned.cleanup_conditions(), condition, &choices);
    }
}

#[test]
fn iteration_plans_publish_ordered_and_distinct_exit_paths() {
    use lang_frontend::ownership_checking::{
        IterationCleanupAction as Action, IterationExitKind, LoanTarget,
    };
    let (_, _, owned) = checked(
        "fun run(flag: Boolean) { for (n in listOf(1)) { if (flag) { continue } else { break } } }",
    );
    let plan = &owned.iterations()[0];
    assert!(matches!(plan.source(), LoanTarget::Temporary(_)));
    assert_eq!(plan.bindings().len(), 1);
    assert_eq!(owned.iteration(plan.descriptor().statement()), Some(plan));
    for exit in plan.exits() {
        let actions = exit.actions();
        match exit.kind() {
            IterationExitKind::Continue(_) => assert!(matches!(
                actions,
                [Action::EndBinding { .. }, Action::EndElement(_)]
            )),
            IterationExitKind::Break(_) => assert!(matches!(
                actions,
                [
                    Action::EndBinding { .. },
                    Action::EndElement(_),
                    Action::FinishProvider(_),
                    Action::EndSource(_),
                    Action::Drop(_)
                ]
            )),
            IterationExitKind::Exhaustion => assert!(matches!(
                actions,
                [
                    Action::FinishProvider(_),
                    Action::EndSource(_),
                    Action::Drop(_)
                ]
            )),
            kind => panic!("unexpected exit {kind:?}"),
        }
    }
    assert_eq!(plan.exits().len(), 3);
}

#[test]
fn iteration_ownership_facts_are_deterministic_across_analyses() {
    let text = "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>, gate: Boolean) {
        var f: () -> Unit = {}
        for (_ in flags) {
            val xs = listOf(1)
            if (gate) {
                f = ({ read(xs) })
                continue
            }
            f = ({ read(xs) })
            break
        }
        val used = f()
    }";
    let mut sources = SourceMap::new();
    let source = sources.add_source("iteration.ko", text).unwrap();
    let parsed = parser_test_assertions::parse_file_twice(&sources, source, "iteration ownership");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (names, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names).unwrap();
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let first = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    let second = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
    assert_eq!(first.iterations().len(), 1);
    assert!(!first.iterations()[0].closure_phi_incomings().is_empty());
    assert_eq!(first.iterations(), second.iterations());
    assert_eq!(first.cleanup_conditions(), second.cleanup_conditions());
    assert_eq!(first.cleanup_steps(), second.cleanup_steps());
    assert_eq!(first.drops(), second.drops());
    assert_eq!(first.loan_ends(), second.loan_ends());
    assert_eq!(first.loans(), second.loans());
    assert_eq!(first.captures(), second.captures());
}

#[test]
fn iteration_plan_publication_is_atomic_on_error_and_skips_unreachable_source() {
    let (_, _, bad) = checked(
        "fun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>) { for (_ in listOf(1)) {} for (_ in xs) { consume(xs)\nbreak } }",
    );
    assert!(!bad.diagnostics().is_empty());
    assert!(bad.iterations().is_empty());
    let (_, _, unreachable) =
        checked("fun run() { for (_ in if (return) listOf(1) else listOf(2)) {} }");
    assert!(unreachable.iterations().is_empty());
}

#[test]
fn nested_return_plan_orders_each_scope_before_its_provider() {
    use lang_frontend::ownership_checking::{IterationCleanupAction as Action, IterationExitKind};
    let (sources, _, owned) = checked(
        "class Node {}\nfun touch(n: Node) {}\nfun run(own outer: Node, flag: Boolean) { for (_ in listOf(1)) { val local = Node()\nfor (_ in listOf(2)) { if (flag) { return }\ntouch(local) } } touch(outer) }",
    );
    assert_eq!(owned.iterations().len(), 2);
    let returns = owned
        .iterations()
        .iter()
        .map(|plan| {
            plan.exits()
                .iter()
                .find(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(returns[0].actions(), returns[1].actions());
    let drops = returns[0]
        .actions()
        .iter()
        .filter_map(|action| match action {
            Action::Drop(fact) => Some(sources.slice(fact.value_origin()).unwrap()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(drops, ["listOf(2)", "local", "listOf(1)", "outer"]);
}

#[test]
fn named_source_break_has_its_own_dead_owner_cleanup() {
    use lang_frontend::ownership_checking::{
        DropTarget, IterationCleanupAction as Action, IterationExitKind,
    };
    let (_, _, owned) = checked("fun run(own xs: List<Int>) { for (_ in xs) { break } }");
    let exit = owned.iterations()[0]
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), IterationExitKind::Break(_)))
        .unwrap();
    assert!(
        matches!(exit.actions().last(), Some(Action::Drop(fact)) if matches!(fact.target(), DropTarget::Named(_)))
    );
}

#[test]
fn closure_derived_loan_ends_before_iteration_binding() {
    use lang_frontend::ownership_checking::{IterationCleanupAction as Action, IterationExitKind};
    let (_, _, owned) = checked(
        "class Node {}\nfun touch(n: Node) {}\nfun run(xs: List<Node>, flag: Boolean) { for (n in xs) { val f = { touch(n) }\nif (flag) { return }\nf() } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plan = &owned.iterations()[0];
    let actions = plan
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
        .unwrap()
        .actions();
    let capture = actions
        .iter()
        .position(|action| matches!(action, Action::EndCaptureLoan { .. }))
        .unwrap();
    let binding = actions
        .iter()
        .position(|action| matches!(action, Action::EndBinding { .. }))
        .unwrap();
    assert!(capture < binding);
}

#[test]
fn aborting_while_condition_has_no_iteration_backedge_cleanup() {
    use lang_frontend::ownership_checking::IterationExitKind;
    let (_, _, owned) =
        checked("fun run() { for (_ in listOf(1)) { while (error(\"stop\")) {} } }");
    let exits = owned.iterations()[0].exits();
    assert_eq!(exits.len(), 1, "{exits:?}");
    assert_eq!(exits[0].kind(), IterationExitKind::Exhaustion);
}

#[test]
fn lambda_iteration_plans_use_their_own_callable_boundary() {
    use lang_frontend::ownership_checking::{IterationCleanupAction as Action, IterationExitKind};
    let (_, _, owned) = checked(
        "fun run() { for (_ in listOf(1)) { val f = { for (_ in listOf(2)) { return } }\nf() } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 2);
    let returning = owned
        .iterations()
        .iter()
        .find(|plan| {
            plan.exits()
                .iter()
                .any(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
        })
        .unwrap();
    let exit = returning
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
        .unwrap();
    assert_eq!(
        exit.actions()
            .iter()
            .filter(|action| matches!(action, Action::FinishProvider(_)))
            .count(),
        1
    );
    let other = owned
        .iterations()
        .iter()
        .find(|plan| plan.descriptor().statement() != returning.descriptor().statement())
        .unwrap();
    assert!(
        other
            .exits()
            .iter()
            .all(|exit| !matches!(exit.kind(), IterationExitKind::Return(_)))
    );
}

#[test]
fn lambda_owned_source_survives_provider_and_returned_owner_is_not_dropped() {
    use lang_frontend::ownership_checking::{
        DropTarget, IterationCleanupAction as Action, IterationExitKind,
    };
    let (_, _, owned) = checked(
        "fun run() { val f: (own List<Int>) -> List<Int> = { xs -> for (_ in xs) {}\nxs } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 1);
    let plan = &owned.iterations()[0];
    let exit = plan
        .exits()
        .iter()
        .find(|exit| exit.kind() == IterationExitKind::Exhaustion)
        .unwrap();
    assert!(!exit.actions().iter().any(|action| matches!(action, Action::Drop(fact) if matches!(fact.target(), DropTarget::Named(_)))));
}

#[test]
fn source_category_plans_preserve_place_and_owner_capabilities() {
    use lang_frontend::ownership_checking::{
        DropTarget, IterationCleanupAction as Action, LoanTarget,
    };
    for (parameter, source, fields, owns) in [
        ("own xs: List<Int>", "xs", 0, true),
        ("xs: List<Int>", "xs", 0, false),
        ("inout xs: List<Int>", "xs", 0, false),
        ("own h: Holder", "h.xs", 1, true),
        ("h: Holder", "h.xs", 1, false),
    ] {
        let (_, _, owned) = checked(&format!(
            "class Holder(val xs: List<Int>) {{}}\nfun run({parameter}) {{ for (_ in {source}) {{}} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let plan = &owned.iterations()[0];
        let LoanTarget::Place(place) = plan.source() else {
            panic!("expected source place")
        };
        assert_eq!(place.fields().len(), fields);
        let drops = plan.exits().iter().flat_map(|exit| exit.actions()).filter(|action| matches!(action, Action::Drop(fact) if fact.target() == DropTarget::Named(place.root()))).count();
        assert_eq!(drops, usize::from(owns), "{parameter}");
    }
}

#[test]
fn provider_loan_ends_before_an_inout_source_is_replaced_after_the_loop() {
    use lang_frontend::ownership_checking::{
        IterationCleanupAction as Action, IterationExitKind, LoanTarget,
    };

    for (parameter, source, replacement) in [
        ("inout xs: List<Int>", "xs", "xs = listOf(2)"),
        ("inout h: Holder", "h.xs", "h.xs = listOf(2)"),
    ] {
        let (_, _, owned) = checked(&format!(
            "class Holder(var xs: List<Int>) {{}}\nfun run({parameter}) {{ for (_ in {source}) {{ break }}\n{replacement} }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{source}: {:?}",
            owned.diagnostics()
        );
        let plan = &owned.iterations()[0];
        assert!(matches!(plan.source(), LoanTarget::Place(_)));
        assert_eq!(
            plan.exits()
                .iter()
                .filter(|exit| matches!(exit.kind(), IterationExitKind::Exhaustion))
                .count(),
            1
        );
        assert_eq!(
            plan.exits()
                .iter()
                .filter(|exit| matches!(exit.kind(), IterationExitKind::Break(_)))
                .count(),
            1
        );
        let statement = plan.descriptor().statement();
        for exit in plan.exits().iter().filter(|exit| {
            matches!(
                exit.kind(),
                IterationExitKind::Exhaustion | IterationExitKind::Break(_)
            )
        }) {
            let actions = exit.actions();
            let finish = actions
                .iter()
                .position(|action| matches!(action, Action::FinishProvider(id) if *id == statement))
                .unwrap();
            let end = actions
                .iter()
                .position(|action| matches!(action, Action::EndSource(id) if *id == statement))
                .unwrap();
            assert_eq!(end, finish + 1, "{source}: {:?}", exit.kind());
            assert_eq!(
                actions
                    .iter()
                    .filter(|action| matches!(action, Action::FinishProvider(_)))
                    .count(),
                1
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|action| matches!(action, Action::EndSource(_)))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn explicit_this_field_source_has_the_same_provider_loan_as_a_bare_field() {
    use lang_frontend::ownership_checking::LoanTarget;

    for (source, target) in [
        ("xs", "xs"),
        ("this.xs", "xs"),
        ("(this).xs", "xs"),
        ("xs", "this.xs"),
        ("this.xs", "this.xs"),
    ] {
        let (_, _, owned) = checked(&format!(
            "class Holder(var xs: List<Int>) {{ inout fun scan(): Unit {{ for (_ in {source}) {{ {target} = listOf(2) }} }} }}"
        ));
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{source} / {target}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
    }

    let (_, _, owned) = checked(
        "class Holder(var xs: List<Int>) { inout fun scan(): Unit { for (_ in this.xs) {} } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(matches!(
        owned.iterations()[0].source(),
        LoanTarget::Place(_)
    ));
}

#[test]
fn explicit_this_field_does_not_upgrade_a_borrowed_receiver_for_inout_calls() {
    for mode in ["borrow", "own"] {
        let (_, _, owned) = checked(&format!(
            "class Cell(var n: Int) {{ inout fun set(): Unit {{ n = 1 }} }}\nclass Holder(val cell: Cell) {{ {mode} fun bad(): Unit {{ this.cell.set() }} }}"
        ));
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{mode}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0134");
    }

    let (_, _, owned) = checked(
        "class Cell(var n: Int) { inout fun set(): Unit { n = 1 } }\nclass Holder(val cell: Cell) { inout fun good(): Unit { this.cell.set() } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn explicit_this_field_respects_receiver_capability_for_writes_and_captures() {
    for (mode, target) in [("borrow", "xs"), ("borrow", "this.xs"), ("own", "this.xs")] {
        let (_, _, owned) = checked(&format!(
            "class Holder(var xs: List<Int>) {{ {mode} fun bad(): Unit {{ {target} = listOf(2) }} }}"
        ));
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{mode} / {target}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
    }

    let (_, _, owned) = checked(
        "class Holder(var xs: List<Int>) { inout fun good(): Unit { this.xs = listOf(2) } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());

    let (_, _, owned) = checked(
        "class Cell(var n: Int) { inout fun set(): Unit { n = 1 } }\nclass Holder(val cell: Cell) { inout fun bad(): Unit { val f: () -> Unit = { this.cell.set() } } }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0134");

    let (_, _, owned) = checked(
        "class Cell(var n: Int) { inout fun set(): Unit { n = 1 }\ninout fun bad(): Unit { val f: () -> Unit = { set() } } }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0134");
}

#[test]
fn source_reuse_and_moved_entry_follow_the_original_owner() {
    use lang_frontend::ownership_checking::{
        DropTarget, IterationCleanupAction as Action, LoanTarget,
    };
    let (_, _, good) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) { for (_ in xs) { break } read(xs) }",
    );
    let plan = &good.iterations()[0];
    let LoanTarget::Place(place) = plan.source() else {
        panic!("expected place")
    };
    assert!(!plan.exits().iter().flat_map(|exit| exit.actions()).any(|action| matches!(action, Action::Drop(fact) if fact.target() == DropTarget::Named(place.root()))));
    let (_, _, bad) = checked(
        "fun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>) { consume(xs)\nfor (_ in xs) {} }",
    );
    assert_eq!(bad.diagnostics().len(), 1);
    assert_eq!(bad.diagnostics()[0].code().to_string(), "L0131");
    assert!(bad.iterations().is_empty());
}

#[test]
fn component_bindings_end_in_reverse_order_without_owning_element_drops() {
    use lang_frontend::ownership_checking::{
        IterationCleanupAction as Action, IterationExitKind, OwnershipBindingKind,
    };
    let (_, _, owned) = checked(
        "class Node {}\nvalue class Parts(val first: Node, val flag: Int, val second: Node)\nfun run(xs: List<Parts>) { for ((a, _, b) in xs) { continue } }",
    );
    let plan = &owned.iterations()[0];
    assert_eq!(plan.bindings().len(), 2);
    assert!(
        plan.bindings()
            .iter()
            .all(|binding| binding.kind() == OwnershipBindingKind::Shared)
    );
    let exit = plan
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), IterationExitKind::Continue(_)))
        .unwrap();
    let ended = exit
        .actions()
        .iter()
        .filter_map(|action| match action {
            Action::EndBinding { symbol, .. } => Some(*symbol),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ended,
        plan.bindings()
            .iter()
            .rev()
            .map(|binding| binding.symbol())
            .collect::<Vec<_>>()
    );
    assert!(
        !exit
            .actions()
            .iter()
            .any(|action| matches!(action, Action::Drop(_)))
    );
}

#[test]
fn lambda_unused_owned_parameters_drop_at_callable_entry() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget, IterationCleanupAction};
    for header in ["xs ->", ""] {
        let (_, parsed, owned) = checked(&format!(
            "fun run() {{ val f: (own List<Int>) -> Unit = {{ {header} for (_ in listOf(1)) {{}} }} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let entries = owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.point(), DropPoint::LambdaEntry(_)))
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1, "{header}");
        assert!(matches!(entries[0].target(), DropTarget::Named(_)));
        if let DropPoint::LambdaEntry(lambda) = entries[0].point() {
            let actions = owned
                .cleanup_steps()
                .iter()
                .filter(|(point, _)| *point == DropPoint::LambdaEntry(lambda))
                .map(|(_, action)| action)
                .collect::<Vec<_>>();
            assert!(matches!(
                actions.first(),
                Some(IterationCleanupAction::BindClosureEnvironment { closure, .. })
                    if *closure == lambda
            ));
            assert!(actions.iter().skip(1).any(|action| matches!(
                action,
                IterationCleanupAction::Drop(fact) if *fact == *entries[0]
            )));
            assert!(matches!(
                parsed.ast().expressions().get(lambda).unwrap().payload(),
                lang_frontend::parser::Expression::Lambda { .. }
            ));
        }
        assert_eq!(owned.iterations().len(), 1);
    }
}

#[test]
fn borrowed_element_closure_cannot_escape_the_callable() {
    let (_, _, owned) = checked(
        "class Node {}\nfun touch(n: Node) {}\nfun run(xs: List<Node>): () -> Unit { for (n in xs) { return ({ touch(n) }) } return ({ -> }) }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0137");
    assert!(owned.iterations().is_empty());
}

#[test]
fn borrowed_element_closure_cannot_survive_its_iteration() {
    for exit in ["", "break", "continue"] {
        let (_, _, owned) = checked(&format!(
            "fun read(n: Int) {{}}\nfun run(xs: List<Int>) {{ var f: () -> Unit = {{}}\nfor (n in xs) {{ f = ({{ read(n) }})\n{exit} }}\nval used = f() }}"
        ));
        assert_eq!(
            owned.diagnostics().len(),
            1,
            "{exit}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.diagnostics()[0].code().to_string(), "L0137", "{exit}");
        assert!(owned.iterations().is_empty(), "{exit}");
    }
}

#[test]
fn borrowed_element_closure_released_before_iteration_end_is_valid() {
    let (_, _, owned) = checked(
        "fun read(n: Int) {}\nfun run(xs: List<Int>) { var f: () -> Unit = {}\nfor (n in xs) { f = ({ read(n) })\nval used = f() } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 1);
}

#[test]
fn moved_closure_cannot_hide_an_iteration_borrowed_capture() {
    let (_, _, owned) = checked(
        "fun read(n: Int) {}\nfun run(xs: List<Int>) { var f: move () -> Unit = move {}\nfor (n in xs) { var g: () -> Unit = { read(n) }\nf = (move { g() })\nif (true) { g = ({}) }\nbreak }\nval used = f() }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0137");
    assert!(owned.iterations().is_empty());
}

#[test]
fn moved_closure_with_iteration_capture_can_finish_in_the_same_body() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureMode, DropPoint, IterationCleanupAction as Action,
    };
    let (sources, parsed, owned) = checked(
        "fun read(n: Int) {}\nfun run(xs: List<Int>) { var f: move () -> Unit = move {}\nfor (n in xs) { var g: () -> Unit = { read(n) }\nf = (move { g() })\nval used = f()\nbreak } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.iterations().len(), 1);
    let capture = owned
        .captures()
        .iter()
        .find(|capture| capture.mode() == ClosureCaptureMode::Shared)
        .unwrap();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    assert!(
        owned.cleanup_steps().iter().any(|(point, action)| {
            *point == DropPoint::CallReturn(call)
                && matches!(action, Action::EndCaptureLoan { source, .. } if *source == capture.source())
        }),
        "the nested borrowed environment must release the element before break: {:?}",
        owned.cleanup_steps()
    );
}

#[test]
fn nested_iteration_capture_cleanup_keeps_the_selected_branch_guard() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureMode, DropPoint, IterationCleanupAction as Action,
    };
    let (sources, parsed, owned) = checked(
        "fun read(n: Int) {}\nfun run(xs: List<Int>, flag: Boolean) { for (n in xs) {\nvar g: () -> Unit = {}\nif (flag) { g = ({ read(n) }) } else { g = ({}) }\nval f: move () -> Unit = move { g() }\nval used = f()\nbreak } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let source = owned
        .captures()
        .iter()
        .find(|capture| capture.mode() == ClosureCaptureMode::Shared)
        .unwrap()
        .source();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let endings = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| match action {
            Action::EndCaptureLoan {
                source: actual,
                condition,
                ..
            } if *point == DropPoint::CallReturn(call) && *actual == source => Some(*condition),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(endings.len(), 1, "{endings:?}");
    assert!(
        endings[0].is_some(),
        "the unselected closure has no element loan"
    );
}

#[test]
fn nested_capture_keeps_branch_local_source_until_outer_call_returns() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureMode, ClosureCaptureSource, DropPoint, DropTarget,
        IterationCleanupAction as Action,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flag: Boolean) {\nval f: move () -> Unit = if (flag) {\nval xs = listOf(1)\nval g: () -> Unit = { read(xs) }\nmove { g() }\n} else move {}\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let source = owned
        .captures()
        .iter()
        .find(|capture| capture.mode() == ClosureCaptureMode::Shared)
        .and_then(|capture| match capture.source() {
            ClosureCaptureSource::Symbol(symbol) => Some(symbol),
            ClosureCaptureSource::This => None,
        })
        .unwrap();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let actions = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| (*point == DropPoint::CallReturn(call)).then_some(action))
        .collect::<Vec<_>>();
    let loan_end = actions
        .iter()
        .position(|action| matches!(action, Action::EndCaptureLoan { source: actual, .. } if *actual == ClosureCaptureSource::Symbol(source)))
        .unwrap();
    let source_drop = actions
        .iter()
        .position(|action| matches!(action, Action::Drop(fact) if fact.target() == DropTarget::Named(source)))
        .unwrap();
    assert!(loan_end < source_drop, "{actions:?}");
    assert!(
        owned.drops().iter().all(|fact| {
            fact.target() != DropTarget::Named(source)
                || fact.point() == DropPoint::CallReturn(call)
        }),
        "the branch-local source must remain owned until f returns: {:?}",
        owned.drops()
    );
}

#[test]
fn sibling_nested_captures_release_shared_source_after_both_loans() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureMode, ClosureCaptureSource, DropPoint, DropTarget,
        IterationCleanupAction as Action,
    };
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run() { val xs = listOf(1)\nval g: () -> Unit = { read(xs) }\nval h: () -> Unit = { read(xs) }\nval f: move () -> Unit = move { val first = g()\nval second = h() }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let source = owned
        .captures()
        .iter()
        .find(|capture| capture.mode() == ClosureCaptureMode::Shared)
        .unwrap()
        .source();
    let ClosureCaptureSource::Symbol(symbol) = source else {
        unreachable!()
    };
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let actions = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| (*point == DropPoint::CallReturn(call)).then_some(action))
        .collect::<Vec<_>>();
    let endings = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            matches!(action, Action::EndCaptureLoan { source: actual, .. } if *actual == source)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    let drops = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            matches!(action, Action::Drop(fact) if fact.target() == DropTarget::Named(symbol))
                .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(endings.len(), 2, "{actions:?}");
    assert_eq!(drops.len(), 1, "{actions:?}");
    assert!(endings[1] < drops[0], "{actions:?}");
}

#[test]
fn sibling_phi_sources_test_last_loan_before_the_next_alias_ends() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupSelectorId, ClosureCaptureMode, DropPoint, DropTarget,
        IterationCleanupAction as Action, IterationPhiIncomingKind,
    };
    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &std::collections::BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Choice { selector, branches } => {
                selected(table, branches[choices[selector]], choices)
            }
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
        }
    }
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {\nvar g: () -> Unit = {}\nvar h: () -> Unit = {}\nfor (_ in flags) { val xs = listOf(1)\n{ g = ({ read(xs) }) }\n{ h = ({ read(xs) }) }\nbreak }\nval f: move () -> Unit = move { val first = g()\nval second = h() }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let incoming = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| matches!(incoming.kind(), IterationPhiIncomingKind::Break(_)))
        .unwrap();
    let slots = incoming
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .filter_map(|source| match (source.target(), source.value()) {
            (Some(slot), CleanupCaptureValue::Owner(actual)) => Some((slot, actual)),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(slots.len(), 2, "two distinct phi slots");
    assert_eq!(
        slots
            .values()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        1,
        "both slots refer to the same actual xs"
    );
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let actions = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| (*point == DropPoint::CallReturn(call)).then_some(action))
        .collect::<Vec<_>>();
    let endings = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| match action {
            Action::EndCaptureLoan {
                value: CleanupCaptureValue::Owner(owner),
                ..
            } if slots.contains_key(owner) => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();
    let tests = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| match action {
            Action::TestLastCaptureLoan { owner, .. } if slots.contains_key(owner) => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();
    let drops = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            matches!(action, Action::Drop(fact) if matches!(fact.target(), DropTarget::RetainedSource(_)))
                .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(endings.len(), 2, "{actions:?}");
    assert_eq!(tests.len(), 2, "{actions:?}");
    assert_eq!(drops.len(), 2, "two guarded candidates: {actions:?}");
    let table = owned.cleanup_conditions();
    let mut locations = actions
        .iter()
        .filter_map(|action| match action {
            Action::TestLastCaptureLoan {
                instance_address,
                capture_slot,
                ..
            } => {
                let address = table.instance_address(*instance_address)?;
                let slot = table.capture_slot_value(*capture_slot)?;
                Some((
                    address.root(),
                    address.capture_path().to_vec(),
                    slot.position(),
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    locations.sort_by(|left, right| left.1.cmp(&right.1));
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0].0, locations[1].0);
    assert_eq!(locations[0].1, [0]);
    assert_eq!(locations[1].1, [1]);
    assert_eq!(locations[0].2, 0);
    assert_eq!(locations[1].2, 0);
    let retained_locations = actions
        .iter()
        .filter_map(|action| match action {
            Action::Drop(fact) if matches!(fact.target(), DropTarget::RetainedSource(_)) => {
                Some((fact.instance_address()?, fact.capture_slot()?))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(retained_locations.len(), 2);
    for (address, slot) in retained_locations {
        let instance = table.instance_address(address).unwrap();
        let capture = table.capture_slot_value(slot).unwrap();
        assert!(locations.iter().any(|location| {
            location.0 == instance.root()
                && location.1 == instance.capture_path()
                && location.2 == capture.position()
        }));
    }
    assert!(
        endings[0] < tests[0] && tests[0] < endings[1],
        "{actions:?}"
    );
    assert!(endings[1] < tests[1] && tests[1] < drops[0], "{actions:?}");

    // Replay only instances produced by Create/Save and selected by the break phi.
    let source_owner = *slots.values().next().unwrap();
    assert!(!slots.contains_key(&source_owner));
    let source_instance = source_owner.index() + 1;
    let mut values = std::collections::BTreeMap::from([(source_owner, source_instance)]);
    for (_, action) in owned.cleanup_steps() {
        if let Action::CreateClosureOwner { owner, .. } = action {
            values.insert(*owner, owner.index() + 1);
        }
    }
    for (_, action) in owned.cleanup_steps() {
        if let Action::SaveOwnerSnapshot { owner, value, .. } = action {
            let snapshot = table.owner_snapshot(*owner).unwrap();
            assert_eq!(snapshot.value(), *value);
            if let [capture] = snapshot.capture_inputs()
                && let Some(&instance) = values.get(&capture.owner())
            {
                values.insert(*owner, instance);
            }
        }
    }
    for binding in incoming.bindings() {
        let created_slot = binding
            .origins()
            .iter()
            .flat_map(|origin| origin.environments())
            .flat_map(|environment| environment.sources())
            .find_map(|source| source.source_capture_slot())
            .unwrap();
        let created_owner = table
            .capture_slot_value(created_slot)
            .unwrap()
            .environment();
        let incoming_value = binding.values().first().unwrap();
        assert_eq!(values[&incoming_value.source()], values[&created_owner]);
        values.insert(binding.target(), values[&incoming_value.source()]);
    }
    let f_owner = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveClosureCapture { owner, input, .. }
                if input.mode() == ClosureCaptureMode::Owned =>
            {
                Some(*owner)
            }
            _ => None,
        })
        .unwrap();
    let entry = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let mut choices = std::collections::BTreeMap::new();
    let apply_incoming =
        |edge: &lang_frontend::ownership_checking::IterationPhiIncoming,
         choices: &mut std::collections::BTreeMap<CleanupSelectorId, usize>| {
            assert!(selected(table, edge.condition(), choices));
            let before = choices.clone();
            let mut writes = Vec::new();
            for binding in edge.bindings() {
                writes.push((
                    binding.availability_selector(),
                    usize::from(selected(table, binding.available_when(), &before)),
                ));
                for write in binding.selector_writes() {
                    writes.push((
                        write.target(),
                        usize::from(selected(table, write.condition(), &before)),
                    ));
                }
            }
            choices.extend(writes);
        };
    apply_incoming(entry, &mut choices);
    for after_break in [false, true] {
        if after_break {
            apply_incoming(incoming, &mut choices);
        }
        for (_, action) in owned.cleanup_steps() {
            if let Action::SaveOwnerSnapshot {
                owner, condition, ..
            } = action
            {
                let snapshot = table.owner_snapshot(*owner).unwrap();
                let is_f = snapshot
                    .capture_inputs()
                    .first()
                    .is_some_and(|input| input.owner() == f_owner);
                if is_f != after_break
                    || condition.is_some_and(|guard| !selected(table, guard, &choices))
                {
                    continue;
                }
                let before = choices.clone();
                for copy in snapshot.copies() {
                    if selected(table, copy.when(), &before) {
                        choices.insert(copy.target(), before[&copy.source()]);
                    }
                }
            }
        }
    }
    let mut captures = std::collections::BTreeMap::new();
    let mut formed_loans = 0;
    for (_, action) in owned.cleanup_steps() {
        if let Action::SaveClosureCapture {
            owner,
            target,
            input,
        } = action
            && selected(table, input.condition(), &choices)
            && let CleanupCaptureValue::Owner(source) = input.value()
        {
            let position = table.capture_slot_value(*target).unwrap().position();
            captures.insert((values[owner], position), values[&source]);
            if input.mode() == ClosureCaptureMode::Shared {
                assert_eq!(source, source_owner);
                formed_loans += 1;
            }
        }
    }
    assert_eq!(captures.len(), 4, "two source and two owned capture edges");
    assert_eq!(formed_loans, 2);
    let source_at = |address, slot| {
        let address = table.instance_address(address).unwrap();
        let mut instance = values[&address.root()];
        for &position in address.capture_path() {
            instance = captures[&(instance, position)];
        }
        let position = table.capture_slot_value(slot).unwrap().position();
        captures[&(instance, position)]
    };
    for (point, action) in owned.cleanup_steps() {
        if *point == DropPoint::CallReturn(call) {
            continue;
        }
        if let Action::EndCaptureLoan {
            value: CleanupCaptureValue::Owner(owner),
            instance_address,
            capture_slot,
            condition,
            ..
        } = action
            && (*owner == source_owner || slots.contains_key(owner))
            && condition.is_none_or(|guard| selected(table, guard, &choices))
        {
            let slot = capture_slot.expect("the tracked source loan must have a capture slot");
            assert_ne!(
                source_at(*instance_address, slot),
                source_instance,
                "the selected source loan ended outside f() return: {point:?} {action:?}"
            );
        }
    }
    let mut loans = formed_loans;
    let mut last_choices = Vec::new();
    let mut last_by_owner = std::collections::BTreeMap::new();
    let mut released = Vec::new();
    for action in actions {
        match action {
            Action::EndCaptureLoan {
                value: CleanupCaptureValue::Owner(owner),
                instance_address,
                capture_slot: Some(slot),
                condition,
                ..
            } if slots.contains_key(owner)
                && condition.is_none_or(|guard| selected(table, guard, &choices)) =>
            {
                assert_eq!(source_at(*instance_address, *slot), source_instance);
                loans -= 1;
            }
            Action::TestLastCaptureLoan {
                owner,
                instance_address,
                capture_slot,
                selector,
                condition,
                ..
            } if slots.contains_key(owner)
                && condition.is_none_or(|guard| selected(table, guard, &choices)) =>
            {
                let actual = source_at(*instance_address, *capture_slot);
                assert_eq!(actual, source_instance);
                let last = loans == 0;
                choices.insert(*selector, usize::from(last));
                last_choices.push(last);
                last_by_owner.insert(*owner, (*selector, last));
            }
            Action::Drop(fact) if matches!(fact.target(), DropTarget::RetainedSource(_)) => {
                let owner = fact.owner().unwrap();
                let (selector, last) = last_by_owner[&owner];
                let actual = source_at(
                    fact.instance_address().unwrap(),
                    fact.capture_slot().unwrap(),
                );
                assert_eq!(actual, source_instance);
                let guard = fact.condition().unwrap();
                let mut without_last_loan = choices.clone();
                without_last_loan.insert(selector, 0);
                assert!(!selected(table, guard, &without_last_loan));
                let mut with_last_loan = choices.clone();
                with_last_loan.insert(selector, 1);
                assert!(selected(table, guard, &with_last_loan));
                if selected(table, guard, &choices) {
                    assert!(last);
                    released.push(actual);
                }
            }
            _ => {}
        }
    }
    assert_eq!(loans, 0);
    assert_eq!(last_choices, [false, true]);
    assert_eq!(released, [source_instance]);
}

#[test]
fn inner_loop_phi_preserves_nested_outer_element_capture() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, ClosureCaptureMode, DropPoint, IterationCleanupAction as Action,
        IterationPhiBoundary, IterationPhiIncomingKind,
    };
    let (sources, parsed, owned) = checked(
        "fun read(n: Int) {}\nfun run(xs: List<Int>, flags: List<Int>) { for (n in xs) {\nval g: () -> Unit = { read(n) }\nval f: move () -> Unit = move { g() }\nfor (_ in flags) {}\nval used = f()\nbreak } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let is_outer = |closure| {
        sources
            .slice(parsed.ast().expressions().get(closure).unwrap().span())
            .unwrap()
            == "move { g() }"
    };
    let plan = owned
        .iterations()
        .iter()
        .find(|plan| {
            plan.closure_phis().iter().any(|phi| {
                phi.boundary() == IterationPhiBoundary::Header
                    && phi
                        .origins()
                        .iter()
                        .any(|origin| is_outer(origin.closure()))
            })
        })
        .expect("inner loop must carry f");
    let header = plan
        .closure_phis()
        .iter()
        .find(|phi| {
            phi.boundary() == IterationPhiBoundary::Header
                && phi
                    .origins()
                    .iter()
                    .any(|origin| is_outer(origin.closure()))
        })
        .unwrap();
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit && phi.symbol() == header.symbol())
        .unwrap();
    let header_outer = header
        .origins()
        .iter()
        .find(|origin| is_outer(origin.closure()))
        .unwrap();
    let exit_outer = exit
        .origins()
        .iter()
        .find(|origin| is_outer(origin.closure()))
        .unwrap();
    let header_source = header_outer
        .sources()
        .iter()
        .find(|source| !source.captured().is_empty())
        .expect("f must carry g's environment");
    let header_nested = &header.origins()[header_source.captured()[0]];
    let exit_source = exit_outer
        .sources()
        .iter()
        .find(|source| source.source() == header_source.source())
        .unwrap();
    let exit_nested = &exit.origins()[exit_source.captured()[0]];
    assert_ne!(header_nested.selector(), exit_nested.selector());
    let incoming = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    fn source_input(
        incoming: &lang_frontend::ownership_checking::IterationPhiIncoming,
        owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
        selector: lang_frontend::ownership_checking::CleanupSelectorId,
        source: lang_frontend::ownership_checking::ClosureCaptureSource,
    ) -> &lang_frontend::ownership_checking::IterationPhiIncomingSource {
        incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == owner)
            .unwrap()
            .origins()
            .iter()
            .find(|origin| origin.target() == selector)
            .unwrap()
            .environments()[0]
            .sources()
            .iter()
            .find(|input| input.input().source() == source)
            .unwrap()
    }
    let entry_source = source_input(
        incoming(IterationPhiIncomingKind::Entry),
        header.owner(),
        header_outer.selector(),
        header_source.source(),
    );
    let entry_nested = &entry_source.captured()[0];
    assert_eq!(entry_nested.target(), header_nested.selector());
    assert_eq!(
        entry_source.value(),
        CleanupCaptureValue::Owner(entry_nested.environments()[0].owner())
    );
    let exhaustion_source = source_input(
        incoming(IterationPhiIncomingKind::Exhaustion),
        exit.owner(),
        exit_outer.selector(),
        header_source.source(),
    );
    let exhaustion_nested = &exhaustion_source.captured()[0];
    assert_eq!(exhaustion_nested.target(), exit_nested.selector());
    assert_eq!(
        exhaustion_source.value(),
        CleanupCaptureValue::Owner(header_source.owner())
    );
    assert_eq!(
        exhaustion_nested.environments()[0].owner(),
        header_source.owner()
    );
    let source = owned
        .captures()
        .iter()
        .find(|capture| capture.mode() == ClosureCaptureMode::Shared)
        .unwrap()
        .source();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    assert!(
        owned.cleanup_steps().iter().any(|(point, action)| {
            *point == DropPoint::CallReturn(call)
                && matches!(action, Action::EndCaptureLoan { source: actual, .. } if *actual == source)
        }),
        "the inner phi must preserve g's element loan until f returns: {:?}",
        owned.cleanup_steps()
    );
}

#[test]
fn loop_phi_preserves_leaf_enclosing_closure_capture() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, DropPoint, DropTarget, IterationCleanupAction as Action,
        IterationPhiIncomingKind,
    };

    let (sources, parsed, owned) = checked(
        "fun run() { val base: move () -> Unit = move {}\nval outer: move () -> Unit = move { var f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert_eq!(owned.iterations().len(), 1);
    let incoming = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let source = incoming
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .find(|source| matches!(source.value(), CleanupCaptureValue::Environment { .. }))
        .unwrap();
    let CleanupCaptureValue::Environment { slot, .. } = source.value() else {
        unreachable!()
    };
    let formed_slot = source.source_capture_slot().unwrap();
    assert_ne!(
        slot, formed_slot,
        "phi must read the formed inner environment"
    );
    assert!(
        source
            .captured()
            .iter()
            .any(|nested| !nested.environments().is_empty()),
        "the captured leaf environment must retain its origin"
    );
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    assert!(owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallReturn(call)
            && matches!(action, Action::Drop(fact) if matches!(fact.target(), DropTarget::Captured { value: CleanupCaptureValue::Environment { .. }, .. }))
    }));
}

#[test]
fn loop_phi_keeps_two_leaf_enclosing_capture_sources_distinct() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, IterationCleanupAction as Action, IterationPhiIncomingKind,
    };

    let (_, _, owned) = checked(
        "fun run() { val a: move () -> Unit = move {}\nval b: move () -> Unit = move {}\nval outer: move () -> Unit = move { var f: move () -> Unit = move { val x = a()\nval y = b() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let incoming = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Entry)
        .unwrap();
    let mut child_owners = std::collections::BTreeSet::new();
    for source in incoming
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
    {
        let CleanupCaptureValue::Environment { slot, .. } = source.value() else {
            continue;
        };
        let formed = owned
            .cleanup_steps()
            .iter()
            .find_map(|(_, action)| match action {
                Action::SaveClosureCapture { target, input, .. } if *target == slot => {
                    Some(input.value())
                }
                _ => None,
            })
            .unwrap();
        let CleanupCaptureValue::Owner(child) = formed else {
            panic!("outer environment must own a formed leaf closure")
        };
        assert!(source.captured().iter().any(|nested| {
            nested
                .environments()
                .iter()
                .any(|environment| environment.owner() == child)
        }));
        child_owners.insert(child);
    }
    assert_eq!(child_owners.len(), 2, "each capture keeps its own child");
}

#[test]
fn loop_phi_publishes_conditional_leaf_after_phi_carries_choice() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupSelectorId,
        CleanupSelectorSource, IterationPhiIncomingKind,
    };

    // 收集条件 DAG 引用的 selector；用于区分“形成时的控制选择”与“循环 presence”。
    fn referenced_selectors(
        conditions: &CleanupConditions,
        condition: CleanupConditionId,
    ) -> Vec<(CleanupSelectorId, CleanupSelectorSource)> {
        let mut out = Vec::new();
        let mut pending = vec![condition];
        while let Some(condition) = pending.pop() {
            if let CleanupCondition::Choice { selector, branches } =
                conditions.get(condition).unwrap()
            {
                out.push((*selector, conditions.selector(*selector).unwrap().source()));
                pending.extend(branches.iter().copied());
            }
        }
        out
    }

    // 未显式赋值的 selector 视为选中分支 1，只用于隔离出 flag 控制选择的影响。
    fn selected(
        conditions: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match conditions.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => selected(
                conditions,
                branches[choices.get(selector).copied().unwrap_or(1)],
                choices,
            ),
        }
    }

    let (_, _, owned) = checked(
        "fun run(flag: Boolean) {\nval base: move () -> Unit = if (flag) (move {}) else (move {})\nval outer: move () -> Unit = move { var f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(!owned.iterations().is_empty());
    assert!(!owned.cleanup_steps().is_empty());
    assert!(!owned.drops().is_empty());

    let conditions = owned.cleanup_conditions();
    let plan = &owned.iterations()[0];
    let incoming = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    // 嵌套的 leaf 来源存在位；根 binding origins 自身不直接对应条件分支。
    let entry_leaves: Vec<_> = incoming(IterationPhiIncomingKind::Entry)
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .flat_map(|source| source.captured())
        .collect();
    assert!(
        entry_leaves.len() >= 2,
        "conditional base must expose both leaf presence slots"
    );

    let flag_selector = entry_leaves
        .iter()
        .flat_map(|leaf| referenced_selectors(conditions, leaf.condition()))
        .find_map(|(id, source)| matches!(source, CleanupSelectorSource::Control(_)).then_some(id))
        .expect("the conditional leaf depends on a source control selection");

    // Entry 必须把形成时的控制选择写成两个互补的 leaf presence 位。
    let mut active_target = BTreeMap::new();
    for flag in [0usize, 1] {
        let choices = BTreeMap::from([(flag_selector, flag)]);
        let active: Vec<_> = entry_leaves
            .iter()
            .filter(|leaf| selected(conditions, leaf.condition(), &choices))
            .map(|leaf| leaf.target())
            .collect();
        assert_eq!(
            active.len(),
            1,
            "exactly one leaf presence is set for flag branch {flag}: {active:?}"
        );
        active_target.insert(flag, active[0]);
    }
    assert_ne!(
        active_target[&0], active_target[&1],
        "the two flag branches select different leaf presence slots"
    );

    // Exhaustion 必须从 header phi presence 转发，而不是重新读取源码控制选择。
    let exhaustion_leaves: Vec<_> = incoming(IterationPhiIncomingKind::Exhaustion)
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .flat_map(|source| source.captured())
        .collect();
    assert_eq!(exhaustion_leaves.len(), entry_leaves.len());
    for leaf in &exhaustion_leaves {
        let sources = referenced_selectors(conditions, leaf.condition());
        assert!(
            sources
                .iter()
                .any(|(_, source)| matches!(source, CleanupSelectorSource::IterationPhi { .. })),
            "exhaustion presence must be transported from the header phi: {sources:?}"
        );
    }
}

#[test]
fn loop_phi_transports_conditional_leaf_presence_across_jump_edges() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupSelectorSource,
        IterationPhiIncomingKind,
    };

    fn referenced_selectors(
        conditions: &CleanupConditions,
        condition: CleanupConditionId,
    ) -> Vec<CleanupSelectorSource> {
        let mut out = Vec::new();
        let mut pending = vec![condition];
        while let Some(condition) = pending.pop() {
            if let CleanupCondition::Choice { selector, branches } =
                conditions.get(condition).unwrap()
            {
                out.push(conditions.selector(*selector).unwrap().source());
                pending.extend(branches.iter().copied());
            }
        }
        out
    }

    for body in ["continue", "break"] {
        let source = format!(
            "fun run(flag: Boolean) {{\nval base: move () -> Unit = if (flag) (move {{}}) else (move {{}})\nval outer: move () -> Unit = move {{ var f: move () -> Unit = move {{ base() }}\nfor (_ in listOf(1)) {{ {body} }}\nval used = f() }}\nval used = outer() }}"
        );
        let (_, _, owned) = checked(&source);
        assert!(
            owned.diagnostics().is_empty(),
            "body={body}: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.deferred().is_empty(),
            "body={body}: {:?}",
            owned.deferred()
        );
        let conditions = owned.cleanup_conditions();
        let plan = &owned.iterations()[0];
        let kinds: Vec<_> = plan
            .closure_phi_incomings()
            .iter()
            .map(|incoming| incoming.kind())
            .collect();
        let jump = kinds.iter().any(|kind| {
            matches!(
                (body, kind),
                ("continue", IterationPhiIncomingKind::Continue(_))
                    | ("break", IterationPhiIncomingKind::Break(_))
            )
        });
        assert!(jump, "body={body} must record its jump edge: {kinds:?}");

        // 除 Entry（形成时写入控制选择）外，每条边都必须从 header phi presence 转发。
        for incoming in plan.closure_phi_incomings() {
            if incoming.kind() == IterationPhiIncomingKind::Entry {
                continue;
            }
            let leaves: Vec<_> = incoming
                .bindings()
                .iter()
                .flat_map(|binding| binding.origins())
                .flat_map(|origin| origin.environments())
                .flat_map(|environment| environment.sources())
                .flat_map(|source| source.captured())
                .collect();
            for leaf in leaves {
                let sources = referenced_selectors(conditions, leaf.condition());
                assert!(
                    sources
                        .iter()
                        .any(|source| matches!(source, CleanupSelectorSource::IterationPhi { .. })),
                    "body={body} {:?} leaf presence must be transported from the header phi: {sources:?}",
                    incoming.kind()
                );
            }
        }
    }
}

#[test]
fn enclosing_leaf_snapshot_locates_formation_choice_in_the_parent_capture() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureSlotId, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
        CleanupConditions, CleanupOwnerValueId, CleanupSelectorId, CleanupSelectorSource,
        ClosureCaptureEffect, DropPoint, IterationCleanupAction as Action,
    };

    fn selected(
        conditions: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match conditions.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                selected(conditions, branches[choices[selector]], choices)
            }
        }
    }

    #[derive(Clone)]
    struct Instance {
        environment: usize,
        choices: BTreeMap<CleanupSelectorId, usize>,
    }

    #[derive(Default)]
    struct Replay {
        next_environment: usize,
        owners: BTreeMap<CleanupOwnerValueId, Instance>,
        captures: BTreeMap<(usize, CleanupCaptureSlotId), Instance>,
        control: BTreeMap<CleanupSelectorId, usize>,
    }

    impl Replay {
        fn create(&mut self, steps: &[(DropPoint, Action)], owner: CleanupOwnerValueId) {
            assert!(steps.iter().any(|(_, action)| {
                matches!(action, Action::CreateClosureOwner { owner: formed, .. } if *formed == owner)
            }));
            self.next_environment += 1;
            self.owners.insert(
                owner,
                Instance {
                    environment: self.next_environment,
                    choices: BTreeMap::new(),
                },
            );
        }

        fn capture(
            &mut self,
            steps: &[(DropPoint, Action)],
            conditions: &CleanupConditions,
            owner: CleanupOwnerValueId,
        ) {
            let (_, Action::SaveClosureCapture { target, input, .. }) = steps
                .iter()
                .find(|(_, action)| {
                    matches!(action, Action::SaveClosureCapture { owner: receiver, .. } if *receiver == owner)
                })
                .unwrap()
            else {
                unreachable!()
            };
            assert!(selected(conditions, input.condition(), &self.control));
            assert_eq!(input.effect(), ClosureCaptureEffect::Move);
            let source = match input.value() {
                CleanupCaptureValue::Owner(source) => self.owners.remove(&source).unwrap(),
                CleanupCaptureValue::Environment { owner, slot, .. } => {
                    let environment = self.owners[&owner].environment;
                    self.captures.remove(&(environment, slot)).unwrap()
                }
                CleanupCaptureValue::Place(_) => panic!("owned capture must have a value source"),
            };
            let environment = self.owners[&owner].environment;
            assert!(
                self.captures
                    .insert((environment, *target), source)
                    .is_none()
            );
        }

        fn snapshot(
            &mut self,
            steps: &[(DropPoint, Action)],
            conditions: &CleanupConditions,
            owner: CleanupOwnerValueId,
        ) -> Instance {
            let (_, Action::SaveOwnerSnapshot { condition, value, .. }) = steps
                .iter()
                .find(|(_, action)| {
                    matches!(action, Action::SaveOwnerSnapshot { owner: saved, .. } if *saved == owner)
                })
                .unwrap()
            else {
                unreachable!()
            };
            assert!(condition.is_none_or(|guard| selected(conditions, guard, &self.control)));
            let snapshot = conditions.owner_snapshot(owner).unwrap();
            assert_eq!(*value, snapshot.value());
            let inputs: Vec<_> = snapshot
                .capture_inputs()
                .iter()
                .filter(|input| selected(conditions, input.condition(), &self.control))
                .collect();
            assert_eq!(inputs.len(), 1);
            let mut instance = self.owners.remove(&inputs[0].owner()).unwrap();
            let prior_choices = instance.choices.clone();
            for copy in snapshot.copies() {
                if !selected(conditions, copy.when(), &self.control) {
                    continue;
                }
                let choice = match copy.source_value() {
                    Some(CleanupCaptureValue::Environment { owner, slot, .. }) => {
                        let environment = if owner == inputs[0].owner() {
                            instance.environment
                        } else {
                            self.owners[&owner].environment
                        };
                        self.captures[&(environment, slot)].choices[&copy.source()]
                    }
                    Some(CleanupCaptureValue::Owner(owner)) => {
                        self.owners[&owner].choices[&copy.source()]
                    }
                    Some(CleanupCaptureValue::Place(_)) => {
                        panic!("selector copy cannot read a non-owning place")
                    }
                    None => prior_choices
                        .get(&copy.source())
                        .or_else(|| self.control.get(&copy.source()))
                        .copied()
                        .expect("snapshot copy must read a formed choice"),
                };
                instance.choices.insert(copy.target(), choice);
                self.control.insert(copy.target(), choice);
            }
            self.owners.insert(owner, instance.clone());
            instance
        }
    }

    let (sources, parsed, owned) = checked(
        "fun run(flag: Boolean) {\nval base: move () -> Unit = if (flag) (move {}) else (move {})\nval outer: move () -> Unit = move { val f: move () -> Unit = move { base() }\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let (parent_owner, parent_slot, base_input) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveClosureCapture {
                owner,
                target,
                input,
            } if matches!(input.value(), CleanupCaptureValue::Owner(_)) => {
                Some((*owner, *target, input.value()))
            }
            _ => None,
        })
        .expect("outer saves the formed base instance");
    let CleanupCaptureValue::Owner(base_owner) = base_input else {
        unreachable!()
    };
    assert!(owned.cleanup_steps().iter().any(|(_, action)| {
        matches!(action, Action::CreateClosureOwner { owner, .. } if *owner == parent_owner)
    }));
    let (inner_owner, inner_slot, inner_capture_input) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveClosureCapture {
                owner,
                target,
                input,
            } if matches!(input.value(), CleanupCaptureValue::Environment { .. }) => {
                Some((*owner, *target, *input))
            }
            _ => None,
        })
        .expect("inner closure reads its immediate environment");
    let inner_input = inner_capture_input.value();
    assert!(owned.cleanup_steps().iter().any(|(_, action)| {
        matches!(action, Action::CreateClosureOwner { owner, .. } if *owner == inner_owner)
    }));
    let CleanupCaptureValue::Environment { owner, slot, .. } = inner_input else {
        unreachable!()
    };
    assert_eq!(owner, parent_owner);
    assert_eq!(slot, parent_slot);
    let f_snapshot = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveOwnerSnapshot { owner, value, .. }
                if sources.slice(parsed.ast().expressions().get(*value).unwrap().span())
                    == Ok("move { base() }") =>
            {
                Some(owned.cleanup_conditions().owner_snapshot(*owner).unwrap())
            }
            _ => None,
        })
        .expect("f saves the selected base leaf");
    assert_eq!(f_snapshot.copies().len(), 1);
    assert_eq!(f_snapshot.capture_inputs()[0].owner(), inner_owner);
    let f_copy = f_snapshot.copies()[0];
    assert_eq!(
        f_copy.source_value(),
        Some(CleanupCaptureValue::Environment {
            owner: inner_owner,
            source: inner_capture_input.source(),
            slot: inner_slot,
        })
    );
    let base_snapshot = owned
        .cleanup_conditions()
        .owner_snapshot(base_owner)
        .unwrap();
    assert_eq!(base_snapshot.copies().len(), 1);
    let base_copy = base_snapshot.copies()[0];
    assert_eq!(f_copy.source(), base_copy.target());
    let base_save = owned
        .cleanup_steps()
        .iter()
        .find(|(_, action)| matches!(action, Action::SaveOwnerSnapshot { owner, value, .. } if *owner == base_owner && *value == base_snapshot.value()))
        .expect("the base snapshot is saved after its RHS");
    assert_eq!(
        base_save.0,
        DropPoint::AfterExpression(base_snapshot.value())
    );
    assert_eq!(base_snapshot.capture_inputs().len(), 2);
    let selector = owned
        .cleanup_conditions()
        .selector(base_copy.source())
        .unwrap();
    assert_eq!(selector.branch_count(), 2);
    let CleanupSelectorSource::Control(control) = selector.source() else {
        panic!("base choice must come from the if expression");
    };
    assert_eq!(
        sources.slice(parsed.ast().expressions().get(control).unwrap().span()),
        Ok("if (flag) (move {}) else (move {})")
    );
    assert!(matches!(
        owned.cleanup_conditions().get(base_copy.when()),
        Some(CleanupCondition::Always)
    ));
    assert!(matches!(
        owned.cleanup_conditions().get(f_copy.when()),
        Some(CleanupCondition::Always)
    ));
    let steps = owned.cleanup_steps();
    let creation = |wanted: CleanupOwnerValueId| {
        steps.iter().position(|(_, action)| {
            matches!(action, Action::CreateClosureOwner { owner, .. } if *owner == wanted)
        }).unwrap()
    };
    let capture = |wanted: CleanupOwnerValueId| {
        steps.iter().position(|(_, action)| {
            matches!(action, Action::SaveClosureCapture { owner, .. } if *owner == wanted)
        }).unwrap()
    };
    assert_eq!(
        steps[creation(parent_owner)].0,
        steps[capture(parent_owner)].0
    );
    assert!(creation(parent_owner) < capture(parent_owner));
    assert_eq!(
        steps[creation(inner_owner)].0,
        steps[capture(inner_owner)].0
    );
    assert!(creation(inner_owner) < capture(inner_owner));
    let base_save_index = steps
        .iter()
        .position(|(_, action)| {
            matches!(action, Action::SaveOwnerSnapshot { owner, .. } if *owner == base_owner)
        })
        .unwrap();
    let f_save_index = steps
        .iter()
        .position(|(_, action)| {
            matches!(action, Action::SaveOwnerSnapshot { owner, .. } if *owner == f_snapshot.owner())
        })
        .unwrap();
    assert!(base_save_index < capture(parent_owner));
    assert!(capture(inner_owner) < f_save_index);
    assert_eq!(
        steps[f_save_index].0,
        DropPoint::AfterExpression(f_snapshot.value())
    );
    for arm in 0..2 {
        let formation_choices = BTreeMap::from([(base_copy.source(), arm)]);
        let selected_inputs: Vec<_> = base_snapshot
            .capture_inputs()
            .iter()
            .filter(|input| {
                selected(
                    owned.cleanup_conditions(),
                    input.condition(),
                    &formation_choices,
                )
            })
            .collect();
        assert_eq!(selected_inputs.len(), 1);
        let branch_owner = selected_inputs[0].owner();
        assert!(steps.iter().any(|(_, action)| {
            matches!(action, Action::CreateClosureOwner { owner, .. } if *owner == branch_owner)
        }));
        let Action::SaveClosureCapture { target, input, .. } = steps[capture(parent_owner)].1
        else {
            unreachable!()
        };
        assert_eq!(input.value(), CleanupCaptureValue::Owner(base_owner));
        assert!(selected(
            owned.cleanup_conditions(),
            input.condition(),
            &formation_choices
        ));
        assert_eq!(target, parent_slot);
        let Action::SaveClosureCapture { target, input, .. } = steps[capture(inner_owner)].1 else {
            unreachable!()
        };
        assert_eq!(input.value(), inner_input);
        assert_eq!(target, inner_slot);
        assert!(selected(
            owned.cleanup_conditions(),
            input.condition(),
            &formation_choices
        ));
        let CleanupCaptureValue::Environment {
            owner: source_owner,
            slot: source_slot,
            ..
        } = f_copy.source_value().unwrap()
        else {
            unreachable!()
        };
        assert_eq!(source_owner, inner_owner);
        assert_eq!(source_slot, inner_slot);
    }

    // 两次形成过程复用同一批静态 owner；只回放选中的形成、捕获、快照动作。
    // 保留第一次环境作隔离性探针；这里不模拟函数调用后的消费和释放。
    let conditions = owned.cleanup_conditions();
    let mut replay = Replay::default();
    let mut observations = Vec::new();
    for arm in 0..2 {
        replay.control.insert(base_copy.source(), arm);
        let branch_owner = base_snapshot
            .capture_inputs()
            .iter()
            .find(|input| selected(conditions, input.condition(), &replay.control))
            .unwrap()
            .owner();
        replay.create(steps, branch_owner);
        replay.snapshot(steps, conditions, base_owner);
        replay.create(steps, parent_owner);
        let parent_environment = replay.owners[&parent_owner].environment;
        replay.capture(steps, conditions, parent_owner);
        // 捕获已把选择搬进环境；后续快照不能依赖先前调用的临时控制状态。
        replay.control.remove(&base_copy.source());
        replay.control.remove(&base_copy.target());
        replay.create(steps, inner_owner);
        let inner_environment = replay.owners[&inner_owner].environment;
        replay.capture(steps, conditions, inner_owner);
        assert!(
            !replay
                .captures
                .contains_key(&(parent_environment, parent_slot))
        );
        let saved = replay.snapshot(steps, conditions, f_snapshot.owner());
        observations.push((
            parent_environment,
            inner_environment,
            saved.choices[&f_copy.target()],
        ));
    }
    assert_ne!(observations[0].0, observations[1].0);
    assert_ne!(observations[0].1, observations[1].1);
    assert_eq!(observations[0].2, 0);
    assert_eq!(observations[1].2, 1);
    assert_eq!(
        replay.captures[&(observations[0].1, inner_slot)].choices[&base_copy.target()],
        0
    );
}

#[test]
fn enclosing_leaf_snapshot_does_not_locate_outer_path_choice_in_the_capture() {
    use lang_frontend::ownership_checking::IterationCleanupAction as Action;

    let (sources, parsed, owned) = checked(
        "fun run(flag: Boolean) {\nval base: move () -> Unit = move {}\nval outer: move () -> Unit = if (flag) (move { val f: move () -> Unit = move { base() }\nval used = f() }) else (move {})\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let f_snapshot = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveOwnerSnapshot { owner, value, .. }
                if sources.slice(parsed.ast().expressions().get(*value).unwrap().span())
                    == Ok("move { base() }") =>
            {
                Some(owned.cleanup_conditions().owner_snapshot(*owner).unwrap())
            }
            _ => None,
        })
        .expect("f saves the outer branch condition");
    assert!(!f_snapshot.copies().is_empty());
    assert!(
        f_snapshot
            .copies()
            .iter()
            .all(|copy| copy.source_value().is_none())
    );
}

#[test]
fn loop_phi_defers_known_or_opaque_leaf_enclosing_capture() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun make(): move () -> Unit = move {}\nfun run(flag: Boolean) {\nval base: move () -> Unit = if (flag) (move {}) else (make())\nval outer: move () -> Unit = move { var f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1, "{:?}", owned.deferred());
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn loop_phi_publishes_enclosing_closure_with_owned_descendant() {
    use lang_frontend::ownership_checking::{CleanupCaptureValue, DropTarget};

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val base: move () -> Unit = move { read(xs) }
            val outer: move () -> Unit = move {
                var f: move () -> Unit = move { base() }
                for (_ in listOf(1)) {}
                val used = f()
            }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(!owned.iterations().is_empty());
    // 静态唯一、owned move 的紧邻后代必须带捕获槽与实例地址释放。
    assert!(
        owned.drops().iter().any(|fact| {
            matches!(
                fact.target(),
                DropTarget::Captured {
                    value: CleanupCaptureValue::Owner(_),
                    ..
                }
            ) && fact.capture_slot().is_some()
                && fact.instance_address().is_some()
        }),
        "owned descendant must be released by instance: {:?}",
        owned.drops()
    );
}

#[test]
fn non_loop_enclosing_capture_keeps_known_and_opaque_drops() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, DropPoint, DropTarget, IterationCleanupAction,
    };

    let (sources, parsed, owned) = checked(
        "fun make(): move () -> Unit = move {}\nfun read(xs: List<Int>) {}\nfun run(flag: Boolean, own xs: List<Int>) {\nval base: move () -> Unit = if (flag) (move { read(xs) }) else (make())\nval outer: move () -> Unit = move {\nval inner: move () -> Unit = move { base() }\nval used = inner() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let inner_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("inner()")).then_some(id))
        .unwrap();
    let parent = owned
        .drops()
        .iter()
        .filter(|fact| {
            fact.point() == DropPoint::CallReturn(inner_call)
                && matches!(
                    fact.target(),
                    DropTarget::Captured {
                        value: CleanupCaptureValue::Environment { .. },
                        ..
                    }
                )
        })
        .collect::<Vec<_>>();
    assert_eq!(parent.len(), 2);
    assert_eq!(
        parent.iter().filter(|fact| fact.owner().is_some()).count(),
        1
    );
    assert_eq!(parent[0].capture_slot(), parent[1].capture_slot());
    assert_eq!(parent[0].instance_address(), parent[1].instance_address());
    let DropTarget::Captured {
        owner: environment,
        closure,
        ..
    } = parent[0].target()
    else {
        unreachable!()
    };
    let snapshot = owned
        .cleanup_conditions()
        .owner_snapshot(environment)
        .unwrap();
    assert_eq!(snapshot.capture_inputs().len(), 1);
    let formed = snapshot.capture_inputs()[0].owner();
    assert!(owned.cleanup_steps().iter().any(|(_, action)| matches!(
        action,
        IterationCleanupAction::CreateClosureOwner { owner, closure: created }
            if *owner == formed && *created == closure
    )));
    assert!(owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::LambdaEntry(closure)
            && matches!(
                action,
                IterationCleanupAction::BindClosureEnvironment {
                    owner,
                    closure: entered,
                } if *owner == formed && *entered == closure
            )
    }));
    assert!(owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(inner_call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { callee, closure: called }
                    if *callee == environment && *called == Some(closure)
            )
    }));
    let base_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("base()")).then_some(id))
        .unwrap();
    assert!(!owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(base_call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { .. }
            )
    }));
}

#[test]
fn call_entry_passes_unique_environment_on_the_current_continuation_path() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};

    for body in [
        "val f: move () -> Unit = move { read(xs) }\nif (flag) { return }\nval used = f()",
        "if (flag) { val f: move () -> Unit = move { read(xs) }\nval used = f() }",
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(flag: Boolean, own xs: List<Int>) {{ {body} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
        let expression = |text: &str| {
            parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
                .unwrap()
        };
        let call = expression("f()");
        let closure = expression("move { read(xs) }");
        let passes = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| match action {
                IterationCleanupAction::PassClosureEnvironment {
                    callee,
                    closure: passed,
                } if *point == DropPoint::CallEntry(call) => Some((*callee, *passed)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            passes.len(),
            1,
            "the reachable call must carry its unique environment: {body}"
        );
        assert_eq!(passes[0].1, Some(closure));
        assert!(
            owned
                .drops()
                .iter()
                .any(|drop| drop.point() == DropPoint::CallReturn(call)
                    && drop.owner() == Some(passes[0].0)),
            "the same owner stays live through the call"
        );
    }
}

#[test]
fn call_entry_does_not_publish_mutable_callee_environment_without_retention() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {\nvar f: (Int) -> Unit = { n -> read(xs) }\nval used = f(1) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("f(1)")).then_some(id))
        .unwrap();
    assert!(!owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { .. }
            )
    }));
}

#[test]
fn call_entry_does_not_reuse_callee_owner_consumed_by_argument() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own g: move (Unit) -> Unit) {}\nfun run(own xs: List<Int>) {\nval f: move (Unit) -> Unit = move { arg -> read(xs) }\nval used = f(consume(f)) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()) == Ok("f(consume(f))")).then_some(id))
        .unwrap();
    assert!(!owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { .. }
            )
    }));
}

#[test]
fn call_entry_does_not_treat_conditional_callee_consumption_as_unconditional() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction};

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own g: move (Unit) -> Unit) {}\nfun noop(): Unit {}\nfun run(flag: Boolean, own xs: List<Int>) {\nval f: move (Unit) -> Unit = move { arg -> read(xs) }\nval used = f(if (flag) (consume(f)) else (noop())) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (sources.slice(node.span()) == Ok("f(if (flag) (consume(f)) else (noop()))"))
                .then_some(id)
        })
        .unwrap();
    assert!(!owned.cleanup_steps().iter().any(|(point, action)| {
        *point == DropPoint::CallEntry(call)
            && matches!(
                action,
                IterationCleanupAction::PassClosureEnvironment { .. }
            )
    }));
}

#[test]
fn loop_phi_keeps_non_closure_enclosing_capture_without_deferred() {
    use lang_frontend::ownership_checking::CleanupCaptureValue;

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run() { val xs = listOf(1)\nval outer: move () -> Unit = move { val f: () -> Unit = { read(xs) }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert_eq!(owned.iterations().len(), 1);
    let enclosing_sources = owned
        .iterations()
        .iter()
        .flat_map(|iteration| iteration.closure_phi_incomings())
        .flat_map(|incoming| incoming.bindings())
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .filter_map(|source| match source.value() {
            CleanupCaptureValue::Environment { slot, .. } => Some((source, slot)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!enclosing_sources.is_empty());
    for (source, outer_slot) in enclosing_sources {
        let inner_slot = source
            .source_capture_slot()
            .expect("phi must read the formed inner environment");
        assert_ne!(inner_slot, outer_slot);
        let slots = owned.cleanup_conditions();
        assert_eq!(
            slots.capture_slot_value(inner_slot).unwrap().source(),
            slots.capture_slot_value(outer_slot).unwrap().source()
        );
    }
}

#[test]
fn loop_phi_reads_formed_owned_capture_after_enclosing_move() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, ClosureCaptureEffect, ClosureCaptureMode,
        IterationCleanupAction as Action, IterationPhiIncomingKind,
    };

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) {
            val outer: move () -> Unit = move {
                var f: move () -> Unit = move { read(xs) }
                for (_ in listOf(1)) {}
                val used = f()
            }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let mut checked_sources = 0;
    let mut transport = None;
    for iteration in owned.iterations() {
        for incoming in iteration.closure_phi_incomings() {
            if incoming.kind() != IterationPhiIncomingKind::Entry {
                continue;
            }
            for binding in incoming.bindings() {
                for origin in binding.origins() {
                    for environment in origin.environments() {
                        for source in environment.sources() {
                            let CleanupCaptureValue::Environment {
                                owner: outer,
                                slot: outer_slot,
                                ..
                            } = source.value()
                            else {
                                continue;
                            };
                            let inner_slot = source
                                .source_capture_slot()
                                .expect("phi reads the formed inner environment");
                            assert_eq!(
                                source.transport_value(),
                                Some(CleanupCaptureValue::Environment {
                                    owner: environment.owner(),
                                    source: source.input().source(),
                                    slot: inner_slot,
                                })
                            );
                            assert_ne!(source.transport_value(), Some(source.value()));
                            let slots = owned.cleanup_conditions();
                            assert_eq!(source.input().mode(), ClosureCaptureMode::Owned);
                            assert_eq!(source.input().effect(), ClosureCaptureEffect::Move);
                            assert_ne!(inner_slot, outer_slot);
                            assert_eq!(
                                slots.capture_slot_value(outer_slot).unwrap().environment(),
                                outer
                            );
                            assert_ne!(
                                slots.capture_slot_value(inner_slot).unwrap().environment(),
                                outer
                            );
                            transport.get_or_insert((
                                outer,
                                outer_slot,
                                environment.owner(),
                                inner_slot,
                                source.transport_value().unwrap(),
                                binding.target(),
                                binding.values()[0].source(),
                            ));
                            checked_sources += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(
        checked_sources > 0,
        "owned enclosing capture must reach a phi incoming"
    );
    let (outer, outer_slot, inner, inner_slot, transport_value, header, entry_source) =
        transport.unwrap();
    assert_eq!(entry_source, inner);
    let saves = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            Action::SaveClosureCapture {
                owner,
                target,
                input,
            } if *target == outer_slot || *target == inner_slot => Some((*owner, *target, *input)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(saves.len(), 2);
    let outer_save = *saves
        .iter()
        .find(|(_, target, _)| *target == outer_slot)
        .unwrap();
    let inner_save = *saves
        .iter()
        .find(|(_, target, _)| *target == inner_slot)
        .unwrap();
    assert_eq!(outer_save.0, outer);
    assert_eq!(inner_save.0, inner);
    let CleanupCaptureValue::Owner(xs_owner) = outer_save.2.value() else {
        panic!("outer formation must consume the xs instance")
    };
    assert!(
        matches!(inner_save.2.value(), CleanupCaptureValue::Environment { owner, slot, .. }
        if owner == outer && slot == outer_slot)
    );
    // Lambda body is planned separately; actual execution forms the outer environment first.
    let formation_order = [outer_save, inner_save];
    let exhaustion = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    let exit = exhaustion
        .bindings()
        .iter()
        .find(|binding| {
            binding
                .values()
                .iter()
                .any(|value| value.source() == header)
        })
        .unwrap();
    assert_eq!(exit.values().len(), 1);
    let exit_target = exit.target();
    let exit_origin = exit
        .origins()
        .iter()
        .find(|origin| {
            origin
                .environments()
                .iter()
                .flat_map(|environment| environment.sources())
                .any(|source| source.input().source() == inner_save.2.source())
        })
        .unwrap();
    let exit_transport = exit_origin
        .environments()
        .iter()
        .flat_map(|environment| environment.sources())
        .find(|source| source.input().source() == inner_save.2.source())
        .unwrap()
        .transport_value()
        .unwrap();
    let header_capture_slot = owned
        .cleanup_conditions()
        .phi_capture_slot(
            header,
            owned.iterations()[0].capture_graph().nodes()[exit_origin.node()].closure(),
            inner_save.2.source(),
        )
        .unwrap();
    assert!(
        matches!(exit_transport, CleanupCaptureValue::Environment { owner, slot, .. }
        if owner == header && slot == header_capture_slot)
    );
    assert_ne!(header_capture_slot, inner_slot);
    let mut capture_slots = BTreeMap::new();
    for round in 0..2_u32 {
        let xs_instance = 100 + round;
        let outer_instance = 200 + round;
        let inner_instance = 300 + round;
        let mut owners = BTreeMap::from([
            (xs_owner, xs_instance),
            (outer, outer_instance),
            (inner, inner_instance),
        ]);
        for (owner, target, input) in &formation_order {
            let target_position = owned
                .cleanup_conditions()
                .capture_slot_value(*target)
                .unwrap()
                .position();
            let value = match input.value() {
                CleanupCaptureValue::Owner(source) => owners.remove(&source).unwrap(),
                CleanupCaptureValue::Environment { owner, slot, .. } => {
                    let source_position = owned
                        .cleanup_conditions()
                        .capture_slot_value(slot)
                        .unwrap()
                        .position();
                    capture_slots
                        .remove(&(owners[&owner], source_position))
                        .unwrap()
                }
                CleanupCaptureValue::Place(_) => panic!("owned move must read a saved instance"),
            };
            assert!(
                capture_slots
                    .insert((owners[owner], target_position), value)
                    .is_none()
            );
        }
        let outer_position = owned
            .cleanup_conditions()
            .capture_slot_value(outer_slot)
            .unwrap()
            .position();
        assert!(!capture_slots.contains_key(&(outer_instance, outer_position)));
        let CleanupCaptureValue::Environment { owner, slot, .. } = transport_value else {
            panic!("phi must read the formed inner environment")
        };
        let entry_position = owned
            .cleanup_conditions()
            .capture_slot_value(slot)
            .unwrap()
            .position();
        assert_eq!(
            capture_slots[&(owners[&owner], entry_position)],
            xs_instance
        );
        let entry_instance = owners.remove(&entry_source).unwrap();
        assert_eq!(entry_instance, inner_instance);
        assert!(owners.insert(header, entry_instance).is_none());
        let CleanupCaptureValue::Environment { owner, slot, .. } = exit_transport else {
            panic!("exhaustion must read the header environment")
        };
        assert_eq!(owner, header);
        let exit_position = owned
            .cleanup_conditions()
            .capture_slot_value(slot)
            .unwrap()
            .position();
        assert_eq!(capture_slots[&(owners[&owner], exit_position)], xs_instance);
        let header_instance = owners.remove(&header).unwrap();
        assert!(owners.insert(exit_target, header_instance).is_none());
        assert_eq!(owners[&exit_target], inner_instance);
    }
    let inner_position = owned
        .cleanup_conditions()
        .capture_slot_value(inner_slot)
        .unwrap()
        .position();
    assert_eq!(capture_slots[&(300, inner_position)], 100);
    assert_eq!(capture_slots[&(301, inner_position)], 101);
}

#[test]
fn recursive_loop_carried_capture_does_not_publish_a_truncated_phi() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;
    let (_, _, owned) = checked(
        "fun run(flags: List<Int>) { var f: move () -> Unit = move {}\nfor (_ in flags) { f = move { f() } }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1);
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::RecursiveClosureCapture
    );
    assert!(
        owned.iterations().is_empty(),
        "recursive captured environments need an unbounded transport representation"
    );
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn recursive_owned_chain_with_shared_child_stays_atomically_deferred() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(xs: List<Int>, flags: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in flags) {\n    val g: () -> Unit = { read(xs) }\n    f = move { val old = f()\nval borrowed = g() }\n}\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1);
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::RecursiveClosureCapture
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn recursive_chain_keeps_scoped_shared_source_atomically_deferred() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(first: List<Int>, flags: List<Int>) {
var f: move () -> Unit = move {}
{
    val xs = listOf(1)
    val borrowed: () -> Unit = { read(xs) }
    for (_ in first) { f = move { f() } }
    f = move { val old = f()\nval used = borrowed() }
}
for (_ in flags) { f = move { f() } }
val used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(
        owned
            .deferred()
            .iter()
            .any(|deferred| deferred.reason() == OwnershipDeferredReason::RecursiveClosureCapture)
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn closure_holding_two_recursive_loop_roots_stays_atomically_deferred() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, OwnershipDeferredReason,
    };

    let (sources, _, owned) = checked(
        "fun run(first: List<Int>, second: List<Int>) {\nvar f: move () -> Unit = move {}\nfor (_ in first) { f = move { f() } }\nvar g: move () -> Unit = move {}\nfor (_ in second) { g = move { g() } }\nval outer: move () -> Unit = move { val x = f()\nval y = g() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let outer_captures = owned
        .captures()
        .iter()
        .filter(|capture| {
            owned
                .captures()
                .iter()
                .filter(|other| other.lambda() == capture.lambda())
                .count()
                == 2
        })
        .collect::<Vec<_>>();
    assert_eq!(outer_captures.len(), 2);
    assert_eq!(
        outer_captures
            .iter()
            .map(|capture| sources.slice(capture.reference_span()).unwrap())
            .collect::<Vec<_>>(),
        ["f", "g"]
    );
    assert!(outer_captures.iter().all(|capture| {
        capture.mode() == ClosureCaptureMode::Owned
            && capture.effect() == ClosureCaptureEffect::Move
    }));
    assert!(
        owned.deferred().iter().any(|deferred| {
            deferred.reason() == OwnershipDeferredReason::RecursiveClosureCapture
        })
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn mutually_recursive_loop_captures_do_not_publish_a_truncated_phi() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun run(flags: List<Int>) { var f: move () -> Unit = move {}\nvar g: move () -> Unit = move {}\nfor (_ in flags) { { f = move { g() } }\ng = move { f() } }\nval used = g() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1);
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::RecursiveClosureCapture
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn recursive_capture_discards_earlier_iteration_facts_atomically() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun run(first: List<Int>, second: List<Int>) { for (_ in first) {}\nvar f: move () -> Unit = move {}\nfor (_ in second) { f = move { f() } }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1);
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::RecursiveClosureCapture
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn owned_capture_reads_the_prior_phi_environment_before_replacement() {
    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupOwnerValue, ClosureCaptureSource, DropPoint,
        IterationCleanupAction as Action, IterationPhiBoundary,
    };

    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Int>) { var g: move () -> Unit = move {}\nvar f: move () -> Unit = move {}\nfor (_ in flags) { { f = move { g() } }\ng = move {} }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let captured_g = owned
        .captures()
        .iter()
        .find_map(|capture| {
            (sources.slice(capture.reference_span()).unwrap() == "g").then_some(capture.source())
        })
        .unwrap();
    let ClosureCaptureSource::Symbol(g_symbol) = captured_g else {
        panic!("g capture must resolve to a binding")
    };
    let plan = &owned.iterations()[0];
    let header_g = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header && phi.symbol() == g_symbol)
        .unwrap();
    let f_lambda = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (sources.slice(node.span()).unwrap() == "move { g() }").then_some(id)
        })
        .unwrap();
    let (formation_index, created_f) = owned
        .cleanup_steps()
        .iter()
        .enumerate()
        .find_map(|(index, (point, action))| match action {
            Action::CreateClosureOwner { owner, closure }
                if *point == DropPoint::AfterExpression(f_lambda) && *closure == f_lambda =>
            {
                Some((index, *owner))
            }
            _ => None,
        })
        .unwrap();
    let Some(CleanupOwnerValue::Closure { inputs, .. }) =
        owned.cleanup_conditions().owner_value(created_f)
    else {
        panic!("f formation must define an environment")
    };
    assert!(inputs.iter().any(|input| {
        input.source() == captured_g
            && input.value() == CleanupCaptureValue::Owner(header_g.owner())
    }));
    let (replacement_index, replacement_owner) = owned
        .cleanup_steps()
        .iter()
        .enumerate()
        .find_map(|(index, (_, action))| match action {
            Action::CommitOwnerSnapshot { owner, target }
                if index > formation_index && *target == g_symbol =>
            {
                Some((index, *owner))
            }
            _ => None,
        })
        .unwrap();
    assert_ne!(replacement_owner, header_g.owner());
    assert!(
        formation_index < replacement_index,
        "f must capture the old g instance before g is replaced"
    );
}

#[test]
fn two_round_owned_capture_keeps_each_prior_environment_instance() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorId, ClosureCaptureSource, DropPoint,
        DropTarget, IterationCleanupAction as Action, IterationClosurePhiBinding,
        IterationClosurePhiOrigin, IterationPhiBoundary, IterationPhiIncoming,
        IterationPhiIncomingKind, IterationPhiIncomingOrigin,
    };

    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                let branch = choices.get(selector).unwrap_or_else(|| {
                    panic!("condition {condition:?} reads uninitialized selector {selector:?}: {:?}; choices: {choices:?}", table.selector(*selector))
                });
                selected(table, branches[*branch], choices)
            }
        }
    }

    fn source_of(edge: &IterationPhiIncoming, target: CleanupOwnerValueId) -> CleanupOwnerValueId {
        let binding = edge
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap();
        assert_eq!(binding.values().len(), 1, "this edge has one actual value");
        binding.values()[0].source()
    }

    struct ReplayBefore<'a> {
        choices: &'a BTreeMap<CleanupSelectorId, usize>,
        owners: &'a BTreeMap<CleanupOwnerValueId, u32>,
        captured: &'a BTreeMap<(u32, usize), (CleanupOwnerValueId, u32)>,
    }

    #[derive(Default)]
    struct ReplayWrites {
        choices: Vec<(CleanupSelectorId, usize)>,
        owners: Vec<(CleanupOwnerValueId, u32)>,
        consumed: Vec<CleanupOwnerValueId>,
    }

    fn copy_origin(
        table: &CleanupConditions,
        root: CleanupOwnerValueId,
        origins: &[IterationClosurePhiOrigin],
        layout: &IterationClosurePhiOrigin,
        origin: &IterationPhiIncomingOrigin,
        before: &ReplayBefore<'_>,
        writes: &mut ReplayWrites,
    ) {
        assert_eq!(origin.target(), layout.selector());
        let present = selected(table, origin.condition(), before.choices);
        if !present {
            return;
        }
        let environments = origin
            .environments()
            .iter()
            .filter(|environment| selected(table, environment.condition(), before.choices))
            .collect::<Vec<_>>();
        assert_eq!(environments.len(), 1);
        let environment = environments[0];
        for source in environment.sources() {
            if !selected(table, source.input().condition(), before.choices) {
                continue;
            }
            let slot = layout
                .sources()
                .iter()
                .find(|slot| slot.source() == source.input().source())
                .unwrap();
            if let Some(target) = source.target() {
                assert_eq!(target, slot.owner());
                let CleanupCaptureValue::Owner(actual) = source.value() else {
                    panic!("owned capture must read an actual source owner")
                };
                let static_slot = table
                    .phi_capture_slot(root, layout.closure(), slot.source())
                    .unwrap();
                assert_eq!(source.capture_slot(), Some(static_slot));
                let (read_address, source_slot) = source
                    .transport_read()
                    .expect("owned incoming reads a published instance and source slot");
                let read_address = table.instance_address(read_address).unwrap();
                assert_eq!(read_address.root(), environment.instance_root());
                assert_eq!(read_address.capture_path(), environment.capture_path());
                if environment.capture_path().is_empty() {
                    assert!(matches!(
                        source.transport_value(),
                        Some(CleanupCaptureValue::Environment { owner, slot, .. })
                            if owner == environment.owner() && slot == source_slot
                    ));
                } else {
                    assert_eq!(source.transport_value(), None);
                }
                let source_layout = table.capture_slot_value(source_slot).unwrap();
                assert_eq!(source_layout.source(), slot.source());
                assert_eq!(source_layout.closure(), layout.closure());
                assert_ne!(source_slot, static_slot);
                let position = source_layout.position();
                let mut instance = before.owners[&read_address.root()];
                for &capture in read_address.capture_path() {
                    instance = before.captured[&(instance, capture)].1;
                }
                let (saved_source, captured) = before.captured[&(instance, position)];
                if let Some(source_instance) = before.owners.get(&actual) {
                    assert_eq!(*source_instance, captured);
                } else {
                    assert_eq!(actual, saved_source);
                }
                writes.owners.push((target, captured));
            }
            for nested in source.captured() {
                let nested_layout = slot
                    .captured()
                    .iter()
                    .map(|&index| &origins[index])
                    .find(|layout| layout.selector() == nested.target())
                    .unwrap();
                copy_origin(table, root, origins, nested_layout, nested, before, writes);
            }
        }
    }

    fn copy(
        table: &CleanupConditions,
        edge: &IterationPhiIncoming,
        targets: &[&IterationClosurePhiBinding],
        choices: &mut BTreeMap<CleanupSelectorId, usize>,
        owners: &mut BTreeMap<CleanupOwnerValueId, u32>,
        captured: &BTreeMap<(u32, usize), (CleanupOwnerValueId, u32)>,
    ) {
        let before_choices = choices.clone();
        let before_owners = owners.clone();
        assert!(selected(table, edge.condition(), &before_choices));
        let before = ReplayBefore {
            choices: &before_choices,
            owners: &before_owners,
            captured,
        };
        let mut writes = ReplayWrites::default();
        for target in targets {
            let binding = edge
                .bindings()
                .iter()
                .find(|binding| binding.target() == target.owner())
                .unwrap();
            let available = selected(table, binding.available_when(), &before_choices);
            writes
                .choices
                .push((binding.availability_selector(), usize::from(available)));
            for write in binding.selector_writes() {
                writes.choices.push((
                    write.target(),
                    usize::from(selected(table, write.condition(), &before_choices)),
                ));
            }
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert_eq!(values.len(), usize::from(available));
            assert_eq!(target.root_origins().count(), binding.origins().len());
            let selected_origins = binding
                .origins()
                .iter()
                .filter(|origin| selected(table, origin.condition(), &before_choices))
                .flat_map(|origin| origin.environments())
                .filter(|environment| selected(table, environment.condition(), &before_choices))
                .map(|environment| before_owners[&environment.owner()])
                .collect::<Vec<_>>();
            assert_eq!(
                selected_origins,
                values
                    .iter()
                    .map(|value| before_owners[&value.source()])
                    .collect::<Vec<_>>()
            );
            if let Some(value) = values.first() {
                writes.consumed.push(value.source());
                writes
                    .owners
                    .push((target.owner(), before_owners[&value.source()]));
            }
            for origin in binding.origins() {
                let layout = target
                    .origins()
                    .iter()
                    .find(|layout| layout.selector() == origin.target())
                    .unwrap();
                copy_origin(
                    table,
                    target.owner(),
                    target.origins(),
                    layout,
                    origin,
                    &before,
                    &mut writes,
                );
            }
        }
        choices.extend(writes.choices);
        for source in writes.consumed {
            assert!(
                owners.remove(&source).is_some(),
                "phi move consumes its source"
            );
        }
        owners.extend(writes.owners);
    }

    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Int>) { var g: move () -> Unit = move {}\nvar f: move () -> Unit = move {}\nfor (_ in flags) { { f = move { g() } }\ng = move {} }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let symbol = |name| {
        names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id()
    };
    let (f, g) = (symbol("f"), symbol("g"));
    let capture = ClosureCaptureSource::Symbol(g);
    let plan = &owned.iterations()[0];
    let phi = |boundary, name| {
        plan.closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.symbol() == name)
            .unwrap()
    };
    let (header_f, header_g) = (
        phi(IterationPhiBoundary::Header, f),
        phi(IterationPhiBoundary::Header, g),
    );
    let exit_f = phi(IterationPhiBoundary::Exit, f);
    let edge = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    let entry = edge(IterationPhiIncomingKind::Entry);
    let backedge = edge(IterationPhiIncomingKind::Fallthrough);
    let exhaustion = edge(IterationPhiIncomingKind::Exhaustion);
    let f_lambda = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            (sources.slice(node.span()).unwrap() == "move { g() }").then_some(id)
        })
        .unwrap();
    let created_f = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::CreateClosureOwner { owner, closure } if *closure == f_lambda => Some(*owner),
            _ => None,
        })
        .unwrap();
    let Some(CleanupOwnerValue::Closure { inputs, .. }) =
        owned.cleanup_conditions().owner_value(created_f)
    else {
        panic!("body lambda forms a concrete environment")
    };
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].source(), capture);
    assert_eq!(
        inputs[0].value(),
        CleanupCaptureValue::Owner(header_g.owner())
    );
    let table = owned.cleanup_conditions();
    let capture_edges = table.closure_capture_edges(created_f).unwrap();
    assert_eq!(capture_edges.len(), 1);
    assert_eq!(capture_edges[0].input(), inputs[0]);
    let slot = capture_edges[0].target();
    let position = table.capture_slot_value(slot).unwrap().position();
    for (edge, target, expected) in [
        (backedge, header_f.owner(), slot),
        (
            exhaustion,
            exit_f.owner(),
            table
                .phi_capture_slot(header_f.owner(), f_lambda, capture)
                .unwrap(),
        ),
    ] {
        let source_slots = edge
            .bindings()
            .iter()
            .filter(|binding| binding.target() == target)
            .flat_map(|binding| binding.origins())
            .flat_map(|origin| origin.environments())
            .flat_map(|environment| environment.sources())
            .filter(|source| source.input().source() == capture)
            .map(|source| source.source_capture_slot())
            .collect::<Vec<_>>();
        assert_eq!(source_slots, [Some(expected)]);
    }
    let committed = |target| {
        owned
            .cleanup_steps()
            .iter()
            .find_map(|(_, action)| match action {
                Action::CommitOwnerSnapshot {
                    owner,
                    target: actual,
                } if *actual == target => Some(*owner),
                _ => None,
            })
            .unwrap()
    };
    let (next_f, next_g) = (committed(f), committed(g));
    let snapshot = table.owner_snapshot(next_f).unwrap();
    let g_snapshot = table.owner_snapshot(next_g).unwrap();
    assert_eq!(snapshot.value(), f_lambda);
    assert_eq!(snapshot.capture_inputs().len(), 1);
    assert_eq!(snapshot.capture_inputs()[0].owner(), created_f);
    let action_index = |predicate: &dyn Fn(&Action) -> bool| {
        owned
            .cleanup_steps()
            .iter()
            .position(|(_, action)| predicate(action))
            .unwrap()
    };
    let formation = action_index(
        &|action| matches!(action, Action::CreateClosureOwner { owner, .. } if *owner == created_f),
    );
    let capture_write = action_index(&|action| {
        matches!(action, Action::SaveClosureCapture { owner, target, input }
            if *owner == created_f && *target == slot && *input == capture_edges[0].input())
    });
    let f_save = action_index(
        &|action| matches!(action, Action::SaveOwnerSnapshot { owner, .. } if *owner == next_f),
    );
    let old_f_drop = action_index(&|action| {
        matches!(action, Action::Drop(fact) if matches!(fact.target(), DropTarget::Captured { owner, closure, source, .. }
            if owner == header_f.owner() && closure == f_lambda && source == capture))
    });
    let f_commit = action_index(
        &|action| matches!(action, Action::CommitOwnerSnapshot { owner, .. } if *owner == next_f),
    );
    let g_save = action_index(
        &|action| matches!(action, Action::SaveOwnerSnapshot { owner, .. } if *owner == next_g),
    );
    assert!(
        formation < capture_write
            && capture_write < f_save
            && f_save < old_f_drop
            && old_f_drop < f_commit
    );
    assert!(f_commit < g_save);
    let save_guard = |index, expected_owner, expected_value| {
        let (
            point,
            Action::SaveOwnerSnapshot {
                owner,
                value,
                condition,
            },
        ) = &owned.cleanup_steps()[index]
        else {
            panic!("expected a saved owner action")
        };
        assert_eq!(*owner, expected_owner);
        assert_eq!(*value, expected_value);
        assert_eq!(*point, DropPoint::AfterExpression(*value));
        *condition
    };
    let f_save_guard = save_guard(f_save, next_f, snapshot.value());
    let g_save_guard = save_guard(g_save, next_g, g_snapshot.value());
    assert_eq!(source_of(backedge, header_f.owner()), next_f);
    assert_eq!(source_of(backedge, header_g.owner()), next_g);
    assert_eq!(source_of(exhaustion, exit_f.owner()), header_f.owner());
    for (owner, point) in [
        (header_f.owner(), DropPoint::AfterExpression(f_lambda)),
        (
            exit_f.owner(),
            DropPoint::CallReturn(
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .find_map(|(id, node)| {
                        (sources.slice(node.span()).unwrap() == "f()").then_some(id)
                    })
                    .unwrap(),
            ),
        ),
    ] {
        let expected = table.phi_capture_slot(owner, f_lambda, capture).unwrap();
        assert_eq!(
            table.capture_slot_value(expected).unwrap().position(),
            position
        );
        assert!(owned.drops().iter().any(|fact| {
            fact.point() == point
                && matches!(fact.target(), DropTarget::Captured { owner: actual, closure, source, .. }
                    if actual == owner && closure == f_lambda && source == capture)
                && fact.capture_slot() == Some(expected)
        }), "both old and final f need a captured-edge release candidate");
    }

    let mut current = BTreeMap::from([
        (source_of(entry, header_f.owner()), 2_u32),
        (source_of(entry, header_g.owner()), 1_u32),
    ]);
    let mut choices = BTreeMap::new();
    let mut captured_instances = BTreeMap::new();
    copy(
        table,
        entry,
        &[header_f, header_g],
        &mut choices,
        &mut current,
        &captured_instances,
    );
    let mut released = Vec::new();
    for round in 0..2_u32 {
        let old_g = current[&header_g.owner()];
        let new_f = 10 + round;
        let before_formation = current.clone();
        let Action::SaveClosureCapture {
            owner,
            target,
            input,
        } = owned.cleanup_steps()[capture_write].1
        else {
            panic!("the formed environment must save its checked capture")
        };
        assert_eq!(owner, created_f);
        assert_eq!(target, slot);
        assert!(selected(table, input.condition(), &choices));
        let CleanupCaptureValue::Owner(source_owner) = input.value() else {
            panic!("this body lambda reads the prior g owner")
        };
        let captured_g = current.remove(&source_owner).unwrap();
        assert_eq!(captured_g, before_formation[&source_owner]);
        assert!(!current.contains_key(&source_owner));
        current.insert(owner, new_f);
        assert!(
            captured_instances
                .insert((current[&owner], position), (source_owner, captured_g))
                .is_none()
        );
        assert_eq!(captured_g, old_g);
        let before_save = choices.clone();
        assert!(f_save_guard.is_none_or(|guard| selected(table, guard, &before_save)));
        for selector in snapshot.copies() {
            if selected(table, selector.when(), &before_save) {
                choices.insert(selector.target(), before_save[&selector.source()]);
            }
        }
        let active_inputs = snapshot
            .capture_inputs()
            .iter()
            .filter(|input| selected(table, input.condition(), &choices))
            .collect::<Vec<_>>();
        assert_eq!(active_inputs.len(), 1);
        let saved_f = current[&active_inputs[0].owner()];
        current.insert(next_f, saved_f);
        let old_drops = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| match action {
                Action::Drop(fact)
                    if *point == DropPoint::AfterExpression(f_lambda)
                        && matches!(fact.target(), DropTarget::Captured { owner, closure, source, .. }
                            if owner == header_f.owner() && closure == f_lambda && source == capture)
                        && fact.condition().is_none_or(|guard| selected(table, guard, &choices)) =>
                {
                    let static_slot = fact.capture_slot().unwrap();
                    assert_eq!(static_slot, table.phi_capture_slot(header_f.owner(), f_lambda, capture).unwrap());
                    let instance = current[&header_f.owner()];
                    let source_instance = current[&fact.owner().unwrap()];
                    let (_, captured) = captured_instances.remove(&(instance, table.capture_slot_value(static_slot).unwrap().position())).unwrap();
                    assert_eq!(captured, source_instance);
                    Some(captured)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(old_drops.len(), usize::from(round > 0));
        released.extend(old_drops);
        let before_g_save = choices.clone();
        assert!(g_save_guard.is_none_or(|guard| selected(table, guard, &before_g_save)));
        for selector in g_snapshot.copies() {
            if selected(table, selector.when(), &before_g_save) {
                choices.insert(selector.target(), before_g_save[&selector.source()]);
            }
        }
        current.insert(next_g, 20 + round);
        copy(
            table,
            backedge,
            &[header_f, header_g],
            &mut choices,
            &mut current,
            &captured_instances,
        );
        assert_eq!(
            captured_instances[&(current[&header_f.owner()], position)].1,
            old_g
        );
        assert_ne!(current[&header_g.owner()], old_g);
    }
    copy(
        table,
        exhaustion,
        &[exit_f],
        &mut choices,
        &mut current,
        &captured_instances,
    );
    let final_f = current[&exit_f.owner()];
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let final_drops = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| match action {
            Action::Drop(fact)
                if *point == DropPoint::CallReturn(call)
                    && matches!(fact.target(), DropTarget::Captured { owner, closure, source, .. }
                        if owner == exit_f.owner() && closure == f_lambda && source == capture)
                    && fact
                        .condition()
                        .is_none_or(|guard| selected(table, guard, &choices)) =>
            {
                let static_slot = fact.capture_slot().unwrap();
                assert_eq!(
                    static_slot,
                    table
                        .phi_capture_slot(exit_f.owner(), f_lambda, capture)
                        .unwrap()
                );
                let (_, captured) = captured_instances
                    .remove(&(
                        final_f,
                        table.capture_slot_value(static_slot).unwrap().position(),
                    ))
                    .unwrap();
                assert_eq!(captured, current[&fact.owner().unwrap()]);
                Some(captured)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(final_drops.len(), 1);
    released.extend(final_drops);
    assert_eq!(
        released,
        [1, 20],
        "old g instances release in capture order"
    );
    assert!(captured_instances.is_empty());
}

#[test]
fn iteration_plan_exposes_finite_owned_capture_graph() {
    use lang_frontend::ownership_checking::{ClosureCaptureMode, ClosureCaptureSource};

    let (sources, parsed, owned) = checked(
        "fun run(flags: List<Int>) { var g: move () -> Unit = move {}\nvar f: move () -> Unit = move {}\nfor (_ in flags) { { f = move { g() } }\ng = move {} }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let graph = owned.iterations()[0].capture_graph();
    let mut closures = std::collections::BTreeSet::new();
    for node in graph.nodes() {
        assert!(
            closures.insert(node.closure().index()),
            "one node per lambda identity"
        );
        for source in node.sources() {
            for &target in source.captured() {
                assert!(target < graph.nodes().len(), "edge stays inside the graph");
            }
        }
    }
    let body_f = graph
        .nodes()
        .iter()
        .find(|node| {
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(node.closure())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                == "move { g() }"
        })
        .unwrap();
    let g_source = body_f
        .sources()
        .iter()
        .find(|source| matches!(source.capture().source(), ClosureCaptureSource::Symbol(_)))
        .unwrap();
    assert_eq!(g_source.capture().mode(), ClosureCaptureMode::Owned);
    assert!(
        !g_source.captured().is_empty(),
        "owned g keeps its possible prior environments"
    );
    let plan = &owned.iterations()[0];
    let table = owned.cleanup_conditions();
    let mut all_slots = std::collections::BTreeSet::new();
    for phi in plan.closure_phis() {
        let mut seen = std::collections::BTreeSet::new();
        let mut reachable = std::collections::BTreeSet::new();
        let tree_roots = phi
            .root_origins()
            .map(|origin| origin.node())
            .collect::<Vec<_>>();
        assert_eq!(phi.root_nodes(), tree_roots);
        let mut pending = phi.root_nodes().to_vec();
        while let Some(node) = pending.pop() {
            if !reachable.insert(node) {
                continue;
            }
            for source in graph.nodes()[node].sources() {
                pending.extend_from_slice(source.captured());
            }
        }
        let expected = reachable
            .into_iter()
            .flat_map(|node| {
                graph.nodes()[node]
                    .sources()
                    .iter()
                    .map(move |source| (node, source.position()))
            })
            .collect::<std::collections::BTreeSet<_>>();
        for capture in phi.capture_layout() {
            assert!(seen.insert((capture.node(), capture.position())));
            assert!(
                all_slots.insert(capture.slot()),
                "phi roots cannot share capture slots"
            );
            let node = &graph.nodes()[capture.node()];
            let edge = node
                .sources()
                .iter()
                .find(|source| source.position() == capture.position())
                .unwrap();
            let slot = table.capture_slot_value(capture.slot()).unwrap();
            assert_eq!(slot.environment(), phi.owner());
            assert_eq!(slot.closure(), node.closure());
            assert_eq!(slot.source(), edge.capture().source());
            assert_eq!(slot.position(), capture.position());
        }
        assert_eq!(
            seen, expected,
            "each phi must cover its full reachable graph"
        );
        for origin in phi.origins() {
            assert_eq!(graph.nodes()[origin.node()].closure(), origin.closure());
            for source in origin.sources() {
                let edge = graph.nodes()[origin.node()]
                    .sources()
                    .iter()
                    .find(|edge| edge.capture().source() == source.source())
                    .unwrap();
                for &index in source.captured() {
                    let nested = &phi.origins()[index];
                    assert!(edge.captured().contains(&nested.node()));
                    assert_eq!(graph.nodes()[nested.node()].closure(), nested.closure());
                }
            }
        }
    }
    let mut nested_incomings = 0;
    for incoming in plan.closure_phi_incomings() {
        for binding in incoming.bindings() {
            let phi = plan
                .closure_phis()
                .iter()
                .find(|phi| phi.owner() == binding.target())
                .unwrap();
            assert_eq!(
                binding.capture_slots_to_clear(),
                phi.capture_layout()
                    .iter()
                    .map(|capture| capture.slot())
                    .collect::<Vec<_>>()
            );
            for origin in binding.origins() {
                let layout = phi
                    .origins()
                    .iter()
                    .find(|layout| layout.selector() == origin.target())
                    .unwrap();
                assert_eq!(origin.node(), layout.node());
                for environment in origin.environments() {
                    for source in environment.sources() {
                        let Some(slot) = layout
                            .sources()
                            .iter()
                            .find(|slot| slot.source() == source.input().source())
                        else {
                            assert!(source.captured().is_empty());
                            continue;
                        };
                        for nested in source.captured() {
                            nested_incomings += 1;
                            let target = slot
                                .captured()
                                .iter()
                                .map(|&index| &phi.origins()[index])
                                .find(|target| target.selector() == nested.target())
                                .unwrap();
                            assert_eq!(nested.node(), target.node());
                        }
                    }
                }
            }
        }
    }
    assert!(
        nested_incomings > 0,
        "fixture must exercise nested capture transport"
    );
}

#[test]
fn alternative_owned_captures_defer_when_phi_loses_exclusive_paths() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (sources, parsed, owned) = checked(
        r#"fun read(xs: List<Int>) {}
fun run(own xs: List<Int>, flag: Boolean, flags: List<Int>) {
    var base: move () -> Unit = move { read(xs) }
    var f: move () -> Unit = move {}
    var g: move () -> Unit = move {}
    if (flag) { f = move { base() } } else { g = move { base() } }
    var outer: move () -> Unit = move { val first = f()
val second = g() }
    for (_ in flags) {}
    val used = outer()
}"#,
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let [deferred] = owned.deferred() else {
        panic!("forwarded phi must not publish independent nested presence bits");
    };
    assert_eq!(
        deferred.reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert_eq!(
        sources.slice(
            parsed
                .ast()
                .expressions()
                .get(deferred.expression())
                .unwrap()
                .span()
        ),
        Ok("move { read(xs) }")
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn same_lambda_parent_paths_defer_until_instance_transport() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>, next: List<Int>) {
            var first: move () -> Unit = move {}
            var second: move () -> Unit = move {}
            for (_ in flags) {
                second = first
                val xs = listOf(1)
                { first = move { read(xs) } }
            }
            var outer: move () -> Unit = move { val a = first()\nval b = second() }
            for (_ in next) {}
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let [deferred] = owned.deferred() else {
        panic!("coexisting captures of one lambda need instance-qualified transport");
    };
    assert_eq!(
        deferred.reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert_eq!(
        sources.slice(
            parsed
                .ast()
                .expressions()
                .get(deferred.expression())
                .unwrap()
                .span()
        ),
        Ok("move { read(xs) }")
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn same_lambda_shared_capture_paths_defer_until_instance_loan_ends() {
    use lang_frontend::ownership_checking::{ClosureCaptureMode, OwnershipDeferredReason};

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>, next: List<Int>) {
            val xs = listOf(1)
            var first: () -> Unit = {}
            var second: () -> Unit = {}
            for (_ in flags) {
                second = first
                { first = { read(xs) } }
            }
            var outer: move () -> Unit = move { val a = first()\nval b = second() }
            for (_ in next) {}
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(
        owned
            .captures()
            .iter()
            .filter(|capture| capture.mode() == ClosureCaptureMode::Shared)
            .count(),
        1,
        "the repeated inner lambda must borrow its source"
    );
    let [deferred] = owned.deferred() else {
        panic!("two inner instances cannot share one static loan-end target");
    };
    assert_eq!(
        deferred.reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert_eq!(
        sources.slice(
            parsed
                .ast()
                .expressions()
                .get(deferred.expression())
                .unwrap()
                .span()
        ),
        Ok("{ read(xs) }")
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn first_loop_forms_two_instances_before_outer_capture() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorId, ClosureCaptureSource,
        IterationCleanupAction as Action, IterationPhiBoundary, IterationPhiCaptureSlot,
        IterationPhiIncoming, IterationPhiIncomingKind,
    };

    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                selected(table, branches[choices[selector]], choices)
            }
        }
    }

    fn source_of(
        incoming: &IterationPhiIncoming,
        target: CleanupOwnerValueId,
    ) -> CleanupOwnerValueId {
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap();
        let [source] = binding.values() else {
            panic!("one checked root source")
        };
        source.source()
    }

    fn copy_roots(
        table: &CleanupConditions,
        incoming: &IterationPhiIncoming,
        targets: [CleanupOwnerValueId; 2],
        choices: &mut BTreeMap<CleanupSelectorId, usize>,
        owners: &mut BTreeMap<CleanupOwnerValueId, u32>,
    ) {
        let before_choices = choices.clone();
        let before_owners = owners.clone();
        assert!(selected(table, incoming.condition(), &before_choices));
        let mut selector_writes = Vec::new();
        let mut owner_writes = Vec::new();
        for target in targets {
            let binding = incoming
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap();
            let available = selected(table, binding.available_when(), &before_choices);
            selector_writes.push((binding.availability_selector(), usize::from(available)));
            let active = binding
                .values()
                .iter()
                .filter(|source| selected(table, source.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert_eq!(active.len(), usize::from(available));
            let active_roots = binding
                .root_sources()
                .iter()
                .filter(|source| selected(table, source.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert_eq!(active_roots.len(), active.len());
            if let [source] = active.as_slice() {
                assert_eq!(active_roots[0].source(), source.source());
            }
            for write in binding.selector_writes() {
                selector_writes.push((
                    write.target(),
                    usize::from(selected(table, write.condition(), &before_choices)),
                ));
            }
            if let Some(source) = active.first() {
                owner_writes.push((target, source.source(), before_owners[&source.source()]));
            }
        }
        choices.extend(selector_writes);
        for (_, source, _) in &owner_writes {
            owners
                .remove(source)
                .expect("phi consumes each root source");
        }
        for (target, _, handle) in owner_writes {
            owners.insert(target, handle);
        }
    }

    fn assert_capture_transport(
        table: &CleanupConditions,
        incoming: &IterationPhiIncoming,
        expected: (CleanupOwnerValueId, u32),
        layout: &[IterationPhiCaptureSlot],
        choices: &BTreeMap<CleanupSelectorId, usize>,
        owners: &BTreeMap<CleanupOwnerValueId, u32>,
        captures: &BTreeMap<(u32, usize), u32>,
    ) {
        let (target, expected_source) = expected;
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap();
        let origins = binding
            .origins()
            .iter()
            .filter(|origin| selected(table, origin.condition(), choices))
            .collect::<Vec<_>>();
        let [origin] = origins.as_slice() else {
            panic!("one selected lambda origin")
        };
        let environments = origin
            .environments()
            .iter()
            .filter(|environment| selected(table, environment.condition(), choices))
            .collect::<Vec<_>>();
        let [environment] = environments.as_slice() else {
            panic!("one selected environment instance")
        };
        let sources = environment
            .sources()
            .iter()
            .filter(|source| selected(table, source.input().condition(), choices))
            .collect::<Vec<_>>();
        let [source] = sources.as_slice() else {
            panic!("one captured source transported on this edge")
        };
        let CleanupCaptureValue::Environment { owner, slot, .. } = source
            .transport_value()
            .expect("formed capture slot is published")
        else {
            panic!("transport reads the formed environment")
        };
        let handle = owners[&environment.owner()];
        assert_eq!(owners[&owner], handle);
        let position = table.capture_slot_value(slot).unwrap().position();
        assert_eq!(captures[&(handle, position)], expected_source);
        let target_slot = source.capture_slot().expect("phi capture target exists");
        assert!(source.target().is_some());
        assert!(layout.iter().any(|slot| {
            slot.node() == origin.node()
                && slot.position() == position
                && slot.slot() == target_slot
        }));
        assert!(binding.capture_slots_to_clear().contains(&target_slot));
    }

    fn save_snapshot(
        table: &CleanupConditions,
        owner: CleanupOwnerValueId,
        choices: &mut BTreeMap<CleanupSelectorId, usize>,
        owners: &mut BTreeMap<CleanupOwnerValueId, u32>,
    ) -> u32 {
        let snapshot = table.owner_snapshot(owner).unwrap();
        let before = choices.clone();
        let writes = snapshot
            .copies()
            .iter()
            .filter(|copy| selected(table, copy.when(), &before))
            .map(|copy| (copy.target(), before[&copy.source()]))
            .collect::<Vec<_>>();
        choices.extend(writes);
        let inputs = snapshot
            .capture_inputs()
            .iter()
            .filter(|input| selected(table, input.condition(), choices))
            .collect::<Vec<_>>();
        let [input] = inputs.as_slice() else {
            panic!("one selected source for this owner snapshot: {inputs:?}")
        };
        let handle = owners
            .remove(&input.owner())
            .expect("snapshot moves its selected instance");
        owners.insert(owner, handle);
        handle
    }

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>) {
            var first: move () -> Unit = move {}
            var second: move () -> Unit = move {}
            for (_ in flags) {
                second = first
                val xs = listOf(1)
                { first = move { read(xs) } }
            }
            var outer: move () -> Unit = move { val a = first()\nval b = second() }
            val used = outer()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert_eq!(owned.iterations().len(), 1);
    let table = owned.cleanup_conditions();
    let closure = |text| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
            .unwrap()
    };
    let inner = closure("move { read(xs) }");
    let outer = closure("move { val a = first()\nval b = second() }");
    let created = |closure| {
        owned
            .cleanup_steps()
            .iter()
            .find_map(|(_, action)| match action {
                Action::CreateClosureOwner {
                    owner,
                    closure: actual,
                } if *actual == closure => Some(*owner),
                _ => None,
            })
            .unwrap()
    };
    let inner_owner = created(inner);
    let outer_owner = created(outer);
    let saved = |owner| {
        owned
            .cleanup_steps()
            .iter()
            .filter_map(|(_, action)| match action {
                Action::SaveClosureCapture {
                    owner: actual,
                    target,
                    input,
                } if *actual == owner => Some((*target, *input)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let inner_captures = saved(inner_owner);
    let [inner_capture] = inner_captures.as_slice() else {
        panic!("inner owns one source")
    };
    let CleanupCaptureValue::Owner(xs_owner) = inner_capture.1.value() else {
        panic!("inner captures the round-local owner")
    };
    assert!(
        matches!(table.owner_value(xs_owner), Some(CleanupOwnerValue::Expression { expression, .. })
        if sources.slice(parsed.ast().expressions().get(*expression).unwrap().span()) == Ok("listOf(1)"))
    );
    let outer_captures = saved(outer_owner);
    assert_eq!(outer_captures.len(), 2);
    let [first_source, second_source] = outer_captures.as_slice() else {
        unreachable!()
    };
    let (ClosureCaptureSource::Symbol(first), ClosureCaptureSource::Symbol(second)) =
        (first_source.1.source(), second_source.1.source())
    else {
        panic!("outer captures two bindings")
    };
    let plan = &owned.iterations()[0];
    for incoming in plan.closure_phi_incomings() {
        for binding in incoming.bindings() {
            let layout = plan
                .closure_phis()
                .iter()
                .find(|layout| layout.owner() == binding.target())
                .unwrap();
            assert!(
                binding
                    .root_sources()
                    .iter()
                    .all(|source| layout.root_nodes().contains(&source.node()))
            );
        }
    }
    let phi = |boundary, symbol| {
        plan.closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.symbol() == symbol)
            .unwrap()
            .owner()
    };
    let (header_first, header_second) = (
        phi(IterationPhiBoundary::Header, first),
        phi(IterationPhiBoundary::Header, second),
    );
    let (exit_first, exit_second) = (
        phi(IterationPhiBoundary::Exit, first),
        phi(IterationPhiBoundary::Exit, second),
    );
    let layout = |owner| {
        plan.closure_phis()
            .iter()
            .find(|phi| phi.owner() == owner)
            .unwrap()
            .capture_layout()
    };
    let edge = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    let (entry, backedge, exhaustion) = (
        edge(IterationPhiIncomingKind::Entry),
        edge(IterationPhiIncomingKind::Fallthrough),
        edge(IterationPhiIncomingKind::Exhaustion),
    );
    let mut owners = BTreeMap::new();
    let mut choices = BTreeMap::new();
    let mut next_instance = 0_u32;
    for (_, action) in owned.cleanup_steps() {
        if let Action::CreateClosureOwner { owner, .. } = action
            && [
                source_of(entry, header_first),
                source_of(entry, header_second),
            ]
            .contains(owner)
        {
            owners.insert(*owner, next_instance);
            next_instance += 1;
        }
    }
    assert_eq!(
        owners.len(),
        2,
        "both entry instances come from CreateClosureOwner"
    );
    copy_roots(
        table,
        entry,
        [header_first, header_second],
        &mut choices,
        &mut owners,
    );
    let mut captures = BTreeMap::new();
    let mut inner_instances = Vec::new();
    let mut source_instances = Vec::new();
    for round in 0..2 {
        let second_snapshot = source_of(backedge, header_second);
        let old_first = owners[&header_first];
        assert_eq!(
            save_snapshot(table, second_snapshot, &mut choices, &mut owners),
            old_first
        );
        let source_instance = next_instance;
        next_instance += 1;
        owners.insert(xs_owner, source_instance);
        source_instances.push(source_instance);
        let inner_instance = next_instance;
        next_instance += 1;
        owners.insert(inner_owner, inner_instance);
        inner_instances.push(inner_instance);
        assert!(selected(table, inner_capture.1.condition(), &choices));
        let moved_source = owners.remove(&xs_owner).unwrap();
        let inner_position = table
            .capture_slot_value(inner_capture.0)
            .unwrap()
            .position();
        assert!(
            captures
                .insert((inner_instance, inner_position), moved_source)
                .is_none()
        );
        let first_snapshot = source_of(backedge, header_first);
        assert_eq!(
            save_snapshot(table, first_snapshot, &mut choices, &mut owners),
            inner_instance
        );
        assert_capture_transport(
            table,
            backedge,
            (header_first, source_instances[round]),
            layout(header_first),
            &choices,
            &owners,
            &captures,
        );
        if round > 0 {
            assert_capture_transport(
                table,
                backedge,
                (header_second, source_instances[round - 1]),
                layout(header_second),
                &choices,
                &owners,
                &captures,
            );
        }
        copy_roots(
            table,
            backedge,
            [header_first, header_second],
            &mut choices,
            &mut owners,
        );
    }
    assert_eq!(owners[&header_first], inner_instances[1]);
    assert_eq!(owners[&header_second], inner_instances[0]);
    assert_capture_transport(
        table,
        exhaustion,
        (exit_first, source_instances[1]),
        layout(exit_first),
        &choices,
        &owners,
        &captures,
    );
    assert_capture_transport(
        table,
        exhaustion,
        (exit_second, source_instances[0]),
        layout(exit_second),
        &choices,
        &owners,
        &captures,
    );
    copy_roots(
        table,
        exhaustion,
        [exit_first, exit_second],
        &mut choices,
        &mut owners,
    );
    let outer_instance = next_instance;
    owners.insert(outer_owner, outer_instance);
    let outer_positions = outer_captures
        .iter()
        .map(|(target, _)| table.capture_slot_value(*target).unwrap().position())
        .collect::<Vec<_>>();
    for (target, input) in outer_captures {
        assert!(selected(table, input.condition(), &choices));
        let CleanupCaptureValue::Owner(source) = input.value() else {
            panic!("outer reads an exit root handle")
        };
        let handle = owners.remove(&source).unwrap();
        let position = table.capture_slot_value(target).unwrap().position();
        assert!(
            captures
                .insert((outer_instance, position), handle)
                .is_none()
        );
    }
    assert_eq!(captures[&(outer_instance, 0)], inner_instances[1]);
    assert_eq!(captures[&(outer_instance, 1)], inner_instances[0]);
    assert_ne!(inner_instances[0], inner_instances[1]);
    assert_eq!(captures[&(inner_instances[0], 0)], source_instances[0]);
    assert_eq!(captures[&(inner_instances[1], 0)], source_instances[1]);
    assert_ne!(source_instances[0], source_instances[1]);

    let outer_snapshot = owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            Action::SaveOwnerSnapshot { owner, value, .. } if *value == outer => Some(*owner),
            _ => None,
        })
        .expect("the outer binding saves its formed environment");
    assert_eq!(
        save_snapshot(table, outer_snapshot, &mut choices, &mut owners),
        outer_instance
    );
    assert!(owned.cleanup_steps().iter().any(|(_, action)| {
        matches!(action, Action::Drop(fact) if fact.owner() == Some(outer_snapshot))
    }));
    let root = owners
        .remove(&outer_snapshot)
        .expect("drop consumes the saved root");
    let captured_drops = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            Action::Drop(fact) => fact.instance_address().and_then(|address| {
                let address = table.instance_address(address)?;
                (address.root() == outer_snapshot).then_some((address, fact.capture_slot()?))
            }),
            _ => None,
        })
        .map(|(address, slot)| {
            let parent = address
                .capture_path()
                .iter()
                .fold(root, |instance, position| captures[&(instance, *position)]);
            captures[&(parent, table.capture_slot_value(slot).unwrap().position())]
        })
        .collect::<Vec<_>>();
    let mut pending = vec![(root, false)];
    let mut released = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    while let Some((instance, children_done)) = pending.pop() {
        if children_done {
            released.push(instance);
            continue;
        }
        assert!(
            visited.insert(instance),
            "one owned instance cannot be released twice"
        );
        pending.push((instance, true));
        let positions = if instance == outer_instance {
            outer_positions.clone()
        } else {
            captures
                .range((instance, 0)..=(instance, usize::MAX))
                .map(|(&(parent, position), _)| {
                    assert_eq!(parent, instance);
                    position
                })
                .collect()
        };
        for position in positions {
            let child = captures
                .remove(&(instance, position))
                .expect("release consumes the formed edge");
            pending.push((child, false));
        }
    }
    assert!(
        captures.is_empty(),
        "all formed capture edges were consumed"
    );
    assert_eq!(
        captured_drops,
        released[..4],
        "drop addresses select actual instances"
    );
    assert_eq!(
        released,
        [
            source_instances[0],
            inner_instances[0],
            source_instances[1],
            inner_instances[1],
            outer_instance,
        ],
        "the final drop releases each actual child before its parent, in reverse capture order"
    );
}

#[test]
fn nested_loop_phi_replays_distinct_source_instances_across_zero_and_two_rounds() {
    assert_nested_loop_phi_replays_distinct_source_instances("");
}

#[test]
fn nested_loop_phi_continue_replays_distinct_source_instances() {
    assert_nested_loop_phi_replays_distinct_source_instances("continue");
}

#[test]
fn nested_loop_phi_break_preserves_the_new_source_instance() {
    assert_nested_loop_phi_replays_distinct_source_instances("break");
}

#[test]
fn nested_phi_instance_path_uses_original_capture_position() {
    use lang_frontend::ownership_checking::{
        DropPoint, DropTarget, IterationCleanupAction as Action, IterationPhiBoundary,
        IterationPhiIncomingKind,
    };

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Int>) {
            var f: move () -> Unit = move {}
            for (_ in flags) {
                val marker = 1
                val xs = listOf(1)
                val g: () -> Unit = { read(xs) }
                f = move { val keep = marker\nval ignored = g() }
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let plan = &owned.iterations()[0];
    let body = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
        .unwrap();
    let environment = body
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .find(|environment| {
            environment
                .sources()
                .iter()
                .any(|source| !source.captured().is_empty())
        })
        .unwrap();
    assert!(environment.capture_path().is_empty());
    let nested = environment
        .sources()
        .iter()
        .flat_map(|source| source.captured())
        .flat_map(|origin| origin.environments())
        .next()
        .unwrap();
    assert_eq!(nested.instance_root(), environment.owner());
    assert_eq!(
        nested.capture_path(),
        &[1],
        "marker occupies capture position zero"
    );
    assert_eq!(nested.sources()[0].transport_value(), None);
    let (read_address, read_slot) = nested.sources()[0]
        .transport_read()
        .expect("nested phi source reads a formed environment slot");
    assert_eq!(Some(read_slot), nested.sources()[0].source_capture_slot());
    let read_address = owned
        .cleanup_conditions()
        .instance_address(read_address)
        .unwrap();
    assert_eq!(read_address.root(), nested.instance_root());
    assert_eq!(read_address.capture_path(), nested.capture_path());
    let exit = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Exit)
        .unwrap();
    let final_call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let addressed_ends = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| match action {
            Action::EndCaptureLoan {
                instance_address,
                capture_slot,
                ..
            } if *point == DropPoint::CallReturn(final_call) => {
                Some((*instance_address, *capture_slot))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(addressed_ends.len(), 1);
    let address = owned
        .cleanup_conditions()
        .instance_address(addressed_ends[0].0)
        .unwrap();
    assert_eq!(address.root(), exit.owner());
    assert_eq!(address.capture_path(), &[1]);
    let (last_loan_address, last_loan_slot) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(point, action)| match action {
            Action::TestLastCaptureLoan {
                instance_address,
                capture_slot,
                ..
            } if *point == DropPoint::CallReturn(final_call) => {
                Some((*instance_address, *capture_slot))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!((last_loan_address, Some(last_loan_slot)), addressed_ends[0]);
    assert_eq!(
        owned
            .cleanup_conditions()
            .capture_slot_value(last_loan_slot)
            .unwrap()
            .position(),
        0
    );
    let retained_drop = owned
        .drops()
        .iter()
        .find(|fact| {
            fact.point() == DropPoint::CallReturn(final_call)
                && matches!(fact.target(), DropTarget::RetainedSource(_))
        })
        .unwrap();
    assert_eq!(retained_drop.instance_address(), Some(last_loan_address));
    assert_eq!(retained_drop.capture_slot(), Some(last_loan_slot));
    let captured_drop = owned
        .cleanup_steps()
        .iter()
        .find_map(|(point, action)| match action {
            Action::Drop(fact)
                if *point == DropPoint::CallReturn(final_call)
                    && matches!(fact.target(), DropTarget::Captured { owner, .. } if owner == exit.owner()) =>
            {
                Some(*fact)
            }
            _ => None,
        })
        .unwrap();
    let parent = owned
        .cleanup_conditions()
        .instance_address(captured_drop.instance_address().unwrap())
        .unwrap();
    assert_eq!(parent.root(), exit.owner());
    assert!(parent.capture_path().is_empty());
    assert_eq!(
        owned
            .cleanup_conditions()
            .capture_slot_value(captured_drop.capture_slot().unwrap())
            .unwrap()
            .position(),
        1
    );
}

#[test]
fn nested_conditional_capture_defers_until_instance_presence_is_saved() {
    assert_nested_conditional_capture_defers("{ read(xs) }");
}

#[test]
fn nested_optional_capture_defers_until_instance_presence_is_saved() {
    assert_nested_conditional_capture_defers("{}");
}

fn assert_nested_conditional_capture_defers(initial: &str) {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let source = "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: move () -> Unit = move {}
            for (flag in flags) {
                val xs = listOf(1)
                val ys = listOf(2)
                var g: () -> Unit = __INITIAL__
                if (flag) { g = { read(ys) } }
                f = move { val used = g() }
            }
            val used = f()
        }"
    .replace("__INITIAL__", initial);
    let (_, _, owned) = checked(&source);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1, "{:?}", owned.deferred());
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn nested_known_and_opaque_capture_defers_until_full_origin_is_saved() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own cb: move () -> Unit, flags: List<Boolean>) {
            var f: move () -> Unit = move {}
            for (flag in flags) {
                val xs = listOf(1)
                var g: move () -> Unit = cb
                if (flag) { g = move { read(xs) } }
                f = move { val used = g() }
                break
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1, "{:?}", owned.deferred());
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn nested_known_and_call_result_capture_defers_until_full_origin_is_saved() {
    use lang_frontend::ownership_checking::OwnershipDeferredReason;

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun make(): move () -> Unit = move {}\nfun run(flags: List<Boolean>) {
            var f: move () -> Unit = move {}
            for (flag in flags) {
                val xs = listOf(1)
                val g: move () -> Unit = if (flag) (move { read(xs) }) else (make())
                val next: move () -> Unit = move { val used = g() }
                f = next
                break
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert_eq!(owned.deferred().len(), 1, "{:?}", owned.deferred());
    assert_eq!(
        owned.deferred()[0].reason(),
        OwnershipDeferredReason::AmbiguousClosureInstanceTransport
    );
    assert!(owned.iterations().is_empty());
    assert!(owned.cleanup_steps().is_empty());
    assert!(owned.drops().is_empty());
    assert!(owned.loan_ends().is_empty());
}

#[test]
fn nested_single_lambda_capture_versions_keep_the_formation_choice() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupSelectorId,
        IterationCleanupAction, IterationPhiIncomingKind,
    };

    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                selected(table, branches[choices[selector]], choices)
            }
        }
    }

    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: move () -> Unit = move {}
            for (flag in flags) {
                var xs = listOf(0)
                if (flag) { xs = listOf(1) } else { xs = listOf(2) }
                val g: move () -> Unit = move { read(xs) }
                val next: move () -> Unit = move { val used = g() }
                f = next
            }
            val used = f()
        }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let table = owned.cleanup_conditions();
    let snapshots = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(_, action)| match action {
            IterationCleanupAction::SaveOwnerSnapshot { owner, .. } => {
                Some(table.owner_snapshot(*owner).unwrap())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        snapshots.len(),
        3,
        "g, next and f each save the capture choice"
    );
    let copies = snapshots
        .iter()
        .map(|snapshot| *snapshot.copies().first().expect("saved capture choice"))
        .collect::<Vec<_>>();
    assert!(
        snapshots
            .iter()
            .all(|snapshot| snapshot.copies().len() == 1)
    );
    assert_eq!(copies[0].target(), copies[1].source());
    assert_eq!(copies[1].target(), copies[2].source());
    let incoming = owned.iterations()[0]
        .closure_phi_incomings()
        .iter()
        .find(|incoming| incoming.kind() == IterationPhiIncomingKind::Fallthrough)
        .expect("fallthrough transports f to the next header");
    let inner = incoming
        .bindings()
        .iter()
        .flat_map(|binding| binding.origins())
        .flat_map(|origin| origin.environments())
        .flat_map(|environment| environment.sources())
        .flat_map(|source| source.captured())
        .flat_map(|origin| origin.environments())
        .find(|environment| environment.capture_path() == [0])
        .expect("f captures the formed g instance");
    let versions = inner.sources();
    assert!(
        table
            .owner_snapshot(inner.instance_root())
            .is_some_and(|snapshot| snapshot
                .copies()
                .iter()
                .any(|copy| copy.target() == copies[2].target())),
        "nested condition must be saved on its carried root instance"
    );
    assert_eq!(versions.len(), 2);
    assert!(versions[0].source_capture_slot().is_some());
    assert_eq!(
        versions[0].source_capture_slot(),
        versions[1].source_capture_slot()
    );
    assert_ne!(versions[0].value(), versions[1].value());
    for version in versions {
        let Some(CleanupCondition::Choice { selector, .. }) =
            table.get(version.input().condition())
        else {
            panic!("nested source version needs a saved choice")
        };
        assert_eq!(*selector, copies[2].target());
    }
    for arm in 0..2 {
        let mut choices = BTreeMap::from([(copies[0].source(), arm)]);
        for copy in &copies {
            let before = choices.clone();
            if selected(table, copy.when(), &before) {
                choices.insert(copy.target(), before[&copy.source()]);
            }
        }
        let initial = versions
            .iter()
            .filter(|version| selected(table, version.input().condition(), &choices))
            .collect::<Vec<_>>();
        assert_eq!(initial.len(), 1);
        choices.insert(copies[0].source(), 1 - arm);
        let active = versions
            .iter()
            .filter(|version| selected(table, version.input().condition(), &choices))
            .collect::<Vec<_>>();
        assert_eq!(
            active.len(),
            1,
            "later flag changes cannot replace g's source"
        );
        assert_eq!(active[0].value(), initial[0].value());
    }
}

fn assert_nested_loop_phi_replays_distinct_source_instances(transfer: &str) {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureValue, CleanupCondition, CleanupConditionId, CleanupConditions,
        CleanupOwnerValue, CleanupOwnerValueId, CleanupSelectorId,
        IterationCleanupAction as Action, IterationPhiBoundary, IterationPhiIncoming,
        IterationPhiIncomingKind, IterationPhiIncomingOrigin,
    };

    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                let branch = choices.get(selector).unwrap_or_else(|| {
                    panic!(
                        "selector {selector:?} is uninitialized: {:?}; initialized: {choices:?}",
                        table.selector(*selector),
                    )
                });
                selected(table, branches[*branch], choices)
            }
        }
    }

    fn replay_origin(
        table: &CleanupConditions,
        origin: &IterationPhiIncomingOrigin,
        choices: &BTreeMap<CleanupSelectorId, usize>,
        owners: &BTreeMap<CleanupOwnerValueId, u32>,
        owner_writes: &mut Vec<(CleanupOwnerValueId, u32)>,
    ) -> Option<u32> {
        let present = selected(table, origin.condition(), choices);
        if !present {
            return None;
        }
        let environments = origin
            .environments()
            .iter()
            .filter(|environment| selected(table, environment.condition(), choices))
            .collect::<Vec<_>>();
        assert_eq!(environments.len(), 1, "one actual captured environment");
        let environment = environments[0];
        for source in environment.sources() {
            if !selected(table, source.input().condition(), choices) {
                continue;
            }
            if let Some(target) = source.target() {
                let CleanupCaptureValue::Owner(actual) = source.value() else {
                    panic!("owned source slot must read an actual owner");
                };
                owner_writes.push((target, owners[&actual]));
            }
            for nested in source.captured() {
                replay_origin(table, nested, choices, owners, owner_writes);
            }
        }
        Some(owners[&environment.owner()])
    }

    fn replay(
        table: &CleanupConditions,
        incoming: &IterationPhiIncoming,
        target: CleanupOwnerValueId,
        choices: &mut BTreeMap<CleanupSelectorId, usize>,
        owners: &mut BTreeMap<CleanupOwnerValueId, u32>,
    ) {
        let before_choices = choices.clone();
        let before_owners = owners.clone();
        assert!(selected(table, incoming.condition(), &before_choices));
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap();
        let available = selected(table, binding.available_when(), &before_choices);
        let values = binding
            .values()
            .iter()
            .filter(|value| selected(table, value.condition(), &before_choices))
            .collect::<Vec<_>>();
        assert_eq!(values.len(), usize::from(available));
        let mut choice_writes = vec![(binding.availability_selector(), usize::from(available))];
        for write in binding.selector_writes() {
            choice_writes.push((
                write.target(),
                usize::from(selected(table, write.condition(), &before_choices)),
            ));
        }
        let mut owner_writes = Vec::new();
        let selected_origins = binding
            .origins()
            .iter()
            .filter_map(|origin| {
                replay_origin(
                    table,
                    origin,
                    &before_choices,
                    &before_owners,
                    &mut owner_writes,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            selected_origins,
            values
                .iter()
                .map(|value| before_owners[&value.source()])
                .collect::<Vec<_>>()
        );
        if let Some(value) = values.first() {
            owner_writes.push((target, before_owners[&value.source()]));
        }
        choices.extend(choice_writes);
        owners.extend(owner_writes);
    }

    let (sources, parsed, owned) = checked(&
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {\nvar f: move () -> Unit = move {}\nfor (_ in flags) { val xs = listOf(1)\nval g: () -> Unit = { read(xs) }\nf = move { g() }\n__TRANSFER__ }\nval used = f() }"
            .replace("__TRANSFER__", transfer),
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let f = names
        .symbols()
        .iter()
        .find(|symbol| sources.slice(symbol.span()).unwrap() == "f")
        .unwrap()
        .id();
    let plan = &owned.iterations()[0];
    let phi = |boundary| {
        plan.closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.symbol() == f)
            .unwrap()
    };
    let header = phi(IterationPhiBoundary::Header);
    let exit = phi(IterationPhiBoundary::Exit);
    let new_origin = |binding: &lang_frontend::ownership_checking::IterationClosurePhiBinding| {
        binding.origins().iter().position(|origin| {
            sources
                .slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(origin.closure())
                        .unwrap()
                        .span(),
                )
                .unwrap()
                == "move { g() }"
        })
    };
    let header_new = new_origin(header).map(|index| &header.origins()[index]);
    let exit_new = &exit.origins()[new_origin(exit).unwrap()];
    let exit_g = &exit.origins()[exit_new.sources()[0].captured()[0]];
    let edge = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    let entry = edge(IterationPhiIncomingKind::Entry);
    let body_edge = plan
        .closure_phi_incomings()
        .iter()
        .find(|incoming| match incoming.kind() {
            IterationPhiIncomingKind::Fallthrough => transfer.is_empty(),
            IterationPhiIncomingKind::Continue(_) => transfer == "continue",
            IterationPhiIncomingKind::Break(_) => transfer == "break",
            _ => false,
        })
        .unwrap();
    let exhaustion = edge(IterationPhiIncomingKind::Exhaustion);
    let body_target = if transfer == "break" {
        exit.owner()
    } else {
        header.owner()
    };
    let body_origin = if transfer == "break" {
        exit_new
    } else {
        header_new.expect("a continuing edge carries the body lambda into the header")
    };
    let body_binding = if transfer == "break" { exit } else { header };
    let body_edge_new = body_edge
        .bindings()
        .iter()
        .find(|binding| binding.target() == body_target)
        .unwrap()
        .origins()
        .iter()
        .find(|origin| origin.target() == body_origin.selector())
        .unwrap();
    let created = |closure| {
        owned
            .cleanup_steps()
            .iter()
            .find_map(|(_, action)| match action {
                Action::CreateClosureOwner { owner, closure: id } if *id == closure => Some(*owner),
                _ => None,
            })
            .expect("the body must create this closure environment")
    };
    let fresh_f = body_edge_new.environments()[0].owner();
    assert_eq!(body_edge_new.environments()[0].instance_root(), fresh_f);
    assert!(body_edge_new.environments()[0].capture_path().is_empty());
    let direct_source = &body_edge_new.environments()[0].sources()[0];
    let fresh_g = match direct_source.value() {
        CleanupCaptureValue::Owner(owner) => owner,
        other => panic!("g must have an actual environment owner: {other:?}"),
    };
    assert_ne!(fresh_f, body_target);
    assert_ne!(direct_source.target(), Some(fresh_g));
    let nested = &body_edge_new.environments()[0].sources()[0].captured()[0];
    assert_eq!(nested.environments()[0].owner(), fresh_g);
    assert_eq!(nested.environments()[0].instance_root(), fresh_f);
    assert_eq!(nested.environments()[0].capture_path(), &[0]);
    assert_eq!(
        nested.environments()[0].sources()[0].transport_value(),
        None
    );
    if transfer != "break" {
        let forwarded = exhaustion
            .bindings()
            .iter()
            .find(|binding| binding.target() == exit.owner())
            .unwrap()
            .origins()
            .iter()
            .find(|origin| origin.target() == exit_new.selector())
            .unwrap();
        let environment = &forwarded.environments()[0];
        assert_eq!(environment.instance_root(), header.owner());
        assert!(environment.capture_path().is_empty());
        let nested = &environment.sources()[0].captured()[0].environments()[0];
        assert_eq!(nested.instance_root(), header.owner());
        assert_eq!(nested.capture_path(), &[0]);
        assert_eq!(nested.sources()[0].transport_value(), None);
    }
    let table = owned.cleanup_conditions();
    let created_g =
        created(body_binding.origins()[body_origin.sources()[0].captured()[0]].closure());
    let fresh_xs = match table.owner_value(created_g) {
        Some(CleanupOwnerValue::Closure { inputs, .. }) => match inputs[0].value() {
            CleanupCaptureValue::Owner(owner) => owner,
            other => panic!("g must capture this round's xs owner: {other:?}"),
        },
        other => panic!("g must be a created closure owner: {other:?}"),
    };
    let xs_incoming = &nested.environments()[0].sources()[0];
    assert_eq!(xs_incoming.value(), CleanupCaptureValue::Owner(fresh_xs));
    assert_ne!(xs_incoming.target(), Some(fresh_xs));
    let initial = entry
        .bindings()
        .iter()
        .find(|binding| binding.target() == header.owner())
        .unwrap()
        .values()[0]
        .source();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let xs_capture = exit_g.sources()[0].source();
    let copy_snapshot = |owner: CleanupOwnerValueId,
                         value: lang_frontend::ast::ExpressionId,
                         point: lang_frontend::ownership_checking::DropPoint,
                         choices: &mut BTreeMap<CleanupSelectorId, usize>| {
        assert_eq!(
            point,
            lang_frontend::ownership_checking::DropPoint::AfterExpression(value)
        );
        let snapshot = table.owner_snapshot(owner).unwrap();
        assert_eq!(snapshot.value(), value);
        let before = choices.clone();
        for copy in snapshot.copies() {
            if selected(table, copy.when(), &before) {
                let value = *before.get(&copy.source()).unwrap_or_else(|| {
                    panic!(
                        "snapshot copy {copy:?} reads absent source {:?}; choices {before:?}",
                        table.selector(copy.source())
                    )
                });
                choices.insert(copy.target(), value);
            }
        }
    };
    let release = |point: lang_frontend::ownership_checking::DropPoint,
                   environment_slot: CleanupOwnerValueId,
                   source_slot: CleanupOwnerValueId,
                   choices: &mut BTreeMap<CleanupSelectorId, usize>,
                   owners: &BTreeMap<CleanupOwnerValueId, u32>,
                   captures: &BTreeMap<(u32, usize), u32>,
                   expected: Option<u32>| {
        let mut ended = 0;
        let mut tested = 0;
        let mut dropped = Vec::new();
        let mut loan_location = None;
        for (at, action) in owned.cleanup_steps() {
            if *at != point {
                continue;
            }
            match action {
                Action::SaveOwnerSnapshot {
                    owner,
                    value,
                    condition,
                } if condition.is_none_or(|guard| selected(table, guard, choices)) => {
                    assert_eq!(*value, body_origin.closure());
                    copy_snapshot(*owner, *value, point, choices);
                }
                Action::EndCaptureLoan {
                    owner,
                    instance_address,
                    capture_slot,
                    condition,
                    closure,
                    source,
                    value,
                    ..
                } if *source == xs_capture
                    && condition.is_none_or(|guard| selected(table, guard, choices)) =>
                {
                    assert_eq!(*owner, environment_slot);
                    assert_eq!(*closure, exit_g.closure());
                    assert_eq!(*value, CleanupCaptureValue::Owner(source_slot));
                    let address = table.instance_address(*instance_address).unwrap();
                    let slot = table.capture_slot_value(capture_slot.unwrap()).unwrap();
                    assert_eq!(address.capture_path(), &[0]);
                    assert_eq!(slot.position(), 0);
                    let mut instance = owners[&address.root()];
                    for &position in address.capture_path() {
                        instance = captures[&(instance, position)];
                    }
                    assert_eq!(Some(captures[&(instance, slot.position())]), expected);
                    loan_location = Some((*instance_address, capture_slot.unwrap()));
                    ended += 1;
                }
                Action::TestLastCaptureLoan {
                    owner,
                    instance_address,
                    capture_slot,
                    selector,
                    condition,
                    ..
                } if *owner == source_slot
                    && condition.is_none_or(|guard| selected(table, guard, choices)) =>
                {
                    assert_eq!(ended, 1, "test only after ending this capture loan");
                    assert_eq!(loan_location, Some((*instance_address, *capture_slot)));
                    choices.insert(*selector, 1);
                    tested += 1;
                }
                Action::Drop(fact)
                    if fact.owner() == Some(source_slot)
                        && fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, choices)) =>
                {
                    assert_eq!(tested, 1, "drop only after the last-loan test");
                    assert_eq!(
                        Some((
                            fact.instance_address().unwrap(),
                            fact.capture_slot().unwrap()
                        )),
                        loan_location
                    );
                    dropped.push(owners[&source_slot]);
                }
                _ => {}
            }
        }
        assert_eq!(ended, usize::from(expected.is_some()));
        assert_eq!(tested, usize::from(expected.is_some()));
        assert_eq!(dropped, expected.into_iter().collect::<Vec<_>>());
    };
    let mut choices = BTreeMap::new();
    let mut owners = BTreeMap::from([(initial, 0_u32)]);
    let mut captures = BTreeMap::new();
    replay(table, entry, header.owner(), &mut choices, &mut owners);
    let zero_choices = choices.clone();
    let zero_owners = owners.clone();
    replay(table, exhaustion, exit.owner(), &mut choices, &mut owners);
    assert_eq!(owners[&exit.owner()], 0, "zero rounds keep the entry f");
    assert_eq!(choices[&exit_g.selector()], 0, "zero rounds clear absent g");
    release(
        lang_frontend::ownership_checking::DropPoint::CallReturn(call),
        exit_new.sources()[0].owner(),
        exit_g.sources()[0].owner(),
        &mut choices,
        &owners,
        &captures,
        None,
    );
    choices = zero_choices;
    owners = zero_owners;
    let rounds = if transfer == "break" { 1 } else { 2 };
    for round in 1..=rounds {
        owners.insert(fresh_f, round * 10);
        owners.insert(fresh_g, round * 10 + 1);
        owners.insert(fresh_xs, round * 10 + 2);
        captures.insert((round * 10, 0), round * 10 + 1);
        captures.insert((round * 10 + 1, 0), round * 10 + 2);
        for (at, action) in owned.cleanup_steps() {
            if *at
                == lang_frontend::ownership_checking::DropPoint::AfterExpression(exit_g.closure())
                && let Action::SaveOwnerSnapshot {
                    owner,
                    value,
                    condition,
                } = action
                && condition.is_none_or(|guard| selected(table, guard, &choices))
            {
                assert_eq!(*value, exit_g.closure());
                copy_snapshot(*owner, *value, *at, &mut choices);
            }
        }
        if let Some(header_new) = header_new {
            release(
                lang_frontend::ownership_checking::DropPoint::AfterExpression(
                    body_origin.closure(),
                ),
                header_new.sources()[0].owner(),
                header.origins()[header_new.sources()[0].captured()[0]].sources()[0].owner(),
                &mut choices,
                &owners,
                &captures,
                (round > 1).then_some((round - 1) * 10 + 2),
            );
        }
        replay(table, body_edge, body_target, &mut choices, &mut owners);
        assert_eq!(owners[&body_target], round * 10);
        assert_eq!(owners[&body_origin.sources()[0].owner()], round * 10 + 1);
        assert_eq!(
            owners[&body_binding.origins()[body_origin.sources()[0].captured()[0]].sources()[0]
                .owner()],
            round * 10 + 2
        );
    }
    if transfer != "break" {
        replay(table, exhaustion, exit.owner(), &mut choices, &mut owners);
    }
    assert_eq!(owners[&exit.owner()], rounds * 10);
    assert_eq!(owners[&exit_new.sources()[0].owner()], rounds * 10 + 1);
    assert_eq!(owners[&exit_g.sources()[0].owner()], rounds * 10 + 2);
    release(
        lang_frontend::ownership_checking::DropPoint::CallReturn(call),
        exit_new.sources()[0].owner(),
        exit_g.sources()[0].owner(),
        &mut choices,
        &owners,
        &captures,
        Some(rounds * 10 + 2),
    );
}

#[test]
fn returned_moved_closure_cannot_hide_an_iteration_borrowed_capture() {
    let (_, _, owned) = checked(
        "fun read(n: Int) {}\nfun run(xs: List<Int>): move () -> Unit { for (n in xs) {\nval g: () -> Unit = { read(n) }\nval f: move () -> Unit = move { g() }\nreturn f }\nreturn move {} }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0137");
    assert!(owned.iterations().is_empty());
}

#[test]
fn moved_closure_keeps_the_captured_value_before_source_reassignment() {
    let (_, _, owned) = checked(
        "fun read(n: Int) {}\nfun empty(): () -> Unit = ({})\nfun deliver(own f: move () -> Unit) {}\nfun run(n: Int, flag: Boolean) { var g: () -> Unit = empty()\nval f: move () -> Unit = move { g() }\nif (flag) { g = ({ read(n) }) } else { g = ({ read(n) }) }\nval sent = deliver(f)\nval used = g() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn lambda_tail_cannot_return_borrowed_move_only_source() {
    let (_, _, owned) =
        checked("fun run() { val f: (List<Int>) -> List<Int> = { xs -> for (_ in xs) {}\nxs } }");
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0133");
    assert!(owned.iterations().is_empty());
}

#[test]
fn indexed_temporary_source_keeps_the_backing_container_owner() {
    use lang_frontend::{
        ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget},
        parser::{Expression, Statement},
    };
    for jump in ["", "break", "continue", "return"] {
        let (_, parsed, owned) = checked(&format!(
            "fun run() {{ for (_ in listOf(listOf(1))[0]) {{ {jump} }} }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{jump}: {:?}",
            owned.diagnostics()
        );
        let (statement, source) = parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| match node.payload() {
                Statement::For { source, .. } => Some((id, *source)),
                _ => None,
            })
            .unwrap();
        let Expression::Index { receiver, .. } =
            parsed.ast().expressions().get(source).unwrap().payload()
        else {
            panic!("index source")
        };
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), DropTarget::Temporary(_)))
            .collect::<Vec<_>>();
        assert!(
            drops
                .iter()
                .all(|fact| fact.target() == DropTarget::Temporary(*receiver)),
            "{jump}: {drops:?}"
        );
        let owner = drops[0]
            .owner()
            .expect("the backing container has an owner identity");
        assert!(drops.iter().all(|fact| fact.owner() == Some(owner)));
        assert!(matches!(
            owned.cleanup_conditions().owner_value(owner),
            Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == *receiver
        ));
        assert_eq!(
            drops
                .iter()
                .filter(|fact| fact.point() == DropPoint::LoopExit(statement))
                .count(),
            1
        );
        assert_eq!(
            drops.len(),
            if matches!(jump, "break" | "return") {
                2
            } else {
                1
            }
        );
        assert_eq!(owned.iterations().len(), 1);
    }
}

#[test]
fn lambda_tail_cannot_return_a_borrowed_closure() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run() { val f: (List<Int>) -> () -> Unit = { xs -> for (_ in xs) {}\nval inner = { read(xs) }\ninner } }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0137");
    assert!(owned.iterations().is_empty());
}

#[test]
fn source_index_exit_cleans_only_the_evaluated_backing_owner() {
    use lang_frontend::{
        ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget},
        parser::{Expression, Statement},
    };
    let (_, parsed, owned) = checked(
        "fun run(flag: Boolean) { for (_ in listOf(listOf(1))[if (flag) return else 0]) {} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let jump = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| matches!(node.payload(), Expression::Return { .. }).then_some(id))
        .unwrap();
    let owner = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            Expression::Index { receiver, .. } => Some(*receiver),
            _ => None,
        })
        .unwrap();
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.point() == DropPoint::ControlTransfer(jump))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].target(), DropTarget::Temporary(owner));
    let value = drops[0].owner().expect("pending backing owner identity");
    assert!(matches!(
        owned.cleanup_conditions().owner_value(value),
        Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == owner
    ));
    let statement = parsed
        .ast()
        .statements()
        .iter()
        .find_map(|(id, node)| matches!(node.payload(), Statement::For { .. }).then_some(id))
        .unwrap();
    let normal = owned
        .drops()
        .iter()
        .find(|fact| {
            fact.point() == DropPoint::LoopExit(statement)
                && fact.target() == DropTarget::Temporary(owner)
        })
        .unwrap();
    assert_eq!(
        normal.owner(),
        Some(value),
        "both exits own the same backing definition"
    );
    let (_, _, abort) = checked("fun run() { for (_ in listOf(listOf(1))[error(\"stop\")]) {} }");
    assert!(abort.diagnostics().is_empty());
    assert!(abort.drops().is_empty());
    assert!(abort.iterations().is_empty());
}

#[test]
fn source_borrow_conflicts_with_an_earlier_exclusive_argument() {
    let (_, _, owned) = checked(
        "fun use(inout xs: List<Int>, n: Int) {}\nfun run(inout xs: List<Int>, flag: Boolean) { use(&xs, if (flag) { for (_ in xs) {}\n0 } else 0) }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
    assert!(owned.iterations().is_empty());
}

#[test]
fn multi_index_source_keeps_the_original_temporary_owner() {
    use lang_frontend::{
        ownership_checking::{CleanupOwnerValue, DropTarget, LoanTarget},
        parser::{Expression, Statement},
    };
    for index in ["0", "if (flag) return else 0"] {
        let (sources, parsed, owned) = checked(&format!(
            "fun run(flag: Boolean) {{ for (_ in listOf(listOf(listOf(1)))[0][{index}]) {{}} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let owner = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                (sources.slice(node.span()).unwrap() == "listOf(listOf(listOf(1)))"
                    && matches!(node.payload(), Expression::Call { .. }))
                .then_some(id)
            })
            .unwrap();
        let statement = parsed
            .ast()
            .statements()
            .iter()
            .find_map(|(id, node)| matches!(node.payload(), Statement::For { .. }).then_some(id))
            .unwrap();
        assert_eq!(
            owned.iteration(statement).unwrap().source(),
            &LoanTarget::Temporary(owner)
        );
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Temporary(owner))
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), if index == "0" { 1 } else { 2 });
        let definition = drops[0].owner();
        for fact in drops {
            let value = fact
                .owner()
                .expect("every backing drop identifies its owner");
            assert_eq!(
                Some(value),
                definition,
                "all exits share one backing definition"
            );
            assert!(matches!(
                owned.cleanup_conditions().owner_value(value),
                Some(CleanupOwnerValue::Expression { expression, .. }) if *expression == owner
            ));
        }
    }
}

#[test]
fn returning_control_tails_cannot_hide_borrowed_closures() {
    for tail in [
        "if (flag) inner else inner",
        "when { flag -> inner\nelse -> inner }",
        "if (flag) { val local = { read(xs) }\nlocal } else inner",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(flag: Boolean) {{ val f: (List<Int>) -> () -> Unit = {{ xs -> for (_ in xs) {{}}\nval inner = {{ read(xs) }}\n{tail} }} }}"
        ));
        assert!(!owned.diagnostics().is_empty(), "{tail}");
        assert!(
            owned
                .diagnostics()
                .iter()
                .all(|d| d.code().to_string() == "L0137"),
            "{tail}: {:?}",
            owned.diagnostics()
        );
        assert!(owned.iterations().is_empty());
    }
}

#[test]
fn multi_index_named_source_blocks_owner_move_and_keeps_it_alive() {
    let (_, _, owned) = checked(
        "fun consume(own xs: List<List<List<Int>>>) {}\nfun run(own xs: List<List<List<Int>>>) { for (_ in xs[0][0]) { consume(xs)\nbreak } }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0135");
    assert!(owned.iterations().is_empty());
}

#[test]
fn local_control_results_do_not_escape_and_returned_calls_do_not_return_the_closure() {
    for tail in [
        "val local = if (flag) inner else inner\nval result = local()",
        "if (flag) inner() else inner()",
        "when { flag -> inner()\nelse -> inner() }",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(flag: Boolean) {{ val f: (List<Int>) -> Unit = {{ xs -> for (_ in xs) {{}}\nval inner = {{ read(xs) }}\n{tail} }} }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{tail}: {:?}",
            owned.diagnostics()
        );
        assert_eq!(owned.iterations().len(), 1);
    }
}

#[test]
fn multi_index_places_preserve_prefix_aliases_and_named_owner_cleanup() {
    use lang_frontend::ownership_checking::{
        DropPoint, DropTarget, ElementIndexIdentity, LoanTarget,
    };
    let (_, _, owned) = checked(
        "fun run(own xs: List<List<List<Int>>>, i: Int) { for (_ in xs[0]) {}\nfor (_ in xs[0][0]) {}\nfor (_ in xs[0][1]) {}\nfor (_ in xs[i][0]) {} }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let plans = owned.iterations();
    let places = plans
        .iter()
        .map(|plan| match plan.source() {
            LoanTarget::Place(place) => place,
            LoanTarget::Temporary(_) | LoanTarget::This(_) => panic!("named source lost its place"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        places[1].elements(),
        &[
            ElementIndexIdentity::Known(0),
            ElementIndexIdentity::Known(0)
        ]
    );
    assert!(places[0].overlaps(places[1]));
    assert!(!places[1].overlaps(places[2]));
    assert!(places[1].overlaps(places[3]));
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == DropTarget::Named(places[0].root()))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1);
    assert!(matches!(drops[0].point(), DropPoint::LoopExit(_)));
}

#[test]
fn control_result_aliases_preserve_each_borrowed_closure_origin() {
    for initializer in [
        "if (flag) first else first",
        "if (flag) first else second",
        "when { flag -> first\nelse -> second }",
        "if (flag) { val branch = { read(xs) }\nbranch } else second",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun leak(xs: List<Int>, flag: Boolean): () -> Unit {{ for (_ in xs) {{}}\nval first = {{ read(xs) }}\nval second = {{ read(xs) }}\nval alias = {initializer}\nreturn alias }}"
        ));
        assert!(!owned.diagnostics().is_empty(), "{initializer}");
        assert!(
            owned
                .diagnostics()
                .iter()
                .all(|d| d.code().to_string() == "L0137"),
            "{initializer}: {:?}",
            owned.diagnostics()
        );
        assert!(owned.iterations().is_empty());
    }
}

#[test]
fn control_result_capture_loans_protect_each_possible_source_until_alias_last_use() {
    for consumed in ["xs", "ys"] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun consume(own xs: List<Int>) {{}}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) {{ val alias: () -> Unit = if (flag) ({{ read(xs) }}) else ({{ read(ys) }})\nval consumed = consume({consumed})\nval result = alias() }}"
        ));
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135"),
            "{consumed}: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn control_join_releases_closures_dead_on_every_successor() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean) { val f = { read(xs) }\nif (flag) { val ignored = f() }\nval consumed = consume(xs) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn control_argument_keeps_pending_callee_and_borrowed_argument_capture_loans() {
    for call in [
        "f(if (flag) 0 else 1, take(xs))",
        "invoke(f, if (flag) 0 else 1, take(xs))",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun take(own xs: List<Int>): Int {{ return 0 }}\nfun invoke(f: (Int, Int) -> Unit, a: Int, b: Int) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ val f: (Int, Int) -> Unit = {{ a, b -> read(xs) }}\nval result = {call} }}"
        ));
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0135"),
            "{call}: {:?}",
            owned.diagnostics()
        );
        let completed = call.replace("take(xs)", "0");
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun take(own xs: List<Int>): Int {{ return 0 }}\nfun invoke(f: (Int, Int) -> Unit, a: Int, b: Int) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ val f: (Int, Int) -> Unit = {{ a, b -> read(xs) }}\nval result = {completed}\nval consumed = take(xs) }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{completed}: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn captured_owner_and_callee_survive_branch_cleanup_until_call_returns() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    for call in ["f(0)", "f(if (flag) 0 else 1)"] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ for (_ in listOf(0)) {{ val f: (Int) -> Unit = {{ n -> read(xs) }}\nif (flag) {{}} else {{}}\nval used = {call}\nbreak }} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let source = owned
            .captures()
            .iter()
            .find_map(|capture| match capture.source() {
                ClosureCaptureSource::Symbol(symbol) => Some(symbol),
                ClosureCaptureSource::This => None,
            })
            .unwrap();
        let call_id = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == call).then_some(id))
            .unwrap();
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.target() == DropTarget::Named(source))
            .collect::<Vec<_>>();
        // The zero-iteration exhaustion path has its own owner cleanup.
        assert_eq!(drops.len(), 2, "{call}: {drops:?}");
        assert_eq!(
            drops
                .iter()
                .filter(|fact| fact.point() == DropPoint::CallReturn(call_id))
                .count(),
            1,
            "{call}: {drops:?}"
        );
        assert_eq!(
            drops
                .iter()
                .filter(|fact| matches!(fact.point(), DropPoint::LoopExit(_)))
                .count(),
            1,
            "{call}: {drops:?}"
        );
    }
}

#[test]
fn pending_callee_scope_cleanup_orders_local_capture_before_its_source() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    for transfer in ["return", "break", "continue"] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(flag: Boolean) {{ for (_ in listOf(0)) {{ val xs = listOf(1)\nval f: (Int) -> Unit = {{ n -> read(xs) }}\nval used = f(if (flag) {transfer} else 0) }} }}"
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let source = owned
            .captures()
            .iter()
            .find_map(|capture| match capture.source() {
                ClosureCaptureSource::Symbol(symbol) => Some(symbol),
                ClosureCaptureSource::This => None,
            })
            .unwrap();
        let exit = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == transfer).then_some(id))
            .unwrap();
        let drops = owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == DropPoint::ControlTransfer(exit))
            .collect::<Vec<_>>();
        assert_eq!(
            drops
                .iter()
                .filter(|fact| fact.target() == DropTarget::Named(source))
                .count(),
            1,
            "{transfer}: {drops:?}"
        );
        let source_index = drops
            .iter()
            .position(|fact| fact.target() == DropTarget::Named(source))
            .unwrap();
        assert!(
            source_index > 0,
            "closure owner must drop before its captured local"
        );
    }
}

#[test]
fn conditional_closure_drops_only_the_selected_source_after_call() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, ClosureCaptureSource, DropPoint, DropTarget,
    };
    for (control, call_text) in [
        ("if (flag) ({ read(xs) }) else ({ read(ys) })", "chosen()"),
        (
            "if (flag) ({ read(xs) }) else ({ read(ys) })",
            "invoke(chosen)",
        ),
        (
            "when { flag -> ({ read(xs) })\nelse -> ({ read(ys) }) }",
            "chosen()",
        ),
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun invoke(f: () -> Unit) {{ val used = f() }}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) {{ val f: () -> Unit = {control}\nval chosen = f\nval used = {call_text} }}",
        ));
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == call_text).then_some(id))
            .unwrap();
        for (name, selected) in [("xs", 0), ("ys", 1)] {
            let symbol = owned
                .captures()
                .iter()
                .find_map(|capture| {
                    if sources.slice(capture.reference_span()).unwrap() != name {
                        return None;
                    }
                    match capture.source() {
                        ClosureCaptureSource::Symbol(symbol) => Some(symbol),
                        _ => None,
                    }
                })
                .unwrap();
            let drops = owned
                .drops()
                .iter()
                .filter(|fact| fact.target() == DropTarget::Named(symbol))
                .collect::<Vec<_>>();
            let late = drops
                .iter()
                .filter(|fact| fact.point() == DropPoint::CallReturn(call))
                .collect::<Vec<_>>();
            assert_eq!(late.len(), 1, "{name}: {drops:?}");
            let guard = late[0]
                .condition()
                .expect("only the selected source survives the join");
            let Some(CleanupCondition::Choice { branches, .. }) =
                owned.cleanup_conditions().get(guard)
            else {
                panic!("expected saved branch choice")
            };
            assert_eq!(
                owned.cleanup_conditions().get(branches[selected]),
                Some(&CleanupCondition::Always)
            );
            assert_eq!(
                owned.cleanup_conditions().get(branches[1 - selected]),
                Some(&CleanupCondition::Never)
            );
            assert!(drops.iter().any(|fact| matches!(fact.point(), DropPoint::BranchExit { branch, .. } if branch == 1-selected)), "unselected source must release at its branch: {drops:?}");
        }
    }
}

#[test]
fn nested_closure_cleanup_does_not_read_an_unexecuted_inner_choice() {
    assert_selected_closure_source_survives(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, own zs: List<Int>, outer: Boolean, inner: Boolean) { val f: () -> Unit = if (outer) (if (inner) ({ read(xs) }) else ({ read(ys) })) else ({ read(zs) })\nval used = f() }",
        &[
            ("xs", &[(0, 0), (1, 0)]),
            ("ys", &[(0, 0), (1, 1)]),
            ("zs", &[(0, 1)]),
        ],
    );
}

#[test]
fn branch_local_closure_chain_snapshots_remain_guarded_after_the_join() {
    assert_selected_closure_source_survives(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { val f: () -> Unit = if (flag) { val inner: () -> Unit = { read(xs) }\nval g: () -> Unit = { inner() }\ng } else ({ read(ys) })\nval used = f() }",
        &[("xs", &[(0, 0)]), ("ys", &[(0, 1)])],
    );
}

#[test]
fn copying_an_earlier_choice_inside_a_later_branch_keeps_the_branch_guard_first() {
    assert_selected_closure_source_survives(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean, pick: Boolean) { val previous: () -> Unit = if (flag) ({ read(xs) }) else ({ read(ys) })\nval f: () -> Unit = if (pick) { val g: () -> Unit = { val observed = previous() }\ng } else previous\nval used = f() }",
        &[
            ("xs", &[(0, 0), (1, 0)]),
            ("xs", &[(0, 0), (1, 1)]),
            ("ys", &[(0, 1), (1, 0)]),
            ("ys", &[(0, 1), (1, 1)]),
        ],
    );
}

fn assert_selected_closure_source_survives(text: &str, cases: &[(&str, &[(usize, usize)])]) {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, ClosureCaptureSource, DropPoint,
        DropTarget,
    };
    fn enabled(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &[(usize, usize)],
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                let branch = choices
                    .iter()
                    .find(|(id, _)| *id == selector.index())
                    .expect("cleanup may only read an initialized selector")
                    .1;
                enabled(table, branches[branch], choices)
            }
        }
    }
    let (sources, parsed, owned) = checked(text);
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let expression = |text| {
        parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == text).then_some(id))
            .unwrap()
    };
    let call = expression("f()");
    let mut controls = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| match node.payload() {
            lang_frontend::parser::Expression::If { .. } => Some((node.span().start(), id)),
            _ => None,
        })
        .collect::<Vec<_>>();
    controls.sort_by_key(|(start, _)| *start);
    for &(selected, choices) in cases {
        let copied = owned
            .cleanup_steps()
            .iter()
            .flat_map(|(_, action)| match action {
                lang_frontend::ownership_checking::IterationCleanupAction::SaveOwnerSnapshot {
                    owner,
                    ..
                } => owned
                    .cleanup_conditions()
                    .owner_snapshot(*owner)
                    .unwrap()
                    .copies()
                    .iter()
                    .map(|copy| copy.target().index())
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut choices = choices
            .iter()
            .copied()
            .map(|(control, arm)| {
                let selectors = owned
                    .cleanup_conditions()
                    .selectors()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, selector)| {
                        (selector.control() == Some(controls[control].1)
                            && !copied.contains(&index))
                        .then_some(index)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    selectors.len(),
                    1,
                    "this fixture has one direct selector per control"
                );
                (selectors[0], arm)
            })
            .collect::<Vec<_>>();
        for (_, action) in owned.cleanup_steps() {
            let lang_frontend::ownership_checking::IterationCleanupAction::SaveOwnerSnapshot {
                owner,
                condition,
                ..
            } = action
            else {
                continue;
            };
            if condition.is_some_and(|guard| !enabled(owned.cleanup_conditions(), guard, &choices))
            {
                continue;
            }
            let writes = owned
                .cleanup_conditions()
                .owner_snapshot(*owner)
                .unwrap()
                .copies()
                .iter()
                .filter(|copy| enabled(owned.cleanup_conditions(), copy.when(), &choices))
                .map(|copy| {
                    (
                        copy.target().index(),
                        choices
                            .iter()
                            .find(|(id, _)| *id == copy.source().index())
                            .expect("only initialized selectors can be copied")
                            .1,
                    )
                })
                .collect::<Vec<_>>();
            choices.extend(writes);
        }
        for capture in owned.captures() {
            let ClosureCaptureSource::Symbol(symbol) = capture.source() else {
                continue;
            };
            let name = sources.slice(capture.reference_span()).unwrap();
            if !["xs", "ys", "zs"].contains(&name) {
                continue;
            }
            let late = owned
                .drops()
                .iter()
                .filter(|fact| {
                    fact.target() == DropTarget::Named(symbol)
                        && fact.point() == DropPoint::CallReturn(call)
                })
                .collect::<Vec<_>>();
            assert!(!late.is_empty(), "{name}: {:?}", owned.drops());
            assert_eq!(
                late.iter()
                    .filter(|fact| fact.condition().is_none_or(|guard| enabled(
                        owned.cleanup_conditions(),
                        guard,
                        &choices
                    )))
                    .count(),
                usize::from(name == selected)
            );
        }
    }
}

#[test]
fn pending_conditional_closure_keeps_sources_until_call_or_argument_transfer() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    for template in [
        "(if (flag) first else second)(if (early) return else 0)",
        "invoke(if (flag) ({ read(xs) }) else ({ read(ys) }), if (early) return else 0)",
    ] {
        for argument in ["if (early) return else 0", "if (early) 1 else 0"] {
            let call = template.replace("if (early) return else 0", argument);
            let declarations = if call.starts_with("(if") {
                "val first: (Int) -> Unit = { n -> read(xs) }\nval second: (Int) -> Unit = { n -> read(ys) }\n"
            } else {
                ""
            };
            let (sources, parsed, owned) = checked(&format!(
                "fun read(xs: List<Int>) {{}}\nfun invoke(f: () -> Unit, n: Int) {{ val used = f() }}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean, early: Boolean) {{ {declarations}val used = {call} }}"
            ));
            assert!(
                owned.diagnostics().is_empty(),
                "{call}: {:?}",
                owned.diagnostics()
            );
            let expression = |text: &str| {
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .find_map(|(id, node)| {
                        (sources.slice(node.span()).unwrap() == text).then_some(id)
                    })
                    .unwrap()
            };
            for capture in owned.captures() {
                let ClosureCaptureSource::Symbol(symbol) = capture.source() else {
                    continue;
                };
                let mut points = vec![DropPoint::CallReturn(expression(&call))];
                if argument.contains("return") {
                    points.push(DropPoint::ControlTransfer(expression("return")));
                }
                for point in points {
                    let drops = owned
                        .drops()
                        .iter()
                        .filter(|fact| fact.point() == point)
                        .collect::<Vec<_>>();
                    let source = drops
                        .iter()
                        .position(|fact| fact.target() == DropTarget::Named(symbol))
                        .expect("selected source must survive to call completion or transfer");
                    assert!(
                        drops[source].condition().is_some(),
                        "only the selected capture remains at {point:?}"
                    );
                    assert!(
                        drops[..source]
                            .iter()
                            .any(|fact| matches!(fact.target(), DropTarget::Temporary(_))),
                        "temporary closure must drop before its captured source: {drops:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn pending_owned_closure_drops_slots_only_before_value_delivery() {
    use lang_frontend::ownership_checking::{DropPoint, DropTarget};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun invoke(own f: move () -> Unit, n: Int) { val used = f() }\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean, early: Boolean) { val used = invoke(if (flag) (move { read(xs) }) else (move { read(ys) }), if (early) return else 0) }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let transfer = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "return").then_some(id))
        .unwrap();
    let slots = owned
        .drops()
        .iter()
        .filter(|fact| matches!(fact.target(), DropTarget::Captured { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        slots.len(),
        2,
        "callee owns slots after successful delivery: {slots:?}"
    );
    for slot in slots {
        assert_eq!(slot.point(), DropPoint::ControlTransfer(transfer));
        assert!(
            slot.condition().is_some(),
            "only the selected environment has this slot"
        );
        let at_exit = owned
            .drops()
            .iter()
            .filter(|fact| fact.point() == slot.point())
            .collect::<Vec<_>>();
        let slot_index = at_exit.iter().position(|fact| *fact == slot).unwrap();
        assert!(
            at_exit[slot_index + 1..]
                .iter()
                .any(|fact| matches!(fact.target(), DropTarget::Temporary(_))),
            "environment must outlive its slots"
        );
    }
}

#[test]
fn pending_temporary_capture_ends_between_environment_and_source_drop() {
    use lang_frontend::ownership_checking::{
        ClosureCaptureSource, DropTarget, IterationCleanupAction as Action, IterationExitKind,
    };
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun invoke(f: () -> Unit, n: Int) { val used = f() }\nfun run(early: Boolean) { for (_ in listOf(0)) { val xs = listOf(1)\nval used = invoke(({ read(xs) }), if (early) return else 0) } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let capture = owned
        .captures()
        .iter()
        .find(|capture| matches!(capture.source(), ClosureCaptureSource::Symbol(_)))
        .unwrap();
    let ClosureCaptureSource::Symbol(source) = capture.source() else {
        unreachable!()
    };
    let exit = owned
        .iterations()
        .iter()
        .flat_map(|plan| plan.exits())
        .find(|exit| matches!(exit.kind(), IterationExitKind::Return(_)))
        .unwrap();
    let actions = exit.actions();
    let end = actions.iter().position(|action| matches!(action, Action::EndCaptureLoan { source: candidate, .. } if *candidate == capture.source())).unwrap();
    assert!(
        matches!(actions[end-1], Action::Drop(fact) if matches!(fact.target(), DropTarget::Temporary(_))),
        "{actions:?}"
    );
    assert!(
        matches!(actions[end+1], Action::Drop(fact) if fact.target() == DropTarget::Named(source)),
        "{actions:?}"
    );
}

#[test]
fn closure_reassignment_preserves_new_capture_and_releases_old_capture() {
    for tail in [
        "val releasedOld = consume(xs)\nval used = f()\nval releasedNew = consume(ys)",
        "val used = f()\nval releasedOld = consume(xs)\nval releasedNew = consume(ys)",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun consume(own xs: List<Int>) {{}}\nfun run(own xs: List<Int>, own ys: List<Int>) {{ var f: () -> Unit = {{ read(xs) }}\nf = ({{ read(ys) }})\n{tail} }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{tail}: {:?}",
            owned.diagnostics()
        );
    }
    for rhs in [
        "({ read(xs) })",
        "if (flag) ({ read(xs) }) else ({ read(ys) })",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(xs: List<Int>, ys: List<Int>, flag: Boolean): () -> Unit {{ var f: () -> Unit = {{}}\nf = {rhs}\nreturn f }}"
        ));
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0137"),
            "assigned closure cannot hide its borrowed origin: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn assigned_closure_retains_its_source_until_the_new_value_is_dropped() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>) { var f: () -> Unit = {}\nf = ({ read(xs) })\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let source = owned
        .captures()
        .iter()
        .find_map(|capture| match capture.source() {
            ClosureCaptureSource::Symbol(symbol) => Some(symbol),
            _ => None,
        })
        .unwrap();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let drops = owned
        .drops()
        .iter()
        .filter(|fact| fact.target() == DropTarget::Named(source))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 1, "{drops:?}");
    assert_eq!(
        drops[0].point(),
        DropPoint::CallReturn(call),
        "assignment must transfer captures to the new binding"
    );
}

#[test]
fn closure_reassignment_tracks_aliases_unused_values_and_field_escape() {
    for update in [
        "f = g\nval used = f()",
        "f = if (flag) g else g\nval used = f()",
        "f = g",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun consume(own xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ var f: () -> Unit = {{}}\nval g: () -> Unit = {{ read(xs) }}\n{update}\nval released = consume(xs) }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{update}: {:?}",
            owned.diagnostics()
        );
    }
    let (_, _, owned) = checked(
        "class Sink(var callback: () -> Unit) {}\nfun read(xs: List<Int>) {}\nfun store(sink: Sink, xs: List<Int>, flag: Boolean) { sink.callback = if (flag) ({ read(xs) }) else ({ read(xs) }) }",
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0137"),
        "conditional field RHS must not hide borrowed captures: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn closure_reassignment_keeps_old_capture_until_the_entire_rhs_finishes() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { var f: () -> Unit = { read(xs) }\nf = if (flag) { val old = f()\nval invalid = consume(xs)\nval replacement: () -> Unit = { read(ys) }\nreplacement } else ({ read(ys) })\nval used = f() }",
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0135"),
        "RHS has not delivered a replacement, so the old environment still borrows xs: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn replacement_cleanup_waits_for_rhs_completion_or_leaving_scope() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    for tail in [
        "val replacement: () -> Unit = { read(ys) }\nreplacement",
        "return",
    ] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) {{ var f: () -> Unit = {{ read(xs) }}\nf = if (flag) {{ val observed = f()\n{tail} }} else ({{ read(ys) }})\nval used = f() }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{tail}: {:?}",
            owned.diagnostics()
        );
        let source = owned
            .captures()
            .iter()
            .find_map(|capture| match capture.source() {
                ClosureCaptureSource::Symbol(symbol)
                    if sources.slice(capture.reference_span()).unwrap() == "xs" =>
                {
                    Some(symbol)
                }
                _ => None,
            })
            .unwrap();
        let rhs = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                matches!(node.payload(), lang_frontend::parser::Expression::If { .. }).then_some(id)
            })
            .unwrap();
        let old_call = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| sources.slice(node.span()).unwrap() == "f()")
            .min_by_key(|(_, node)| node.span().start())
            .unwrap()
            .0;
        let old = owned
            .drops()
            .iter()
            .filter(|fact| {
                sources.slice(fact.value_origin()).unwrap() == "f"
                    && matches!(fact.target(), DropTarget::Named(_))
            })
            .collect::<Vec<_>>();
        assert!(
            old.iter()
                .all(|fact| fact.point() != DropPoint::CallReturn(old_call)),
            "old environment is still a replacement obligation: {old:?}"
        );
        assert!(
            old.iter()
                .any(|fact| fact.point() == DropPoint::AfterExpression(rhs)),
            "normal RHS completion must dispose the replaced environment: {old:?}"
        );
        assert!(
            owned
                .drops()
                .iter()
                .any(|fact| fact.target() == DropTarget::Named(source)
                    && fact.point() == DropPoint::AfterExpression(rhs))
        );
        if tail == "return" {
            let exit = parsed
                .ast()
                .expressions()
                .iter()
                .find_map(|(id, node)| {
                    matches!(
                        node.payload(),
                        lang_frontend::parser::Expression::Return { .. }
                    )
                    .then_some(id)
                })
                .unwrap();
            let drops = owned
                .drops()
                .iter()
                .filter(|fact| fact.point() == DropPoint::ControlTransfer(exit))
                .collect::<Vec<_>>();
            let source_drop = drops
                .iter()
                .position(|fact| fact.target() == DropTarget::Named(source))
                .unwrap();
            assert!(
                drops[..source_drop].iter().any(|fact| old.contains(fact)),
                "old environment must release before source on return: {drops:?}"
            );
        }
    }
}

#[test]
fn replacement_self_transfer_never_drops_the_transferred_old_value_twice() {
    use lang_frontend::ownership_checking::{ClosureCaptureSource, DropPoint, DropTarget};
    for rhs in ["f", "if (flag) f else f"] {
        let (sources, parsed, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ var f: () -> Unit = {{ read(xs) }}\nf = {rhs}\nval used = f() }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{rhs}: {:?}",
            owned.diagnostics()
        );
        let source = owned
            .captures()
            .iter()
            .find_map(|capture| match capture.source() {
                ClosureCaptureSource::Symbol(symbol) => Some(symbol),
                _ => None,
            })
            .unwrap();
        let call = parsed
            .ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
            .unwrap();
        let named = owned
            .drops()
            .iter()
            .filter(|fact| matches!(fact.target(), DropTarget::Named(_)))
            .collect::<Vec<_>>();
        assert_eq!(
            named.len(),
            2,
            "one transferred environment and one source: {named:?}"
        );
        assert!(
            named
                .iter()
                .all(|fact| fact.point() == DropPoint::CallReturn(call))
        );
        assert_eq!(named[1].target(), DropTarget::Named(source));
    }
}

#[test]
fn inner_loop_exit_does_not_end_an_outer_replacement_capture() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { var f: () -> Unit = { read(xs) }\nf = if (flag) { loop { val observed = f()\nbreak }\nval invalid = consume(xs)\nval replacement: () -> Unit = { read(ys) }\nreplacement } else ({ read(ys) })\nval used = f() }",
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0135"),
        "inner break does not finish the outer RHS: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn replacement_loop_exit_releases_the_environment_that_leaves_the_loop() {
    for (header, jump) in [
        ("loop", "break"),
        ("while (flag)", "continue"),
        ("for (_ in listOf(0))", "continue"),
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun consume(own xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ {header} {{ var f: () -> Unit = {{ read(xs) }}\nf = if (flag) {{ val old = f()\n{jump} }} else ({{}})\nval used = f()\nbreak }}\nval released = consume(xs) }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{header}/{jump}: exited local environment cannot keep borrowing xs: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn nested_replacement_can_dispose_the_old_value_before_outer_rhs_finishes() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { var f: () -> Unit = { read(xs) }\nf = if (flag) { f = ({ read(ys) })\nval released = consume(xs)\nval observed = f()\nval replacement: () -> Unit = { read(ys) }\nreplacement } else f\nval used = f() }",
    );
    assert!(
        owned.diagnostics().is_empty(),
        "an explicit nested replacement ended the old xs capture: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn loop_exit_preserves_a_closure_held_by_another_live_environment() {
    for boundary in ["", "loop { break }\n"] {
        let (_, _, owned) = checked(&format!(
            "fun read(xs: List<Int>) {{}}\nfun consume(own xs: List<Int>) {{}}\nfun run(own xs: List<Int>) {{ val f: () -> Unit = {{ read(xs) }}\nval g: () -> Unit = {{ val used = f() }}\n{boundary}val invalid = consume(xs)\nval used = g() }}"
        ));
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0135"),
            "{boundary:?}: g still borrows f and f still borrows xs: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn loop_exit_releases_a_dead_capture_chain_from_the_outer_environment_inward() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean) { loop { val f: () -> Unit = { read(xs) }\nvar g: () -> Unit = { val used = f() }\ng = if (flag) { val observed = g()\nbreak } else ({})\nval used = g()\nbreak }\nval released = consume(xs) }",
    );
    assert!(
        owned.diagnostics().is_empty(),
        "both g and its captured f leave scope, so xs must become available: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn inout_assignment_cannot_export_an_iteration_borrowed_closure() {
    for value in [
        "({ read(n) })",
        "if (flag) ({ read(n) }) else ({})",
        "if (flag) { val captured: () -> Unit = { read(n) }\ncaptured } else ({})",
        "when (flag) { true -> { val captured: () -> Unit = { read(n) }\ncaptured }\nelse -> ({}) }",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(n: Int) {{}}\nfun run(inout slot: () -> Unit, xs: List<Int>, flag: Boolean) {{ for (n in xs) {{ slot = {value}\nbreak }} }}"
        ));
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0137"),
            "{value}: the caller's slot outlives this iteration capture: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.drops().is_empty(),
            "invalid cleanup must not be published"
        );
        assert!(
            owned.captures().is_empty(),
            "invalid captures must not be published"
        );
    }
}

#[test]
fn inout_assignment_accepts_owned_and_capture_free_environments() {
    for (ty, value) in [
        ("() -> Unit", "({})"),
        ("() -> Unit", "if (flag) ({}) else ({})"),
        ("move () -> Unit", "move { read(n) }"),
        ("move () -> Unit", "if (flag) move { read(n) } else move {}"),
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(n: Int) {{}}\nfun run(inout slot: {ty}, xs: List<Int>, flag: Boolean) {{ for (n in xs) {{ slot = {value}\nbreak }} }}"
        ));
        assert!(
            owned.diagnostics().is_empty(),
            "{value}: owned snapshots and capture-free values can outlive this call: {:?}",
            owned.diagnostics()
        );
    }
}

#[test]
fn value_argument_cannot_hide_an_iteration_capture_in_a_branch_local() {
    let (_, _, owned) = checked(
        "fun read(n: Int) {}\nfun deliver(own callback: () -> Unit) {}\nfun run(xs: List<Int>, flag: Boolean) { for (n in xs) { val sent = deliver(if (flag) { val captured: () -> Unit = { read(n) }\ncaptured } else ({}) )\nbreak } }",
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0137"),
        "Value delivery must check the branch's actual environment: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn construction_cannot_hide_an_iteration_capture_in_a_branch_local() {
    let (_, _, owned) = checked(
        "class Stored(val callback: () -> Unit)\nfun read(n: Int) {}\nfun run(xs: List<Int>, flag: Boolean) { for (n in xs) { val stored = Stored(if (flag) { val captured: () -> Unit = { read(n) }\ncaptured } else ({}) )\nbreak } }",
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0137"),
        "field initialization must check the branch's actual environment: {:?}",
        owned.diagnostics()
    );
}

#[test]
fn branch_local_owned_snapshots_can_be_delivered_or_stored() {
    let (_, _, owned) = checked(
        "class Stored(val callback: move () -> Unit)\nfun read(n: Int) {}\nfun deliver(own callback: move () -> Unit) {}\nfun run(xs: List<Int>, flag: Boolean) { for (n in xs) { val sent = deliver(if (flag) { val captured: move () -> Unit = move { read(n) }\ncaptured } else move {})\nval stored = Stored(when (flag) { true -> { val captured: move () -> Unit = move { read(n) }\ncaptured }\nelse -> move {} })\nbreak } }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
}

#[test]
fn element_storage_cannot_keep_an_iteration_capture_past_the_element_access() {
    for value in [
        "({ read(n) })",
        "if (flag) { val captured: () -> Unit = { read(n) }\ncaptured } else ({})",
        "when (flag) { true -> { val captured: () -> Unit = { read(n) }\ncaptured }\nelse -> ({}) }",
    ] {
        let (_, _, owned) = checked(&format!(
            "fun read(n: Int) {{}}\nfun run(inout slots: Array<() -> Unit>, xs: List<Int>, flag: Boolean) {{ for (n in xs) {{ slots[0] = {value}\nbreak }} }}"
        ));
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().to_string() == "L0137"),
            "{value}: an outer container must not retain the iteration's borrowed environment: {:?}",
            owned.diagnostics()
        );
        assert!(
            owned.drops().is_empty(),
            "invalid cleanup must not be published"
        );
        assert!(
            owned.captures().is_empty(),
            "invalid captures must not be published"
        );
    }
}

#[test]
fn element_storage_accepts_owned_snapshots_and_capture_free_values() {
    for container in ["Array", "MutableList"] {
        for (ty, value) in [
            ("() -> Unit", "({})"),
            ("move () -> Unit", "move { read(n) }"),
            (
                "move () -> Unit",
                "if (flag) { val captured: move () -> Unit = move { read(n) }\ncaptured } else move {}",
            ),
        ] {
            let (_, _, owned) = checked(&format!(
                "fun read(n: Int) {{}}\nfun run(inout slots: {container}<{ty}>, xs: List<Int>, flag: Boolean) {{ for (n in xs) {{ slots[0] = {value}\nbreak }} }}"
            ));
            assert!(
                owned.diagnostics().is_empty(),
                "{container}/{value}: storage owns its environment: {:?}",
                owned.diagnostics()
            );
        }
    }
}

#[test]
fn loop_carried_conditional_closure_preserves_its_sources_until_the_exit_call() {
    assert_loop_carried_sources_survive_exit("");
}

#[test]
fn loop_carried_conditional_closure_preserves_sources_on_continue() {
    assert_loop_carried_sources_survive_exit("continue");
}

#[test]
fn loop_carried_conditional_closure_preserves_sources_on_break() {
    assert_loop_carried_sources_survive_exit("break");
}

fn assert_loop_carried_sources_survive_exit(transfer: &str) {
    use lang_frontend::ownership_checking::{
        CleanupOwnerValue, ClosureCaptureSource, DropPoint, DropTarget,
    };

    let (sources, parsed, owned) = checked(&format!(
        "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>) {{ var f: () -> Unit = {{}}\nfor (flag in flags) {{ f = if (flag) ({{ read(xs) }}) else ({{ read(ys) }})\n{transfer} }}\nval used = f() }}",
    ));
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    for name in ["xs", "ys"] {
        let symbol = owned
            .captures()
            .iter()
            .find_map(|capture| {
                if sources.slice(capture.reference_span()).unwrap() != name {
                    return None;
                }
                match capture.source() {
                    ClosureCaptureSource::Symbol(symbol) => Some(symbol),
                    ClosureCaptureSource::This => None,
                }
            })
            .unwrap();
        assert!(
            owned.drops().iter().any(|drop| {
                if drop.point() != DropPoint::CallReturn(call) || drop.condition().is_none() {
                    return false;
                }
                match drop.target() {
                    DropTarget::Named(owner) => owner == symbol,
                    DropTarget::RetainedSource(ClosureCaptureSource::Symbol(owner)) => {
                        owner == symbol
                            && matches!(
                                drop.owner().and_then(|owner| owned
                                    .cleanup_conditions()
                                    .owner_value(owner)),
                                Some(CleanupOwnerValue::IterationPhiSourceOwner { .. })
                            )
                    }
                    _ => false,
                }
            }),
            "{name}: the final selected source must remain conditionally owned until f() returns: {:?}",
            owned.drops()
        );
    }
}

#[test]
fn loop_carried_branch_choices_release_the_previous_source_after_two_rounds() {
    use std::collections::BTreeMap;

    use lang_frontend::ownership_checking::{
        CleanupCaptureSlotId, CleanupCaptureValue, CleanupCondition, CleanupConditionId,
        CleanupConditions, CleanupOwnerValue, CleanupOwnerValueId, CleanupSelection,
        CleanupSelectorId, CleanupSelectorSource, ClosureCaptureSource, DropPoint, DropTarget,
        IterationCleanupAction as Action, IterationClosurePhiBinding, IterationPhiBoundary,
        IterationPhiIncoming, IterationPhiIncomingKind,
    };

    fn selected(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        choices: &BTreeMap<CleanupSelectorId, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => {
                selected(table, branches[choices[selector]], choices)
            }
        }
    }

    #[derive(Default)]
    struct FormedCaptures {
        slots: BTreeMap<(u32, CleanupCaptureSlotId), u32>,
        layouts: BTreeMap<u32, CleanupOwnerValueId>,
    }

    fn replay(
        table: &CleanupConditions,
        incoming: &IterationPhiIncoming,
        layouts: &[IterationClosurePhiBinding],
        choices: &mut BTreeMap<CleanupSelectorId, usize>,
        owners: &mut BTreeMap<CleanupOwnerValueId, u32>,
        phi_capture_slots: &mut BTreeMap<CleanupCaptureSlotId, u32>,
        formed: &FormedCaptures,
    ) {
        let before_choices = choices.clone();
        let before_owners = owners.clone();
        let before_phi_capture_slots = phi_capture_slots.clone();
        assert!(selected(table, incoming.condition(), &before_choices));
        let mut choice_writes = Vec::new();
        let mut owner_writes = Vec::new();
        let mut owner_clears = Vec::new();
        let mut capture_writes = Vec::new();
        let mut capture_clears = Vec::new();
        for binding in incoming.bindings() {
            owner_clears.push(binding.target());
            capture_clears.extend_from_slice(binding.capture_slots_to_clear());
            let layout = layouts
                .iter()
                .find(|layout| layout.owner() == binding.target())
                .expect("each incoming binding has a preallocated layout");
            for origin in layout.origins() {
                for source in origin.sources() {
                    owner_clears.push(source.owner());
                }
            }
            let available = selected(table, binding.available_when(), &before_choices);
            choice_writes.push((binding.availability_selector(), usize::from(available)));
            let values = binding
                .values()
                .iter()
                .filter(|value| selected(table, value.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert_eq!(values.len(), usize::from(available));
            let roots = binding
                .root_sources()
                .iter()
                .filter(|source| selected(table, source.condition(), &before_choices))
                .collect::<Vec<_>>();
            assert!(
                roots
                    .iter()
                    .all(|source| layout.root_nodes().contains(&source.node()))
            );
            if !layout.root_nodes().is_empty() {
                assert_eq!(roots.len(), values.len());
                if let [value] = values.as_slice() {
                    assert_eq!(roots[0].source(), value.source());
                }
            }
            let actual = values.first().map(|value| before_owners[&value.source()]);
            if let Some(actual) = actual {
                owner_writes.push((binding.target(), actual));
            }
            for write in binding.selector_writes() {
                choice_writes.push((
                    write.target(),
                    usize::from(selected(table, write.condition(), &before_choices)),
                ));
            }
            let mut origins = Vec::new();
            for origin in binding.origins() {
                let present = selected(table, origin.condition(), &before_choices);
                if !present {
                    continue;
                }
                let environments = origin
                    .environments()
                    .iter()
                    .filter(|environment| selected(table, environment.condition(), &before_choices))
                    .collect::<Vec<_>>();
                assert_eq!(environments.len(), 1);
                let environment = environments[0];
                origins.push(before_owners[&environment.owner()]);
                for source in environment.sources() {
                    if !selected(table, source.input().condition(), &before_choices) {
                        continue;
                    }
                    if let Some(target) = source.target() {
                        let CleanupCaptureValue::Owner(value) = source.value() else {
                            panic!("carried source must read an actual owner");
                        };
                        let CleanupCaptureValue::Environment {
                            owner: source_environment,
                            slot: source_slot,
                            ..
                        } = source
                            .transport_value()
                            .expect("tracked source must publish its transport location")
                        else {
                            panic!("phi must read the formed environment capture slot");
                        };
                        assert_eq!(source_environment, environment.owner());
                        let source_layout = table.capture_slot_value(source_slot).unwrap();
                        assert_eq!(source_layout.source(), source.input().source());
                        let captured = match table.owner_value(source_layout.environment()).unwrap()
                        {
                            CleanupOwnerValue::Closure { .. } => {
                                assert_eq!(
                                    source_layout.environment(),
                                    formed.layouts[&before_owners[&source_environment]]
                                );
                                formed.slots[&(before_owners[&source_environment], source_slot)]
                            }
                            CleanupOwnerValue::IterationPhi { .. } => {
                                assert_eq!(source_layout.environment(), source_environment);
                                before_phi_capture_slots[&source_slot]
                            }
                            value => panic!("unexpected transport layout: {value:?}"),
                        };
                        assert_eq!(captured, before_owners[&value]);
                        owner_writes.push((target, captured));
                        capture_writes.push((
                            source
                                .capture_slot()
                                .expect("tracked source has a phi capture slot"),
                            captured,
                        ));
                    }
                }
            }
            if !binding.origins().is_empty() {
                assert_eq!(origins, actual.into_iter().collect::<Vec<_>>());
            }
        }
        choices.extend(choice_writes);
        for owner in owner_clears {
            owners.remove(&owner);
        }
        owners.extend(owner_writes);
        for slot in capture_clears {
            phi_capture_slots.remove(&slot);
        }
        phi_capture_slots.extend(capture_writes);
    }

    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flags: List<Boolean>) { var f: () -> Unit = {}\nfor (flag in flags) { f = if (flag) ({ read(xs) }) else ({ read(ys) }) }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    let names = resolve_names(&sources, &parsed, &standard_environments().0).unwrap();
    let symbol = |name| {
        names
            .symbols()
            .iter()
            .find(|symbol| sources.slice(symbol.span()).unwrap() == name)
            .unwrap()
            .id()
    };
    let captured_symbol = |name| {
        owned
            .captures()
            .iter()
            .find_map(|capture| {
                (sources.slice(capture.reference_span()).unwrap() == name).then(|| {
                    match capture.source() {
                        ClosureCaptureSource::Symbol(symbol) => symbol,
                        ClosureCaptureSource::This => panic!("fixture captures a named source"),
                    }
                })
            })
            .unwrap()
    };
    let xs = captured_symbol("xs");
    let ys = captured_symbol("ys");
    let f = symbol("f");
    let plan = &owned.iterations()[0];
    let phi = |boundary, name| {
        plan.closure_phis()
            .iter()
            .find(|phi| phi.boundary() == boundary && phi.symbol() == name)
            .unwrap()
    };
    let header_f = phi(IterationPhiBoundary::Header, f);
    let exit_f = phi(IterationPhiBoundary::Exit, f);
    let header_xs = phi(IterationPhiBoundary::Header, xs);
    let header_ys = phi(IterationPhiBoundary::Header, ys);
    let header_origin = |name| {
        header_f
            .origins()
            .iter()
            .find(|origin| {
                origin
                    .sources()
                    .iter()
                    .any(|source| source.source() == ClosureCaptureSource::Symbol(name))
            })
            .unwrap()
    };
    let exit_origin = |name| {
        exit_f
            .origins()
            .iter()
            .find(|origin| {
                origin
                    .sources()
                    .iter()
                    .any(|source| source.source() == ClosureCaptureSource::Symbol(name))
            })
            .unwrap()
    };
    let exit_source = |name| {
        exit_origin(name)
            .sources()
            .iter()
            .find(|source| source.source() == ClosureCaptureSource::Symbol(name))
            .unwrap()
            .owner()
    };
    let exit_xs = exit_source(xs);
    let exit_ys = exit_source(ys);
    let edge = |kind| {
        plan.closure_phi_incomings()
            .iter()
            .find(|incoming| incoming.kind() == kind)
            .unwrap()
    };
    let entry = edge(IterationPhiIncomingKind::Entry);
    let backedge = edge(IterationPhiIncomingKind::Fallthrough);
    let exhaustion = edge(IterationPhiIncomingKind::Exhaustion);
    let table = owned.cleanup_conditions();
    for (incoming, phi) in [
        (entry, header_f),
        (backedge, header_f),
        (exhaustion, exit_f),
    ] {
        let binding = incoming
            .bindings()
            .iter()
            .find(|binding| binding.target() == phi.owner())
            .unwrap();
        let expected = phi
            .origins()
            .iter()
            .flat_map(|origin| {
                origin.sources().iter().map(|source| {
                    table
                        .phi_capture_slot(phi.owner(), origin.closure(), source.source())
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(expected.len(), 2, "both alternative captures need clearing");
        assert_eq!(binding.capture_slots_to_clear(), expected);
    }
    let (save_point, save_condition, snapshot) = owned
        .cleanup_steps()
        .iter()
        .find_map(|(point, action)| match action {
            Action::SaveOwnerSnapshot {
                owner,
                value,
                condition,
            } => {
                let snapshot = table.owner_snapshot(*owner).unwrap();
                assert_eq!(snapshot.value(), *value);
                assert_eq!(*point, DropPoint::AfterExpression(*value));
                Some((*point, *condition, snapshot))
            }
            _ => None,
        })
        .unwrap();
    let save_index = owned
        .cleanup_steps()
        .iter()
        .position(|(point, action)| {
            *point == save_point
                && matches!(action, Action::SaveOwnerSnapshot { owner, .. } if *owner == snapshot.owner())
        })
        .unwrap();
    let commit_index = owned
        .cleanup_steps()
        .iter()
        .position(|(point, action)| {
            *point == save_point
                && matches!(action, Action::CommitOwnerSnapshot { owner, target } if *owner == snapshot.owner() && *target == f)
        })
        .unwrap();
    assert!(save_index < commit_index);
    for (index, (point, action)) in owned.cleanup_steps().iter().enumerate() {
        if *point == save_point && matches!(action, Action::EndCaptureLoan { .. }) {
            assert!(save_index < index && index < commit_index);
        }
    }
    let control = snapshot
        .copies()
        .iter()
        .find_map(|copy| {
            let selector = table.selector(copy.source()).unwrap();
            (selector.source() == CleanupSelectorSource::Control(snapshot.value())
                && selector.selection() == CleanupSelection::Branch)
                .then_some(copy.source())
        })
        .unwrap();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "f()").then_some(id))
        .unwrap();
    let source_symbol = |target| match target {
        DropTarget::Named(name)
        | DropTarget::RetainedSource(ClosureCaptureSource::Symbol(name))
            if name == xs || name == ys =>
        {
            Some(name)
        }
        DropTarget::Captured {
            source: ClosureCaptureSource::Symbol(name),
            ..
        } if name == xs || name == ys => Some(name),
        _ => None,
    };
    let entry_source = |target| {
        entry
            .bindings()
            .iter()
            .find(|binding| binding.target() == target)
            .unwrap()
            .values()[0]
            .source()
    };
    let source_slots = [
        entry_source(header_xs.owner()),
        entry_source(header_ys.owner()),
        header_xs.owner(),
        header_ys.owner(),
        header_origin(xs).sources()[0].owner(),
        header_origin(ys).sources()[0].owner(),
        exit_xs,
        exit_ys,
    ];
    for (point, action) in owned.cleanup_steps() {
        match action {
            Action::Drop(fact)
                if source_symbol(fact.target()).is_some()
                    || fact
                        .owner()
                        .is_some_and(|owner| source_slots.contains(&owner)) =>
            {
                assert!(
                    *point == exhaustion.point()
                        || *point == DropPoint::CallReturn(call)
                        || (*point == save_point
                            && matches!(fact.target(), DropTarget::RetainedSource(_))),
                    "either source can be captured in a later round: {point:?} {fact:?}"
                );
            }
            Action::EndCaptureLoan { source, value, .. }
                if matches!(source, ClosureCaptureSource::Symbol(name) if *name == xs || *name == ys)
                    || matches!(value, CleanupCaptureValue::Owner(owner) if source_slots.contains(owner)) =>
            {
                assert!(
                    *point == save_point || *point == DropPoint::CallReturn(call),
                    "a carried capture must survive until replacement or call return: {point:?} {action:?}"
                );
            }
            Action::TestLastCaptureLoan { owner, .. } if source_slots.contains(owner) => {
                assert!(*point == save_point || *point == DropPoint::CallReturn(call));
            }
            _ => {}
        }
    }

    for order in [[xs, ys], [ys, xs]] {
        let mut choices = BTreeMap::new();
        let mut owners = BTreeMap::new();
        let mut phi_capture_slots = BTreeMap::new();
        let mut formed = FormedCaptures::default();
        let mut live_loans = BTreeMap::from([(1_u32, 0_usize), (2_u32, 0_usize)]);
        for (target, instance) in [
            (header_f.owner(), 0),
            (header_xs.owner(), 1),
            (header_ys.owner(), 2),
        ] {
            let source = entry
                .bindings()
                .iter()
                .find(|binding| binding.target() == target)
                .unwrap()
                .values()[0]
                .source();
            owners.insert(source, instance);
        }
        replay(
            table,
            entry,
            plan.closure_phis(),
            &mut choices,
            &mut owners,
            &mut phi_capture_slots,
            &formed,
        );
        for (round, chosen) in order.into_iter().enumerate() {
            choices.insert(control, usize::from(chosen == ys));
            let before = choices.clone();
            assert!(
                save_condition.is_none_or(|guard| selected(table, guard, &before)),
                "the selected RHS must save its selectors before replacing f"
            );
            for copy in snapshot.copies() {
                if selected(table, copy.when(), &before) {
                    choices.insert(copy.target(), before[&copy.source()]);
                }
            }
            let active_inputs = snapshot
                .capture_inputs()
                .iter()
                .filter(|input| selected(table, input.condition(), &before))
                .collect::<Vec<_>>();
            assert_eq!(active_inputs.len(), 1);
            let Some(CleanupOwnerValue::Closure {
                expression, inputs, ..
            }) = table.owner_value(active_inputs[0].owner())
            else {
                panic!("selected RHS must create a closure environment");
            };
            assert_eq!(inputs.len(), 1);
            let CleanupCaptureValue::Owner(source) = inputs[0].value() else {
                panic!("selected closure must capture the source owner");
            };
            assert_eq!(
                source,
                if chosen == xs {
                    header_xs.owner()
                } else {
                    header_ys.owner()
                }
            );
            let source_instance = owners[&source];
            let formed_owner = active_inputs[0].owner();
            let captures = owned
                .cleanup_steps()
                .iter()
                .enumerate()
                .filter_map(|(index, (point, action))| match action {
                    Action::SaveClosureCapture {
                        owner,
                        target,
                        input,
                    } if *owner == formed_owner => Some((index, *point, *target, *input)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let &[(capture_index, capture_point, formed_slot, formed_input)] = captures.as_slice()
            else {
                panic!("the selected closure saves exactly one capture at formation");
            };
            let creates = owned
                .cleanup_steps()
                .iter()
                .enumerate()
                .filter_map(|(index, (point, action))| match action {
                    Action::CreateClosureOwner { owner, closure } if *owner == formed_owner => {
                        Some((index, *point, *closure))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let &[(create_index, create_point, created_closure)] = creates.as_slice() else {
                panic!("the selected closure creates exactly one environment");
            };
            assert_eq!(created_closure, *expression);
            assert_eq!(create_point, DropPoint::AfterExpression(*expression));
            assert_eq!(capture_point, create_point);
            assert!(create_index < capture_index && capture_index < save_index);
            assert!(save_index < commit_index);
            assert_eq!(formed_input, inputs[0]);
            assert!(selected(table, formed_input.condition(), &choices));
            assert!(
                formed
                    .slots
                    .insert((10 + round as u32, formed_slot), source_instance)
                    .is_none()
            );
            assert!(
                formed
                    .layouts
                    .insert(10 + round as u32, formed_owner)
                    .is_none()
            );
            owners.insert(formed_owner, 10 + round as u32);
            *live_loans.get_mut(&source_instance).unwrap() += 1;
            let ended = owned
                .cleanup_steps()
                .iter()
                .filter_map(|(point, action)| match action {
                    Action::EndCaptureLoan {
                        owner,
                        closure,
                        condition,
                        source,
                        value,
                        ..
                    } if *point == save_point
                        && condition.is_none_or(|guard| selected(table, guard, &choices)) =>
                    {
                        let old = order[round.checked_sub(1).expect("entry f has no capture")];
                        assert_eq!(*owner, header_f.owner());
                        assert_eq!(*closure, header_origin(old).closure());
                        assert_eq!(*source, ClosureCaptureSource::Symbol(old));
                        let old_owner = if old == xs {
                            header_xs.owner()
                        } else {
                            header_ys.owner()
                        };
                        assert_eq!(*value, CleanupCaptureValue::Owner(old_owner));
                        let old_instance = owners[&old_owner];
                        let count = live_loans.get_mut(&old_instance).unwrap();
                        assert_eq!(*count, 1);
                        *count -= 1;
                        Some(*source)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                ended,
                if round == 0 {
                    Vec::new()
                } else {
                    vec![ClosureCaptureSource::Symbol(order[round - 1])]
                },
                "replacement releases the preceding round's capture"
            );
            let mut last_loan_true = choices.clone();
            for (point, action) in owned.cleanup_steps() {
                if *point == save_point
                    && let Action::TestLastCaptureLoan {
                        condition,
                        selector,
                        ..
                    } = action
                {
                    assert!(
                        condition.is_some_and(|guard| !selected(table, guard, &choices)),
                        "available named sources must not query retained-source cleanup"
                    );
                    choices.insert(*selector, 0);
                    last_loan_true.insert(*selector, 1);
                }
            }
            for (point, action) in owned.cleanup_steps() {
                if *point == save_point
                    && let Action::Drop(fact) = action
                    && matches!(fact.target(), DropTarget::RetainedSource(_))
                {
                    assert!(
                        fact.condition().is_some_and(|guard| !selected(
                            table,
                            guard,
                            &last_loan_true
                        )),
                        "named availability must forbid retained drop even if last-loan were true"
                    );
                }
            }
            owners.insert(snapshot.owner(), 10 + round as u32);
            replay(
                table,
                backedge,
                plan.closure_phis(),
                &mut choices,
                &mut owners,
                &mut phi_capture_slots,
                &formed,
            );
            assert_eq!(owners[&header_f.owner()], 10 + round as u32);
            let inactive = if chosen == xs { ys } else { xs };
            assert!(
                !owners.contains_key(&header_origin(inactive).sources()[0].owner()),
                "the inactive capture source must not retain a prior phi value"
            );
            let capture_slot = |name| {
                table
                    .phi_capture_slot(
                        header_f.owner(),
                        header_origin(name).closure(),
                        ClosureCaptureSource::Symbol(name),
                    )
                    .unwrap()
            };
            assert_eq!(phi_capture_slots[&capture_slot(chosen)], source_instance);
            assert!(
                !phi_capture_slots.contains_key(&capture_slot(inactive)),
                "the unselected alternative must clear its previous capture slot"
            );
        }
        let last = order[1];
        assert_eq!(
            live_loans[&owners[&header_xs.owner()]],
            usize::from(last == xs)
        );
        assert_eq!(
            live_loans[&owners[&header_ys.owner()]],
            usize::from(last == ys)
        );
        let source_at_exit = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| match action {
                Action::Drop(fact)
                    if *point == exhaustion.point()
                        && fact
                            .condition()
                            .is_none_or(|guard| selected(table, guard, &choices))
                        && (source_symbol(fact.target()).is_some()
                            || fact
                                .owner()
                                .is_some_and(|owner| source_slots.contains(&owner))) =>
                {
                    let name = source_symbol(fact.target()).expect("source drop target");
                    let expected_owner = if name == xs {
                        header_xs.owner()
                    } else {
                        header_ys.owner()
                    };
                    assert_eq!(fact.owner(), Some(expected_owner));
                    let instance = owners[&expected_owner];
                    assert_eq!(live_loans[&instance], 0, "cannot drop a borrowed source");
                    Some((name, instance))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            source_at_exit,
            [(order[0], if order[0] == xs { 1 } else { 2 })],
            "the unheld source drops at loop exit"
        );
        replay(
            table,
            exhaustion,
            plan.closure_phis(),
            &mut choices,
            &mut owners,
            &mut phi_capture_slots,
            &formed,
        );
        assert_eq!(owners[&exit_f.owner()], 11);
        assert_eq!(owners[&exit_source(last)], if last == xs { 1 } else { 2 });
        let mut ended = Vec::new();
        let mut tested = Vec::new();
        let mut dropped = Vec::new();
        for (point, action) in owned.cleanup_steps() {
            if *point != DropPoint::CallReturn(call) {
                continue;
            }
            match action {
                Action::EndCaptureLoan {
                    owner,
                    closure,
                    condition,
                    source,
                    value,
                    ..
                } if condition.is_none_or(|guard| selected(table, guard, &choices)) => {
                    assert_eq!(*owner, exit_f.owner());
                    assert_eq!(*closure, exit_origin(last).closure());
                    assert_eq!(*source, ClosureCaptureSource::Symbol(last));
                    assert_eq!(*value, CleanupCaptureValue::Owner(exit_source(last)));
                    let instance = owners[&exit_source(last)];
                    let count = live_loans.get_mut(&instance).unwrap();
                    assert_eq!(*count, 1);
                    *count -= 1;
                    ended.push(*source);
                }
                Action::TestLastCaptureLoan {
                    condition,
                    owner,
                    selector,
                    ..
                } if condition.is_none_or(|guard| selected(table, guard, &choices)) => {
                    assert_eq!(*owner, exit_source(last));
                    assert_eq!(ended.len(), 1);
                    assert_eq!(live_loans[&owners[owner]], 0);
                    choices.insert(*selector, 1);
                    tested.push(*owner);
                }
                Action::Drop(fact)
                    if fact
                        .condition()
                        .is_none_or(|guard| selected(table, guard, &choices))
                        && (source_symbol(fact.target()).is_some()
                            || fact.owner() == Some(exit_xs)
                            || fact.owner() == Some(exit_ys)) =>
                {
                    assert_eq!(fact.owner(), Some(exit_source(last)));
                    assert_eq!(tested.len(), 1);
                    dropped.push(source_symbol(fact.target()));
                }
                _ => {}
            }
        }
        assert_eq!(ended, [ClosureCaptureSource::Symbol(last)]);
        assert_eq!(tested, [exit_source(last)]);
        assert_eq!(dropped, [Some(last)]);
    }
}

#[test]
fn replacement_saves_new_snapshot_before_old_cleanup_and_commits_it_afterward() {
    use lang_frontend::ownership_checking::{DropPoint, IterationCleanupAction as Action};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, own zs: List<Int>, flag: Boolean) { var f: () -> Unit = { read(xs) }\nf = if (flag) { val observed = f()\nval replacement: () -> Unit = { read(ys) }\nreplacement } else { val observed = f()\nval replacement: () -> Unit = { read(zs) }\nreplacement }\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let rhs = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            sources
                .slice(node.span())
                .unwrap()
                .starts_with("if (flag)")
                .then_some(id)
        })
        .unwrap();
    let actions = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(point, action)| (*point == DropPoint::AfterExpression(rhs)).then_some(action))
        .collect::<Vec<_>>();
    let save = actions
        .iter()
        .position(|action| matches!(action, Action::SaveOwnerSnapshot { .. }))
        .unwrap();
    let drop_old = actions
        .iter()
        .position(|action| matches!(action, Action::Drop(_)))
        .unwrap();
    let end_capture = actions
        .iter()
        .position(|action| matches!(action, Action::EndCaptureLoan { .. }))
        .unwrap();
    let commit = actions
        .iter()
        .position(|action| matches!(action, Action::CommitOwnerSnapshot { .. }))
        .unwrap();
    assert!(
        save < drop_old && drop_old < end_capture && end_capture < commit,
        "{actions:?}"
    );
    let Action::SaveOwnerSnapshot { owner: saved, .. } = actions[save] else {
        unreachable!()
    };
    let Action::CommitOwnerSnapshot {
        owner: committed, ..
    } = actions[commit]
    else {
        unreachable!()
    };
    assert_eq!(saved, committed);
}

#[test]
fn moved_conditional_environment_slots_refer_to_the_destination_owner_value() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, DropPoint, DropTarget};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { var f: move () -> Unit = if (flag) (move { read(xs) }) else (move { read(ys) })\nval g = f\nval used = g() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "g()").then_some(id))
        .unwrap();
    let slots = owned
        .drops()
        .iter()
        .filter_map(|fact| match fact.target() {
            DropTarget::Captured { owner, closure, .. }
                if fact.point() == DropPoint::CallReturn(call) =>
            {
                Some((owner, closure))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(slots.len(), 2);
    assert!(
        owned
            .drops()
            .iter()
            .filter(|fact| {
                matches!(fact.target(), DropTarget::Captured { .. })
                    && fact.point() == DropPoint::CallReturn(call)
            })
            .all(|fact| {
                let DropTarget::Captured { closure, source, .. } = fact.target() else {
                    unreachable!();
                };
                fact.capture_slot()
                    .and_then(|slot| owned.cleanup_conditions().capture_slot_value(slot))
                    .is_some_and(|layout| {
                        layout.source() == source
                            && matches!(
                                owned.cleanup_conditions().owner_value(layout.environment()),
                                Some(CleanupOwnerValue::Closure { expression, .. }) if *expression == closure
                            )
                    })
            }),
        "a saved environment retains its original slot layout"
    );
    assert_eq!(
        slots[0].0, slots[1].0,
        "both alternatives belong to the moved value"
    );
    let CleanupOwnerValue::Snapshot(snapshot) =
        owned.cleanup_conditions().owner_value(slots[0].0).unwrap()
    else {
        panic!("destination snapshot")
    };
    let source_owner = snapshot.capture_inputs()[0].owner();
    assert_ne!(
        snapshot.owner(),
        source_owner,
        "moving the saved value has an explicit source relation"
    );
    assert!(
        snapshot
            .capture_inputs()
            .iter()
            .all(|input| input.owner() == source_owner)
    );
    let source = owned
        .cleanup_conditions()
        .owner_snapshot(source_owner)
        .unwrap();
    for input in source.capture_inputs() {
        let CleanupOwnerValue::Closure { expression, .. } = owned
            .cleanup_conditions()
            .owner_value(input.owner())
            .unwrap()
        else {
            panic!("original environment")
        };
        assert!(slots.iter().any(|(_, closure)| closure == expression));
    }
    assert_eq!(source.capture_inputs().len(), 2);
}

#[test]
fn pending_temporary_capture_loan_end_names_its_created_environment() {
    use lang_frontend::ownership_checking::{CleanupOwnerValue, IterationCleanupAction as Action};
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { val used = (if (flag) ({ read(xs) }) else ({ read(ys) }))() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let mut count = 0;
    for (_, action) in owned.cleanup_steps() {
        if let Action::EndCaptureLoan { owner, closure, .. } = action {
            assert!(
                matches!(owned.cleanup_conditions().owner_value(*owner), Some(CleanupOwnerValue::Closure { expression, .. }) if expression == closure)
            );
            assert!(owned.cleanup_steps().iter().any(|(_, action)| matches!(action, Action::CreateClosureOwner { owner: created, closure: expression } if created == owner && expression == closure)));
            count += 1;
        }
    }
    assert_eq!(count, 2);
}

#[test]
fn snapshot_binds_the_full_rhs_even_when_its_capture_origin_is_opaque() {
    use lang_frontend::ownership_checking::{CleanupCondition, IterationCleanupAction as Action};
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own cb: move () -> Unit, flag: Boolean) { val f: move () -> Unit = if (flag) (move { read(xs) }) else cb\nval used = f() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let (
        _,
        Action::SaveOwnerSnapshot {
            owner,
            value,
            condition,
        },
    ) = owned
        .cleanup_steps()
        .iter()
        .find(|(_, action)| matches!(action, Action::SaveOwnerSnapshot { .. }))
        .unwrap()
    else {
        unreachable!()
    };
    assert!(condition.is_none(), "both arms reach the save");
    let snapshot = owned.cleanup_conditions().owner_snapshot(*owner).unwrap();
    assert_eq!(snapshot.value(), *value);
    assert_eq!(
        sources
            .slice(parsed.ast().expressions().get(*value).unwrap().span())
            .unwrap(),
        "if (flag) (move { read(xs) }) else cb"
    );
    assert_eq!(snapshot.capture_inputs().len(), 1);
    let CleanupCondition::Choice { branches, .. } = owned
        .cleanup_conditions()
        .get(snapshot.capture_inputs()[0].condition())
        .unwrap()
    else {
        panic!("conditional provenance")
    };
    for (arm, expected) in [CleanupCondition::Always, CleanupCondition::Never]
        .iter()
        .enumerate()
    {
        assert_eq!(
            owned.cleanup_conditions().get(branches[arm]).unwrap(),
            expected
        );
        // The opaque arm has no local capture provenance, but still binds the complete RHS value.
        assert_eq!(snapshot.value(), *value);
    }
}

#[test]
fn when_first_alternative_does_not_read_a_skipped_later_alternative() {
    use lang_frontend::ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupSelection,
        ClosureCaptureSource, DropPoint, DropTarget, IterationCleanupAction as Action,
    };
    fn enabled(
        table: &CleanupConditions,
        condition: CleanupConditionId,
        values: &std::collections::BTreeMap<usize, usize>,
    ) -> bool {
        match table.get(condition).unwrap() {
            CleanupCondition::Always => true,
            CleanupCondition::Never => false,
            CleanupCondition::Choice { selector, branches } => enabled(
                table,
                branches[*values
                    .get(&selector.index())
                    .expect("only executed alternatives have selector values")],
                values,
            ),
        }
    }
    let (sources, parsed, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, first: Boolean, gate: Boolean) { var f: () -> Unit = { read(xs) }\nval g: () -> Unit = when { first, if (gate) { { f = ({ read(ys) }) }\ntrue } else false -> f\nelse -> f }\nval used = g() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let control = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| {
            matches!(
                node.payload(),
                lang_frontend::parser::Expression::When { .. }
            )
            .then_some(id)
        })
        .unwrap();
    let call = parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, node)| (sources.slice(node.span()).unwrap() == "g()").then_some(id))
        .unwrap();
    let table = owned.cleanup_conditions();
    let copied = owned
        .cleanup_steps()
        .iter()
        .flat_map(|(_, action)| match action {
            Action::SaveOwnerSnapshot { owner, .. } => table
                .owner_snapshot(*owner)
                .unwrap()
                .copies()
                .iter()
                .map(|copy| copy.target().index())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect::<std::collections::BTreeSet<_>>();
    // First match skips gate entirely; the two other paths evaluate it and select old/new f.
    for (first, gate, selected) in [(0, None, "xs"), (1, Some(0), "ys"), (1, Some(1), "xs")] {
        let mut values = table
            .selectors()
            .iter()
            .enumerate()
            .filter_map(|(index, selector)| {
                if copied.contains(&index) {
                    return None;
                }
                let choice = match selector.selection() {
                    CleanupSelection::WhenAlternative {
                        entry: 0,
                        alternative: 0,
                    } => {
                        assert_eq!(selector.control(), Some(control));
                        Some(first)
                    }
                    CleanupSelection::WhenAlternative {
                        entry: 0,
                        alternative: 1,
                    } => gate,
                    CleanupSelection::Branch => gate,
                    _ => panic!("unexpected control in fixture"),
                };
                choice.map(|choice| (index, choice))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        for (_, action) in owned.cleanup_steps() {
            if let Action::SaveOwnerSnapshot {
                condition, owner, ..
            } = action
            {
                if condition.is_some_and(|condition| !enabled(table, condition, &values)) {
                    continue;
                }
                let writes = table
                    .owner_snapshot(*owner)
                    .unwrap()
                    .copies()
                    .iter()
                    .filter(|copy| enabled(table, copy.when(), &values))
                    .map(|copy| {
                        (
                            copy.target().index(),
                            *values
                                .get(&copy.source().index())
                                .expect("copied source was evaluated"),
                        )
                    })
                    .collect::<Vec<_>>();
                values.extend(writes);
            }
        }
        for name in ["xs", "ys"] {
            let source = owned
                .captures()
                .iter()
                .find_map(|capture| {
                    (sources.slice(capture.reference_span()).unwrap() == name)
                        .then_some(capture.source())
                })
                .unwrap();
            let ClosureCaptureSource::Symbol(symbol) = source else {
                unreachable!()
            };
            let count = owned
                .drops()
                .iter()
                .filter(|fact| {
                    fact.target() == DropTarget::Named(symbol)
                        && fact.point() == DropPoint::CallReturn(call)
                        && fact
                            .condition()
                            .is_none_or(|condition| enabled(table, condition, &values))
                })
                .count();
            assert_eq!(
                count,
                usize::from(name == selected),
                "{name}: first={first}, gate={gate:?}"
            );
        }
    }
}

#[test]
fn loop_phi_publishes_statically_unique_owned_descendant() {
    use lang_frontend::ownership_checking::{CleanupCaptureValue, DropTarget};

    let (_, _, owned) = checked(
        "fun take(own xs: List<Int>) {}\nfun run(own xs: List<Int>) {\nval base: move () -> Unit = move { take(xs) }\nval outer: move () -> Unit = move { var f: move () -> Unit = move { base() }\nfor (_ in listOf(1)) {}\nval used = f() }\nval used = outer() }",
    );
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    assert!(owned.deferred().is_empty(), "{:?}", owned.deferred());
    assert!(!owned.iterations().is_empty());
    assert!(
        owned.drops().iter().any(|fact| {
            matches!(
                fact.target(),
                DropTarget::Captured {
                    value: CleanupCaptureValue::Owner(_),
                    ..
                }
            ) && fact.capture_slot().is_some()
                && fact.instance_address().is_some()
        }),
        "the owned descendant must remain droppable through the phi: {:?}",
        owned.drops()
    );
}
