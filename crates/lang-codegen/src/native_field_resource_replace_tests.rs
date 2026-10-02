//! Direct field exchange composed with ordinary concrete resource class cleanup.

use super::{SymbolKind, analyze, emit_link_and_run, symbol};

const RESOURCES: &str = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
class Guard(val name: String, val first: Leaf, val second: Leaf) {
    deinit() { println(this.name) }
}
class Holder(var state: Guard, val sibling: Leaf) {
    deinit() { println("parent body") }
}
fun replacement(): Guard {
    println("evaluate new")
    return Guard("new body", Leaf("new first"), Leaf("new second"))
}
fun inspect(guard: Guard): Unit { println("inspect old") }
"#;

#[test]
fn field_resource_replace_native_commit_preserves_parent_and_lexical_owners() {
    let source = format!(
        r#"{RESOURCES}
fun entry(): Unit {{
    val holder = Holder(Guard("old body", Leaf("old first"), Leaf("old second")), Leaf("sibling"))
    val old = replace(&holder.state, replacement())
    println("after replace")
    inspect(old)
    println("after inspect")
}}
"#
    );
    let expected = b"evaluate new\nafter replace\ninspect old\nafter inspect\nold body\nold second\nold first\nparent body\nsibling\nnew body\nnew second\nnew first\n";
    assert_run_and_counted(&source, 8, expected);
}

#[test]
fn field_resource_replace_native_return_and_borrow_transfer_old_once() {
    let source = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
class Holder(var state: Leaf)
fun takeOld(): Leaf {
    val holder = Holder(Leaf("returned old"))
    return replace(&holder.state, Leaf("returned replacement"))
}
fun inspect(leaf: Leaf): Unit { println("inspect old") }
fun entry(): Unit {
    val returned = takeOld()
    println("received old")
    val holder = Holder(Leaf("borrowed old"))
    inspect(replace(&holder.state, Leaf("borrowed replacement")))
    println("after inspect")
}
"#;
    assert_run_and_counted(
        source,
        6,
        b"returned replacement\nreceived old\ninspect old\nborrowed old\nafter inspect\nborrowed replacement\nreturned old\n",
    );
}

#[test]
fn field_resource_replace_native_pending_control_exit_keeps_old_field() {
    for exit in ["return", "break", "continue"] {
        let source = format!(
            r#"
class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}
class Holder(var state: Leaf) {{ deinit() {{ println("parent body") }} }}
fun exercise(flag: Boolean): Unit {{
    val holder = Holder(Leaf("old"))
    var once = true
    loop {{
        if (!once) {{ break }}
        once = false
        val old = replace(&holder.state, if (flag) {{ {exit} }} else {{ Leaf("new") }})
        println("committed")
        break
    }}
    println("after loop")
}}
fun entry(): Unit {{ exercise(true); exercise(false); println("done") }}
"#
        );
        let expected = if exit == "return" {
            "parent body\nold\ncommitted\nold\nafter loop\nparent body\nnew\ndone\n"
        } else {
            "after loop\nparent body\nold\ncommitted\nold\nafter loop\nparent body\nnew\ndone\n"
        };
        assert_run_and_counted(&source, 5, expected.as_bytes());
    }
}

#[test]
fn field_resource_replace_native_abort_does_not_commit_or_run_deinit() {
    let source = r#"
class Leaf { deinit() { println("unexpected leaf deinit") } }
class Holder(var state: Leaf) { deinit() { println("unexpected parent deinit") } }
fun entry(): Unit {
    val holder = Holder(Leaf())
    println("before abort")
    val old = replace(&holder.state, error("stop"))
    println("unexpected commit")
}
"#;
    let run = emit_link_and_run("field-resource-abort.ko", source, "entry");
    assert!(!run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"before abort\n");
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(run.status.signal(), Some(6), "{run:?}");
    }
}

fn assert_run_and_counted(source: &str, allocations: usize, expected: &[u8]) {
    let run = emit_link_and_run("field-resource-replace.ko", source, "entry");
    super::boxed_enum_tests::assert_success(&run, expected);
    let analysis = analyze("field-resource-counted.ko", source);
    let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "entry", SymbolKind::Function),
    )
    .expect("field resource replacement lowers to verified SSA");
    let ir = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let counted = super::boxed_enum_tests::run_counted_allocations(&ir, allocations);
    super::boxed_enum_tests::assert_success(&counted, expected);
}
