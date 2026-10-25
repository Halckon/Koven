//! SPEC-0316 × SPEC-0317: the ordinary value-class receiver intersection.
use super::*;

fn receivers(pipeline: Pipeline) {
    for move_only in [false, true] {
        let field = if move_only { ",val text:String" } else { "" };
        let payload = if move_only { ",\"kept\".clone()" } else { "" };
        for and in [true, false] {
            let operator = if and { "&&" } else { "||" };
            let expression = format!("leftGate.left() {operator} rightGate.right()");
            for site in [Use::Local, Use::Argument, Use::Condition] {
                let statement = statement(&expression, site);
                let consumer = format!(
                    r#"
value class Gate(val flag:Boolean{field}){{
    fun left():Boolean{{println("lhs");return this.flag}}
    fun right():Boolean{{println("rhs");return this.flag}}
}}
fun finish(own gate:Gate):Unit{{println("gate")}}
fun show(flag:Boolean):Unit{{if(flag){{println("true")}}else{{println("false")}}}}
fun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}
fun consume(own root:List<String>):Unit{{println("consume")}}
fun scan(left:Boolean,right:Boolean):Unit{{
    val root=listOf("a","b","c")
    val leftGate=Gate(left{payload})
    val rightGate=Gate(right{payload})
    {{
        borrow val parent=take(root,2)
        borrow val child=take(parent,1)
        borrow val sibling=take(parent,2)
        {statement}
        read(parent);read(child);read(sibling)
    }}
    consume(root);finish(leftGate);finish(rightGate)
}}
fun main():Unit{{scan(true,true);scan(true,false);scan(false,true);scan(false,false)}}
"#
                );
                let label = format!("0317-receiver-{pipeline:?}-{move_only}-{and}-{site:?}");
                let lowered = lower_positive(&consumer, &expression, pipeline);
                let span = lowered.expected_span;
                let (program, entry) = lowered.require_success(&label);
                assertions::short_circuit_receiver_transport(&program, span, and);
                let mut expected = String::new();
                for (left, right) in [(true, true), (true, false), (false, true), (false, false)] {
                    expected.push_str("lhs\n");
                    if left == and {
                        expected.push_str("rhs\n");
                    }
                    expected.push_str(if if and { left && right } else { left || right } {
                        "true\n"
                    } else {
                        "false\n"
                    });
                    expected.push_str("a\nb\na\na\nb\nconsume\ngate\ngate\n");
                }
                run_unmodified(&program, entry, &label, expected.as_bytes());
            }
        }
    }
}

#[test]
fn range_short_circuit_value_receiver_single() {
    receivers(Pipeline::Single);
}
#[test]
fn range_short_circuit_value_receiver_unit() {
    receivers(Pipeline::Unit);
}
#[test]
fn range_short_circuit_value_receiver_unit_reversed() {
    receivers(Pipeline::UnitReversed);
}
#[test]
fn range_short_circuit_value_receiver_constant_unit() {
    receivers(Pipeline::UnitConstant);
}
#[test]
fn range_short_circuit_value_receiver_constant_unit_reversed() {
    receivers(Pipeline::UnitConstantReversed);
}
