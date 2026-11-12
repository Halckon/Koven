//! Map 查询 runtime 合同尚未启用的源码边界。
#[path = "lower_frontend_map_key_return_tests.rs"]
mod map_key_return_tests;
use super::LoweringErrorKind;

#[path = "lower_frontend_map_loop_tests.rs"]
mod loop_owner_tests;

#[test]
fn map_nullable_value_owned_remove_remains_unsupported() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    let analysis = UnitSource::analyze(
        "lowering.ko",
        "class Token(val n: Int)\nfun inspect(inout m: MutableMap<Int, Token?>): Unit {\n val result = m.remove(1)\n}",
    )
    .expect("original nullable remove source passes complete Unit frontend admission");
    analysis.assert_frontend_clean();
    let declaration = analysis.declaration("inspect", SymbolKind::Function);
    let Err(error) = analysis.lower_entry(declaration) else {
        panic!("nullable owned remove requires a separate Missing contract")
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
    assert_eq!(
        error.span.unwrap().source_id(),
        analysis.parsed().source_id()
    );
}

#[test]
fn map_operands_follow_argument_control_flow() {
    for operation in [
        "m.put(if (true) { 1 } else { 2 }, 3)",
        "m.put(1, if (true) { 3 } else { 4 })",
        "m.get(if (true) { 1 } else { 2 })",
        "m.contains(if (true) { 1 } else { 2 })",
        "m.remove(if (true) { 1 } else { 2 })",
    ] {
        let source = format!(
            "fun entry(): Unit {{\n var m = mutableMapOf<Int, Int>()\n {operation}\n m.size\n}}"
        );
        assert_map_source_lowers(&source);
    }
    for key in ["key", "\"key\".clone()"] {
        let source = format!(
            "fun entry(): Unit {{\n var m = mutableMapOf<String, Int>()\n val key = \"key\".clone()\n m.put({key}, if (true) {{ 3 }} else {{ 4 }})\n m.size\n}}"
        );
        assert_map_source_lowers(&source);
    }
}

#[test]
fn map_mutation_collects_drop_glue_before_abort() {
    for (declarations, body) in [
        (
            "class Resource(val n: Int) { deinit() { println(\"drop\") } }",
            "var m = mutableMapOf<Int, Resource>()\n m.put(1, Resource(1))\n error(\"stop\")",
        ),
        (
            "",
            "var m = mutableMapOf<String, Int>()\n val key = \"key\".clone()\n m.remove(key)\n error(key)",
        ),
    ] {
        let source = format!("{declarations}\nfun entry(): Unit {{\n {body}\n}}");
        assert_map_source_lowers(&source);
    }
}

#[test]
fn map_put_control_exit_mixed_return_key() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { return } else { 8 }, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn map_put_control_exit_mixed_return_value() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, if (flag) { return } else { 8 })\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn map_put_control_exit_mixed_reversed_key() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { 8 } else { return }, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn map_put_control_exit_mixed_reversed_value() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, if (flag) { 8 } else { return })\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 1);
}

#[test]
fn map_put_control_exit_both_return_key() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { return } else { return }, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn map_put_control_exit_both_return_value() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, if (flag) { return } else { return })\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn map_put_control_exit_direct_return_key() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(return, 8)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn map_put_control_exit_direct_return_value() {
    let source = "fun suffix(): Unit { println(\"suffix\") }\nfun inspect(own flag: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(1, return)\n suffix()\n m.size\n}\nfun entry(): Unit { inspect(false); inspect(true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 0);
}

#[test]
fn map_put_control_exit_key_then_value() {
    let source = "fun inspect(own flag: Boolean, own other: Boolean): Unit {\n var m = mutableMapOf<Int, Int>()\n m.put(if (flag) { return } else { 8 }, if (other) { return } else { 8 })\n m.size\n}\nfun entry(): Unit { inspect(false, false); inspect(false, true); inspect(true, false); inspect(true, true) }";
    let program = assert_map_source_lowers(source);
    assert_map_put_count(&program, 1);
}

fn assert_map_put_count(program: &crate::ssa::model::Program, expected: usize) {
    use crate::ssa::model::Operation;
    let function = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name.contains("inspect"))
        .unwrap();
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::MapPut { .. }))
            .count(),
        expected
    );
    if expected == 0 {
        assert!(
            !function
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.operation, Operation::DirectCall { .. })),
            "a fully diverged put must not lower the suffix call"
        );
    }
}

fn assert_map_source_lowers(source: &str) -> crate::ssa::model::Program {
    lower_map_unit(source, "entry").0
}

fn lower_map_unit(
    source: &str,
    entry_name: &str,
) -> (
    crate::ssa::model::Program,
    crate::native_tests::boxed_enum_tests::unit_source::UnitSource,
) {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    let analysis = UnitSource::analyze("lowering.ko", source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    analysis.assert_frontend_clean();
    let declaration = analysis.declaration(entry_name, SymbolKind::Function);
    let (program, entry) = analysis
        .lower_entry(declaration)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    assert_eq!(
        program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap()
            .origin,
        crate::ssa::model::Origin::Source(analysis.declaration_span(declaration))
    );
    crate::llvm::render_verified_program(&program)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    (program, analysis)
}

#[test]
fn map_remove_temporary_null_comparison_inspects_real_drop_facts() {
    use lang_frontend::{
        ownership_checking::{UnitDropPoint, UnitDropTarget},
        parser::{BinaryOperator, Expression, LiteralKind},
    };
    let source = "fun entry() { var m = mutableMapOf<Int, String>(); m.put(1, \"text\".clone()); val present = m.remove(1) != null; if (present) { println(\"present\") } }";
    let (_, analysis) = lower_map_unit(source, "entry");
    let [remove] = analysis.types().map_removes() else {
        panic!("one original Map remove descriptor")
    };
    let removed = remove.expression();
    let drops = analysis
        .ownership()
        .drops()
        .iter()
        .filter(|fact| fact.target() == UnitDropTarget::Temporary(removed))
        .collect::<Vec<_>>();
    assert_eq!(
        drops.len(),
        1,
        "frontend publishes the actual result cleanup"
    );
    let UnitDropPoint::AfterBinaryOperands(comparison) = drops[0].point() else {
        panic!("the original temporary is cleaned after the null comparison operands")
    };
    assert_eq!(comparison.source_unit(), removed.source_unit());
    assert_eq!(
        removed.source_unit(),
        analysis.names().names().index().source_units()[0].id()
    );
    let expressions = analysis.parsed().ast().expressions();
    let Expression::Binary {
        left,
        operator: BinaryOperator::NotEqual,
        right,
        ..
    } = expressions.get(comparison.expression()).unwrap().payload()
    else {
        panic!("the fact belongs to the original non-null comparison")
    };
    assert_eq!(*left, removed.expression());
    assert!(matches!(
        expressions.get(*right).unwrap().payload(),
        Expression::Literal(LiteralKind::Null)
    ));
    assert_eq!(
        drops[0].value_origin(),
        expressions.get(removed.expression()).unwrap().span()
    );
    assert_eq!(
        analysis.types().expression_type(removed),
        Some(remove.result_type())
    );
}

#[test]
fn map_remove_single_for_guarded_owner_is_carried() {
    let source =
        include_str!("../../tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko");
    loop_owner_tests::unit_identity::check_guarded_original(source);
}

pub(in crate::ssa) mod return_control_tests {
//! SPEC-0241: original enum condition facts and explicit limits through strict Unit.
use super::super::{LoweringErrorKind, Operation};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::native_tests::return_control_tests::guide_litmus_04;
use crate::test_support::diagnostics::assert_eq_diagnostic;
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::{LoanKind, UnitLoanTarget},
    type_checking::{ExpressionCategory, ParameterMode, UnitConstructionTarget},
};

#[test]
fn guide_litmus_04_uses_three_boolean_tag_tests_without_constructing_a_condition() {
    let analysis = UnitSource::analyze("lowering.ko", guide_litmus_04())
        .expect("the exact Guide example passes strict Unit admission");
    analysis.assert_frontend_clean();
    let declaration = analysis.declaration("area", SymbolKind::Function);
    let (program, _) = analysis
        .lower_entry(declaration)
        .expect("exact Guide example lowers");
    let instructions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .flat_map(|function| &function.instructions)
        .collect::<Vec<_>>();
    assert_eq!(
        instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::TaggedDiscriminant { .. }
            ))
            .count(),
        3
    );
    assert_eq!(
        instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Compare { .. }))
            .count(),
        3
    );
    assert!(
        !instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::TaggedConstruct { .. }))
    );
    crate::llvm::render_verified_program(&program).expect("Boolean conditions verify through LLVM");
}

#[test]
fn direct_enum_case_call_argument_has_temporary_loan_and_verified_ssa() {
    let analysis = UnitSource::analyze(
        "lowering.ko",
        "enum class Shape { Point }\nfun take(shape: Shape): Int = 0\nfun entry(): Int = take(Shape.Point)",
    )
    .expect("the original direct enum case Borrow source passes strict Unit admission");
    analysis.assert_frontend_clean();
    assert!(analysis.ownership().deferred().is_empty());
    let [source] = analysis.names().names().index().source_units() else {
        panic!("one original memory document");
    };
    assert_eq!(source.source_id(), analysis.parsed().source_id());
    assert_eq!(
        source.key(),
        &lang_frontend::name_resolution::SourceUnitKey::MemoryDocument
    );
    assert_eq!(
        analysis.sources().source_name(source.source_id()),
        Ok("lowering.ko")
    );
    let construction = analysis
        .types()
        .constructions()
        .iter()
        .find(|construction| {
            assert_eq!(construction.expression().source_unit(), source.id());
            let node = analysis
                .parsed()
                .ast()
                .expressions()
                .get(construction.expression().expression())
                .unwrap();
            analysis.sources().slice(node.span()) == Ok("Shape.Point")
        })
        .expect("original direct enum case construction");
    assert!(matches!(
        construction.target(),
        UnitConstructionTarget::EnumCase(_)
    ));
    assert!(construction.arguments().is_empty());
    let operand = construction.expression();
    let loans = analysis
        .ownership()
        .loans()
        .iter()
        .filter(|loan| loan.argument() == operand)
        .collect::<Vec<_>>();
    assert_eq!(loans.len(), 1);
    let loan = loans[0];
    assert_eq!(loan.kind(), LoanKind::Shared);
    assert_eq!(loan.target(), &UnitLoanTarget::Temporary(operand));
    let call = analysis
        .types()
        .call(loan.call())
        .expect("real outer take call");
    assert_eq!(call.expression().source_unit(), source.id());
    assert_eq!(call.arguments().len(), 1);
    assert_eq!(
        (call.arguments()[0].mode(), call.arguments()[0].category()),
        (ParameterMode::Borrow, ExpressionCategory::Temporary)
    );
    let call_span = analysis
        .parsed()
        .ast()
        .expressions()
        .get(call.expression().expression())
        .unwrap()
        .span();
    assert_eq!(analysis.sources().slice(call_span), Ok("take(Shape.Point)"));
    assert_eq!(loan.end_span(), call_span);
    // UnitLoanFact's public contract ends a successful synchronous loan at call return;
    // unlike the legacy API, Unit represents that endpoint on the same loan fact.
    let ends = analysis
        .ownership()
        .loans_ending_at(call.expression())
        .filter(|end| end.argument() == operand)
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 1);
    assert!(std::ptr::eq(ends[0], loan));
    assert_eq!(ends[0].call(), call.expression());
    assert_eq!(ends[0].end_span(), call_span);
    for (name, original_declaration) in [
        ("take", "fun take(shape: Shape): Int = 0"),
        ("entry", "fun entry(): Int = take(Shape.Point)"),
    ] {
        let declaration = analysis.declaration(name, SymbolKind::Function);
        let span = analysis.declaration_span(declaration);
        assert_eq!(analysis.sources().slice(span), Ok(original_declaration));
        let (program, root) = analysis
            .lower_entry(declaration)
            .expect("the original direct case Borrow source lowers from each real root");
        let function = program
            .module(root.module())
            .and_then(|module| module.function(root))
            .expect("the real root belongs to its own lowered Program");
        assert_eq!(function.origin, crate::ssa::model::Origin::Source(span));
        assert!(!function.blocks.is_empty(), "the real root has a body");
        crate::ssa::verify::verify_program(&program)
            .expect("each original root independently verifies through SSA");
        if name == "entry" {
            assert_eq!(
                program
                    .modules
                    .iter()
                    .flat_map(|module| &module.functions)
                    .flat_map(|function| &function.instructions)
                    .filter(|instruction| matches!(
                        instruction.operation,
                        Operation::TaggedConstruct { .. }
                    ))
                    .count(),
                1
            );
        }
        // No native entry wrapper: take has its original Borrow parameter.
        crate::llvm::render_verified_program(&program)
            .expect("each direct enum case Borrow root verifies through LLVM");
    }
}

#[test]
fn enum_tag_slice_does_not_claim_general_payload_grouped_or_move_only_equality() {
    for (source, condition) in [
        (
            "enum class Flag { On, Off }\nfun score(s: Flag, other: Flag): Int = when (s) { other -> 1; else -> 0 }",
            "other",
        ),
        (
            "enum class Flag { On, Off }\nfun score(s: Flag): Int = when (s) { (Flag.On) -> 1; else -> 0 }",
            "(Flag.On)",
        ),
        (
            "enum class Flag { On(item: Int), Off }\nfun score(s: Flag): Int = when (s) { Flag.On(1) -> 1; else -> 0 }",
            "Flag.On(1)",
        ),
        (
            "enum class Flag { On(text: String), Off }\nfun score(own s: Flag): Int = when (s) { Flag.Off -> 1; else -> 0 }",
            "Flag.Off",
        ),
    ] {
        let analysis = UnitSource::analyze("lowering.ko", source)
            .expect("each original negative source passes strict Unit admission");
        analysis.assert_frontend_clean();
        assert!(analysis.ownership().deferred().is_empty());
        let [document] = analysis.names().names().index().source_units() else {
            panic!("one original MemoryDocument");
        };
        assert_eq!(
            document.key(),
            &lang_frontend::name_resolution::SourceUnitKey::MemoryDocument
        );
        assert_eq!(document.source_id(), analysis.parsed().source_id());
        assert_eq!(
            analysis.sources().source_name(document.source_id()),
            Ok("lowering.ko")
        );
        assert_eq!(
            analysis.sources().source_text(document.source_id()),
            Ok(source)
        );
        if condition == "Flag.Off" {
            // The original MoveOnly fixture reaches the snapshot-tag rejection, not
            // the later Copyable enum construction guard.
            assert!(
                analysis.ownership().drops().iter().any(|fact| {
                    let lang_frontend::ownership_checking::UnitDropPoint::AfterExpression(
                        expression,
                    ) = fact.point()
                    else {
                        return false;
                    };
                    let node = analysis.parsed().ast().expressions()
                        .get(expression.expression()).unwrap();
                    if expression.source_unit() != document.id()
                        || analysis.sources().slice(node.span()) != Ok("s")
                        || !matches!(node.payload(), lang_frontend::parser::Expression::Name)
                    {
                        return false;
                    }
                    analysis.names().names().references().iter().any(|reference| {
                        reference.source_unit() == document.id()
                            && reference.span() == node.span()
                            && matches!(reference.target(),
                                lang_frontend::name_resolution::UnitReferenceTarget::Symbol(root)
                                    if fact.target() == lang_frontend::ownership_checking::UnitDropTarget::Named(*root))
                    })
                }),
                "original last-use subject cleanup must require a tag snapshot"
            );
            assert!(!analysis.ownership().drops().iter().any(|fact| matches!(
                fact.point(),
                lang_frontend::ownership_checking::UnitDropPoint::WhenComparisonCompleted { .. }
                    | lang_frontend::ownership_checking::UnitDropPoint::WhenAlternativeNoMatch { .. }
            )), "the enum fixture must not fail at the String comparison cleanup guard");
        }
        let declaration = analysis.declaration("score", SymbolKind::Function);
        assert_eq!(
            analysis
                .sources()
                .slice(analysis.declaration_span(declaration)),
            Ok(&source[source.find("fun score").unwrap()..])
        );
        let error = analysis
            .lower_entry(declaration)
            .err()
            .expect("unsupported enum equality must fail explicitly");
        assert_eq_diagnostic!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{source}: {error:?}"
        );
        let span = error.span.expect("source range");
        assert_eq!(span.source_id(), analysis.parsed().source_id());
        assert_eq!(analysis.sources().slice(span), Ok(condition), "{source}");
        let start = source.find(&format!("{condition} ->")).unwrap();
        assert_eq!(span.start(), start, "{source}");
        assert_eq!(span.end(), start + condition.len(), "{source}");
    }
}
}
