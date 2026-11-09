//! Same-root parent/child/sibling source fixtures with normal native execution.
use super::*;

#[derive(Clone, Copy, Debug)]
enum Payload {
    String,
    MoveOnly,
    Resource,
}

fn ownership_case(pipeline: Pipeline, payload: Payload) {
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
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"a\"),Item(\"b\"),Item(\"c\")",
            "println(item.text)",
            "c\nb\na\n",
        ),
    };
    let branch = "if(flag){read(parent);read(child);read(sibling)}else{read(parent);read(child);read(sibling)}";
    let consumer = format!(
        r#"
{declaration}
fun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}
fun consume(own root:List<{element}>):Unit{{println("consume")}}
fun scan(flag:Boolean):Unit{{
    val root=listOf({values})
    {{
        borrow val parent=take(root,2)
        borrow val child=take(parent,1)
        borrow val sibling=take(parent,2)
        {branch}
        read(parent)
        read(child)
        read(sibling)
    }}
    consume(root)
}}
fun main():Unit{{scan(true);scan(false)}}
"#
    );
    let label = format!("{pipeline:?}-{payload:?}-parent-child-sibling");
    let lowered = lower_positive(&consumer, branch, pipeline);
    let branch_span = lowered.expected_span;
    let (program, entry) = lowered.require_success(&label);
    assertions::if_transport(&program, branch_span, 3);
    let one_scan = format!("a\nb\na\na\nb\na\nb\na\na\nb\nconsume\n{drops}");
    run_unmodified(&program, entry, &label, one_scan.repeat(2).as_bytes());
}

macro_rules! ownership_tests {
    ($single:ident, $unit:ident, $reversed:ident, $payload:ident) => {
        #[test]
        fn $single() {
            ownership_case(Pipeline::Single, Payload::$payload);
        }
        #[test]
        fn $unit() {
            ownership_case(Pipeline::Unit, Payload::$payload);
        }
        #[test]
        fn $reversed() {
            ownership_case(Pipeline::UnitReversed, Payload::$payload);
        }
    };
}

ownership_tests!(
    range_local_control_string_siblings_single,
    range_local_control_string_siblings_unit,
    range_local_control_string_siblings_unit_reversed,
    String
);
ownership_tests!(
    range_local_control_move_only_siblings_single,
    range_local_control_move_only_siblings_unit,
    range_local_control_move_only_siblings_unit_reversed,
    MoveOnly
);
ownership_tests!(
    range_local_control_resource_siblings_single,
    range_local_control_resource_siblings_unit,
    range_local_control_resource_siblings_unit_reversed,
    Resource
);

fn carrier_scope_layout(case: &str, binding: &str, body: &str, expected: &[u8]) {
    let consumer = format!(
        r#"
class Item(val text:String){{deinit(){{println("drop");println(this.text)}}}}
fun read(view:View<Item>):Unit{{for(item in view){{println(item.text)}}}}
fun scan(own flag:Boolean):Unit{{
    val root=listOf(Item("first"),Item("second"))
    {body}
}}
fun main():Unit{{scan(true);scan(false)}}
"#
    );
    for (variant, suffix) in [
        ("plain", ""),
        (
            "sibling",
            "\nfun unrelated(): Unit { var a = 1; var b = 2; swap(&a, &b) }",
        ),
    ] {
        let consumer = format!("{consumer}{suffix}");
        for pipeline in [Pipeline::Single, Pipeline::Unit, Pipeline::UnitReversed] {
            let label = format!("carrier-{case}-{variant}-{pipeline:?}");
            let lowered = lower_positive(&consumer, binding, pipeline);
            let (program, entry) = lowered.require_success(&label);
            run_unmodified(&program, entry, &label, expected);
        }
    }
}

#[test]
fn range_carrier_gate_descriptor_ends_before_outer_if() {
    carrier_scope_layout(
        "descriptor-ended",
        "if(flag){println(\"after\")}",
        "{borrow val parent=take(root,1);read(parent)}\nif(flag){println(\"after\")}",
        b"first\nafter\ndrop\nsecond\ndrop\nfirst\nfirst\ndrop\nsecond\ndrop\nfirst\n",
    );
}

#[test]
fn range_carrier_gate_child_ends_before_outer_if() {
    carrier_scope_layout(
        "child-ended",
        "if(flag){println(\"after\")}",
        "{borrow val parent=take(root,1);borrow val child=(parent);read(child)}\nif(flag){println(\"after\")}",
        b"first\nafter\ndrop\nsecond\ndrop\nfirst\nfirst\ndrop\nsecond\ndrop\nfirst\n",
    );
}

#[test]
fn range_carrier_gate_descriptor_begins_after_outer_if() {
    carrier_scope_layout(
        "descriptor-before",
        "if(flag){println(\"left\")}else{println(\"right\")}",
        "if(flag){println(\"left\")}else{println(\"right\")}\n{borrow val parent=take(root,1);read(parent)}",
        b"left\nfirst\ndrop\nsecond\ndrop\nfirst\nright\nfirst\ndrop\nsecond\ndrop\nfirst\n",
    );
}

#[test]
fn range_carrier_gate_child_is_live_across_return() {
    carrier_scope_layout(
        "child-return",
        "if(flag){read(child);return}",
        "borrow val parent=take(root,1)\nborrow val child=(parent)\nif(flag){read(child);return}\nread(child);println(\"continued\")",
        b"first\ndrop\nsecond\ndrop\nfirst\nfirst\ncontinued\ndrop\nsecond\ndrop\nfirst\n",
    );
}

#[test]
fn range_carrier_gate_descriptor_ends_on_explicit_return_arm() {
    carrier_scope_layout(
        "descriptor-return",
        "if(flag){read(part);return}else{read(part)}",
        "{borrow val part=take(root,1);if(flag){read(part);return}else{read(part)};read(part)}",
        b"first\ndrop\nsecond\ndrop\nfirst\nfirst\nfirst\ndrop\nsecond\ndrop\nfirst\n",
    );
}

#[test]
fn range_carrier_gate_composes_return_and_normal_scope_end() {
    carrier_scope_layout(
        "return-and-scope-end",
        "if(flag){read(child);return}",
        "{borrow val parent=take(root,1);borrow val child=(parent);if(flag){read(child);return};read(child)}\nif(!flag){println(\"after\")}",
        b"first\ndrop\nsecond\ndrop\nfirst\nfirst\nafter\ndrop\nsecond\ndrop\nfirst\n",
    );
}

#[test]
fn range_carrier_gate_return_precedes_child_acquisition() {
    carrier_scope_layout(
        "return-before-child",
        "if(flag){read(parent);return}",
        "borrow val parent=take(root,1)\nif(flag){read(parent);return}\nborrow val child=(parent);read(child);println(\"continued\")",
        b"first\ndrop\nsecond\ndrop\nfirst\nfirst\ncontinued\ndrop\nsecond\ndrop\nfirst\n",
    );
}

#[test]
fn range_carrier_gate_descriptor_ends_on_unbraced_return() {
    carrier_scope_layout(
        "unbraced-return",
        "if(flag) return",
        "borrow val part=take(root,1)\nif(flag) return\nread(part)",
        b"drop\nsecond\ndrop\nfirst\nfirst\ndrop\nsecond\ndrop\nfirst\n",
    );
}
