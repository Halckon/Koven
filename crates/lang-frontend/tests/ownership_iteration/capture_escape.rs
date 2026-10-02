use super::*;

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
fn lambda_tail_cannot_return_a_borrowed_closure() {
    let (_, _, owned) = checked(
        "fun read(xs: List<Int>) {}\nfun run() { val f: (List<Int>) -> () -> Unit = { xs -> for (_ in xs) {}\nval inner = { read(xs) }\ninner } }",
    );
    assert_eq!(owned.diagnostics().len(), 1, "{:?}", owned.diagnostics());
    assert_eq!(owned.diagnostics()[0].code().to_string(), "L0137");
    assert!(owned.iterations().is_empty());
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
