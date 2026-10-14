//! SPEC-0241: enum condition facts and explicit limits of the single-file slice.
use super::{Analysis, Operation, analyze, lower_scalar_file};
use crate::native_tests::return_control_tests::guide_litmus_04;
use lang_frontend::{
    ownership_checking::{LoanEndPoint, LoanKind, LoanTarget},
    type_checking::{ConstructionTarget, ExpressionCategory, ParameterMode},
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
    let analysis = analyze(guide_litmus_04());
    assert_frontend(&analysis);
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
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
    let analysis = analyze(
        "enum class Shape { Point }\nfun take(shape: Shape): Int = 0\nfun entry(): Int = take(Shape.Point)",
    );
    assert_frontend(&analysis);
    assert!(analysis.owned.deferred().is_empty());
    let construction = analysis
        .typed
        .constructions()
        .iter()
        .find(|construction| {
            let node = analysis
                .parsed
                .ast()
                .expressions()
                .get(construction.expression())
                .unwrap();
            analysis.sources.slice(node.span()) == Ok("Shape.Point")
        })
        .expect("original direct enum case construction");
    assert!(matches!(
        construction.target(),
        ConstructionTarget::EnumCase(_)
    ));
    assert!(construction.arguments().is_empty());
    let operand = construction.expression();
    let loans = analysis
        .owned
        .loans()
        .iter()
        .filter(|loan| loan.argument() == operand)
        .collect::<Vec<_>>();
    assert_eq!(loans.len(), 1);
    let loan = loans[0];
    assert_eq!(loan.kind(), LoanKind::Shared);
    assert_eq!(loan.target(), &LoanTarget::Temporary(operand));
    let call = analysis
        .typed
        .call(loan.call())
        .expect("real outer take call");
    assert_eq!(call.arguments().len(), 1);
    assert_eq!(
        (call.arguments()[0].mode(), call.arguments()[0].category()),
        (ParameterMode::Borrow, ExpressionCategory::Temporary)
    );
    let call_span = analysis
        .parsed
        .ast()
        .expressions()
        .get(call.expression())
        .unwrap()
        .span();
    assert_eq!(analysis.sources.slice(call_span), Ok("take(Shape.Point)"));
    assert_eq!(loan.end_span(), call_span);
    let ends = analysis
        .owned
        .loan_ends()
        .iter()
        .filter(|end| end.call() == call.expression() && end.argument() == operand)
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 1);
    assert_eq!(ends[0].point(), LoanEndPoint::CallReturn(call.expression()));
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("the original direct case Borrow source lowers");
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
    crate::llvm::render_verified_program(&program)
        .expect("direct enum case Borrow verifies through LLVM");
}
