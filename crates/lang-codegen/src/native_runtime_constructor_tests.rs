//! Ordinary source object/link/run: logical callback count, length, and Resource cleanup.
use super::*;

#[path = "native_runtime_constructor_tests/evaluation.rs"]
pub(crate) mod evaluation;

#[derive(Clone)]
pub(crate) struct RuntimeCase {
    pub(crate) text: String,
    pub(crate) label: String,
    pub(crate) length: usize,
    resource: bool,
    element: &'static str,
    container: &'static str,
    named: bool,
}

/// Separate native intent from SSA layout checks: callback side effects must execute at runtime.
pub(crate) fn cases() -> Vec<RuntimeCase> {
    let mut cases = Vec::new();
    for (element, declarations, source, bodies) in [
        (
            "Int",
            "",
            "val scale = 7",
            ["index", "index + scale", "index + scale"],
        ),
        (
            "String",
            "",
            "val scale = \"suffix\"",
            ["\"value\"", "\"value\" + scale", "\"value\" + scale"],
        ),
        (
            "Leaf",
            "class Leaf(val number: Int) { deinit() { println(\"leaf\") } }",
            "val scale = Leaf(7)",
            [
                "Leaf(index)",
                "Leaf(index + scale.number)",
                "Leaf(index + scale.number)",
            ],
        ),
        (
            "Unit",
            "fun touch(index: Int): Unit {}",
            "val scale = 7",
            [
                "touch(index)",
                "touch(index + scale)",
                "touch(index + scale)",
            ],
        ),
    ] {
        for container in ["Array", "List"] {
            for (environment, body) in bodies.iter().enumerate() {
                for named in [false, true] {
                    for length in [0, 1, 3] {
                        let literal = format!(
                            "{}{{ index -> println(\"call\")\n{body} }}",
                            if environment == 2 { "move " } else { "" }
                        );
                        let declaration = if named {
                            format!("val callback: (Int)->{element} = {literal}\n")
                        } else {
                            String::new()
                        };
                        let initializer = if named { "callback" } else { &literal };
                        let value_check = if length == 0 || element == "Unit" {
                            String::new()
                        } else {
                            format!("verify(items[{}])\n", length - 1)
                        };
                        let number = if length == 0 { 0 } else { length - 1 }
                            + if environment == 0 { 0 } else { 7 };
                        let verifier = match element {
                            "Int" => format!(
                                "fun verify(value: Int): Unit {{ if (value != {number}) {{ error(\"element value\") }} }}"
                            ),
                            "String" => format!(
                                "fun verify(value: String): Unit {{ if (value != \"{}\") {{ error(\"element value\") }} }}",
                                if environment == 0 {
                                    "value"
                                } else {
                                    "valuesuffix"
                                }
                            ),
                            "Leaf" => format!(
                                "fun verify(value: Leaf): Unit {{ if (value.number != {number}) {{ error(\"element value\") }} }}"
                            ),
                            _ => String::new(),
                        };
                        cases.push(RuntimeCase {
                            text: format!("{declarations}\n{verifier}\nfun entry(): Int {{ {source}\n{declaration}val items = {container}<{element}>({length}, {initializer})\n{value_check}return items.size }}\nfun nativeEntry(): Unit {{ if (entry() != {length}) {{ error(\"logical length\") }} }}"),
                            label: format!("{element}/{container}/env{environment}/named{named}/length{length}"),
                            length,
                            resource: element == "Leaf",
                            element,
                            container,
                            named,
                        });
                    }
                }
            }
        }
    }
    cases
}

pub(crate) fn assert_output(case: &RuntimeCase, output: &Output) {
    assert!(output.status.success(), "{}: {output:?}", case.label);
    assert!(output.stderr.is_empty(), "{}: {output:?}", case.label);
    let text = std::str::from_utf8(&output.stdout).expect("fixture stdout is UTF-8");
    let calls = text.lines().filter(|line| *line == "call").count();
    let drops = text.lines().filter(|line| *line == "leaf").count();
    assert_eq!(
        calls, case.length,
        "{}: exactly one invocation per logical element, including Unit zero stride",
        case.label
    );
    assert_eq!(
        drops,
        if case.resource { case.length + 1 } else { 0 },
        "{}: Resource deinitialization total matches elements plus capture source",
        case.label
    );
    assert_eq!(
        text.lines().count(),
        calls + drops,
        "{}: unexpected side effects",
        case.label
    );
}

#[test]
fn runtime_constructor_native_single_matrix_executes_logical_callbacks() {
    let cases = cases();
    assert_eq!(cases.len(), 144);
    for case in &cases {
        let output = emit_link_and_run("runtime-constructor.ko", &case.text, "nativeEntry");
        assert_output(case, &output);
    }
}

// Ordinary observable ordering; no allocator substitution, fault injection, or sanitizer.
pub(crate) fn ordered_resource_cases() -> Vec<(String, &'static str)> {
    let mut cases = Vec::new();
    for container in ["Array", "List"] {
        for (initializer, expected) in [
            (
                "{ index -> make(index) }",
                "call-zero\ncall-one\ncall-two\nafter\ntwo\none\nzero\nenv\n",
            ),
            (
                "{ index -> println(scale.name)\nmake(index) }",
                "env\ncall-zero\nenv\ncall-one\nenv\ncall-two\nafter\ntwo\none\nzero\nenv\n",
            ),
            (
                "move { index -> println(scale.name)\nmake(index) }",
                "env\ncall-zero\nenv\ncall-one\nenv\ncall-two\nenv\nafter\ntwo\none\nzero\n",
            ),
        ] {
            let text = format!(
                r#"class Resource(val name: String) {{ deinit() {{ println(this.name) }} }}
fun make(index: Int): Resource {{
    if (index == 0) {{ println("call-zero")
return Resource("zero") }}
    if (index == 1) {{ println("call-one")
return Resource("one") }}
    println("call-two")
    return Resource("two")
}}
fun entry(): Unit {{
    val scale = Resource("env")
    val items = {container}<Resource>(3, {initializer})
    println("after")
    if (items.size != 3) {{ error("length") }}
}}"#
            );
            cases.push((text, expected));
        }
    }
    cases
}

#[test]
fn runtime_constructor_native_single_orders_callbacks_elements_and_environment_cleanup() {
    for (text, expected) in ordered_resource_cases() {
        let output = emit_link_and_run("runtime-order.ko", &text, "entry");
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

/// Exercise the same value/count oracle through a generic Borrow Fn helper.
/// The API is separate so the unit entry can place it in another source file.
pub(crate) fn helper_cases() -> Vec<(RuntimeCase, String)> {
    cases()
        .into_iter()
        .filter(|case| case.named && case.length == 3)
        .flat_map(|case| {
            [true, false].into_iter().map(move |explicit| {
                let mut instance = case.clone();
                let constructor = format!("{}<{}>(3, callback)", case.container, case.element);
                assert_eq!(instance.text.matches(&constructor).count(), 1);
                let callee = if explicit {
                    format!("generate<{}>", case.element)
                } else {
                    "generate".to_owned()
                };
                instance.text = instance.text.replacen(&constructor, &format!("{callee}(3, callback)"), 1);
                let api = format!(
                    "fun <T> generate(size: Int, initializer: (Int)->T): {}<T> = {}<T>(size, initializer)",
                    case.container, case.container,
                );
                instance.label = format!("generic-helper/explicit{explicit}/{}", case.label);
                (instance, api)
            })
        })
        .collect()
}

#[test]
fn runtime_constructor_native_single_generic_helper_preserves_element_and_environment() {
    let cases = helper_cases();
    assert_eq!(cases.len(), 48);
    for (case, api) in cases {
        let text = format!("{api}\n{}", case.text);
        let output = emit_link_and_run("runtime-helper.ko", &text, "nativeEntry");
        assert_output(&case, &output);
    }
}
