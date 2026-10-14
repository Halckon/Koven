//! SPEC-0317: normal source, exact edge identities and unmodified native LLVM.
use super::*;

#[path = "local_short_circuit_boundary.rs"]
mod boundary;

#[derive(Clone, Copy, Debug)]
enum Use {
    Local,
    Argument,
    Condition,
}

fn statement(expression: &str, use_site: Use) -> String {
    match use_site {
        Use::Local => format!("val result={expression};show(result)"),
        Use::Argument => format!("show({expression})"),
        Use::Condition => {
            format!("if({expression}){{println(\"true\")}}else{{println(\"false\")}}")
        }
    }
}

fn fixture(
    declaration: &str,
    element: &str,
    values: &str,
    read: &str,
    statement: &str,
    calls: &str,
) -> String {
    format!(
        r#"
{declaration}
fun show(result:Boolean):Unit{{if(result){{println("true")}}else{{println("false")}}}}
fun lhs(flag:Boolean):Boolean{{println("lhs");return flag}}
fun rhs(view:View<{element}>,flag:Boolean):Boolean{{println("rhs");for(item in view){{{read}}};return flag}}
fun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}
fun consume(own root:List<{element}>):Unit{{println("consume")}}
fun scan(left:Boolean,right:Boolean,n:Int):Unit{{
    val root=listOf({values})
    {{
        borrow val parent=take(root,2)
        borrow val child=take(parent,1)
        borrow val sibling=take(parent,2)
        {statement}
        read(parent)
        read(child)
        read(sibling)
    }}
    consume(root)
}}
fun main():Unit{{{calls}}}
"#
    )
}

fn dynamic(pipeline: Pipeline, and: bool, use_site: Use) {
    let operator = if and { "&&" } else { "||" };
    let expression = format!("lhs(left) {operator} rhs(child,right)");
    for (payload, declaration, element, values, read, drops) in [
        (
            "string",
            "",
            "String",
            "\"a\".clone(),\"b\".clone(),\"c\".clone()",
            "println(item)",
            "",
        ),
        (
            "move",
            "class Item(val text:String){}",
            "Item",
            "Item(\"a\"),Item(\"b\"),Item(\"c\")",
            "println(item.text)",
            "",
        ),
        (
            "resource",
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"a\"),Item(\"b\"),Item(\"c\")",
            "println(item.text)",
            "c\nb\na\n",
        ),
    ] {
        let consumer = fixture(
            declaration,
            element,
            values,
            read,
            &statement(&expression, use_site),
            "scan(true,true,1);scan(true,false,1);scan(false,true,1);scan(false,false,1)",
        );
        let label = format!("0317-{pipeline:?}-{and}-{use_site:?}-{payload}");
        let lowered = lower(&consumer, &expression, pipeline);
        let span = lowered.expected_span;
        let (program, entry) = lowered.require_success(&label);
        assertions::short_circuit_transport(&program, span, and);
        let mut expected = String::new();
        for (left, right) in [(true, true), (true, false), (false, true), (false, false)] {
            expected.push_str("lhs\n");
            if left == and {
                expected.push_str("rhs\na\n");
            }
            expected.push_str(if if and { left && right } else { left || right } {
                "true\n"
            } else {
                "false\n"
            });
            expected.push_str(&format!("a\nb\na\na\nb\nconsume\n{drops}"));
        }
        run_unmodified(&program, entry, &label, expected.as_bytes());
    }
}

fn fixed(pipeline: Pipeline) {
    let constant = matches!(
        pipeline,
        Pipeline::SingleConstant | Pipeline::UnitConstant | Pipeline::UnitConstantReversed
    );
    for (and, left) in [(true, true), (true, false), (false, true), (false, false)] {
        let operator = if and { "&&" } else { "||" };
        let lhs = if constant {
            "FLAG"
        } else if left {
            "true"
        } else {
            "false"
        };
        let declaration = if constant {
            format!("const val FLAG:Boolean={left}")
        } else {
            String::new()
        };
        let expression = format!("{lhs} {operator} rhs(child,right)");
        let consumer = fixture(
            &declaration,
            "String",
            "\"a\",\"b\",\"c\"",
            "println(item)",
            &statement(&expression, Use::Local),
            "scan(true,true,1);scan(false,false,1)",
        );
        let label = format!("0317-fixed-{pipeline:?}-{and}-{left}");
        let lowered = lower(&consumer, &expression, pipeline);
        let (program, entry) = lowered.require_success(&label);
        let mut expected = String::new();
        for right in [true, false] {
            if left == and {
                expected.push_str("rhs\na\n");
            }
            expected.push_str(if if and { left && right } else { left || right } {
                "true\n"
            } else {
                "false\n"
            });
            expected.push_str("a\nb\na\na\nb\nconsume\n");
        }
        run_unmodified(&program, entry, &label, expected.as_bytes());
    }
}

fn scalar_operands(pipeline: Pipeline) {
    for (expression, expected) in [
        ("!(left) && (n>=1)", "false\nfalse\ntrue\ntrue\n"),
        ("(n!=0) || !right", "true\ntrue\ntrue\ntrue\n"),
    ] {
        let consumer = format!(
            r#"
fun show(flag:Boolean):Unit{{if(flag){{println("true")}}else{{println("false")}}}}
fun read(view:View<String>):Unit{{}}
fun scan(left:Boolean,right:Boolean,n:Int):Unit{{val root=listOf("a");{{borrow val part=take(root,1);show({expression});read(part)}}}}
fun main():Unit{{scan(true,true,1);scan(true,false,1);scan(false,true,1);scan(false,false,1)}}
"#
        );
        let label = format!("0317-scalar-{pipeline:?}-{expression}");
        let lowered = lower(&consumer, expression, pipeline);
        let (program, entry) = lowered.require_success(&label);
        run_unmodified(&program, entry, &label, expected.as_bytes());
    }
}

#[test]
fn range_short_circuit_and_local_single() {
    dynamic(Pipeline::Single, true, Use::Local);
}

#[test]
fn range_short_circuit_and_argument_single() {
    dynamic(Pipeline::Single, true, Use::Argument);
}

#[test]
fn range_short_circuit_and_condition_single() {
    dynamic(Pipeline::Single, true, Use::Condition);
}

#[test]
fn range_short_circuit_or_local_single() {
    dynamic(Pipeline::Single, false, Use::Local);
}

#[test]
fn range_short_circuit_or_argument_single() {
    dynamic(Pipeline::Single, false, Use::Argument);
}

#[test]
fn range_short_circuit_or_condition_single() {
    dynamic(Pipeline::Single, false, Use::Condition);
}

#[test]
fn range_short_circuit_fixed_single() {
    fixed(Pipeline::Single);
}

#[test]
fn range_short_circuit_scalar_single() {
    scalar_operands(Pipeline::Single);
}

#[test]
fn range_short_circuit_and_local_unit() {
    dynamic(Pipeline::Unit, true, Use::Local);
}

#[test]
fn range_short_circuit_and_argument_unit() {
    dynamic(Pipeline::Unit, true, Use::Argument);
}

#[test]
fn range_short_circuit_and_condition_unit() {
    dynamic(Pipeline::Unit, true, Use::Condition);
}

#[test]
fn range_short_circuit_or_local_unit() {
    dynamic(Pipeline::Unit, false, Use::Local);
}

#[test]
fn range_short_circuit_or_argument_unit() {
    dynamic(Pipeline::Unit, false, Use::Argument);
}

#[test]
fn range_short_circuit_or_condition_unit() {
    dynamic(Pipeline::Unit, false, Use::Condition);
}

#[test]
fn range_short_circuit_fixed_unit() {
    fixed(Pipeline::Unit);
}

#[test]
fn range_short_circuit_scalar_unit() {
    scalar_operands(Pipeline::Unit);
}

#[test]
fn range_short_circuit_and_local_reversed() {
    dynamic(Pipeline::UnitReversed, true, Use::Local);
}

#[test]
fn range_short_circuit_and_argument_reversed() {
    dynamic(Pipeline::UnitReversed, true, Use::Argument);
}

#[test]
fn range_short_circuit_and_condition_reversed() {
    dynamic(Pipeline::UnitReversed, true, Use::Condition);
}

#[test]
fn range_short_circuit_or_local_reversed() {
    dynamic(Pipeline::UnitReversed, false, Use::Local);
}

#[test]
fn range_short_circuit_or_argument_reversed() {
    dynamic(Pipeline::UnitReversed, false, Use::Argument);
}

#[test]
fn range_short_circuit_or_condition_reversed() {
    dynamic(Pipeline::UnitReversed, false, Use::Condition);
}

#[test]
fn range_short_circuit_fixed_reversed() {
    fixed(Pipeline::UnitReversed);
}

#[test]
fn range_short_circuit_scalar_reversed() {
    scalar_operands(Pipeline::UnitReversed);
}

#[test]
fn range_short_circuit_and_local_unit_constant() {
    dynamic(Pipeline::UnitConstant, true, Use::Local);
}

#[test]
fn range_short_circuit_and_argument_unit_constant() {
    dynamic(Pipeline::UnitConstant, true, Use::Argument);
}

#[test]
fn range_short_circuit_and_condition_unit_constant() {
    dynamic(Pipeline::UnitConstant, true, Use::Condition);
}

#[test]
fn range_short_circuit_or_local_unit_constant() {
    dynamic(Pipeline::UnitConstant, false, Use::Local);
}

#[test]
fn range_short_circuit_or_argument_unit_constant() {
    dynamic(Pipeline::UnitConstant, false, Use::Argument);
}

#[test]
fn range_short_circuit_or_condition_unit_constant() {
    dynamic(Pipeline::UnitConstant, false, Use::Condition);
}

#[test]
fn range_short_circuit_fixed_unit_constant() {
    fixed(Pipeline::UnitConstant);
}

#[test]
fn range_short_circuit_scalar_unit_constant() {
    scalar_operands(Pipeline::UnitConstant);
}

#[test]
fn range_short_circuit_and_local_reversed_constant() {
    dynamic(Pipeline::UnitConstantReversed, true, Use::Local);
}

#[test]
fn range_short_circuit_and_argument_reversed_constant() {
    dynamic(Pipeline::UnitConstantReversed, true, Use::Argument);
}

#[test]
fn range_short_circuit_and_condition_reversed_constant() {
    dynamic(Pipeline::UnitConstantReversed, true, Use::Condition);
}

#[test]
fn range_short_circuit_or_local_reversed_constant() {
    dynamic(Pipeline::UnitConstantReversed, false, Use::Local);
}

#[test]
fn range_short_circuit_or_argument_reversed_constant() {
    dynamic(Pipeline::UnitConstantReversed, false, Use::Argument);
}

#[test]
fn range_short_circuit_or_condition_reversed_constant() {
    dynamic(Pipeline::UnitConstantReversed, false, Use::Condition);
}

#[test]
fn range_short_circuit_fixed_reversed_constant() {
    fixed(Pipeline::UnitConstantReversed);
}

#[test]
fn range_short_circuit_scalar_reversed_constant() {
    scalar_operands(Pipeline::UnitConstantReversed);
}

#[test]
fn range_short_circuit_fixed_single_constant_fixture() {
    fixed(Pipeline::SingleConstant);
}

#[test]
fn range_short_circuit_migrated_plain_operands() {
    for pipeline in [
        Pipeline::Single,
        Pipeline::Unit,
        Pipeline::UnitReversed,
        Pipeline::UnitConstant,
        Pipeline::UnitConstantReversed,
    ] {
        for expression in ["flag && flag", "flag || flag"] {
            let source = format!(
                "fun show(flag:Boolean):Unit{{if(flag){{println(\"true\")}}else{{println(\"false\")}}}}\nfun read(view:View<String>):Unit{{}}\nfun scan(flag:Boolean):Unit{{val root=listOf(\"a\");{{borrow val part=take(root,1);show({expression});read(part)}}}}\nfun main():Unit{{scan(true);scan(false)}}"
            );
            let label = format!("0317-migrated-{pipeline:?}-{expression}");
            let (program, entry) = lower(&source, expression, pipeline).require_success(&label);
            run_unmodified(&program, entry, &label, b"true\nfalse\n");
        }
    }
}

#[test]
fn range_short_circuit_readonly_receiver_and_copyable_value_argument() {
    for pipeline in [
        Pipeline::Single,
        Pipeline::Unit,
        Pipeline::UnitReversed,
        Pipeline::UnitConstant,
        Pipeline::UnitConstantReversed,
    ] {
        let expression = "gate.test() && identity(flag)";
        let source = format!(
            "class Gate(val flag:Boolean){{fun test():Boolean{{println(\"receiver\");return flag}}}}\nfun identity(own flag:Boolean):Boolean{{println(\"own-copyable\");return flag}}\nfun read(view:View<String>):Unit{{}}\nfun show(flag:Boolean):Unit{{if(flag){{println(\"true\")}}else{{println(\"false\")}}}}\nfun scan(flag:Boolean):Unit{{val root=listOf(\"a\");val gate=Gate(flag);{{borrow val part=take(root,1);show({expression});read(part)}}}}\nfun main():Unit{{scan(true);scan(false)}}"
        );
        let label = format!("0317-receiver-{pipeline:?}");
        let lowered = lower(&source, expression, pipeline);
        if matches!(pipeline, Pipeline::Single) {
            let error = lowered
                .result
                .err()
                .expect("single's existing ordinary receiver boundary remains independent");
            assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
            assert_eq!(
                lowered.sources.slice(error.span.unwrap()).unwrap(),
                "gate.test()"
            );
        } else {
            let (program, entry) = lowered.require_success(&label);
            run_unmodified(
                &program,
                entry,
                &label,
                b"receiver\nown-copyable\ntrue\nreceiver\nfalse\n",
            );
        }
    }
}

#[test]
fn range_short_circuit_copyable_value_argument_single() {
    let expression = "identity(flag) && identity(flag)";
    let source = format!(
        "fun identity(own flag:Boolean):Boolean{{println(\"copyable\");return flag}}\nfun read(view:View<String>):Unit{{}}\nfun show(flag:Boolean):Unit{{if(flag){{println(\"true\")}}else{{println(\"false\")}}}}\nfun scan(flag:Boolean):Unit{{val root=listOf(\"a\");{{borrow val part=take(root,1);show({expression});read(part)}}}}\nfun main():Unit{{scan(true);scan(false)}}"
    );
    let (program, entry) =
        lower(&source, expression, Pipeline::Single).require_success("0317-copyable-single");
    run_unmodified(
        &program,
        entry,
        "0317-copyable-single",
        b"copyable\ncopyable\ntrue\ncopyable\nfalse\n",
    );
}
