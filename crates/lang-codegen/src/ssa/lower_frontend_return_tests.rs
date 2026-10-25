//! SPEC-0241: enum condition facts and explicit limits of the single-file slice.
use super::{Analysis, LoweringErrorKind, Operation, analyze, lower_scalar_file};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::native_tests::return_control_tests::guide_litmus_04;
use lang_frontend::{
    name_resolution::SymbolKind,
    ownership_checking::{LoanKind, UnitLoanTarget},
    type_checking::{ExpressionCategory, ParameterMode, UnitConstructionTarget},
};

fn assert_frontend(analysis: &Analysis) {
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
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
}

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
        let analysis = analyze(source);
        assert_frontend(&analysis);
        let error = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .err()
        .expect("unsupported enum equality must fail explicitly");
        assert_eq!(
            error.kind,
            LoweringErrorKind::UnsupportedNode,
            "{source}: {error:?}"
        );
        assert_eq!(
            analysis
                .sources
                .slice(error.span.expect("source range"))
                .unwrap(),
            condition,
            "{source}"
        );
    }
}
