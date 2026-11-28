//! The write is admissible in isolation; it invalidates a non-null flow fact.
use super::*;
use lang_frontend::{
    diagnostic::{DiagnosticCodeCatalog, Severity},
    name_resolution::{ReferenceTarget, SourceUnitKey, SymbolKind},
    parser::AssignmentOperator,
};

pub(super) fn check(text: &str) {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "main.ko", text);
    assert_eq!(sources.source_text(source), Ok(text));
    let ast = file.ast();
    let primary = sources.span(source, 98, 105).unwrap();
    let annotation = sources.span(source, 92, 95).unwrap();
    assert_eq!(sources.slice(primary), Ok("current"));
    assert_eq!(sources.slice(annotation), Ok("Int"));
    let catalog = DiagnosticCodeCatalog::try_new(&["L0084"]).unwrap();
    let mut expected = Diagnostic::new(
        &sources,
        Severity::Error,
        catalog.resolve("L0084").unwrap(),
        "expression type does not match the expected type",
        primary,
    )
    .unwrap();
    expected
        .add_label(&sources, annotation, "expected Int, found Int?")
        .unwrap();
    let reads = ast
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (matches!(node.payload(), Expression::Name)
                && sources.slice(node.span()) == Ok("current"))
            .then_some((node.span(), id))
        })
        .collect::<Vec<_>>();
    let [
        (first_span, guarded),
        (write_span, target),
        (last_span, later),
    ] = reads.as_slice()
    else {
        panic!("guard, assignment target, and later read")
    };
    assert!([first_span.start(), write_span.start(), last_span.start()].is_sorted());
    assert_eq!(*last_span, primary);

    let (ne, te) = standard_environments();
    for (rooted, input) in [
        (false, SourceUnitInput::memory_document(source, &file)),
        (true, SourceUnitInput::new("root", "main.ko", source, &file)),
    ] {
        let inputs = [input];
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        assert!(index.diagnostics().is_empty());
        let [entry] = index.source_units() else {
            panic!("one source")
        };
        assert_eq!(entry.source_id(), source);
        assert!(match (rooted, entry.key()) {
            (false, SourceUnitKey::MemoryDocument) => true,
            (true, SourceUnitKey::Rooted { root, logical_path }) =>
                root.as_str() == "root" && logical_path.as_str() == "main.ko",
            _ => false,
        });
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &ne)
            .unwrap()
            .validate()
            .unwrap();
        let local = names.names().source_units()[0].resolution();
        let binding = local
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "current")
            .unwrap();
        assert_eq!(binding.kind(), SymbolKind::Variable);
        for (span, _) in &reads {
            assert!(
                local
                    .references()
                    .iter()
                    .any(|reference| reference.span() == *span
                        && reference.target() == &ReferenceTarget::Symbol(binding.id()))
            );
        }

        let types = check_compilation_unit_types(&sources, &inputs, &names, &te).unwrap();
        assert_eq!(types.diagnostics(), std::slice::from_ref(&expected));
        let unit = entry.id();
        let id = |expression| UnitExpressionId::new(unit, expression);
        let [write] = types.assignments() else {
            panic!("actual well-typed write before later error")
        };
        assert_eq!(write.operator(), AssignmentOperator::Assign);
        assert_eq!(write.target(), id(*target));
        assert!(write.falls_through());
        let int = types.types().builtin(BuiltinType::Int).unwrap();
        let nullable = write.target_type();
        assert_eq!(
            types.types().get(nullable),
            Some(&UnitTypeKind::Nullable(int))
        );
        assert_eq!(types.expression_type(id(*guarded)), Some(nullable));
        assert_eq!(types.expression_type(id(*target)), Some(int));
        assert_eq!(
            sources.slice(
                ast.expressions()
                    .get(write.value().expression())
                    .unwrap()
                    .span()
            ),
            Ok("null")
        );
        assert_eq!(
            types
                .expression_type(id(*later))
                .and_then(|ty| types.types().get(ty)),
            Some(&UnitTypeKind::Error)
        );
        let Err(FinalTypeAdmissionError::Diagnostics(final_diagnostics)) =
            complete_compilation_unit_types(&sources, &inputs, &names, &te, &types)
        else {
            panic!("the later read must fail final type admission")
        };
        assert_eq!(final_diagnostics.as_slice(), types.diagnostics());
        assert!(types.validate().is_err());
    }
}
