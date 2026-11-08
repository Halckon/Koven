use super::*;
use lang_frontend::{
    name_resolution::{SourceUnitId, SymbolId, UnitSymbolId},
    source::Span,
    type_checking::{CompilationUnitTypes, UnitExpressionId},
};

use super::unit_entries::checked_entries_with_names;

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
fn expected_lambdas_adopt_and_publish_all_parameter_modes() {
    let text = "fun inspect(borrow input: Int): Unit {}\n\
                fun mutate(inout input: Int): Unit {}\n\
                fun consume(own input: Int): Unit {}\n\
                fun applyBorrow(callback: (borrow Int) -> Unit): Unit {}\n\
                fun applyOwn(callback: (own Int) -> Unit): Unit {}\n\
                fun applyInout(callback: (inout Int) -> Unit): Unit {}\n\
                fun use(): Unit {\n\
                    val reader: (borrow Int) -> Unit = { item -> inspect(item) }\n\
                    val writer: (inout Int) -> Unit = { item -> mutate(&item) }\n\
                    val owner: (own Int) -> Unit = { item -> }\n\
                    val moved: move (borrow Int) -> Unit = move { item -> inspect(item) }\n\
                    val appliedBorrow = applyBorrow({ item -> })\n\
                    val appliedOwn = applyOwn({ item -> })\n\
                    val appliedInout = applyInout({ item -> })\n\
                }";
    let (sources, parsed) = parsed(text);
    let (names, types) = environments();
    for (names, typed) in checked_entries_with_names(&sources, &parsed, &names, &types) {
        let resolved_source = &names.names().source_units()[0];
        let source = resolved_source.source_unit();
        let resolution = resolved_source.resolution();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());

        let named_modes = resolution
            .symbols()
            .iter()
            .filter(|symbol| symbol.kind() == SymbolKind::ValueParameter)
            .map(|symbol| {
                (
                    symbol.name(),
                    typed
                        .signatures()
                        .declarations()
                        .iter()
                        .filter_map(|decl| decl.callable())
                        .flat_map(|call| call.parameters())
                        .find(|param| {
                            param.symbol().is_some_and(|id| {
                                id.source_unit() == source && id.symbol() == symbol.id()
                            })
                        })
                        .map(|param| param.mode()),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            named_modes,
            [
                ("input", Some(ParameterMode::Borrow)),
                ("input", Some(ParameterMode::Inout)),
                ("input", Some(ParameterMode::Value)),
                ("callback", Some(ParameterMode::Borrow)),
                ("callback", Some(ParameterMode::Borrow)),
                ("callback", Some(ParameterMode::Borrow)),
            ]
        );

        let lambda_modes = resolution
            .symbols()
            .iter()
            .filter(|symbol| symbol.kind() == SymbolKind::LambdaParameter)
            .map(|symbol| typed.body_parameter_mode(body_symbol(&typed, source, symbol.id())))
            .collect::<Vec<_>>();
        assert_eq!(
            lambda_modes,
            [
                Some(ParameterMode::Borrow),
                Some(ParameterMode::Inout),
                Some(ParameterMode::Value),
                Some(ParameterMode::Borrow),
                Some(ParameterMode::Borrow),
                Some(ParameterMode::Value),
                Some(ParameterMode::Inout),
            ]
        );

        let lambda_type_modes = parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, node)| {
                matches!(node.payload(), Expression::Lambda { .. }).then_some(id)
            })
            .map(|id| {
                match typed
                    .expression_type(UnitExpressionId::new(source, id))
                    .and_then(|ty| typed.types().get(ty))
                {
                    Some(UnitTypeKind::Function { parameters, .. }) => parameters[0].mode(),
                    other => panic!("expected typed lambda function, got {other:?}"),
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            lambda_type_modes,
            [
                ParameterMode::Borrow,
                ParameterMode::Inout,
                ParameterMode::Value,
                ParameterMode::Borrow,
                ParameterMode::Borrow,
                ParameterMode::Value,
                ParameterMode::Inout,
            ]
        );
    }
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
    for (names, typed) in checked_entries_with_names(&sources, &parsed_file, &names, &types) {
        let resolved_source = &names.names().source_units()[0];
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
