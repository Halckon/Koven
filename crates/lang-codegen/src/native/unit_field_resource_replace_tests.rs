//! Cross-file direct class-field resource replacement through both native unit entries.

use super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations};

const PROVIDER: &str = r#"package p
class Leaf(val name: String) { deinit() { println(this.name) } }
class Guard(val name: String, val first: Leaf, val second: Leaf) {
    deinit() { println(this.name) }
}
fun replacement(): Guard {
    println("evaluate new")
    return Guard("new body", Leaf("new first"), Leaf("new second"))
}
fun inspect(guard: Guard): Unit { println("inspect old") }
"#;

#[test]
fn unit_field_resource_replace_native_commit_preserves_parent_and_lexical_owners() {
    let consumer = r#"package q
import p.Leaf
import p.Guard
class Holder(var state: Guard, val sibling: Leaf) {
    deinit() { println("parent body") }
}
fun entry(): Unit {
    val holder = Holder(Guard("old body", Leaf("old first"), Leaf("old second")), Leaf("sibling"))
    val old = replace(&holder.state, p.replacement())
    println("after replace")
    p.inspect(old)
    println("after inspect")
}
"#;
    let expected = b"evaluate new\nafter replace\ninspect old\nafter inspect\nold body\nold second\nold first\nparent body\nsibling\nnew body\nnew second\nnew first\n";
    for constants in [false, true] {
        let (run, ir) = run_unit(PROVIDER, consumer, constants);
        assert_success(&run, expected);
        assert_success(&run_counted_allocations(&ir, 8), expected);
    }
}

#[test]
fn unit_field_resource_replace_native_return_and_borrow_transfer_old_once() {
    let provider = r#"package p
class Leaf(val name: String) { deinit() { println(this.name) } }
class Holder(var state: Leaf)
fun takeOld(): Leaf {
    val holder = Holder(Leaf("returned old"))
    return replace(&holder.state, Leaf("returned replacement"))
}
fun inspect(leaf: Leaf): Unit { println("inspect old") }
"#;
    let consumer = r#"package q
import p.Leaf
class Holder(var state: Leaf)
fun entry(): Unit {
    val returned = p.takeOld()
    println("received old")
    val holder = Holder(Leaf("borrowed old"))
    p.inspect(replace(&holder.state, Leaf("borrowed replacement")))
    println("after inspect")
}
"#;
    let expected = b"returned replacement\nreceived old\ninspect old\nborrowed old\nafter inspect\nborrowed replacement\nreturned old\n";
    for constants in [false, true] {
        let (run, ir) = run_unit(provider, consumer, constants);
        assert_success(&run, expected);
        assert_success(&run_counted_allocations(&ir, 6), expected);
    }
}

#[test]
fn unit_field_resource_replace_native_pending_control_exit_keeps_old_field() {
    for exit in ["return", "break", "continue"] {
        let provider = format!(
            r#"package p
class Leaf(val name: String) {{ deinit() {{ println(this.name) }} }}
class Holder(var state: Leaf) {{ deinit() {{ println("parent body") }} }}
fun exercise(own flag: Boolean): Unit {{
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
"#
        );
        let consumer = "package q\nfun entry(): Unit { p.exercise(true); p.exercise(false); println(\"done\") }";
        // Preserve the base entry's existing argument-control-transfer boundary.
        let (run, ir) = run_unit(&provider, consumer, true);
        let expected = if exit == "return" {
            "parent body\nold\ncommitted\nold\nafter loop\nparent body\nnew\ndone\n"
        } else {
            "after loop\nparent body\nold\ncommitted\nold\nafter loop\nparent body\nnew\ndone\n"
        };
        assert_success(&run, expected.as_bytes());
        assert_success(&run_counted_allocations(&ir, 5), expected.as_bytes());
    }
}

#[test]
fn unit_field_resource_replace_native_abort_does_not_commit_or_run_deinit() {
    let provider = r#"package p
class Leaf { deinit() { println("unexpected leaf deinit") } }
"#;
    let consumer = r#"package q
import p.Leaf
class Holder(var state: Leaf) { deinit() { println("unexpected parent deinit") } }
fun entry(): Unit {
    val holder = Holder(Leaf())
    println("before abort")
    val old = replace(&holder.state, error("stop"))
    println("unexpected commit")
}
"#;
    for constants in [false, true] {
        let (run, _) = run_unit(provider, consumer, constants);
        assert!(!run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"before abort\n");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(run.status.signal(), Some(6), "{run:?}");
        }
    }
}
