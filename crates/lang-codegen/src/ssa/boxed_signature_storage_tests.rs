//! Signature-only storage mapping without construction demand.
use super::{render_program, render_verified_program};
use crate::test_support::diagnostics::{assert_diagnostic, assert_text_eq};

#[test]
fn boxed_enum_signature_only_storage_accepts_box_or_enum_first() {
    for (first, second) in [("Box<Expr>", "Expr"), ("Expr", "Box<Expr>")] {
        let source = format!(
            "enum class Expr {{ Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }}\n\
             fun first(value: {first}): Unit {{}}\n\
             fun second(value: {second}): Unit {{}}"
        );
        use lang_frontend::{
            analysis::analyze_basic_unit_ownership,
            lexer::lex,
            name_resolution::{
                SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
            },
            ownership_checking::owned_compilation_unit_view,
            parser::parse_file,
            source::SourceMap,
            type_checking::{
                check_compilation_unit_types, complete_compilation_unit_types,
                standard_environments,
            },
        };
        let mut sources = SourceMap::new();
        let source_id = sources.add_source("lowering.ko", &source).unwrap();
        let lexed = lex(&sources, source_id).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let inputs = [SourceUnitInput::memory_document(source_id, &parsed)];
        let (name_env, type_env) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let raw_names =
            resolve_compilation_unit_names(&sources, &inputs, &index, &name_env).unwrap();
        assert!(raw_names.diagnostics().is_empty());
        let names = raw_names.validate().unwrap();
        let [document] = names.names().index().source_units() else {
            panic!("one original memory document")
        };
        assert_eq!(document.source_id(), source_id);
        assert_eq!(
            document.key(),
            &lang_frontend::name_resolution::SourceUnitKey::MemoryDocument
        );
        assert_eq!(sources.source_name(source_id), Ok("lowering.ko"));
        assert_eq!(sources.source_text(source_id), Ok(source.as_str()));
        let raw_types = check_compilation_unit_types(&sources, &inputs, &names, &type_env).unwrap();
        assert!(raw_types.diagnostics().is_empty());
        assert!(raw_types.constructions().is_empty());
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &type_env, &raw_types)
                .unwrap();
        assert!(complete.is_same_analysis(&raw_types));
        let (typed, raw_owned) =
            analyze_basic_unit_ownership(&sources, &inputs, &names, &type_env, raw_types)
                .unwrap()
                .into_result()
                .unwrap();
        assert!(raw_owned.diagnostics().is_empty());
        assert!(raw_owned.deferred().is_empty());
        let owned = raw_owned.validate().unwrap();
        let entries = ["first", "second"].map(|name| {
            let declarations = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|declaration| declaration.name() == name)
                .collect::<Vec<_>>();
            let [declaration] = declarations.as_slice() else {
                panic!("one original named root")
            };
            (
                declaration.id(),
                parsed.ast().items().get(declaration.root()).unwrap().span(),
            )
        });
        assert!(
            entries[0].0 < entries[1].0,
            "original declaration demand order"
        );
        let view =
            owned_compilation_unit_view(&sources, &inputs, &names, &type_env, &typed, &owned)
                .unwrap();
        // Both original signatures share one mapper/Program, unlike independent root lowering.
        // Swapping their parameter types retains Box-first and enum-first demand observations.
        let lower = || {
            let (program, roots) = crate::ssa::unit_lower::orchestrate::lower_roots(
                &view,
                &[entries[0].0, entries[1].0],
            )
            .expect("either recursive storage demand root must lower");
            assert_eq!(program.modules.len(), 1);
            let module = &program.modules[0];
            assert_eq!(module.functions.len(), 2);
            assert_eq!(roots.len(), 2);
            for (function, (declaration, span)) in module.functions.iter().zip(entries) {
                assert_eq!(function.id, roots[&declaration]);
                assert_eq!(function.origin, crate::ssa::model::Origin::Source(span));
                let entry = function.block(function.entry_block().unwrap()).unwrap();
                assert_eq!(entry.parameters.len(), 1);
                assert!(matches!(
                    function.entity(entry.parameters[0]).unwrap().ty,
                    crate::ssa::model::EntityType::Loan {
                        kind: crate::ssa::model::LoanKind::Shared,
                        ..
                    }
                ));
            }
            crate::ssa::verify::verify_program(&program).unwrap();
            program
        };
        let program = lower();
        let rendered = render_program(&program);
        assert_diagnostic!(rendered.contains("heap_owner"), "{rendered}");
        assert_diagnostic!(rendered.contains("tagged_union"), "{rendered}");
        assert_text_eq!(rendered, render_program(&lower()));
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

#[test]
fn boxed_enum_signature_only_unit_roots_accept_box_or_enum_first() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use crate::ssa::model::{EntityType, LoanKind, Origin};
    use lang_frontend::name_resolution::SymbolKind;

    for (first, second) in [("Box<Expr>", "Expr"), ("Expr", "Box<Expr>")] {
        let source = format!(
            "enum class Expr {{ Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }}\n\
             fun first(value: {first}): Unit {{}}\n\
             fun second(value: {second}): Unit {{}}"
        );
        // Independent Unit roots supplement the shared-mapper multi-root observation above.
        let unit = UnitSource::analyze("lowering.ko", &source)
            .unwrap_or_else(|error| panic!("{first}/{second}: strict Unit admission: {error:?}"));
        unit.assert_frontend_clean();
        assert!(unit.ownership().deferred().is_empty());
        assert!(unit.types().constructions().is_empty());
        let [document] = unit.names().names().index().source_units() else {
            panic!("one original memory document");
        };
        assert_eq!(document.source_id(), unit.parsed().source_id());
        assert_eq!(
            document.key(),
            &lang_frontend::name_resolution::SourceUnitKey::MemoryDocument
        );
        assert_eq!(
            unit.sources().source_name(document.source_id()),
            Ok("lowering.ko")
        );
        assert_eq!(
            unit.sources().source_text(document.source_id()),
            Ok(source.as_str())
        );
        for (name, parameter_type) in [("first", first), ("second", second)] {
            let declaration = unit.declaration(name, SymbolKind::Function);
            let span = unit.declaration_span(declaration);
            let original_declaration = format!("fun {name}(value: {parameter_type}): Unit {{}}");
            assert_eq!(
                unit.sources().slice(span),
                Ok(original_declaration.as_str())
            );
            let lower_root = || {
                let (program, root) = unit.lower_entry(declaration).unwrap_or_else(|error| {
                    panic!("{first}/{second}: {name}: Unit lowering: {error:?}")
                });
                let function = program
                    .module(root.module())
                    .and_then(|module| module.function(root))
                    .expect("original root belongs to its own lowered Program");
                assert_eq!(function.origin, Origin::Source(span));
                assert!(!function.blocks.is_empty(), "{name} has its original body");
                assert_eq!(function.blocks[0].parameters.len(), 1);
                assert!(
                    matches!(
                        function
                            .entity(function.blocks[0].parameters[0])
                            .map(|parameter| parameter.ty),
                        Some(EntityType::Loan {
                            kind: LoanKind::Shared,
                            ..
                        })
                    ),
                    "{first}/{second}: {name} retains its original Borrow parameter"
                );
                crate::ssa::verify::verify_program(&program).unwrap_or_else(|error| {
                    panic!("{first}/{second}: {name}: full Unit SSA verification: {error:?}")
                });
                let rendered = render_program(&program);
                assert_diagnostic!(rendered.contains("heap_owner"), "{name}: {rendered}");
                assert_diagnostic!(rendered.contains("tagged_union"), "{name}: {rendered}");
                render_verified_program(&program).unwrap_or_else(|error| {
                    panic!("{first}/{second}: {name}: full Unit LLVM verification: {error:?}")
                });
                rendered
            };
            assert_eq!(
                lower_root(),
                lower_root(),
                "{first}/{second}: {name}: repeated Unit root lowering is deterministic"
            );
        }
    }
}

#[path = "owner_consumer_test_support.rs"]
pub(crate) mod owner_consumer_test_support;
