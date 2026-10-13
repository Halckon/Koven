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
    let lowered = lower(&consumer, branch, pipeline);
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
