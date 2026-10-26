//! 后续实参提前退出时，先结束此前实参的借用，再清理 temporary owner。
use crate::llvm::render_verified_program;
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::ssa::{model::Origin, verify::verify_program};
use lang_frontend::name_resolution::SymbolKind;

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
        let source =
            format!("class Node()\nfun outer(node: Node, own result: Int): Unit {{}}\n{body}");
        let analysis = UnitSource::analyze("lowering.ko", &source)
            .unwrap_or_else(|error| panic!("{body}: strict Unit admission: {error:?}"));
        analysis.assert_frontend_clean();
        assert!(
            analysis.ownership().deferred().is_empty(),
            "{body}: {:?}",
            analysis.ownership().deferred()
        );
        // Both original parameterized functions are roots; test has no caller.
        for (name, original_declaration) in [
            ("outer", "fun outer(node: Node, own result: Int): Unit {}"),
            ("test", body),
        ] {
            let declaration = analysis.declaration(name, SymbolKind::Function);
            let span = analysis.declaration_span(declaration);
            assert_eq!(analysis.sources().slice(span), Ok(original_declaration));
            let (program, root) = analysis
                .lower_entry(declaration)
                .unwrap_or_else(|error| panic!("{body}: root {name}: lowering: {error:?}"));
            let function = program
                .module(root.module())
                .and_then(|module| module.function(root))
                .expect("the real root belongs to its own lowered Program");
            assert_eq!(function.origin, Origin::Source(span));
            assert!(
                !function.blocks.is_empty(),
                "{body}: root {name} has a body"
            );
            verify_program(&program)
                .unwrap_or_else(|error| panic!("{body}: root {name}: SSA: {error:?}"));
            // Verify the full LLVM module with None native entry, not a wrapper.
            render_verified_program(&program)
                .unwrap_or_else(|error| panic!("{body}: root {name}: LLVM: {error:?}"));
        }
    }
}

#[test]
fn grouped_temporary_has_one_owner_at_argument_merge() {
    let analysis = UnitSource::analyze(
        "lowering.ko",
        "class Node()\nfun outer(node: Node, own result: Int): Unit {}\nfun test(flag: Boolean): Unit { outer((Node()), if (flag) 0 else 1)\nif (flag) {} else {} }",
    )
    .expect("the original grouped temporary source passes strict Unit admission");
    analysis.assert_frontend_clean();
    let declaration = analysis.declaration("test", SymbolKind::Function);
    let (program, _) = analysis
        .lower_entry(declaration)
        .expect("grouping must not duplicate ownership on a merge edge");
    render_verified_program(&program).expect("each temporary owner must be transferred once");
}
