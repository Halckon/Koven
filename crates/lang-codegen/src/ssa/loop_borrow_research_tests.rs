//! Research distinguishes validated loop neighbors from ownership-rejected local bindings.
use super::*;
use lang_frontend::name_resolution::{index_compilation_unit, resolve_compilation_unit_names};
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
use lang_frontend::type_checking::check_compilation_unit_types;

#[test]
fn ordinary_loop_borrow_neighbors_preserve_phase_boundary() {
    for (name, body) in [
        (
            "outer-binding",
            "borrow val item = view(source); while (flag) { consume(item); break }",
        ),
        (
            "outer-binding-continue",
            "borrow val item = view(source); while (flag) { consume(item); continue }",
        ),
        (
            "outer-binding-return",
            "borrow val item = view(source); while (flag) { consume(item); return }",
        ),
        (
            "outer-binding-zero",
            "borrow val item = view(source); while (false) { consume(item) }",
        ),
        (
            "lexical-end-before-loop",
            "{ borrow val item = view(source); consume(item) }; while (flag) { consume(source); break }",
        ),
        (
            "temporary-in-loop",
            "while (flag) { consume(view(source)); break }",
        ),
        ("plain-call-loop", "while (flag) { consume(source); break }"),
    ] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun run(source: String, flag: Boolean): Unit {{ {body} }}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "loop-neighbor.ko", &text);
        let (environment, types) = standard_environments();
        for (layout, input) in [
            (
                "rooted",
                SourceUnitInput::new("root", "loop-neighbor.ko", source, &parsed),
            ),
            ("memory", SourceUnitInput::memory_document(source, &parsed)),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let on = with_loan_observation(true, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            assert!(legacy_ownership_eq(&on, &off));
            if name == "temporary-in-loop" {
                assert_eq!(on.diagnostics().len(), 1);
                let diagnostic = &on.diagnostics()[0];
                assert_eq!(diagnostic.code().to_string(), "L0164");
                let span = diagnostic.primary_span();
                assert_eq!(&text[span.start()..span.end()], "view(source)");
                assert!(on.validate().is_err());
                assert!(off.validate().is_err());
                continue;
            }
            let on = on.validate().unwrap();
            let off = off.validate().unwrap();
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let mut previous = None;
            for (setting, owned) in [("on", &on), ("off", &off)] {
                let (result, verified) = crate::ssa::unit_lower::observe_final_verifier(|| {
                    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, owned, entry,
                    )
                });
                let summary = if name == "plain-call-loop" {
                    let (program, _) = result
                        .unwrap_or_else(|error| panic!("{name}/{layout}/{setting}: {error:?}"));
                    assert_eq!(verified.len(), 1);
                    verify_program(&program).unwrap();
                    let run = program.modules[0]
                        .functions
                        .iter()
                        .find(|function| function.name.contains(".run."))
                        .unwrap();
                    assert_eq!(
                        run.instructions
                            .iter()
                            .filter(|instruction| matches!(
                                instruction.operation,
                                Operation::BorrowCall { .. }
                            ))
                            .count(),
                        0
                    );
                    assert_eq!(
                        run.instructions
                            .iter()
                            .filter(|instruction| matches!(
                                instruction.operation,
                                Operation::DirectCall { .. }
                            ))
                            .count(),
                        1
                    );
                    assert!(run.blocks.len() > 1);
                    format!(
                        "{verified:?}\n{}",
                        crate::llvm::render_verified_program(&program).unwrap()
                    )
                } else {
                    assert!(
                        verified.is_empty(),
                        "{name}/{layout}/{setting}: no final verifier on rejection"
                    );
                    let error = result.err().expect("unsupported loop boundary");
                    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
                    let span = error.span.unwrap();
                    assert_eq!(span.source_id(), source);
                    let expected = if name == "outer-binding-return" {
                        "return"
                    } else {
                        &body[body.find("while").unwrap()..]
                    };
                    assert_eq!(
                        &text[span.start()..span.end()],
                        expected,
                        "{name}/{layout}/{setting}"
                    );
                    format!("{error:?}")
                };
                if let Some(previous) = previous.as_ref() {
                    assert_eq!(&summary, previous);
                }
                previous = Some(summary);
            }
        }
    }
}
