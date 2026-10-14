//! SPEC-0315 comparison-driven arm selection and same-root cleanup, no IR edits.
use super::*;

#[derive(Clone, Copy, Debug)]
enum Payload {
    String,
    MoveOnly,
    Resource,
}

fn selection_case(pipeline: Pipeline, operator: ComparisonOperator, payload: Payload) {
    let (constant, expression) = expression(pipeline, operator);
    let expression = expression.replacen('n', "mark(n)", 1);
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
    let branch = format!(
        "if({expression}){{left(parent);left(child);left(sibling)}}else{{right(parent);right(child);right(sibling)}}"
    );
    let source = format!(
        r#"
{constant}
{declaration}
fun mark(n:Int):Int{{println("condition");return n}}
fun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}
fun left(view:View<{element}>):Unit{{println("then");read(view)}}
fun right(view:View<{element}>):Unit{{println("else");read(view)}}
fun consume(own root:List<{element}>):Unit{{println("consume")}}
fun scan(n:Int):Unit{{
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
fun main():Unit{{{CALLS}}}
"#
    );
    let label = format!("0315-{pipeline:?}-{operator:?}-{payload:?}-selection");
    let lowered = lower(&source, &expression, pipeline);
    let condition_span = lowered.expected_span;
    let branch_start = condition_span.start() - "if(".len();
    let branch_span = lowered
        .sources
        .span(
            condition_span.source_id(),
            branch_start,
            branch_start + branch.len(),
        )
        .unwrap();
    assert_eq!(lowered.sources.slice(branch_span).unwrap(), branch);
    let mark_span = lowered
        .sources
        .span(
            condition_span.source_id(),
            condition_span.start(),
            condition_span.start() + "mark(n)".len(),
        )
        .unwrap();
    let (program, entry) = lowered.require_success(&label);
    assert_comparison(&program, condition_span, operator);
    assertions::if_transport(&program, branch_span, 3);
    let function = program.modules[0]
        .functions
        .iter()
        .find(|function| {
            function
                .instructions
                .iter()
                .any(|instruction| instruction.origin.span() == condition_span)
        })
        .unwrap();
    let calls = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin.span() == mark_span
                && matches!(instruction.operation, Operation::DirectCall { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1, "condition operand evaluates exactly once");
    let comparison = function
        .instructions
        .iter()
        .find(|instruction| {
            instruction.origin.span() == condition_span
                && matches!(instruction.operation, Operation::Compare { .. })
        })
        .unwrap();
    let Operation::Compare { left, .. } = comparison.operation else {
        unreachable!()
    };
    assert_eq!(calls[0].results, [EntityId::Value(left)]);
    let term = function
        .blocks
        .iter()
        .filter_map(|block| block.terminator.as_ref())
        .find(|term| term.origin.span() == branch_span)
        .unwrap();
    let TerminatorKind::Conditional { condition, .. } = term.kind else {
        panic!("selected statement-if")
    };
    assert_eq!(comparison.results, [EntityId::Value(condition)]);
    let expected = outcomes(operator)
        .into_iter()
        .map(|outcome| {
            let arm = if outcome { "then" } else { "else" };
            format!(
                "condition\n{arm}\na\nb\n{arm}\na\n{arm}\na\nb\na\nb\na\na\nb\nconsume\n{drops}"
            )
        })
        .collect::<String>();
    run_unmodified(&program, entry, &label, expected.as_bytes());
}

macro_rules! selections {
    ($single:ident, $unit:ident, $reversed:ident, $single_const:ident, $unit_const:ident, $reversed_const:ident, $payload:ident, $operator:ident) => {
        #[test]
        fn $single() {
            selection_case(
                Pipeline::Single,
                ComparisonOperator::$operator,
                Payload::$payload,
            );
        }
        #[test]
        fn $unit() {
            selection_case(
                Pipeline::Unit,
                ComparisonOperator::$operator,
                Payload::$payload,
            );
        }
        #[test]
        fn $reversed() {
            selection_case(
                Pipeline::UnitReversed,
                ComparisonOperator::$operator,
                Payload::$payload,
            );
        }
        #[test]
        fn $single_const() {
            selection_case(
                Pipeline::SingleConstant,
                ComparisonOperator::$operator,
                Payload::$payload,
            );
        }
        #[test]
        fn $unit_const() {
            selection_case(
                Pipeline::UnitConstant,
                ComparisonOperator::$operator,
                Payload::$payload,
            );
        }
        #[test]
        fn $reversed_const() {
            selection_case(
                Pipeline::UnitConstantReversed,
                ComparisonOperator::$operator,
                Payload::$payload,
            );
        }
    };
}

selections!(
    range_comparison_selection_ne_string_single,
    range_comparison_selection_ne_string_unit,
    range_comparison_selection_ne_string_reversed,
    range_comparison_selection_ne_string_single_constant,
    range_comparison_selection_ne_string_unit_constant,
    range_comparison_selection_ne_string_reversed_constant,
    String,
    NotEqual
);

selections!(
    range_comparison_selection_ne_move_only_single,
    range_comparison_selection_ne_move_only_unit,
    range_comparison_selection_ne_move_only_reversed,
    range_comparison_selection_ne_move_only_single_constant,
    range_comparison_selection_ne_move_only_unit_constant,
    range_comparison_selection_ne_move_only_reversed_constant,
    MoveOnly,
    NotEqual
);

selections!(
    range_comparison_selection_ne_resource_single,
    range_comparison_selection_ne_resource_unit,
    range_comparison_selection_ne_resource_reversed,
    range_comparison_selection_ne_resource_single_constant,
    range_comparison_selection_ne_resource_unit_constant,
    range_comparison_selection_ne_resource_reversed_constant,
    Resource,
    NotEqual
);

selections!(
    range_comparison_selection_le_string_single,
    range_comparison_selection_le_string_unit,
    range_comparison_selection_le_string_reversed,
    range_comparison_selection_le_string_single_constant,
    range_comparison_selection_le_string_unit_constant,
    range_comparison_selection_le_string_reversed_constant,
    String,
    LessThanOrEqual
);

selections!(
    range_comparison_selection_le_move_only_single,
    range_comparison_selection_le_move_only_unit,
    range_comparison_selection_le_move_only_reversed,
    range_comparison_selection_le_move_only_single_constant,
    range_comparison_selection_le_move_only_unit_constant,
    range_comparison_selection_le_move_only_reversed_constant,
    MoveOnly,
    LessThanOrEqual
);

selections!(
    range_comparison_selection_le_resource_single,
    range_comparison_selection_le_resource_unit,
    range_comparison_selection_le_resource_reversed,
    range_comparison_selection_le_resource_single_constant,
    range_comparison_selection_le_resource_unit_constant,
    range_comparison_selection_le_resource_reversed_constant,
    Resource,
    LessThanOrEqual
);

selections!(
    range_comparison_selection_gt_string_single,
    range_comparison_selection_gt_string_unit,
    range_comparison_selection_gt_string_reversed,
    range_comparison_selection_gt_string_single_constant,
    range_comparison_selection_gt_string_unit_constant,
    range_comparison_selection_gt_string_reversed_constant,
    String,
    GreaterThan
);

selections!(
    range_comparison_selection_gt_move_only_single,
    range_comparison_selection_gt_move_only_unit,
    range_comparison_selection_gt_move_only_reversed,
    range_comparison_selection_gt_move_only_single_constant,
    range_comparison_selection_gt_move_only_unit_constant,
    range_comparison_selection_gt_move_only_reversed_constant,
    MoveOnly,
    GreaterThan
);

selections!(
    range_comparison_selection_gt_resource_single,
    range_comparison_selection_gt_resource_unit,
    range_comparison_selection_gt_resource_reversed,
    range_comparison_selection_gt_resource_single_constant,
    range_comparison_selection_gt_resource_unit_constant,
    range_comparison_selection_gt_resource_reversed_constant,
    Resource,
    GreaterThan
);

selections!(
    range_comparison_selection_ge_string_single,
    range_comparison_selection_ge_string_unit,
    range_comparison_selection_ge_string_reversed,
    range_comparison_selection_ge_string_single_constant,
    range_comparison_selection_ge_string_unit_constant,
    range_comparison_selection_ge_string_reversed_constant,
    String,
    GreaterThanOrEqual
);

selections!(
    range_comparison_selection_ge_move_only_single,
    range_comparison_selection_ge_move_only_unit,
    range_comparison_selection_ge_move_only_reversed,
    range_comparison_selection_ge_move_only_single_constant,
    range_comparison_selection_ge_move_only_unit_constant,
    range_comparison_selection_ge_move_only_reversed_constant,
    MoveOnly,
    GreaterThanOrEqual
);

selections!(
    range_comparison_selection_ge_resource_single,
    range_comparison_selection_ge_resource_unit,
    range_comparison_selection_ge_resource_reversed,
    range_comparison_selection_ge_resource_single_constant,
    range_comparison_selection_ge_resource_unit_constant,
    range_comparison_selection_ge_resource_reversed_constant,
    Resource,
    GreaterThanOrEqual
);
