//! Owner identity and observable cleanup order across iteration and pending call frames.
use super::resource_exchange_tests::run_unit;
use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations};

const LEAF: &str = "class Leaf(val name: String) { deinit() { println(this.name) } }";

#[test]
fn unit_for_resource_elements_and_local_owners_drop_in_lexical_order() {
    for factory in ["arrayOf", "listOf", "mutableListOf"] {
        for (exit, visits) in [("", 2), ("continue", 2), ("break", 1), ("return", 1)] {
            let provider = format!(
                "package p\n{LEAF}\nfun inspect(leaf: Leaf): Unit {{ println(\"visit\") }}\nfun work(): Unit {{ val outer = Leaf(\"outer\"); for (item in {factory}(Leaf(\"first\"), Leaf(\"second\"))) {{ val local = Leaf(\"local\"); inspect(item); {exit} }}; println(\"after\") }}"
            );
            let consumer = "package q\nfun entry(): Unit { p.work(); println(\"caller\") }";
            let expected = format!(
                "{}second\nfirst\n{}outer\ncaller\n",
                "visit\nlocal\n".repeat(visits),
                if exit == "return" { "" } else { "after\n" }
            );
            for constants in [false, true] {
                let (run, ir) = run_unit(&provider, consumer, constants);
                assert_success(&run, expected.as_bytes());
                assert_success(
                    &run_counted_allocations(&ir, 4 + visits),
                    expected.as_bytes(),
                );
            }
        }
    }
}

#[test]
fn unit_for_pending_borrow_call_cleans_prefix_before_provider() {
    for exit in ["continue", "break", "return"] {
        let provider = format!(
            "package p\n{LEAF}\nfun sink(leaf: Leaf, own flag: Boolean): Unit {{ println(\"committed\") }}\nfun work(own flag: Boolean): Unit {{ val outer = Leaf(\"outer\"); for (_ in arrayOf(Leaf(\"element\"))) {{ val local = Leaf(\"local\"); sink(Leaf(\"prefix\"), if(flag) {{ {exit} }} else {{ true }}); println(\"body-after\") }}; println(\"after\") }}"
        );
        let consumer =
            "package q\nfun entry(): Unit { p.work(true); p.work(false); println(\"caller\") }";
        let expected = format!(
            "prefix\nlocal\nelement\n{}outer\ncommitted\nprefix\nbody-after\nlocal\nelement\nafter\nouter\ncaller\n",
            if exit == "return" { "" } else { "after\n" }
        );
        for constants in [false, true] {
            let (run, ir) = run_unit(&provider, consumer, constants);
            assert_success(&run, expected.as_bytes());
            assert_success(&run_counted_allocations(&ir, 10), expected.as_bytes());
        }
    }
}

#[test]
fn unit_for_outer_pending_call_waits_until_inner_provider_cleanup() {
    let provider = format!(
        r#"package p
{LEAF}
fun sink(leaf: Leaf, own flag: Boolean): Unit {{ println("unexpected call") }}
fun work(own flag: Boolean): Unit {{
    sink(Leaf("prefix"), if(flag) {{
        for (_ in arrayOf(Leaf("element"))) {{ val local = Leaf("local"); return }}
        true
    }} else {{ true }})
    println("unexpected after")
}}
"#
    );
    let consumer = "package q\nfun entry(): Unit { p.work(true); println(\"caller\") }";
    for constants in [false, true] {
        let (run, ir) = run_unit(&provider, consumer, constants);
        assert_success(&run, b"local\nelement\nprefix\ncaller\n");
        assert_success(
            &run_counted_allocations(&ir, 4),
            b"local\nelement\nprefix\ncaller\n",
        );
    }
}

#[test]
fn unit_for_pending_receiver_and_argument_preserve_distinct_loan_identities() {
    let provider = format!(
        r#"package p
{LEAF}
class Host(val leaf: Leaf) {{
    fun sink(other: Leaf, own flag: Boolean): Unit {{ println("unexpected call") }}
    deinit() {{ println("receiver") }}
}}
fun work(own flag: Boolean): Unit {{
    for (_ in arrayOf(Leaf("element"))) {{
        Host(Leaf("receiver field")).sink(Leaf("argument"), if(flag) {{ return }} else {{ true }})
    }}
    println("unexpected after")
}}
"#
    );
    let consumer = "package q\nfun entry(): Unit { p.work(true); println(\"caller\") }";
    for constants in [false, true] {
        let (run, ir) = run_unit(&provider, consumer, constants);
        let expected = b"argument\nreceiver\nreceiver field\nelement\ncaller\n";
        assert_success(&run, expected);
        assert_success(&run_counted_allocations(&ir, 5), expected);
    }
}

#[test]
fn unit_for_abort_does_not_unwind_element_source_or_pending_owners() {
    let provider = format!(
        r#"package p
{LEAF}
fun sink(leaf: Leaf, own flag: Boolean): Unit {{ println("unexpected call") }}
fun work(): Unit {{
    val outer = Leaf("unexpected outer drop")
    for (_ in arrayOf(Leaf("unexpected element drop"))) {{
        val local = Leaf("unexpected local drop")
        println("before abort")
        sink(Leaf("unexpected prefix drop"), error("stop"))
    }}
    println("unexpected after")
}}
"#
    );
    let consumer = "package q\nfun entry(): Unit { p.work(); println(\"unexpected caller\") }";
    for constants in [false, true] {
        let (run, _) = run_unit(&provider, consumer, constants);
        assert!(!run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"before abort\n");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(run.status.signal(), Some(6));
        }
    }
}

#[test]
fn unit_for_element_receiver_and_argument_reuse_the_callers_loan() {
    let provider = r#"package p
class Host(val name: String) {
    fun inspect(other: Host, own flag: Boolean): Unit { println("unexpected call") }
    deinit() { println(this.name) }
}
fun work(own flag: Boolean): Unit {
    for (host in arrayOf(Host("element"))) {
        host.inspect(host, if(flag) { return } else { true })
    }
    println("unexpected after")
}"#;
    let consumer = "package q\nfun entry(): Unit { p.work(true); println(\"caller\") }";
    for constants in [false, true] {
        let (run, ir) = run_unit(provider, consumer, constants);
        assert_success(&run, b"element\ncaller\n");
        assert_success(&run_counted_allocations(&ir, 2), b"element\ncaller\n");
    }
}

#[test]
fn unit_for_pending_field_replace_keeps_previous_owner_and_ends_exclusive_loan() {
    let provider = format!(
        r#"package p
{LEAF}
class Holder(var state: Leaf) {{ deinit() {{ println("parent") }} }}
fun work(own flag: Boolean): Unit {{
    for (_ in listOf(1)) {{
        val holder = Holder(Leaf("old"))
        val previous = replace(&holder.state, if(flag) {{ return }} else {{ Leaf("new") }})
        println("committed")
    }}
    println("after")
}}
"#
    );
    let consumer =
        "package q\nfun entry(): Unit { p.work(true); p.work(false); println(\"caller\") }";
    let expected = b"parent\nold\ncommitted\nold\nparent\nnew\nafter\ncaller\n";
    for constants in [false, true] {
        let (run, ir) = run_unit(&provider, consumer, constants);
        assert_success(&run, expected);
        assert_success(
            &crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
                &ir,
                &[1, 2, 0, 4, 6, 5, 3],
            ),
            expected,
        );
    }
}

#[test]
fn unit_for_resource_value_components_return_cleans_nested_sources_once() {
    let provider = format!(
        "package p\n{LEAF}\nvalue class Parts(val number: Int, val leaf: Leaf, val text: String)\nfun inspect(leaf: Leaf): Unit {{ println(\"inspect\") }}\nfun work(): Int {{ for ((number, leaf, text) in arrayOf(Parts(7, Leaf(\"outer element\"), \"text\"))) {{ for (_ in listOf(Leaf(\"inner element\"))) {{ inspect(leaf); println(text); return number }} }}; return 0 }}"
    );
    let consumer = "package q\nfun entry(): Unit { if(p.work() != 7) { error(\"bad copy return\") }; println(\"caller\") }";
    let expected = b"inspect\ntext\ninner element\nouter element\ncaller\n";
    for constants in [false, true] {
        let (run, ir) = run_unit(&provider, consumer, constants);
        assert_success(&run, expected);
        assert_success(
            &crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
                &ir,
                &[2, 3, 0, 1],
            ),
            expected,
        );
    }
}
