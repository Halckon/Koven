//! SPEC-0241: enum condition facts and explicit limits of the single-file slice.
use super::{Analysis, LoweringErrorKind, Operation, analyze, lower_scalar_file};
use crate::native_tests::return_control_tests::guide_litmus_04;

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
fn direct_enum_case_call_argument_remains_a_precise_missing_fact() {
    let analysis = analyze(
        "enum class Shape { Point }\nfun take(shape: Shape): Int = 0\nfun entry(): Int = take(Shape.Point)",
    );
    assert_frontend(&analysis);
    let error = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .err()
    .expect("direct enum case call argument gap remains");
    assert_eq!(error.kind, LoweringErrorKind::MissingFact);
    assert_eq!(
        analysis
            .sources
            .slice(error.span.expect("source range"))
            .unwrap(),
        "Shape.Point"
    );
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
