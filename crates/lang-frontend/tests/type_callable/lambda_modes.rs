use super::*;
use lang_frontend::{
    name_resolution::{SourceUnitId, SymbolId, UnitSymbolId},
    source::Span,
    type_checking::CompilationUnitTypes,
};

use super::unit_entries::checked_named_entries;

fn body_symbol(
    typed: &CompilationUnitTypes,
    source: SourceUnitId,
    local: SymbolId,
) -> UnitSymbolId {
    typed
        .body_symbol_types()
        .keys()
        .copied()
        .find(|id| id.source_unit() == source && id.symbol() == local)
        .expect("source-qualified lambda parameter")
}

fn headerless_openers(sources: &SourceMap, parsed: &ParsedFile) -> Vec<Span> {
    let mut openers = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Expression::Lambda {
                opener_span,
                arrow_span: None,
                ..
            } => Some(*opener_span),
            _ => None,
        })
        .collect::<Vec<_>>();
    openers.sort_by_key(|span| span.start());
    assert!(openers.iter().all(|&span| sources.slice(span) == Ok("{")));
    openers
}

#[test]
fn headerless_lambdas_publish_contextual_it_or_stable_arity_diagnostics() {
    let valid = "fun applyBorrow(callback: (borrow Int) -> Int): Int = callback(1)\n\
                 fun applyOwn(callback: (own Int) -> Int): Int = callback(1)\n\
                 fun use(): Int {\n\
                     val first = applyBorrow { it }\n\
                     val second = applyOwn({ it })\n\
                     val mutate: (inout Int) -> Int = { it }\n\
                     val unused: (borrow Int) -> Int = { 1 }\n\
                     val shadowed: (borrow Int) -> Unit = {\n\
                         val it = it\n\
                     }\n\
                     val nested: (borrow Int) -> (own Int) -> Int = { ({ it }) }\n\
                     return first + second\n\
                 }";
    let (sources, parsed_file) = parsed(valid);
    let (names, types) = environments();
    for (names, typed) in checked_named_entries(&sources, &parsed_file, &names, &types) {
        let resolved_source = &names.source_units()[0];
        let source = resolved_source.source_unit();
        let resolution = resolved_source.resolution();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());

        let implicit = resolution
            .symbols()
            .iter()
            .filter(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
            .map(|symbol| {
                let id = body_symbol(&typed, source, symbol.id());
                (
                    symbol.name(),
                    symbol.span(),
                    typed.body_parameter_mode(id),
                    typed.symbol_type(id).and_then(|ty| typed.types().get(ty)),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(implicit.len(), 7);
        assert_eq!(
            implicit
                .iter()
                .map(|(_, span, _, _)| *span)
                .collect::<Vec<_>>(),
            headerless_openers(&sources, &parsed_file),
        );
        assert_eq!(
            implicit
                .iter()
                .map(|(_, _, mode, _)| *mode)
                .collect::<Vec<_>>(),
            [
                Some(ParameterMode::Borrow),
                Some(ParameterMode::Value),
                Some(ParameterMode::Inout),
                Some(ParameterMode::Borrow),
                Some(ParameterMode::Borrow),
                Some(ParameterMode::Borrow),
                Some(ParameterMode::Value),
            ]
        );
        assert!(implicit.iter().all(|(name, _, _, ty)| {
            *name == "it" && matches!(ty, Some(UnitTypeKind::Builtin(BuiltinType::Int)))
        }));
    }

    let invalid = "fun invalid(): Unit {\n\
                       val inferred = { it }\n\
                       val zero: () -> Int = { it }\n\
                       val pair: (Int, Int) -> Int = { 1 }\n\
                   }\n\
                   fun outer(it: Int): () -> Int = { -> it }";
    let (sources, parsed_file) = parsed(invalid);
    for typed in checked_entries(&sources, &parsed_file, &names, &types) {
        assert_eq!(codes(typed.diagnostics()), ["L0083", "L0084", "L0084"]);
        assert!(typed.body_parameter_modes().is_empty());
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.primary_span())
                .collect::<Vec<_>>(),
            headerless_openers(&sources, &parsed_file),
        );
    }
}
