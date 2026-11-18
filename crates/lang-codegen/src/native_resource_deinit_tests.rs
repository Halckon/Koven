use super::emit_link_and_run;
use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};

#[test]
fn resource_deinit_native_scope_body_and_reverse_fields() {
    let output = emit_link_and_run(
        "resource-scope.ko",
        r#"
class Leaf(val name: String) {
    deinit() { println(this.name) }
}
class Guard(val first: Leaf, val second: Leaf) {
    deinit() { println("parent alive") }
}
fun main(): Unit {
    val earlier = Leaf("earlier")
    val guard = Guard(Leaf("first"), Leaf("second"))
    println("body 完成")
}
"#,
        "main",
    );
    assert_diagnostic!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        "body 完成\nparent alive\nsecond\nfirst\nearlier\n".as_bytes()
    );
}

const LEAF: &str = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun inspect(item: Leaf): Unit { println("inspect") }
fun consume(own item: Leaf): Unit { println("consume") }
"#;

#[test]
fn resource_deinit_native_transfer_return_and_temporary_are_exactly_once() {
    let source = format!(
        r#"{LEAF}
fun make(): Leaf {{ val local = Leaf("returned"); return local }}
fun branch(flag: Boolean): Unit {{
    val outer = Leaf("outer")
    if (flag) {{ val inner = Leaf("inner"); println("return"); return }}
    println("fallthrough")
}}
fun main(): Unit {{
    val source = make()
    val destination = source
    println("before")
    consume(destination)
    inspect(Leaf("temporary"))
    branch(true)
    branch(false)
    println("done")
}}
"#
    );
    let output = emit_link_and_run("resource-transfer.ko", &source, "main");
    assert_diagnostic!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"before\nconsume\nreturned\ninspect\ntemporary\nreturn\ninner\nouter\nfallthrough\nouter\ndone\n");
}

#[test]
fn resource_deinit_native_continue_break_and_nested_scope_preserve_outer_guard() {
    let source = format!(
        r#"{LEAF}
fun main(): Unit {{
    val outer = Leaf("outer")
    var i: Int = 0
    while (i < 3) {{
        val inner = Leaf("inner")
        i = i + 1
        if (i == 1) {{ println("continue"); continue }}
        if (i == 2) {{ println("break"); break }}
    }}
    println("after loop")
}}
"#
    );
    let output = emit_link_and_run("resource-loop.ko", &source, "main");
    assert_diagnostic!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"continue\ninner\nbreak\ninner\nafter loop\nouter\n"
    );
}

#[test]
fn resource_deinit_native_body_return_still_cleans_fields_but_abort_does_not() {
    let source = r#"
class Leaf { deinit() { println("field") } }
class Returning(val child: Leaf) { deinit() { println("body"); return } }
fun main(): Unit { val resource = Returning(Leaf()); println("scope") }
"#;
    let output = emit_link_and_run("resource-return.ko", source, "main");
    assert_diagnostic!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"scope\nbody\nfield\n");
    let source = r#"
class Leaf { deinit() { println("field must not run") } }
class Aborting(val child: Leaf) { deinit() { println("body abort"); error("stop") } }
fun main(): Unit { val resource = Aborting(Leaf()); println("scope") }
"#;
    let output = emit_link_and_run("resource-abort.ko", source, "main");
    use std::os::unix::process::ExitStatusExt;
    assert_eq_diagnostic!(output.status.signal(), Some(6), "{output:?}");
    assert_eq!(output.stdout, b"scope\nbody abort\n");
}

#[test]
fn resource_deinit_native_nested_resource_allocations_have_unique_frees() {
    use super::SymbolKind;
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
class Bundle(val first: Leaf, val second: Leaf) { deinit() { println("bundle") } }
class Wrapper(val bundle: Bundle)
fun entry(): Unit {
    val wrapper = Wrapper(Bundle(Leaf("a" + "1"), Leaf("b" + "2")))
    println("held")
}
"#;
    let analysis = UnitSource::analyze("resource-counts.ko", source)
        .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("entry", SymbolKind::Function))
        .unwrap();
    let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let output = super::boxed_enum_tests::run_counted_allocations(&ir, 6);
    assert_diagnostic!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"held\nbundle\nb2\na1\n");
}

#[test]
fn resource_deinit_native_replacement_is_after_rhs_and_keeps_declaration_order() {
    let source = format!(
        r#"{LEAF}
fun make(): Leaf {{ println("new rhs"); return Leaf("new") }}
fun main(): Unit {{
    var first = Leaf("old")
    val second = Leaf("second")
    first = make()
    println("after replacement")
}}
"#
    );
    let output = emit_link_and_run("resource-replace.ko", &source, "main");
    assert_diagnostic!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"new rhs\nold\nafter replacement\nsecond\nnew\n"
    );
}

#[test]
fn resource_deinit_native_branch_replacement_keeps_return_cleanup_order() {
    let source = format!(
        r#"{LEAF}
fun run(flag: Boolean, early: Boolean): Unit {{
    var first = Leaf("old")
    val second = Leaf("second")
    if (flag) {{ first = Leaf("left") }} else {{ first = Leaf("right") }}
    if (early) {{ println("return"); return }}
    println("fallthrough")
}}
fun main(): Unit {{
    run(true, true)
    run(false, true)
    run(true, false)
    run(false, false)
}}
"#
    );
    let output = emit_link_and_run("resource-branch-replace.ko", &source, "main");
    assert_diagnostic!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        concat!(
            "old\nreturn\nsecond\nleft\n",
            "old\nreturn\nsecond\nright\n",
            "old\nfallthrough\nsecond\nleft\n",
            "old\nfallthrough\nsecond\nright\n",
        )
        .as_bytes()
    );
}

#[test]
fn resource_deinit_native_conditional_owner_is_rejected_without_overwriting_output() {
    use super::TestDirectory;
    use crate::native_tests::boxed_enum_tests::unit_source::{SourceRejectionKind, UnitSource};
    use lang_frontend::{
        name_resolution::{SourceUnitInput, SourceUnitKey},
        ownership_checking::OwnershipDeferredReason,
        parser::Expression,
    };
    let source = format!(
        r#"{LEAF}
fun run(flag: Boolean): Unit {{
    val resource = Leaf("resource")
    if (flag) {{ consume(resource) }}
    println("after branch")
}}
fun main(): Unit {{ run(true); run(false) }}
"#
    );
    // Unit rejects incomplete resource transport before entry selection or native emission.
    // This replaces the retired single-file UnsupportedSource/resource-origin wrapper contract.
    let directory = TestDirectory::create();
    let path = directory.join("preserved.o");
    std::fs::write(&path, b"previous object").unwrap();
    std::fs::write(directory.join("neighbor.txt"), b"untouched neighbor").unwrap();
    let before = directory.snapshot();
    let Err(rejection) = UnitSource::analyze("resource-conditional.ko", &source) else {
        panic!("conditional resource transport must not publish an executable UnitSource");
    };
    assert_eq!(
        rejection.report.stages,
        [("parse", 0), ("names", 0), ("types", 0), ("ownership", 0)]
    );
    assert!(rejection.report.diagnostics.is_empty());
    let SourceRejectionKind::IncompleteBasicOwnership(recovery) = rejection.kind else {
        panic!("the real ownership admission, not a source diagnostic, must reject");
    };
    let inputs = [SourceUnitInput::memory_document(
        recovery.parsed.source_id(),
        &recovery.parsed,
    )];
    assert!(recovery.typed.types().is_compatible_with(
        &rejection.sources,
        &inputs,
        &recovery.names,
        &recovery.environment,
    ));
    assert!(recovery.owned.is_compatible_with(&recovery.typed));
    assert!(recovery.owned.diagnostics().is_empty());
    let [deferred] = recovery.owned.deferred() else {
        panic!("one exact resource lifetime recovery fact is required");
    };
    assert_eq!(deferred.reason(), OwnershipDeferredReason::ResourceLifetime);
    let [unit] = recovery.names.names().index().source_units() else {
        panic!("one original memory document is required");
    };
    assert_eq!(unit.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(unit.source_id(), recovery.parsed.source_id());
    assert_eq!(deferred.expression().source_unit(), unit.id());
    let node = recovery
        .parsed
        .ast()
        .expressions()
        .get(deferred.expression().expression())
        .unwrap();
    assert!(matches!(node.payload(), Expression::If { .. }));
    let span = node.span();
    assert_eq!(span.source_id(), unit.source_id());
    assert_eq!(
        rejection.sources.source_name(span.source_id()),
        Ok("resource-conditional.ko")
    );
    assert_eq!(
        rejection.sources.source_text(span.source_id()),
        Ok(source.as_str())
    );
    let expected = "if (flag) { consume(resource) }";
    assert_eq!(rejection.sources.slice(span), Ok(expected));
    let start = source.find(expected).unwrap();
    assert_eq!((span.start(), span.end()), (start, start + expected.len()));
    let expected_deferred = recovery.owned.deferred().to_vec();
    let rejected = (*recovery.owned)
        .validate()
        .expect_err("the original recovery still cannot grant validated ownership");
    assert_eq!(rejected.deferred(), expected_deferred);
    // No entry, sealed view, emit, LLVM, linker, or program run occurs on this Err path.
    assert_eq!(std::fs::read(&path).unwrap(), b"previous object");
    assert_eq!(directory.snapshot(), before);
}

#[test]
fn resource_deinit_native_local_move_before_return_has_one_current_owner() {
    use super::boxed_enum_tests::{
        assert_success, lower_to_llvm, run_counted_allocations_in_order,
    };
    for moves in [
        "val moved = local",
        "val moved = (local); val moved_again = ((moved))",
    ] {
        for stop in [true, false] {
            let source = format!(
                r#"{LEAF}
class Holder(var state: Leaf) {{ deinit() {{ println("holder") }} }}
fun work(own stop: Boolean): Unit {{
    val holder = Holder(Leaf("old"))
    val local = Leaf("local")
    {moves}
    if (stop) {{ return }}
    val old = replace(&holder.state, Leaf("new"))
    inspect(old)
    println("helper-done")
}}
fun entry(): Unit {{ work({stop}); println("done") }}
"#
            );
            let llvm = lower_to_llvm("resource-local-move.ko", &source);
            // Allocation identities: old, holder, local, then replacement on fallthrough.
            // Moving a binding must neither allocate nor leave another owner to release.
            let (order, stdout): (&[usize], &[u8]) = if stop {
                (&[2, 0, 1], b"local\nholder\nold\ndone\n")
            } else {
                (
                    &[0, 2, 3, 1],
                    b"inspect\nhelper-done\nold\nlocal\nholder\nnew\ndone\n",
                )
            };
            assert_success(&run_counted_allocations_in_order(&llvm, order), stdout);
        }
    }
}

#[test]
fn resource_deinit_native_local_move_scope_does_not_leave_stale_cfg_owner() {
    use super::boxed_enum_tests::{
        assert_success, lower_to_llvm, run_counted_allocations_in_order,
    };
    let source = format!(
        r#"{LEAF}
fun work(own stop: Boolean): Unit {{
    {{
        val local = Leaf("local")
        val moved = (local)
        println("inside")
    }}
    if (stop) {{ println("after scope"); return }}
    println("fallthrough")
}}
fun entry(): Unit {{ work(true); work(false) }}
"#
    );
    let llvm = lower_to_llvm("resource-move-scope.ko", &source);
    assert_success(
        &run_counted_allocations_in_order(&llvm, &[0, 1]),
        b"inside\nlocal\nafter scope\ninside\nlocal\nfallthrough\n",
    );
}

#[test]
fn resource_deinit_native_local_copyable_alias_keeps_both_bindings() {
    let source = r#"
fun work(own stop: Boolean): Unit {
    val original = 7
    val copied = (original)
    if (stop) { if (original == 7 && copied == 7) { println("both"); return } }
    if (copied == 7 && original == 7) { println("both") }
}
fun <T : Copyable> keep(own original: T): T {
    val copied = original
    return original
}
fun main(): Unit { work(true); work(false); if (keep(9) == 9) { println("generic") } }
"#;
    let output = emit_link_and_run("copyable-local-alias.ko", source, "main");
    super::boxed_enum_tests::assert_success(&output, b"both\nboth\ngeneric\n");
}

#[test]
fn resource_deinit_native_local_move_nullable_wrap_keeps_one_owner() {
    use super::boxed_enum_tests::{
        assert_success, lower_to_llvm, run_counted_allocations_in_order,
    };
    // Resource nullable storage is deferred; use the supported pure class handle ABI.
    let source = r#"
class Node {}
fun consumeNullable(own node: Node?): Unit {
    if (node == null) { println("unexpected") } else { println("present") }
}
fun work(own stop: Boolean): Unit {
    val original = Node()
    val wrapped: Node? = (original)
    if (stop) { println("return"); return }
    consumeNullable(wrapped)
}
fun entry(): Unit { work(true); work(false) }
"#;
    let llvm = lower_to_llvm("local-move-nullable.ko", source);
    assert_success(
        &run_counted_allocations_in_order(&llvm, &[0, 1]),
        b"return\npresent\n",
    );
}

#[test]
fn resource_deinit_native_nested_control_scopes_drop_only_their_owners() {
    use super::boxed_enum_tests::{assert_success, lower_to_llvm, run_counted_allocations};
    // Exercise every condition pair: branch-local cleanup must preserve the outer guard.
    for (name, body, allocations, expected) in [
        (
            "two-inner",
            r#"if (outer) {
                if (inner) { val a = Leaf("a"); val b = Leaf("b"); println("inner") }
                println("outer")
            }
            println("done")"#,
            6,
            "done\nroot\ndone\nroot\nouter\ndone\nroot\ninner\nb\na\nouter\ndone\nroot\n",
        ),
        (
            "one-per-scope",
            r#"if (outer) {
                val a = Leaf("a")
                if (inner) { val b = Leaf("b"); println("inner") }
                println("outer")
            }
            println("done")"#,
            7,
            "done\nroot\ndone\nroot\nouter\na\ndone\nroot\ninner\nb\nouter\na\ndone\nroot\n",
        ),
        (
            "explicit-return",
            r#"if (outer) {
                if (inner) { val a = Leaf("a"); val b = Leaf("b"); println("inner"); return }
                println("outer")
            }
            println("done")"#,
            6,
            "done\nroot\ndone\nroot\nouter\ndone\nroot\ninner\nb\na\nroot\n",
        ),
    ] {
        let source = format!(
            r#"{LEAF}
fun work(own outer: Boolean, own inner: Boolean): Unit {{
    val root = Leaf("root")
    {body}
}}
fun entry(): Unit {{ work(false, false); work(false, true); work(true, false); work(true, true) }}
"#
        );
        let llvm = lower_to_llvm(&format!("nested-resource-{name}.ko"), &source);
        assert_success(
            &run_counted_allocations(&llvm, allocations),
            expected.as_bytes(),
        );
    }
}

#[test]
fn resource_deinit_native_pending_value_argument_obeys_each_control_exit() {
    let source = format!(
        r#"{LEAF}
fun take(own left: Leaf, own right: Leaf): Unit {{ println("take") }}
fun returning(flag: Boolean): Unit {{
    val outer = Leaf("outer")
    take(Leaf("pending"), if (flag) {{ return }} else {{ Leaf("right") }})
    println("after")
}}
fun breaking(flag: Boolean): Unit {{
    val outer = Leaf("outer")
    var count = 0
    while (count < 2) {{
        count = count + 1
        val inner = Leaf("inner")
        take(Leaf("pending"), if (flag) {{ break }} else {{ Leaf("right") }})
        println("unexpected continuation")
        break
    }}
    println("after")
}}
fun continuing(flag: Boolean): Unit {{
    val outer = Leaf("outer")
    var count = 0
    while (count < 2) {{
        count = count + 1
        val inner = Leaf("inner")
        take(Leaf("pending"), if (flag) {{ continue }} else {{ Leaf("right") }})
        println("unexpected continuation")
    }}
    println("after")
}}
fun main(): Unit {{
    println("return")
    returning(true)
    println("normal")
    returning(false)
    println("break")
    breaking(true)
    println("continue")
    continuing(true)
    println("done")
}}
"#
    );
    let output = emit_link_and_run("pending-resource-exits.ko", &source, "main");
    assert_diagnostic!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"return\npending\nouter\nnormal\ntake\nright\npending\nafter\nouter\nbreak\npending\ninner\nafter\nouter\ncontinue\npending\ninner\npending\ninner\nafter\nouter\ndone\n"
    );
    assert!(output.stderr.is_empty(), "{output:?}");

    let aborting = format!(
        r#"{LEAF}
fun take(own left: Leaf, own right: Leaf): Unit {{ println("take") }}
fun makePending(): Leaf {{
    val result = Leaf("pending")
    println("made pending")
    return result
}}
fun run(flag: Boolean): Unit {{
    val outer = Leaf("outer")
    println("made outer")
    take(makePending(), if (flag) {{ error("stop") }} else {{ Leaf("right") }})
    println("after")
}}
fun main(): Unit {{ println("before abort"); run(true) }}
"#
    );
    let output = emit_link_and_run("pending-resource-abort.ko", &aborting, "main");
    use std::os::unix::process::ExitStatusExt;
    assert_eq_diagnostic!(output.status.signal(), Some(6), "{output:?}");
    assert_eq!(output.stdout, b"before abort\nmade outer\nmade pending\n");
    assert!(output.stderr.starts_with(b"abort: stop\n"), "{output:?}");
}

#[test]
fn direct_resource_closure_native_keeps_original_output_and_repeated_drop_order() {
    for (source, expected) in [
        (
            "class Resource { deinit() {} }\nfun run(): Unit { val text = \"keep\"; val callback = move { println(text); val resource = Resource() }; callback() }",
            "keep\n",
        ),
        (
            "class Resource { deinit() { println(\"released\") } }\nfun run(): Unit { val text = \"keep\"; val callback = move { println(text); val resource = Resource(); println(\"body\") }; val alias = callback; (alias)(); (alias)(); println(\"after\") }",
            "keep\nbody\nreleased\nkeep\nbody\nreleased\nafter\n",
        ),
    ] {
        let output = emit_link_and_run("resource-direct-closure.ko", source, "run");
        assert_eq_diagnostic!(output.status.code(), Some(0), "{output:?}");
        assert_eq_diagnostic!(output.stdout, expected.as_bytes(), "{output:?}");
        assert_eq_diagnostic!(output.stderr, b"", "{output:?}");
    }
}

#[test]
fn resource_closure_native_composes_with_borrow_initializer_return() {
    let source = r#"class Resource { deinit() { println("released") } }
fun view(source: String, own n: Int): borrow String from source = source
fun probe(own flag: Boolean): Unit {
    val text = "capture".clone()
    val callback = move { println(text); val resource = Resource(); println("body") }
    callback()
    val owner = "kept".clone()
    borrow val item = view(owner, if (flag) { return } else { 1 })
    println(item)
}
fun entry(): Unit { probe(true); println("first done"); probe(false) }
"#;
    let expected = b"capture\nbody\nreleased\nfirst done\ncapture\nbody\nreleased\nkept\n";
    let output = emit_link_and_run("resource-borrow-prefix.ko", source, "entry");
    assert_eq_diagnostic!(output.status.code(), Some(0), "{output:?}");
    assert_eq_diagnostic!(output.stdout, expected, "{output:?}");
    assert_eq_diagnostic!(output.stderr, b"", "{output:?}");
}

#[test]
fn resource_deinit_native_enum_drops_active_payloads_in_lexical_order() {
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
class Unselected { deinit() { println("unselected must not run") } }
enum class Payload { Pair(first: Leaf, second: Leaf), Empty }
enum class Packet { Full(payload: Payload), Other(unused: Unselected), Empty }
fun consume(own packet: Packet): Unit { println("consume") }
fun main(): Unit {
    val earlier = Leaf("earlier")
    val held = Packet.Full(Payload.Pair(Leaf("held-first"), Leaf("held-second")))
    consume(Packet.Empty)
    consume(Packet.Full(Payload.Pair(Leaf("moved-first"), Leaf("moved-second"))))
    println("body")
}
"#;
    use super::{SymbolKind, boxed_enum_tests::unit_source::UnitSource};
    let analysis = UnitSource::analyze("resource-enum.ko", source)
        .expect("resource enum must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let (program, entry) = analysis
        .lower_entry(analysis.declaration("main", SymbolKind::Function))
        .expect("resource enum must lower through the Unit capability");
    let module = program.module(entry.module()).unwrap();
    let registered_deinits = (0..module.types.len())
        .filter(|&index| {
            let owner = crate::ssa::model::SsaTypeId {
                module: module.id,
                index,
            };
            module.deinit(owner).is_some()
        })
        .count();
    assert_eq!(
        registered_deinits, 2,
        "drop glue needs both resource bodies"
    );
    let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[4, 3, 2, 1, 0]);
    super::boxed_enum_tests::assert_success(
        &output,
        b"consume\nconsume\nmoved-second\nmoved-first\nbody\nheld-second\nheld-first\nearlier\n",
    );
}

#[test]
fn resource_deinit_native_boxed_enum_releases_descendants_before_their_handles() {
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
enum class Tree { Tip(value: Leaf), Pair(left: Box<Tree>, right: Box<Tree>) }
fun entry(): Unit {
    val root = Tree.Pair(
        Box(Tree.Pair(Box(Tree.Tip(Leaf("a"))), Box(Tree.Tip(Leaf("b"))))),
        Box(Tree.Tip(Leaf("c")))
    )
    println("tree held")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("resource-boxed-enum.ko", source);
    // The inline root owns two subtrees; each Box is freed after its payload.
    let output =
        super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[5, 6, 2, 3, 0, 1, 4]);
    super::boxed_enum_tests::assert_success(&output, b"tree held\nc\nb\na\n");
}

#[test]
fn resource_deinit_native_nullable_box_drops_only_present_handles() {
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
enum class Packet { Full(value: Leaf), Empty }
fun make(flag: Boolean, name: String): Box<Packet>? {
    if (flag) { return Box(Packet.Full(Leaf(name.clone()))) }
    return null
}
fun take(own option: Box<Packet>?): Unit {
    if (option != null) { println("present") } else { println("null") }
}
fun entry(): Unit {
    val earlier = make(true, "held")
    take(make(false, "unused"))
    take(make(true, "moved"))
    take(Box(Packet.Empty))
    println("scope")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("resource-nullable-box.ko", source);
    let output =
        super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[3, 4, 5, 6, 0, 1, 2]);
    super::boxed_enum_tests::assert_success(
        &output,
        b"null\npresent\nmoved\npresent\nscope\nheld\n",
    );
}

#[test]
fn resource_deinit_native_recursive_nullable_box_fields_drop_their_active_tree() {
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
enum class Tree { Tip(value: Leaf), Link(child: Box<Tree>?), Pair(left: Box<Tree>?, right: Box<Tree>?) }
fun entry(): Unit {
    val root = Tree.Pair(
        Box(Tree.Link(Box(Tree.Tip(Leaf("left"))))),
        Box(Tree.Pair(null, Box(Tree.Tip(Leaf("right")))))
    )
    println("scope")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("recursive-nullable-box.ko", source);
    let output =
        super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[3, 4, 5, 0, 1, 2]);
    super::boxed_enum_tests::assert_success(&output, b"scope\nright\nleft\n");
}

#[test]
fn resource_deinit_native_boxed_value_enum_cycle_drops_the_active_child() {
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
value class Branch(val tree: Tree)
enum class Tree { Tip(value: Leaf), Link(child: Box<Branch>), Empty }
fun entry(): Unit {
    val root = Box(Branch(Tree.Link(Box(Branch(Tree.Tip(Leaf("leaf")))))))
    println("held")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("mixed-box-recursion.ko", source);
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[0, 1, 2]);
    super::boxed_enum_tests::assert_success(&output, b"held\nleaf\n");
}

#[test]
fn resource_deinit_native_value_enum_cycle_with_nullable_box_on_the_value() {
    let source = r#"
class Leaf { deinit() { println("leaf") } }
value class Branch(val child: Box<Tree>?)
enum class Tree { Link(branch: Branch), Tip(value: Leaf), Empty }
fun entry(): Unit {
    val root = Branch(Box(Tree.Link(Branch(Box(Tree.Tip(Leaf()))))))
    println("held")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("mixed-box-reverse-recursion.ko", source);
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[0, 1, 2]);
    super::boxed_enum_tests::assert_success(&output, b"held\nleaf\n");
}

#[test]
fn resource_deinit_native_rc_drops_payload_only_after_the_last_owner() {
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun take(own owner: Rc<Leaf>): Unit { println("consume") }
fun takeNullable(own owner: Rc<Leaf>?): Unit {
    if (owner == null) { println("null") } else { println("present") }
}
fun entry(): Unit {
    val earlier = Leaf("earlier")
    val original = Rc(Leaf("payload"))
    take(original.share())
    takeNullable(null)
    takeNullable(original.share())
    println("still held")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("resource-rc-last-owner.ko", source);
    // The shared payload is dropped before its control block, then the earlier local.
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[1, 2, 0]);
    super::boxed_enum_tests::assert_success(
        &output,
        b"consume\nnull\npresent\nstill held\npayload\nearlier\n",
    );
}

#[test]
fn resource_deinit_native_rc_field_chain_keeps_the_owner_through_reads() {
    let source = r#"
class Leaf(val name: String) { deinit() { println("drop") } }
class Wrapper(val child: Leaf)
fun observe(own owner: Rc<Wrapper>): Unit {
    println((owner.value).child.name)
    println(owner.value.child.name)
}
fun entry(): Unit {
    val original = Rc(Wrapper(Leaf("payload".clone())))
    observe(original.share())
    println("held")
    observe(original)
    println("done")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("rc-payload-field-chain.ko", source);
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[0, 1, 2, 3]);
    super::boxed_enum_tests::assert_success(
        &output,
        b"payload\npayload\nheld\npayload\npayload\ndrop\ndone\n",
    );
}

#[test]
fn resource_deinit_native_rc_field_argument_holds_the_handle_until_call_return() {
    let source = r#"
class Leaf(val name: String)
fun nullable(own owner: Rc<Leaf>?): Unit {
    if (owner != null) { println(owner.value.name) }
}
fun named(name: String, own other: Rc<Leaf>): Unit { println(name) }
fun entry(): Unit {
    nullable(Rc(Leaf("nullable".clone())))
    nullable(null)
    val owner = Rc(Leaf("shared".clone()))
    named(owner.value.name, owner.share())
    println("done")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("rc-field-last-use.ko", source);
    let output =
        super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[0, 1, 2, 3, 4, 5]);
    super::boxed_enum_tests::assert_success(&output, b"nullable\nshared\ndone\n");
}

#[test]
fn resource_deinit_native_borrowed_rc_and_nullable_proof_use_handle_storage() {
    let source = r#"
class Leaf(val name: String) { deinit() { println("drop") } }
fun inspect(borrow source: Rc<Leaf>): Unit { println((source.value).name) }
fun retain(borrow source: Rc<Leaf>): Rc<Leaf> = source.share()
fun optional(own owner: Rc<Leaf>?): Unit {
    if (owner != null) { println(owner.value.name) }
}
fun entry(): Unit {
    val source = Rc(Leaf("slot".clone()))
    inspect(source)
    optional(retain(source))
    optional(null)
    println("held")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("borrowed-rc-proven-handle.ko", source);
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[0, 1, 2]);
    super::boxed_enum_tests::assert_success(&output, b"slot\nslot\nheld\ndrop\n");
}

#[test]
fn resource_deinit_native_nullable_rc_forwards_the_live_proof_to_borrow_calls() {
    let source = r#"
class Leaf(val name: String) { deinit() { println("drop") } }
fun inspect(borrow owner: Rc<Leaf>): Unit { println(owner.value.name) }
fun forward(own owner: Rc<Leaf>?, stop: Boolean): Unit {
    if (!(owner != null)) {
        println("null")
    } else {
        inspect((owner))
        if (stop) { return }
        inspect(owner)
    }
    println("tail")
}
fun entry(): Unit {
    forward(Rc(Leaf("early".clone())), true)
    forward(null, false)
    forward(Rc(Leaf("normal".clone())), false)
    println("done")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("rc-proof-forward.ko", source);
    let output =
        super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[0, 1, 2, 3, 4, 5]);
    super::boxed_enum_tests::assert_success(
        &output,
        b"early\ndrop\nnull\ntail\nnormal\nnormal\ntail\ndrop\ndone\n",
    );
}

#[test]
fn resource_deinit_native_rc_guard_keeps_the_proof_on_the_continuing_edge() {
    let source = r#"
class Leaf(val name: String) { deinit() { println("drop") } }
fun inspect(borrow owner: Rc<Leaf>): Unit { println(owner.value.name) }
fun forward(own owner: Rc<Leaf>?): Unit {
    if (owner == null) { println("null"); return }
    inspect((owner))
    inspect(owner)
    println("held")
}
fun entry(): Unit {
    forward(null)
    forward(Rc(Leaf("proof".clone())))
    println("done")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("rc-continuing-proof.ko", source);
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[0, 1, 2]);
    super::boxed_enum_tests::assert_success(&output, b"null\nproof\nproof\nheld\ndrop\ndone\n");
}

#[test]
fn resource_deinit_native_temporary_rc_payload_fields_keep_their_handle_alive() {
    let source = r#"
class Leaf(val num: Int, val name: String) { deinit() { println("drop") } }
fun inspect(borrow name: String, proceed: Boolean): Unit {
    if (proceed) { println(name) }
}
fun run(stop: Boolean): Unit {
    inspect(Rc(Leaf(17, "borrow".clone())).value.name, if (stop) { return } else { true })
    println("resumed")
}
fun entry(): Unit {
    val n = (Rc(Leaf(17, "copy".clone()))).value.num
    if (n == 17) { println("read") }
    run(false)
    run(true)
    println("done")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("temporary-rc-payload-fields.ko", source);
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(
        &ir,
        &[0, 1, 2, 3, 4, 5, 6, 7, 8],
    );
    super::boxed_enum_tests::assert_success(
        &output,
        b"drop\nread\nborrow\ndrop\nresumed\ndrop\ndone\n",
    );
}

#[test]
fn resource_deinit_native_temporary_rc_copy_waits_for_the_enclosing_expression() {
    let source = r#"
class Leaf(val num: Int) { deinit() { println("drop") } }
fun side(): Int { println("side"); return 17 }
fun sink(own num: Int, other: Int): Unit { println("sink") }
fun rhs(): Boolean { println("rhs"); return true }
fun entry(): Unit {
    sink(Rc(Leaf(17)).value.num, side())
    val same = Rc(Leaf(17)).value.num == side()
    if (same) { println("match") }
    if ((Rc(Leaf(17)).value.num == 17) && rhs()) { println("arm") }
    if (!(Rc(Leaf(0)).value.num == 17)) { println("not") }
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("temporary-rc-copy-order.ko", source);
    let output =
        super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[0, 1, 2, 3, 4, 5, 6, 7]);
    super::boxed_enum_tests::assert_success(
        &output,
        b"side\nsink\ndrop\nside\ndrop\nmatch\nrhs\ndrop\narm\ndrop\nnot\n",
    );
}

#[test]
fn resource_deinit_native_conditional_rc_reaches_the_enclosing_call_cleanup() {
    let source = r#"
class Left(val yes: Boolean) { deinit() { println("left") } }
class Right(val yes: Boolean) { deinit() { println("right") } }
fun side(): Int { println("side"); return 0 }
fun sink(own yes: Boolean, other: Int): Unit { println("sink") }
fun run(gate: Boolean, exit: Boolean): Unit {
    sink(Rc(Left(gate)).value.yes && Rc(Right(true)).value.yes,
        if (exit) { return } else { side() })
    println("tail")
}
fun entry(): Unit {
    run(false, false)
    run(true, false)
    run(false, true)
    run(true, true)
    sink(false && Rc(Right(true)).value.yes, side())
    println("done")
}
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("conditional-rc-call-cleanup.ko", source);
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(
        &ir,
        &[0, 1, 4, 5, 2, 3, 6, 7, 10, 11, 8, 9],
    );
    super::boxed_enum_tests::assert_success(
        &output,
        b"side\nsink\nleft\ntail\nside\nsink\nright\nleft\ntail\nleft\nright\nleft\nside\nsink\ndone\n",
    );
}

#[test]
fn resource_deinit_native_const_short_circuit_preserves_temporary_rc_cleanup() {
    let source = r#"
const val ENABLE = true
class Leaf(val yes: Boolean) { deinit() { println("drop") } }
fun side(): Int { println("side"); return 0 }
fun sink(own yes: Boolean, other: Int): Unit { println("sink") }
fun run(flag: Boolean): Unit {
    sink(ENABLE && Rc(Leaf(true)).value.yes, side())
    sink(ENABLE || Rc(Leaf(true)).value.yes, side())
    sink(flag || Rc(Leaf(true)).value.yes, side())
}
fun entry(): Unit { run(true); run(false) }
"#;
    let ir = super::boxed_enum_tests::lower_to_llvm("const-short-circuit-rc-cleanup.ko", source);
    let output =
        super::boxed_enum_tests::run_counted_allocations_in_order(&ir, &[0, 1, 2, 3, 4, 5]);
    super::boxed_enum_tests::assert_success(
        &output,
        b"side\nsink\ndrop\nside\nsink\nside\nsink\nside\nsink\ndrop\nside\nsink\nside\nsink\ndrop\n",
    );
}

#[test]
fn resource_deinit_native_rc_receiver_exits_before_forming_or_lending_an_owner() {
    let source = r#"
class Leaf(val name: String) { deinit() { println("drop") } }
fun inspect(borrow leaf: Leaf, ready: Boolean): Unit {
    if (ready) { println(leaf.name) }
}
fun lend(before: Boolean, after: Boolean): Unit {
    inspect((if (before) { println("early"); return } else { Rc(Leaf("payload".clone())) }).value,
        if (after) { println("late"); return } else { true })
    println("tail")
}
fun retain(stop: Boolean): Unit {
    val owner = (if (stop) { return } else { Rc(Leaf("held".clone())) }).share()
    println(owner.value.name)
    println("held-tail")
}
fun noOwner(): Unit { val value = Rc<Int>(return).value }
fun entry(): Unit {
    lend(true, false)
    lend(false, true)
    lend(false, false)
    retain(true); retain(false)
    noOwner()
    println("done")
}
"#;
    let llvm = super::boxed_enum_tests::lower_to_llvm("rc-receiver-exits.ko", source);
    let output = super::boxed_enum_tests::run_counted_allocations_in_order(
        &llvm,
        &[0, 1, 2, 3, 4, 5, 6, 7, 8],
    );
    super::boxed_enum_tests::assert_success(
        &output,
        b"early\nlate\ndrop\npayload\ndrop\ntail\nheld\nheld-tail\ndrop\ndone\n",
    );
}
