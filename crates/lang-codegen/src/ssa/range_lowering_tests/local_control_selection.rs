//! Observable condition/arm selection without instrumentation or rewritten IR.
use super::*;

const SOURCE: &str = r#"
fun mark(flag:Boolean):Boolean{println("condition");return flag}
fun read(view:View<String>):Unit{for(item in view){println(item)}}
fun consume(own root:List<String>):Unit{println("consume")}
fun scan(flag:Boolean):Unit{
    val root=listOf("kept","excluded")
    {
        borrow val part=take(root,1)
        if(mark(flag)){println("then");read(part)}else{println("else");read(part)}
        read(part)
    }
    consume(root)
}
fun main():Unit{scan(true);scan(false)}
"#;

fn selection_case(pipeline: Pipeline) {
    let branch = "if(mark(flag)){println(\"then\");read(part)}else{println(\"else\");read(part)}";
    run_selection(pipeline, SOURCE, branch, None);
}

fn comparison_selection_case(pipeline: Pipeline, operator: ComparisonOperator) {
    let condition = match operator {
        ComparisonOperator::Equal => "mark(n)==2",
        ComparisonOperator::LessThan => "mark(n)<3",
        _ => unreachable!("only the two frozen Int comparisons are selected"),
    };
    let branch = format!(
        "if({condition}){{println(\"then\");read(part)}}else{{println(\"else\");read(part)}}"
    );
    let source = SOURCE
        .replace(
            "fun mark(flag:Boolean):Boolean{println(\"condition\");return flag}",
            "fun mark(n:Int):Int{println(\"condition\");return n}",
        )
        .replace("fun scan(flag:Boolean)", "fun scan(n:Int)")
        .replace("if(mark(flag))", &format!("if({condition})"))
        .replace("scan(true);scan(false)", "scan(2);scan(4)");
    run_selection(pipeline, &source, &branch, Some(operator));
}

fn run_selection(
    pipeline: Pipeline,
    source: &str,
    branch: &str,
    operator: Option<ComparisonOperator>,
) {
    let label = format!("{pipeline:?}-{operator:?}-observable-selection");
    let lowered = lower_positive(source, branch, pipeline);
    let branch_span = lowered.expected_span;
    let condition_start = branch_span.start() + "if(".len();
    let call_expression = if operator.is_some() {
        "mark(n)"
    } else {
        "mark(flag)"
    };
    let condition_expression = match operator {
        Some(ComparisonOperator::Equal) => "mark(n)==2",
        Some(ComparisonOperator::LessThan) => "mark(n)<3",
        None => "mark(flag)",
        _ => unreachable!(),
    };
    let condition_span = lowered
        .sources()
        .span(
            branch_span.source_id(),
            condition_start,
            condition_start + condition_expression.len(),
        )
        .unwrap();
    let call_span = lowered
        .sources()
        .span(
            branch_span.source_id(),
            condition_start,
            condition_start + call_expression.len(),
        )
        .unwrap();
    let (program, entry) = lowered.require_success(&label);
    let function = program.modules[0]
        .functions
        .iter()
        .find(|function| {
            function.blocks.iter().any(|block| {
                block.terminator.as_ref().is_some_and(|terminator| {
                    terminator.origin.span() == branch_span
                        && matches!(terminator.kind, TerminatorKind::Conditional { .. })
                })
            })
        })
        .expect("the selected source if must retain its exact conditional span");
    let terminator = function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .find(|terminator| {
            terminator.origin.span() == branch_span
                && matches!(terminator.kind, TerminatorKind::Conditional { .. })
        })
        .unwrap();
    let TerminatorKind::Conditional { condition, .. } = terminator.kind else {
        unreachable!()
    };
    let calls = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin.span() == call_span
                && matches!(instruction.operation, Operation::DirectCall { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        calls.len(),
        1,
        "the condition must be evaluated exactly once"
    );
    if let Some(expected) = operator {
        let compare = function
            .instructions
            .iter()
            .find(|instruction| {
                instruction.origin.span() == condition_span
                    && matches!(instruction.operation, Operation::Compare { .. })
            })
            .expect("the exact Int comparison expression must drive the if");
        let Operation::Compare {
            operator: actual,
            left,
            right,
        } = compare.operation
        else {
            unreachable!()
        };
        assert_eq!(actual, expected);
        assert_eq!(calls[0].results, [EntityId::Value(left)]);
        assert_eq!(compare.results, [EntityId::Value(condition)]);
        for operand in [left, right] {
            let ty = function
                .entity(EntityId::Value(operand))
                .unwrap()
                .ty
                .semantic_type();
            assert_eq!(
                program.modules[0].type_kind(ty),
                Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true
                })
            );
        }
    } else {
        assert_eq!(calls[0].results, [EntityId::Value(condition)]);
    }
    let condition_type = function
        .entity(EntityId::Value(condition))
        .unwrap()
        .ty
        .semantic_type();
    assert_eq!(
        program.modules[0].type_kind(condition_type),
        Some(&SsaTypeKind::Boolean)
    );
    // println creates ordinary temporary String owners in each arm. The general
    // verifier checks them; this fixture does not forbid their legitimate drops.
    run_unmodified(
        &program,
        entry,
        &label,
        b"condition\nthen\nkept\nkept\nconsume\ncondition\nelse\nkept\nkept\nconsume\n",
    );
}

#[test]
fn range_local_control_observable_selection_single() {
    selection_case(Pipeline::Single);
}
#[test]
fn range_local_control_observable_selection_unit() {
    selection_case(Pipeline::Unit);
}
#[test]
fn range_local_control_observable_selection_unit_reversed() {
    selection_case(Pipeline::UnitReversed);
}

macro_rules! comparison_selection_tests {
    ($single:ident, $unit:ident, $reversed:ident, $operator:ident) => {
        #[test]
        fn $single() {
            comparison_selection_case(Pipeline::Single, ComparisonOperator::$operator);
        }
        #[test]
        fn $unit() {
            comparison_selection_case(Pipeline::Unit, ComparisonOperator::$operator);
        }
        #[test]
        fn $reversed() {
            comparison_selection_case(Pipeline::UnitReversed, ComparisonOperator::$operator);
        }
    };
}

comparison_selection_tests!(
    range_local_control_eq_if_selection_single,
    range_local_control_eq_if_selection_unit,
    range_local_control_eq_if_selection_unit_reversed,
    Equal
);
comparison_selection_tests!(
    range_local_control_lt_if_selection_single,
    range_local_control_lt_if_selection_unit,
    range_local_control_lt_if_selection_unit_reversed,
    LessThan
);
