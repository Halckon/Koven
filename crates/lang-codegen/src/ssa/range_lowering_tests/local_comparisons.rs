//! SPEC-0315: ordinary typed Int comparisons, without new control-flow lowering.
use super::*;
use lang_frontend::source::Span;

mod selection {
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
        let lowered = lower_positive(&source, &expression, pipeline);
        let condition_span = lowered.expected_span;
        let branch_start = condition_span.start() - "if(".len();
        let branch_span = lowered
            .sources()
            .span(
                condition_span.source_id(),
                branch_start,
                branch_start + branch.len(),
            )
            .unwrap();
        assert_eq!(lowered.sources().slice(branch_span).unwrap(), branch);
        let mark_span = lowered
            .sources()
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
}

// Single cases use same-source Unit admission; real constants select its constant
// capability. Existing Unit constant cases retain their dedicated constant path.
pub(in crate::ssa) fn expression(
    pipeline: Pipeline,
    operator: ComparisonOperator,
) -> (&'static str, String) {
    let spelling = match operator {
        ComparisonOperator::NotEqual => "!=",
        ComparisonOperator::LessThanOrEqual => "<=",
        ComparisonOperator::GreaterThan => ">",
        ComparisonOperator::GreaterThanOrEqual => ">=",
        _ => unreachable!("the two existing comparison families have their own tests"),
    };
    if matches!(
        pipeline,
        Pipeline::SingleConstant | Pipeline::UnitConstant | Pipeline::UnitConstantReversed
    ) {
        ("const val LIMIT:Int=0", format!("n{spelling}LIMIT"))
    } else {
        ("", format!("n{spelling}0"))
    }
}

// Explicit truth tables cover negative, equality, positive and both Int extrema.
// Expected output is independent of the compiler's comparison implementation.
pub(in crate::ssa) fn outcomes(operator: ComparisonOperator) -> [bool; 5] {
    match operator {
        ComparisonOperator::NotEqual => [true, true, false, true, true],
        ComparisonOperator::LessThanOrEqual => [true, true, true, false, false],
        ComparisonOperator::GreaterThan => [false, false, false, true, true],
        ComparisonOperator::GreaterThanOrEqual => [false, false, true, true, true],
        _ => unreachable!(),
    }
}

pub(in crate::ssa) const CALLS: &str =
    "scan(-2147483647-1);scan(-1);scan(0);scan(1);scan(2147483647)";

pub(in crate::ssa) fn assert_comparison(
    program: &Program,
    span: Span,
    expected: ComparisonOperator,
) {
    let module = &program.modules[0];
    let comparisons = module
        .functions
        .iter()
        .flat_map(|function| {
            function.instructions.iter().filter_map(move |instruction| {
                (instruction.origin.span() == span
                    && matches!(instruction.operation, Operation::Compare { .. }))
                .then_some((function, instruction))
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        comparisons.len(),
        1,
        "one Compare at the exact source expression"
    );
    let (function, instruction) = comparisons[0];
    let Operation::Compare {
        operator,
        left,
        right,
    } = instruction.operation
    else {
        unreachable!()
    };
    assert_eq!(operator, expected);
    for operand in [left, right] {
        let ty = function
            .entity(EntityId::Value(operand))
            .unwrap()
            .ty
            .semantic_type();
        assert_eq!(
            module.type_kind(ty),
            Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true
            })
        );
    }
    let [result] = instruction.results.as_slice() else {
        panic!("one Boolean result")
    };
    assert!(matches!(result, EntityId::Value(_)));
    assert_eq!(
        module.type_kind(function.entity(*result).unwrap().ty.semantic_type()),
        Some(&SsaTypeKind::Boolean)
    );
    // No CFG is introduced by the comparison itself.
    assert!(
        !function
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .any(|term| term.origin.span() == span)
    );
}

pub(in crate::ssa) fn comparison_case(
    pipeline: Pipeline,
    position: Position,
    operator: ComparisonOperator,
) {
    comparison_with_use(pipeline, position, operator, false);
}

pub(in crate::ssa) fn comparison_with_use(
    pipeline: Pipeline,
    position: Position,
    operator: ComparisonOperator,
    after_last_read: bool,
) {
    let (declaration, expression) = expression(pipeline, operator);
    let comparison = format!("check({expression})");
    let body = match position {
        Position::BeforeBinding => {
            format!("{comparison}\n{{borrow val part=take(root,1);read(part)}}")
        }
        Position::WhileLive if after_last_read => {
            format!("{{borrow val part=take(root,1);read(part);{comparison}}}")
        }
        Position::WhileLive => {
            format!("{{borrow val part=take(root,1);read(part);{comparison};read(part)}}")
        }
        Position::AfterEnds => format!("{{borrow val part=take(root,1);read(part)}}\n{comparison}"),
    };
    let source = format!(
        r#"
{declaration}
fun check(flag:Boolean):Unit{{if(flag){{println("true")}}else{{println("false")}}}}
fun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}
fun consume(own root:List<String>):Unit{{println("consume")}}
fun scan(n:Int):Unit{{val root=listOf("kept","excluded");{body};consume(root)}}
fun main():Unit{{{CALLS}}}
"#
    );
    let label = format!("0315-{pipeline:?}-{position:?}-{operator:?}-last={after_last_read}");
    let lowered = lower_positive(&source, &expression, pipeline);
    let span = lowered.expected_span;
    let (program, entry) = lowered.require_success(&label);
    assert_comparison(&program, span, operator);
    // With no later descriptor use, the published last-use facts end the
    // descriptor and its source before this comparison, even inside the block.
    assertions::comparison_lifetime(
        &program,
        span,
        if after_last_read {
            Position::AfterEnds
        } else {
            position
        },
    );
    if after_last_read {
        let function = program.modules[0]
            .functions
            .iter()
            .find(|function| {
                function
                    .instructions
                    .iter()
                    .any(|instruction| instruction.origin.span() == span)
            })
            .unwrap();
        let compare = function
            .instructions
            .iter()
            .find(|instruction| {
                instruction.origin.span() == span
                    && matches!(instruction.operation, Operation::Compare { .. })
            })
            .unwrap();
        let read = function
            .instructions
            .iter()
            .find(|instruction| {
                matches!(&instruction.operation, Operation::DirectCall { arguments, .. }
                if matches!(arguments.as_slice(), [EntityId::Loan(_)]))
            })
            .unwrap();
        let Operation::DirectCall { arguments, .. } = &read.operation else {
            unreachable!()
        };
        let EntityId::Loan(loan) = arguments[0] else {
            unreachable!()
        };
        let end = function.instructions.iter().find(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan: actual } if actual == loan)
        }).unwrap();
        let order = &function.blocks[compare.block.index()].instructions;
        let index = |id| order.iter().position(|actual| *actual == id).unwrap();
        assert!(index(read.id) < index(end.id));
        assert!(index(end.id) < index(compare.id));
    }
    let expected = outcomes(operator)
        .into_iter()
        .map(|outcome| {
            let value = if outcome { "true" } else { "false" };
            match position {
                Position::BeforeBinding => format!("{value}\nkept\nconsume\n"),
                Position::WhileLive if after_last_read => format!("kept\n{value}\nconsume\n"),
                Position::WhileLive => format!("kept\n{value}\nkept\nconsume\n"),
                Position::AfterEnds => format!("kept\n{value}\nconsume\n"),
            }
        })
        .collect::<String>();
    run_unmodified(&program, entry, &label, expected.as_bytes());
}

macro_rules! comparisons {
    ($single:ident, $unit:ident, $reversed:ident, $single_const:ident, $unit_const:ident, $reversed_const:ident, $position:ident, $operator:ident) => {
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
        #[test]
        fn $single_const() {
            comparison_case(
                Pipeline::SingleConstant,
                Position::$position,
                ComparisonOperator::$operator,
            );
        }
        #[test]
        fn $unit_const() {
            comparison_case(
                Pipeline::UnitConstant,
                Position::$position,
                ComparisonOperator::$operator,
            );
        }
        #[test]
        fn $reversed_const() {
            comparison_case(
                Pipeline::UnitConstantReversed,
                Position::$position,
                ComparisonOperator::$operator,
            );
        }
    };
}

comparisons!(
    range_comparisons_ne_before_single,
    range_comparisons_ne_before_unit,
    range_comparisons_ne_before_reversed,
    range_comparisons_ne_before_single_constant,
    range_comparisons_ne_before_unit_constant,
    range_comparisons_ne_before_reversed_constant,
    BeforeBinding,
    NotEqual
);

comparisons!(
    range_comparisons_ne_live_single,
    range_comparisons_ne_live_unit,
    range_comparisons_ne_live_reversed,
    range_comparisons_ne_live_single_constant,
    range_comparisons_ne_live_unit_constant,
    range_comparisons_ne_live_reversed_constant,
    WhileLive,
    NotEqual
);

comparisons!(
    range_comparisons_ne_after_single,
    range_comparisons_ne_after_unit,
    range_comparisons_ne_after_reversed,
    range_comparisons_ne_after_single_constant,
    range_comparisons_ne_after_unit_constant,
    range_comparisons_ne_after_reversed_constant,
    AfterEnds,
    NotEqual
);

comparisons!(
    range_comparisons_le_before_single,
    range_comparisons_le_before_unit,
    range_comparisons_le_before_reversed,
    range_comparisons_le_before_single_constant,
    range_comparisons_le_before_unit_constant,
    range_comparisons_le_before_reversed_constant,
    BeforeBinding,
    LessThanOrEqual
);

comparisons!(
    range_comparisons_le_live_single,
    range_comparisons_le_live_unit,
    range_comparisons_le_live_reversed,
    range_comparisons_le_live_single_constant,
    range_comparisons_le_live_unit_constant,
    range_comparisons_le_live_reversed_constant,
    WhileLive,
    LessThanOrEqual
);

comparisons!(
    range_comparisons_le_after_single,
    range_comparisons_le_after_unit,
    range_comparisons_le_after_reversed,
    range_comparisons_le_after_single_constant,
    range_comparisons_le_after_unit_constant,
    range_comparisons_le_after_reversed_constant,
    AfterEnds,
    LessThanOrEqual
);

comparisons!(
    range_comparisons_gt_before_single,
    range_comparisons_gt_before_unit,
    range_comparisons_gt_before_reversed,
    range_comparisons_gt_before_single_constant,
    range_comparisons_gt_before_unit_constant,
    range_comparisons_gt_before_reversed_constant,
    BeforeBinding,
    GreaterThan
);

comparisons!(
    range_comparisons_gt_live_single,
    range_comparisons_gt_live_unit,
    range_comparisons_gt_live_reversed,
    range_comparisons_gt_live_single_constant,
    range_comparisons_gt_live_unit_constant,
    range_comparisons_gt_live_reversed_constant,
    WhileLive,
    GreaterThan
);

comparisons!(
    range_comparisons_gt_after_single,
    range_comparisons_gt_after_unit,
    range_comparisons_gt_after_reversed,
    range_comparisons_gt_after_single_constant,
    range_comparisons_gt_after_unit_constant,
    range_comparisons_gt_after_reversed_constant,
    AfterEnds,
    GreaterThan
);

comparisons!(
    range_comparisons_ge_before_single,
    range_comparisons_ge_before_unit,
    range_comparisons_ge_before_reversed,
    range_comparisons_ge_before_single_constant,
    range_comparisons_ge_before_unit_constant,
    range_comparisons_ge_before_reversed_constant,
    BeforeBinding,
    GreaterThanOrEqual
);

comparisons!(
    range_comparisons_ge_live_single,
    range_comparisons_ge_live_unit,
    range_comparisons_ge_live_reversed,
    range_comparisons_ge_live_single_constant,
    range_comparisons_ge_live_unit_constant,
    range_comparisons_ge_live_reversed_constant,
    WhileLive,
    GreaterThanOrEqual
);

comparisons!(
    range_comparisons_ge_after_single,
    range_comparisons_ge_after_unit,
    range_comparisons_ge_after_reversed,
    range_comparisons_ge_after_single_constant,
    range_comparisons_ge_after_unit_constant,
    range_comparisons_ge_after_reversed_constant,
    AfterEnds,
    GreaterThanOrEqual
);

// Last-use facts end metadata and source before the comparison inside the block.
#[test]
pub(in crate::ssa) fn range_comparisons_ne_last_read_single() {
    comparison_with_use(
        Pipeline::Single,
        Position::WhileLive,
        ComparisonOperator::NotEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ne_last_read_unit() {
    comparison_with_use(
        Pipeline::Unit,
        Position::WhileLive,
        ComparisonOperator::NotEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ne_last_read_reversed() {
    comparison_with_use(
        Pipeline::UnitReversed,
        Position::WhileLive,
        ComparisonOperator::NotEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ne_last_read_single_constant() {
    comparison_with_use(
        Pipeline::SingleConstant,
        Position::WhileLive,
        ComparisonOperator::NotEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ne_last_read_unit_constant() {
    comparison_with_use(
        Pipeline::UnitConstant,
        Position::WhileLive,
        ComparisonOperator::NotEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ne_last_read_reversed_constant() {
    comparison_with_use(
        Pipeline::UnitConstantReversed,
        Position::WhileLive,
        ComparisonOperator::NotEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_le_last_read_single() {
    comparison_with_use(
        Pipeline::Single,
        Position::WhileLive,
        ComparisonOperator::LessThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_le_last_read_unit() {
    comparison_with_use(
        Pipeline::Unit,
        Position::WhileLive,
        ComparisonOperator::LessThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_le_last_read_reversed() {
    comparison_with_use(
        Pipeline::UnitReversed,
        Position::WhileLive,
        ComparisonOperator::LessThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_le_last_read_single_constant() {
    comparison_with_use(
        Pipeline::SingleConstant,
        Position::WhileLive,
        ComparisonOperator::LessThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_le_last_read_unit_constant() {
    comparison_with_use(
        Pipeline::UnitConstant,
        Position::WhileLive,
        ComparisonOperator::LessThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_le_last_read_reversed_constant() {
    comparison_with_use(
        Pipeline::UnitConstantReversed,
        Position::WhileLive,
        ComparisonOperator::LessThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_gt_last_read_single() {
    comparison_with_use(
        Pipeline::Single,
        Position::WhileLive,
        ComparisonOperator::GreaterThan,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_gt_last_read_unit() {
    comparison_with_use(
        Pipeline::Unit,
        Position::WhileLive,
        ComparisonOperator::GreaterThan,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_gt_last_read_reversed() {
    comparison_with_use(
        Pipeline::UnitReversed,
        Position::WhileLive,
        ComparisonOperator::GreaterThan,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_gt_last_read_single_constant() {
    comparison_with_use(
        Pipeline::SingleConstant,
        Position::WhileLive,
        ComparisonOperator::GreaterThan,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_gt_last_read_unit_constant() {
    comparison_with_use(
        Pipeline::UnitConstant,
        Position::WhileLive,
        ComparisonOperator::GreaterThan,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_gt_last_read_reversed_constant() {
    comparison_with_use(
        Pipeline::UnitConstantReversed,
        Position::WhileLive,
        ComparisonOperator::GreaterThan,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ge_last_read_single() {
    comparison_with_use(
        Pipeline::Single,
        Position::WhileLive,
        ComparisonOperator::GreaterThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ge_last_read_unit() {
    comparison_with_use(
        Pipeline::Unit,
        Position::WhileLive,
        ComparisonOperator::GreaterThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ge_last_read_reversed() {
    comparison_with_use(
        Pipeline::UnitReversed,
        Position::WhileLive,
        ComparisonOperator::GreaterThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ge_last_read_single_constant() {
    comparison_with_use(
        Pipeline::SingleConstant,
        Position::WhileLive,
        ComparisonOperator::GreaterThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ge_last_read_unit_constant() {
    comparison_with_use(
        Pipeline::UnitConstant,
        Position::WhileLive,
        ComparisonOperator::GreaterThanOrEqual,
        true,
    );
}
#[test]
pub(in crate::ssa) fn range_comparisons_ge_last_read_reversed_constant() {
    comparison_with_use(
        Pipeline::UnitConstantReversed,
        Position::WhileLive,
        ComparisonOperator::GreaterThanOrEqual,
        true,
    );
}
