//! SPEC-0296: ordinary-source last-use reads precede owned nullable cleanup.
use super::resource_exchange_tests::run_unit;
use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};

const KNOWN_BOUNDARY: &str = include_str!(
    "../../../../docs/development/evidence/map-word-frequency-0295/known-boundary/src/p/provider.ko"
);

pub(super) fn run_single(source: &str, body: &str, expected: &str) {
    let source = format!("{source}\nfun main(): Unit {{ {body} }}");
    let run = emit_link_and_run("owned-null-cleanup.ko", &source, "main");
    assert_success(&run, expected.as_bytes());
}

pub(super) fn run_units(source: &str, body: &str, expected: &str) {
    let provider = format!("package p\n{source}");
    let consumer = format!("package q\nfun entry(): Unit {{ {body} }}");
    for constants in [false, true] {
        let (run, _) = run_unit(&provider, &consumer, constants);
        assert_success(&run, expected.as_bytes());
    }
}

#[test]
fn unit_owned_null_cleanup_known_loop_boundary() {
    run_units(
        KNOWN_BOUNDARY.strip_prefix("package p\n").unwrap(),
        "p.exercise(arrayOf(\"key\", \"key\"))",
        "text present\nkey\nkey\ndone\n",
    );
}

const LAST_USE: &str = r#"
fun exercise(): Unit {
    var texts = mutableMapOf<Int, String>()
    texts.put(1, "value".clone())
    val value = texts.remove(1)
    if (value != null) { println("present") } else { error("missing") }
    val absent = texts.remove(1)
    if (absent == null) { println("missing") } else { error("present") }
}
"#;

#[test]
fn single_owned_null_cleanup_without_loop() {
    run_single(LAST_USE, "exercise()", "present\nmissing\n");
}

#[test]
fn unit_owned_null_cleanup_without_loop() {
    run_units(LAST_USE, "p.exercise()", "present\nmissing\n");
}

#[test]
fn single_owned_null_cleanup_scalar_last_read() {
    run_single(&scalar_last_read(), "exercise()", "present\nmissing\n");
}

#[test]
fn unit_owned_null_cleanup_scalar_last_read() {
    run_units(&scalar_last_read(), "p.exercise()", "present\nmissing\n");
}

fn scalar_last_read() -> String {
    LAST_USE
        .replace(
            "if (value != null)",
            "val present = value != null\n    if (present)",
        )
        .replace(
            "if (absent == null)",
            "val missing = absent == null\n    if (missing)",
        )
}

fn comparison_forms() -> (String, String) {
    let mut source = "value class Text(val text: String)\n".to_owned();
    let mut calls = String::new();
    let mut expected = String::new();
    for (index, condition) in [
        "value != null",
        "null != value",
        "((value)) != null",
        "value == null",
        "null == (value)",
        "((value)) == null",
    ]
    .iter()
    .enumerate()
    {
        for (label, ty, payload) in [
            ("string", "String", "\"text\".clone()"),
            ("inline", "Text", "Text(\"text\".clone())"),
        ] {
            let absent = condition.replace("value", "absent");
            source.push_str(&format!(
                "fun {label}{index}(): Unit {{\n var m = mutableMapOf<Int, {ty}>()\n m.put(1, {payload})\n val value = m.remove(1)\n val present = {condition}\n if (present) {{ println(\"yes\") }} else {{ println(\"no\") }}\n val absent = m.remove(1)\n val missing = {absent}\n if (missing) {{ println(\"yes\") }} else {{ println(\"no\") }}\n}}\n"
            ));
            calls.push_str(&format!("{label}{index}(); "));
            expected.push_str(if index < 3 { "yes\nno\n" } else { "no\nyes\n" });
        }
    }
    source.push_str(&format!("fun exercise(): Unit {{ {calls} }}"));
    (source, expected)
}

#[test]
fn single_owned_null_cleanup_grouped_and_mirrored_reads() {
    let (source, expected) = comparison_forms();
    run_single(&source, "exercise()", &expected);
}

#[test]
fn unit_owned_null_cleanup_grouped_and_mirrored_reads() {
    let (source, expected) = comparison_forms();
    run_units(&source, "p.exercise()", &expected);
}

const REUSE_AND_CONSUME: &str = r#"
class Resource(val text: String) { deinit() { println(this.text) } }
fun consume(own value: Resource): Unit { println("consume") }
fun exercise(): Unit {
    var texts = mutableMapOf<Int, String>()
    texts.put(1, "text".clone())
    val value = texts.remove(1)
    val present = value != null
    if (!present) { error("missing text") }
    val missing = null == value
    if (missing) { error("lost text") }
    println(value!!)
    var resources = mutableMapOf<Int, Resource>()
    resources.put(1, Resource("consumed"))
    resources.put(2, Resource("lexical"))
    val consumed = resources.remove(1)
    val available = consumed != null
    if (!available) { error("missing consumed resource") }
    consume(consumed!!)
    val lexical = resources.remove(2)
    val isPresent = lexical != null
    if (!isPresent) { error("missing resource") }
    println("before scope exit")
}
"#;

#[test]
fn single_owned_null_cleanup_preserves_reuse_consumption_and_lexical_resource() {
    run_single(
        REUSE_AND_CONSUME,
        "exercise()",
        "text\nconsume\nconsumed\nbefore scope exit\nlexical\n",
    );
}

#[test]
fn unit_owned_null_cleanup_preserves_reuse_consumption_and_lexical_resource() {
    run_units(
        REUSE_AND_CONSUME,
        "p.exercise()",
        "text\nconsume\nconsumed\nbefore scope exit\nlexical\n",
    );
}

#[test]
fn unit_owned_null_cleanup_loop_resource_exits() {
    for (exit, expected) in [
        (
            "",
            "present\nbody\ntail\none\nbody\ntail\npresent\nbody\ntail\ntwo\nafter\ndone\n",
        ),
        (
            "continue",
            "present\nbody\none\nbody\npresent\nbody\ntwo\nafter\ndone\n",
        ),
        ("break", "present\nbody\none\nafter\ntwo\ndone\n"),
        ("return", "present\nbody\none\ntwo\ndone\n"),
    ] {
        let source = format!(
            r#"
class Resource(val text: String) {{ deinit() {{ println(this.text) }} }}
fun exercise(args: Array<Int>): Unit {{
    var resources = mutableMapOf<Int, Resource>()
    resources.put(1, Resource("one"))
    resources.put(2, Resource("two"))
    for (key in args) {{
        val removed = resources.remove(key)
        if (removed != null) {{ println("present") }}
        println("body")
        {exit}
        println("tail")
    }}
    println("after")
}}
"#
        );
        run_units(
            &source,
            "p.exercise(arrayOf(1, 1, 2)); println(\"done\")",
            expected,
        );
    }
}

#[test]
fn unit_owned_null_cleanup_zero_and_one_iteration() {
    run_units(
        KNOWN_BOUNDARY.strip_prefix("package p\n").unwrap(),
        "p.exercise(arrayOf<String>()); p.exercise(arrayOf(\"key\"))",
        "done\ntext present\nkey\ndone\n",
    );
}
