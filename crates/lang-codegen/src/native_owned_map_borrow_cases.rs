//! SPEC-0299 ordinary source shared by structural SSA and unmodified native tests.
pub(crate) fn core(payload: &str, grouped: bool) -> String {
    let (declaration, value, read) = match payload {
        "String" => ("", "\"value\".clone()", "value"),
        "Text" => (
            "value class Text(val n: Int, val text: String)",
            "Text(7, \"value\".clone())",
            "\"borrowed inline\"",
        ),
        "Resource" => (
            "class Resource(val text: String) { deinit() { println(\"drop\") } }",
            "Resource(\"value\".clone())",
            "value.text",
        ),
        _ => panic!("selected payload"),
    };
    let condition = if grouped {
        "(removed != null)"
    } else {
        "removed != null"
    };
    let after = if payload == "Text" {
        "if (taken.n == 7) { println(\"owned inline\") } else { error(\"wrong payload\") }"
    } else {
        "observe(taken)"
    };
    format!(
        "{declaration}\nfun observe(value: {payload}) {{ println({read}) }}\nfun entry() {{\n var values = mutableMapOf<Int, {payload}>()\n values.put(1, {value})\n val removed = values.remove(1)\n if ({condition}) {{ observe(removed); observe((removed)) }}\n val taken = removed!!\n {after}\n println(\"end\")\n}}"
    )
}

pub(crate) const PAIR: &str = r#"
fun pair(own prefix: String, first: String, second: String) { println(prefix); println(first); println(second) }
fun entry() {
 var values = mutableMapOf<Int, String>()
 values.put(1, "value".clone())
 val removed = values.remove(1)
 if ((removed != null)) { pair("prefix".clone(), removed, (removed)) }
 println(removed!!)
}
"#;
pub(crate) const SCOPES: &str = r#"
fun observe(value: String) { println(value) }
fun entry() {
 var values = mutableMapOf<Int, String>()
 values.put(1, "first".clone()); values.put(2, "second".clone())
 val first = values.remove(1)
 val second = values.remove(2)
 if ((first == null)) { println("unexpected") } else {
   observe(first)
   if ((second != null)) { observe(second); observe(first) }
 }
 if (second != null) { observe(second) }
 println(first!!); println(second!!)
}
"#;
pub(crate) const MISSING: &str = r#"
fun observe(value: String) { println(value) }
fun entry() {
 var mutable = mutableMapOf<Int, String>()
 val removed = mutable.remove(1)
 if ((removed != null)) { observe(removed) } else { println("missing") }
 println("end")
}
"#;
pub(crate) const ABORT: &str = r#"
fun observe(value: String) { println(value) }
fun entry() {
 var values = mutableMapOf<Int, String>()
 val removed = values.remove(1)
 if ((removed != null)) { observe(removed) }
 val taken = removed!!
 observe(taken)
}
"#;
pub(crate) const RESOURCE_SCOPE: &str = r#"
class Resource(val text: String) { deinit() { println(this.text) } }
fun observe(value: Resource) { println(value.text) }
fun entry() {
 val outer = Resource("outer drop".clone())
 var values = mutableMapOf<Int, Resource>()
 values.put(1, Resource("removed drop".clone()))
 val removed = values.remove(1)
 if ((removed != null)) { observe(removed); observe((removed)) }
 println("end")
}
"#;
pub(crate) fn last_use(payload: &str) -> String {
    let source = core(payload, true);
    let (before, _) = source.split_once(" val taken = removed!!").unwrap();
    format!("{before} println(\"end\")\n}}")
}
pub(crate) fn inline_field(unit: bool) -> String {
    let source = core("Text", true);
    if unit {
        source.replace("value class Text(val n: Int, val text: String)", "value class Text(val n: Int, val text: String) { fun number(): Int = this.n }")
          .replace("println(\"borrowed inline\")", "if (value.number() == 7) { println(\"borrowed inline\") } else { error(\"wrong borrow\") }")
    } else {
        source.replace(
            "println(\"borrowed inline\")",
            "if (value.n == 7) { println(\"borrowed inline\") } else { error(\"wrong borrow\") }",
        )
    }
}
pub(crate) fn inline_explicit_clone() -> String {
    core("Text", true).replace(
        "println(\"borrowed inline\")",
        "println(value.text.clone())",
    )
}

pub(crate) fn missing(payload: &str, assert_non_null: bool) -> String {
    let source = if assert_non_null {
        core(payload, true)
    } else {
        last_use(payload)
    };
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("values.put("))
        .map(|line| {
            if !assert_non_null && line.contains("observe((removed)) }") {
                format!("{line} else {{ println(\"missing\") }}")
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn independent_map() -> String {
    core("String", true).replace(
        "val removed = values.remove(1)",
        "val removed = values.remove(1)\n values.put(1, \"replacement\".clone())",
    )
}

pub(crate) const BORROWED_PREFIX: &str = r#"
fun pair(prefix: String, first: String, second: String) { println(prefix); println(first); println(second) }
fun exercise(prefix: String) {
 var values = mutableMapOf<Int, String>()
 values.put(1, "value".clone())
 val removed = values.remove(1)
 if ((removed != null)) { pair(prefix, removed, (removed)) }
 println(removed!!)
}
fun entry() { exercise("prefix") }
"#;
