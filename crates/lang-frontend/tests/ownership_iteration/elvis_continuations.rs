use super::*;

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
