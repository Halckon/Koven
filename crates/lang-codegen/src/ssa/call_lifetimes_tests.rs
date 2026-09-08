//! 后续实参提前退出时，先结束此前实参的借用，再清理 temporary owner。
use super::*;

#[test]
fn later_argument_transfer_ends_earlier_temporary_borrow_before_drop() {
    for body in [
        "fun test(own subject: Node?, flag: Boolean): Unit { if (subject != null) { outer(subject, if (flag) 0 else 1) } }",
        "fun test(flag: Boolean): Unit = outer(Node(), if (flag) return else 0)",
        "fun test(flag: Boolean): Unit = outer(Node(), when (flag) { true -> 0; false -> 1 })",
        "fun test(flag: Boolean): Unit { loop { outer(Node(), if (flag) break else 0) } }",
        "fun test(flag: Boolean): Unit { loop { outer(Node(), if (flag) continue else 0) } }",
        "fun test(flag: Boolean): Unit { loop { outer(Node(), if (flag) break else 0) } if (flag) {} else {} }",
        "fun test(own subject: Node?): Unit = outer(Node(), if (subject == null) return else 0)",
        "fun test(own subject: Node?, flag: Boolean): Unit { if (subject != null) { if (flag) {} else {}\nouter(subject, 0) } }",
        "fun test(own subject: Node?, flag: Boolean): Unit { if (subject != null) { when { flag -> {}; else -> {} }\nouter(subject, 0) } }",
        "fun test(own subject: Node?): Unit { if (subject != null) { loop { break }\nouter(subject, 0) } }",
        "fun test(own subject: Node?, flag: Boolean): Unit { if (subject != null) { while (flag) { outer(subject, 0)\ncontinue }\nouter(subject, 0) } }",
        "fun test(own subject: Node?, flag: Boolean): Unit { if (subject != null) { loop { if (flag) continue else break }\nouter(subject, 0) } }",
        "fun test(own subject: Node?): Unit = outer(Node(), when (subject) { null -> 0; else -> return })",
        "fun test(own subject: Node?): Unit { loop { outer(Node(), when (subject) { null -> 0; else -> break }) } }",
        "fun test(own subject: Node?): Unit { loop { outer(Node(), when (subject) { null -> 0; else -> continue }) } }",
    ] {
        let analysis = analyze(&format!(
            "class Node()\nfun outer(node: Node, own result: Int): Unit {{}}\n{body}"
        ));
        assert!(
            analysis.parsed.diagnostics().is_empty(),
            "{body}: {:?}",
            analysis.parsed.diagnostics()
        );
        assert!(
            analysis.names.diagnostics().is_empty(),
            "{body}: {:?}",
            analysis.names.diagnostics()
        );
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{:?}",
            analysis.owned.diagnostics()
        );
        let program = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .unwrap_or_else(|error| panic!("{body}: {error:?}"));
        render_verified_program(&program).expect("loan-end and owner cleanup order must verify");
    }
}

#[test]
fn grouped_temporary_has_one_owner_at_argument_merge() {
    let analysis = analyze(
        "class Node()\nfun outer(node: Node, own result: Int): Unit {}\nfun test(flag: Boolean): Unit { outer((Node()), if (flag) 0 else 1)\nif (flag) {} else {} }",
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("grouping must not duplicate ownership on a merge edge");
    render_verified_program(&program).expect("each temporary owner must be transferred once");
}
