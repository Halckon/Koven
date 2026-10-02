//! Resource lexical cleanup composed with owned whole-root replace/swap.

use super::{SymbolKind, analyze, emit_link_and_run, symbol};

const RESOURCES: &str = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
class Guard(val name: String, val first: Leaf, val second: Leaf) {
    deinit() { println(this.name) }
}
fun replacement(): Guard {
    println("evaluate new")
    return Guard("new body", Leaf("new first"), Leaf("new second"))
}
fun consume(own guard: Guard): Unit { println("consume old") }
"#;

#[test]
fn resource_exchange_native_commit_keeps_owners_until_lexical_cleanup() {
    let source = format!(
        r#"{RESOURCES}
fun entry(): Unit {{
    var first = Guard("old body", Leaf("old first"), Leaf("old second"))
    var second = Guard("second body", Leaf("second first"), Leaf("second second"))
    val old = replace(&first, replacement())
    println("after replace")
    consume(old)
    swap(&first, &second)
    println("after swap")
}}
"#
    );
    let expected = b"evaluate new\nafter replace\nconsume old\nold body\nold second\nold first\nafter swap\nnew body\nnew second\nnew first\nsecond body\nsecond second\nsecond first\n";
    let run = emit_link_and_run("resource-exchange.ko", &source, "entry");
    super::boxed_enum_tests::assert_success(&run, expected);
    let analysis = analyze("resource-exchange-counted.ko", &source);
    let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "entry", SymbolKind::Function),
    )
    .expect("resource exchange lowers without introducing an early drop");
    let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    // Three guards each own two leaves. Exchange transfers each allocation unchanged.
    let counted = super::boxed_enum_tests::run_counted_allocations(&ir, 9);
    super::boxed_enum_tests::assert_success(&counted, expected);
}

#[test]
fn resource_exchange_native_replace_return_and_borrowed_result_drop_once() {
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun takeOld(): Leaf {
    var target = Leaf("new root")
    return replace(&target, Leaf("replacement root"))
}
fun inspect(leaf: Leaf): Unit { println("inspect old") }
fun entry(): Unit {
    val returned = takeOld()
    println("received old")
    var target = Leaf("borrowed old")
    inspect(replace(&target, Leaf("borrowed replacement")))
    println("after inspect")
}
"#;
    let run = emit_link_and_run("resource-exchange-return.ko", source, "entry");
    super::boxed_enum_tests::assert_success(
        &run,
        b"replacement root\nreceived old\ninspect old\nborrowed old\nafter inspect\nborrowed replacement\nnew root\n",
    );
}

#[test]
fn resource_exchange_native_pending_control_exit_retains_old_root() {
    for exit in ["return", "break", "continue"] {
        let source = format!(
            r#"
class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}
fun exercise(flag: Boolean): Unit {{
    var target = Leaf("old")
    var once = true
    loop {{
        if (!once) {{ break }}
        once = false
        val old = replace(&target, if (flag) {{ {exit} }} else {{ Leaf("new") }})
        println("committed")
        break
    }}
    println("after loop")
}}
fun entry(): Unit {{ exercise(true); exercise(false); println("done") }}
"#
        );
        let run = emit_link_and_run("resource-exchange-exit.ko", &source, "entry");
        let expected = if exit == "return" {
            "old\ncommitted\nold\nafter loop\nnew\ndone\n"
        } else {
            "after loop\nold\ncommitted\nold\nafter loop\nnew\ndone\n"
        };
        super::boxed_enum_tests::assert_success(&run, expected.as_bytes());
    }
}

#[test]
fn resource_exchange_native_abort_does_not_commit_or_run_resource_cleanup() {
    let source = r#"
class Leaf { deinit() { println("unexpected deinit") } }
fun entry(): Unit {
    var target = Leaf()
    println("before abort")
    val old = replace(&target, error("stop"))
    println("unexpected commit")
}
"#;
    let run = emit_link_and_run("resource-exchange-abort.ko", source, "entry");
    assert!(!run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"before abort\n");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(run.status.signal(), Some(6), "{run:?}");
    }
}
