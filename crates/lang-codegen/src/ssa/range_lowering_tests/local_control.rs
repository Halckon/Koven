//! Positive, normal-source probes for SPEC-0298's bounded local control flow.
use super::*;

#[path = "local_control_support.rs"]
mod support;
use support::{Pipeline, lower, run_unmodified};

#[derive(Clone, Copy, Debug)]
enum Position {
    BeforeBinding,
    WhileLive,
    AfterEnds,
}

fn comparison_case(pipeline: Pipeline, position: Position, operator: ComparisonOperator) {
    let expression = match operator {
        ComparisonOperator::Equal => "n==2",
        ComparisonOperator::LessThan => "n<3",
        _ => unreachable!("the frozen slice has only Int equality and less-than"),
    };
    let comparison = format!("check({expression})");
    let (body, expected): (String, &[u8]) = match position {
        Position::BeforeBinding => (
            format!("{comparison}\n{{borrow val part=take(root,1);read(part)}}"),
            b"true\nkept\nconsume\nfalse\nkept\nconsume\n",
        ),
        Position::WhileLive => (
            format!("{{borrow val part=take(root,1);read(part);{comparison};read(part)}}"),
            b"kept\ntrue\nkept\nconsume\nkept\nfalse\nkept\nconsume\n",
        ),
        Position::AfterEnds => (
            format!("{{borrow val part=take(root,1);read(part)}}\n{comparison}"),
            b"kept\ntrue\nconsume\nkept\nfalse\nconsume\n",
        ),
    };
    // check has no borrow binding: its if cannot trigger scan's binding gate.
    let consumer = format!(
        r#"
fun check(flag:Boolean):Unit{{if(flag){{println("true")}}else{{println("false")}}}}
fun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}
fun consume(own root:List<String>):Unit{{println("consume")}}
fun scan(n:Int):Unit{{
    val root=listOf("kept","excluded")
    {body}
    consume(root)
}}
fun main():Unit{{scan(2);scan(4)}}
"#
    );
    let label = format!("{pipeline:?}-{position:?}-{operator:?}");
    let lowered = lower(&consumer, expression, pipeline);
    let comparison_span = lowered.expected_span;
    let (program, entry) = lowered.require_success(&label);
    let module = &program.modules[0];
    let instruction = module
        .functions
        .iter()
        .flat_map(|function| &function.instructions)
        .find(|instruction| {
            instruction.origin.span() == comparison_span
                && matches!(instruction.operation, Operation::Compare { .. })
        })
        .expect("the Int comparison must produce a Compare at its exact expression span");
    let function = &module.functions[instruction.id.function().index()];
    let Operation::Compare {
        operator: actual,
        left,
        right,
    } = instruction.operation
    else {
        unreachable!()
    };
    assert_eq!(actual, operator);
    for operand in [left, right] {
        let EntityType::Value(ty) = function.entity(EntityId::Value(operand)).unwrap().ty else {
            panic!("comparison operand must be a value")
        };
        assert_eq!(
            module.type_kind(ty),
            Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true
            })
        );
    }
    let [result] = instruction.results.as_slice() else {
        panic!("one Boolean comparison result")
    };
    let EntityType::Value(ty) = function.entity(*result).unwrap().ty else {
        panic!("comparison result must be a value")
    };
    assert_eq!(module.type_kind(ty), Some(&SsaTypeKind::Boolean));
    run_unmodified(&program, entry, &label, expected);
}

// Each position/operator/pipeline is independently selectable and reaches its
// own failure, so libtest collects all three positions even on the initial gate.
macro_rules! comparison_tests {
    ($single:ident, $unit:ident, $reversed:ident, $position:ident, $operator:ident) => {
        #[test]
        fn $single() {
            comparison_case(
                Pipeline::Single,
                Position::$position,
                ComparisonOperator::$operator,
            );
        }
        #[test]
        fn $unit() {
            comparison_case(
                Pipeline::Unit,
                Position::$position,
                ComparisonOperator::$operator,
            );
        }
        #[test]
        fn $reversed() {
            comparison_case(
                Pipeline::UnitReversed,
                Position::$position,
                ComparisonOperator::$operator,
            );
        }
    };
}

comparison_tests!(
    range_local_control_eq_before_single,
    range_local_control_eq_before_unit,
    range_local_control_eq_before_unit_reversed,
    BeforeBinding,
    Equal
);
comparison_tests!(
    range_local_control_eq_live_single,
    range_local_control_eq_live_unit,
    range_local_control_eq_live_unit_reversed,
    WhileLive,
    Equal
);
comparison_tests!(
    range_local_control_eq_after_single,
    range_local_control_eq_after_unit,
    range_local_control_eq_after_unit_reversed,
    AfterEnds,
    Equal
);
comparison_tests!(
    range_local_control_lt_before_single,
    range_local_control_lt_before_unit,
    range_local_control_lt_before_unit_reversed,
    BeforeBinding,
    LessThan
);
comparison_tests!(
    range_local_control_lt_live_single,
    range_local_control_lt_live_unit,
    range_local_control_lt_live_unit_reversed,
    WhileLive,
    LessThan
);
comparison_tests!(
    range_local_control_lt_after_single,
    range_local_control_lt_after_unit,
    range_local_control_lt_after_unit_reversed,
    AfterEnds,
    LessThan
);

const IF_SOURCE: &str = r#"
fun read(view:View<String>):Unit{for(item in view){println(item)}}
fun consume(own root:List<String>):Unit{println("consume")}
fun scan(flag:Boolean):Unit{
    val root=listOf("kept","excluded")
    {
        borrow val part=take(root,1)
        if(flag){read(part)}else{read(part)}
        read(part)
    }
    consume(root)
}
fun main():Unit{scan(true);scan(false)}
"#;

fn if_case(pipeline: Pipeline) {
    let label = format!("{pipeline:?}-if-both-arms");
    let (program, entry) =
        lower(IF_SOURCE, "if(flag){read(part)}else{read(part)}", pipeline).require_success(&label);
    run_unmodified(
        &program,
        entry,
        &label,
        b"kept\nkept\nconsume\nkept\nkept\nconsume\n",
    );
}

#[test]
fn range_local_control_if_both_arms_single() {
    if_case(Pipeline::Single);
}
#[test]
fn range_local_control_if_both_arms_unit() {
    if_case(Pipeline::Unit);
}
#[test]
fn range_local_control_if_both_arms_unit_reversed() {
    if_case(Pipeline::UnitReversed);
}
