//! SPEC-0317: real project sources, native short-circuit selection and range reuse.
use super::*;

#[derive(Clone, Copy, Debug)]
enum Site {
    Initializer,
    Argument,
    StatementIf,
}

#[derive(Clone, Copy, Debug)]
enum Payload {
    String,
    MoveOnly,
    Resource,
}

fn run_source(label: &str, source: &str, expected: &str) {
    let project = Project::new();
    let built = project.build(source);
    assert!(built.status.success(), "{label}: {built:?}");
    assert!(built.stdout.is_empty(), "{label}: {built:?}");
    assert!(built.stderr.is_empty(), "{label}: {built:?}");
    let run = Command::new(project.0.join("program")).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{label}: {run:?}");
    assert_eq!(run.stdout, expected.as_bytes(), "{label}: {run:?}");
    assert!(run.stderr.is_empty(), "{label}: {run:?}");
}

fn dynamic_case(site: Site, payload: Payload, siblings: bool, constant: bool, operator: &str) {
    let (declaration, element, values, read, drops) = match payload {
        Payload::String => (
            "",
            "String",
            "\"a\".clone(),\"b\".clone(),\"c\".clone()",
            "println(item)",
            "",
        ),
        Payload::MoveOnly => (
            "class Item(val text:String){}",
            "Item",
            "Item(\"a\"),Item(\"b\"),Item(\"c\")",
            "println(item.text)",
            "",
        ),
        Payload::Resource => (
            "class Item(val text:String){deinit(){println(\"drop\");println(this.text)}}",
            "Item",
            "Item(\"a\"),Item(\"b\"),Item(\"c\")",
            "println(item.text)",
            "drop\nc\ndrop\nb\ndrop\na\n",
        ),
    };
    // Project loading installs the real trusted range std. Merely declaring a
    // constant selects validated constant-unit compilation, rather than basic.
    let constants = if constant {
        "const val COUNT:Int=2"
    } else {
        ""
    };
    let count = if constant { "COUNT" } else { "2" };
    let (bindings, reads, items) = if siblings {
        (
            "borrow val child=take(parent,1);borrow val sibling=take(parent,2)",
            "read(parent);read(child);read(sibling)",
            "a\nb\na\na\nb\n",
        )
    } else {
        ("", "read(parent)", "a\nb\n")
    };
    let expression = format!("left(lhs) {operator} right(rhs)");
    let statement = match site {
        Site::Initializer => format!("val result={expression};show(result)"),
        Site::Argument => format!("show({expression})"),
        Site::StatementIf => format!(
            "if({expression}){{println(\"true\");{reads}}}else{{println(\"false\");{reads}}}"
        ),
    };
    let source = format!(
        r#"package app
import koven.algorithms.take
{constants}
{declaration}
fun left(value:Boolean):Boolean{{println("lhs");return value}}
fun right(value:Boolean):Boolean{{println("rhs");return value}}
fun show(value:Boolean):Unit{{if(value){{println("true")}}else{{println("false")}}}}
fun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}
fun consume(own root:List<{element}>):Unit{{println("consume")}}
fun scan(lhs:Boolean,rhs:Boolean):Unit{{
    val root=listOf({values})
    {{
        borrow val parent=take(root,{count})
        {bindings}
        {statement}
        println("join")
        {reads}
    }}
    consume(root)
}}
fun main():Unit{{scan(false,false);scan(false,true);scan(true,false);scan(true,true)}}
"#
    );
    let mut expected = String::new();
    for (lhs, rhs) in [(false, false), (false, true), (true, false), (true, true)] {
        expected.push_str("lhs\n");
        if (operator == "&&" && lhs) || (operator == "||" && !lhs) {
            expected.push_str("rhs\n");
        }
        let result = if operator == "&&" {
            lhs && rhs
        } else {
            lhs || rhs
        };
        expected.push_str(if result { "true\n" } else { "false\n" });
        if matches!(site, Site::StatementIf) {
            expected.push_str(items);
        }
        expected.push_str(&format!("join\n{items}consume\n{drops}"));
    }
    run_source(
        &format!("{site:?}/{payload:?}/{siblings}/{constant}/{operator}"),
        &source,
        &expected,
    );
}

fn dynamic_site(site: Site) {
    for constant in [false, true] {
        for operator in ["&&", "||"] {
            dynamic_case(site, Payload::String, false, constant, operator);
        }
    }
}

#[test]
fn public_std_range_boolean_short_circuit_local_initializer() {
    dynamic_site(Site::Initializer);
}

#[test]
fn public_std_range_boolean_short_circuit_call_argument() {
    dynamic_site(Site::Argument);
}

#[test]
fn public_std_range_boolean_short_circuit_statement_if() {
    dynamic_site(Site::StatementIf);
}

#[test]
fn public_std_range_boolean_short_circuit_same_root_cleanup() {
    for payload in [Payload::String, Payload::MoveOnly, Payload::Resource] {
        for constant in [false, true] {
            for operator in ["&&", "||"] {
                dynamic_case(Site::StatementIf, payload, true, constant, operator);
            }
        }
    }
}

#[test]
fn public_std_range_boolean_short_circuit_direct_literal_and_constant_left() {
    for constant in [false, true] {
        let declarations = if constant {
            "const val OFF:Boolean=false\nconst val ON:Boolean=true"
        } else {
            ""
        };
        let (off, on) = if constant {
            ("OFF", "ON")
        } else {
            ("false", "true")
        };
        let source = format!(
            r#"package app
import koven.algorithms.take
{declarations}
fun right(value:Boolean):Boolean{{println("rhs");return value}}
fun show(value:Boolean):Unit{{if(value){{println("true")}}else{{println("false")}}}}
fun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}
fun consume(own root:List<String>):Unit{{println("consume")}}
fun scan(rhs:Boolean):Unit{{
    val root=listOf("kept","excluded")
    {{
        borrow val parent=take(root,1)
        val neverAnd={off} && right(rhs)
        show(neverAnd)
        read(parent)
        val alwaysAnd={on} && right(rhs)
        show(alwaysAnd)
        read(parent)
        show({off} || right(rhs))
        read(parent)
        show({on} || right(rhs))
        read(parent)
    }}
    consume(root)
}}
fun main():Unit{{scan(false);scan(true)}}
"#
        );
        run_source(
            &format!("direct-left/constant={constant}"),
            &source,
            "false\nkept\nrhs\nfalse\nkept\nrhs\nfalse\nkept\ntrue\nkept\nconsume\nfalse\nkept\nrhs\ntrue\nkept\nrhs\ntrue\nkept\ntrue\nkept\nconsume\n",
        );
    }
}

#[test]
fn public_std_range_boolean_short_circuit_bounded_source_rejections() {
    let expression = "left(lhs) && right(rhs)";
    let scoped = |body: &str| format!("{{borrow val parent=take(root,1);{body};read(parent)}}");
    let cases = [
        (
            "nested-left",
            scoped("show((left(lhs) && right(rhs)) || rhs)"),
        ),
        (
            "nested-right",
            scoped("show(left(lhs) && (right(rhs) || rhs))"),
        ),
        (
            "late-binding",
            scoped(&format!(
                "show({expression});borrow val child=take(parent,1);read(child)"
            )),
        ),
        (
            "metadata-alias",
            scoped(&format!(
                "borrow val alias=parent;show({expression});read(alias)"
            )),
        ),
        (
            "ordinary-borrow",
            scoped(&format!(
                "val text=\"text\";borrow val alias=text;show({expression});println(alias)"
            )),
        ),
        (
            "different-roots",
            format!(
                "val other=listOf(\"other\");{{borrow val parent=take(root,1);borrow val sibling=take(other,1);show({expression});read(parent);read(sibling)}};consume(other)"
            ),
        ),
        (
            "before-binding",
            format!("show({expression});{{borrow val parent=take(root,1);read(parent)}}"),
        ),
        (
            "after-scope",
            format!("{{borrow val parent=take(root,1);read(parent)}};show({expression})"),
        ),
        (
            "value-if",
            scoped(&format!(
                "val value=if({expression}){{1}}else{{2}};number(value)"
            )),
        ),
        (
            "new-arm-binding",
            scoped(&format!(
                "if({expression}){{borrow val child=take(parent,1);read(child)}}else{{read(parent)}}"
            )),
        ),
        (
            "loop",
            scoped(&format!(
                "while(lhs){{show({expression});read(parent);break}}"
            )),
        ),
        (
            "hidden-return",
            scoped("show(left(lhs) && (if(rhs){return}else{true}))"),
        ),
    ];
    for constant in [false, true] {
        let declaration = if constant {
            "const val ON:Boolean=true"
        } else {
            ""
        };
        for (label, body) in &cases {
            let project = Project::new();
            let built = project.build(&format!(
                r#"package app
import koven.algorithms.take
{declaration}
fun left(value:Boolean):Boolean=value
fun right(value:Boolean):Boolean=value
fun show(value:Boolean):Unit{{}}
fun number(value:Int):Unit{{}}
fun read(view:View<String>):Unit{{}}
fun consume(own root:List<String>):Unit{{}}
fun scan(lhs:Boolean,rhs:Boolean):Unit{{val root=listOf("kept");{body};consume(root)}}
fun main():Unit{{scan(true,false)}}
"#
            ));
            assert!(!built.status.success(), "{label}/{constant}: {built:?}");
            // Legal frontend source must reach the bounded native capability gate.
            // A parser/type/ownership diagnostic is not a passing rejection.
            assert!(
                String::from_utf8_lossy(&built.stderr).contains(
                    "native object UnsupportedSource: frontend lowering failed with UnsupportedNode"
                ),
                "{label}/{constant}: {built:?}"
            );
            assert!(!project.0.join("program").exists(), "{label}/{constant}");
        }
    }
}
