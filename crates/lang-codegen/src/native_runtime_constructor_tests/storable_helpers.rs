//! Generic Borrow Fn helpers with existing nested and nominal storable elements.
use super::*;

/// Keep the helper separate for the unit facade's real p-to-q source import.
pub(crate) fn cases() -> Vec<(RuntimeCase, String)> {
    let mut cases = Vec::new();
    for (element, declarations, constructor, projection) in [
        ("Array<Int>", "", "arrayOf", "value[0]"),
        ("List<Int>", "", "listOf", "value[0]"),
        (
            "Wrapped",
            "value class Wrapped(val number: Int)",
            "Wrapped",
            "value.number",
        ),
        (
            "Holder<Int>",
            "class Holder<T>(val number: T)",
            "Holder<Int>",
            "value.number",
        ),
    ] {
        for container in ["Array", "List"] {
            for (environment, captured, moved) in [
                ("pointer", false, false),
                ("shared", true, false),
                ("owned", true, true),
            ] {
                for explicit in [true, false] {
                    let number = if captured { "index + scale" } else { "index" };
                    let expected = if captured { 9 } else { 2 };
                    let literal = format!(
                        "{}{{ index -> println(\"call\")\n{constructor}({number}) }}",
                        if moved { "move " } else { "" }
                    );
                    let nested_check = if constructor == "arrayOf" || constructor == "listOf" {
                        "if (value.size != 1) { error(\"nested length\") }\n"
                    } else {
                        ""
                    };
                    let callee = if explicit {
                        format!("generate<{element}>")
                    } else {
                        "generate".to_owned()
                    };
                    let api = format!(
                        "fun <T> generate(size: Int, initializer: (Int)->T): {container}<T> = {container}<T>(size, initializer)"
                    );
                    let text = format!(
                        r#"{declarations}
fun verify(value: {element}): Unit {{
    {nested_check}if ({projection} != {expected}) {{ error("element value") }}
}}
fun entry(): Int {{
    val scale = 7
    val callback: (Int)->{element} = {literal}
    val items = {callee}(3, callback)
    verify(items[2])
    return items.size
}}
fun nativeEntry(): Unit {{ if (entry() != 3) {{ error("logical length") }} }}"#
                    );
                    cases.push((
                        RuntimeCase {
                            text,
                            label: format!(
                                "storable-helper/{element}/{container}/{environment}/explicit{explicit}"
                            ),
                            length: 3,
                            resource: false,
                            element,
                            container,
                            named: true,
                        },
                        api,
                    ));
                }
            }
        }
    }
    cases
}

#[test]
fn runtime_constructor_native_single_storable_helper_elements() {
    let cases = cases();
    assert_eq!(cases.len(), 48);
    for (case, api) in cases {
        let text = format!("{api}\n{}", case.text);
        let output = emit_link_and_run("runtime-storable-helper.ko", &text, "nativeEntry");
        assert_output(&case, &output);
    }
}
