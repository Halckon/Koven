//! Signature-only storage mapping without construction demand.
use super::{analyze, lower_scalar_file, render_program, render_verified_program};

#[test]
fn boxed_enum_signature_only_storage_accepts_box_or_enum_first() {
    for (first, second) in [("Box<Expr>", "Expr"), ("Expr", "Box<Expr>")] {
        let source = format!(
            "enum class Expr {{ Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }}\n\
             fun first(value: {first}): Unit {{}}\n\
             fun second(value: {second}): Unit {{}}"
        );
        let analysis = analyze(&source);
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(analysis.owned.diagnostics().is_empty());
        // Without any construction demand, the first parameter is the mapper's actual entry root.
        assert!(analysis.typed.constructions().is_empty());
        let lower = || {
            lower_scalar_file(
                &analysis.sources,
                &analysis.parsed,
                &analysis.names,
                &analysis.typed,
                &analysis.owned,
            )
            .expect("either recursive storage demand root must lower")
        };
        let program = lower();
        let rendered = render_program(&program);
        assert!(rendered.contains("heap_owner"), "{rendered}");
        assert!(rendered.contains("tagged_union"), "{rendered}");
        assert_eq!(rendered, render_program(&lower()));
        render_verified_program(&program).expect("signature-only recursive Box/enum LLVM verifies");
    }
}

#[test]
fn boxed_enum_mapper_preserves_nonrecursive_value_wrappers() {
    for wrapper in [
        "Box<Wrapped>",
        "Box<Wrapped>?",
        "Rc<Box<Wrapped>>",
        "Array<Box<Wrapped>>",
    ] {
        let analysis = analyze(&format!(
            "value class Wrapped(val value: Int)\nfun inspect(input: {wrapper}): Unit {{}}"
        ));
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(analysis.owned.diagnostics().is_empty());
        assert!(analysis.typed.constructions().is_empty());
        let program = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .unwrap_or_else(|error| panic!("{wrapper}: {error:?}"));
        render_verified_program(&program)
            .expect("existing wrapped Box payload must stay fully defined");
    }
}
