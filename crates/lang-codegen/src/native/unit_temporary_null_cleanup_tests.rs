//! SPEC-0296 phase two: temporary owners end after presence, before branch bodies.
use super::owned_null_cleanup_tests::{run_single, run_units};

fn comparison_forms() -> (String, String) {
    let mut source =
        "value class Text(val text: String)\nclass Resource { deinit() { println(\"drop\") } }\n"
            .to_owned();
    let mut calls = String::new();
    let mut expected = String::new();
    for (index, condition) in [
        "$ != null",
        "null != $",
        "(($)) != null",
        "$ == null",
        "null == (($))",
        "(($)) == null",
    ]
    .iter()
    .enumerate()
    {
        for (label, ty, value) in [
            ("string", "String", "\"text\".clone()"),
            ("inline", "Text", "Text(\"text\".clone())"),
            ("resource", "Resource", "Resource()"),
        ] {
            let condition = condition.replace('$', "m.remove(1)");
            source.push_str(&format!("fun {label}{index}(): Unit {{\n var m = mutableMapOf<Int, {ty}>()\n m.put(1, {value})\n val first = {condition}\n if (first) {{ println(\"yes\") }} else {{ println(\"no\") }}\n if ({condition}) {{ println(\"yes\") }} else {{ println(\"no\") }}\n}}\n"));
            calls.push_str(&format!("{label}{index}(); "));
            if label == "resource" {
                expected.push_str("drop\n");
            }
            expected.push_str(if index < 3 { "yes\nno\n" } else { "no\nyes\n" });
        }
    }
    source.push_str(&format!("fun exercise(): Unit {{ {calls} }}"));
    (source, expected)
}

#[test]
fn single_temporary_null_cleanup_scalar_and_if_matrix() {
    let (source, expected) = comparison_forms();
    run_single(&source, "exercise()", &expected);
}

#[test]
fn unit_temporary_null_cleanup_scalar_and_if_matrix() {
    let (source, expected) = comparison_forms();
    run_units(&source, "p.exercise()", &expected);
}

const RESOURCE_TIMING: &str = r#"
class Resource(val text: String) { deinit() { println(this.text) } }
fun exercise(): Unit {
    var m = mutableMapOf<Int, Resource>()
    m.put(1, Resource("temporary drop"))
    if (((m.remove(1))) != null) { println("temporary body") }
    if (null == ((m.remove(1)))) { println("missing body") }
    m.put(2, Resource("named drop"))
    val value = m.remove(2)
    if (value != null) { println("named body") }
    println("before scope exit")
}
"#;

#[test]
fn single_temporary_null_cleanup_resource_precedes_body_named_stays_lexical() {
    run_single(
        RESOURCE_TIMING,
        "exercise()",
        "temporary drop\ntemporary body\nmissing body\nnamed body\nbefore scope exit\nnamed drop\n",
    );
}

#[test]
fn unit_temporary_null_cleanup_resource_precedes_body_named_stays_lexical() {
    run_units(
        RESOURCE_TIMING,
        "p.exercise()",
        "temporary drop\ntemporary body\nmissing body\nnamed body\nbefore scope exit\nnamed drop\n",
    );
}
