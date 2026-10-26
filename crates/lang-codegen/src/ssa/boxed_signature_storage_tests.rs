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
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    for wrapper in [
        "Box<Wrapped>",
        "Box<Wrapped>?",
        "Rc<Box<Wrapped>>",
        "Array<Box<Wrapped>>",
    ] {
        let text = format!(
            "value class Wrapped(val value: Int)\nfun inspect(input: {wrapper}): Unit {{}}"
        );
        let analysis = UnitSource::analyze("lowering.ko", &text)
            .unwrap_or_else(|error| panic!("{wrapper}: {error:?}"));
        analysis.assert_frontend_clean();
        assert!(analysis.ownership().deferred().is_empty());
        assert!(analysis.types().constructions().is_empty());
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
        assert_eq!(
            analysis.sources().source_text(source.source_id()),
            Ok(text.as_str())
        );
        let declaration = analysis.declaration("inspect", SymbolKind::Function);
        let span = analysis.declaration_span(declaration);
        let original_declaration = format!("fun inspect(input: {wrapper}): Unit {{}}");
        assert_eq!(
            analysis.sources().slice(span),
            Ok(original_declaration.as_str())
        );
        // The original parameter is the storage root; no construction or native wrapper is added.
        let (program, root) = analysis
            .lower_entry(declaration)
            .unwrap_or_else(|error| panic!("{wrapper}: {error:?}"));
        let function = program
            .module(root.module())
            .and_then(|module| module.function(root))
            .expect("inspect belongs to its own lowered Program");
        assert_eq!(function.origin, crate::ssa::model::Origin::Source(span));
        assert!(!function.blocks.is_empty(), "inspect has its original body");
        assert_eq!(function.blocks[0].parameters.len(), 1);
        crate::ssa::verify::verify_program(&program)
            .expect("existing wrapped Box payload must verify through SSA");
        render_verified_program(&program)
            .expect("existing wrapped Box payload must stay fully defined");
    }
}
