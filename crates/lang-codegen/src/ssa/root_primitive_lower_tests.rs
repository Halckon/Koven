//! SPEC-0244：单文件 owned root 原语的 SSA 与 LLVM 闭环。
use super::*;

fn lower_primitive(text: &str) -> crate::ssa::model::Program {
    let analysis = analyze(text);
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap_or_else(|error| {
        panic!(
            "{text}\n{error:?}\nplans: {:?}",
            analysis.owned.ownership_primitives()
        )
    })
}

#[test]
fn root_primitive_single_replace_swap_string() {
    let program = lower_primitive(
        r#"
        fun test(): String {
            var a = "old" + "a"
            var b = "old" + "b"
            val old = replace(&a, "new" + "a")
            println(old)
            swap(&a, &b)
            println(a)
            return b
        }
    "#,
    );
    let ssa = render_program(&program);
    assert!(ssa.contains("root_replace"), "{ssa}");
    assert!(ssa.contains("root_swap"), "{ssa}");
    render_verified_program(&program).expect("root owners verify through LLVM");
}

#[test]
fn root_primitive_single_existing_storage_families() {
    for (declarations, first, second) in [
        ("", "1", "2"),
        ("class Node(val id: Int)", "Node(1)", "Node(2)"),
        (
            "value class Value(val value: Int)",
            "Box(Value(1))",
            "Box(Value(2))",
        ),
        ("", "Rc(1)", "Rc(2)"),
        ("", "listOf(1)", "listOf(2)"),
        (
            "value class Pair(val text: String)",
            "Pair(\"one\")",
            "Pair(\"two\")",
        ),
        ("value class Pair(val number: Int)", "Pair(1)", "Pair(2)"),
    ] {
        let program = lower_primitive(&format!(
            "{declarations}\nfun test(): Unit {{ var a = {first}\nvar b = {second}\nval old = replace(&a, {second})\nswap(&a, &b) }}"
        ));
        render_verified_program(&program).unwrap_or_else(|error| panic!("{first}: {error:?}"));
    }
}

#[test]
fn root_primitive_single_later_value_cfg_and_control_exit() {
    for value in [
        "if (flag) \"new\" else \"other\"",
        "if (flag) return else \"new\"",
        "error(\"stop\")",
    ] {
        let program = lower_primitive(&format!(
            "fun test(flag: Boolean): Unit {{ var a = \"old\"\nval old = replace(&a, {value}) }}"
        ));
        render_verified_program(&program).unwrap_or_else(|error| panic!("{value}: {error:?}"));
    }
}

#[test]
fn root_primitive_single_grouped_named_nested_and_nullable_owners() {
    for text in [
        "fun test(): Unit { var a = \"a\"\nvar b = \"b\"\nval new = \"new\"\nval old = replace(&(a), new)\nval older = replace(&a, replace(&b, \"last\"))\nprintln(old)\nprintln(older)\nprintln(a)\nprintln(b) }",
        "class Node(val id: Int)\nfun test(): Unit { var a: Node? = Node(1)\nvar b: Node? = Node(2)\nval old = replace(&a, Node(3))\nswap(&a, &b) }",
        "enum class State { One(value: String), Two(value: String) }\nfun test(): Unit { var a: State = State.One(\"a\")\nvar b: State = State.Two(\"b\")\nval old = replace(&a, State.Two(\"new\"))\nswap(&a, &b) }",
    ] {
        let program = lower_primitive(text);
        render_verified_program(&program).unwrap_or_else(|error| panic!("{text}: {error:?}"));
    }
}

#[test]
fn root_primitive_single_unit_storage() {
    let program = lower_primitive(
        "fun unit(): Unit {}\nfun test(): Unit { var a = unit()\nvar b = unit()\nval old = replace(&a, unit())\nswap(&a, &b)\nold\nreturn }",
    );
    render_verified_program(&program).expect("zero-sized root storage verifies");
}

#[test]
fn root_primitive_single_unit_control_exit_keeps_source_binding_shape() {
    let program = lower_primitive(
        "fun unit(): Unit {}\nfun test(flag: Boolean): Unit { var a = unit()\nloop { val old = replace(&a, if (flag) break else unit())\nbreak }\nvar b = unit()\nswap(&a, &b) }",
    );
    render_verified_program(&program).expect("Unit root exits preserve source binding shape");
}
