//! Independent ordinary single-file chain and read-only frontend input checks.

use lang_frontend::{
    name_resolution::{NameResolution, ReferenceTarget, SymbolId, SymbolKind, resolve_names},
    ownership_checking::check_ownership,
    type_checking::{
        IntrinsicTypeConstructor, TypeId, TypeKind, TypedFile, check_types, complete_file_types,
        standard_environments,
    },
};

use super::*;

pub(super) fn observe(case: &Case, trace: &mut Trace) -> Outcome<()> {
    let mut sources = SourceMap::new();
    let parsed = parsed(&mut sources, case.path, case.single, trace)?;
    let (environment, types) = standard_environments();
    let names = step(
        "names",
        resolve_names(&sources, &parsed, &environment),
        trace,
    )?;
    diagnostics("names diagnostics", &sources, names.diagnostics(), trace)?;
    let entries: Vec<_> = names
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.name() == "main"
                && symbol.kind() == SymbolKind::Function
                && !symbol.is_synthetic()
                && symbol.span().source_id() == parsed.source_id()
        })
        .collect();
    let [entry] = entries.as_slice() else {
        return Err(Failure {
            stage: "unique real single main".to_owned(),
            detail: format!("candidates={entries:#?}; symbols={:#?}", names.symbols()),
        });
    };
    trace.stages.push(format!(
        "selected real main={entry:#?}; {}",
        origin(&sources, entry.span())
    ));
    let typed = step(
        "raw types",
        check_types(&sources, &parsed, &names, &types),
        trace,
    )?;
    // A raw diagnostic must not hide complete-admission diagnostics. No ownership on failure.
    let raw = diagnostics("raw type diagnostics", &sources, typed.diagnostics(), trace);
    inspect_inputs(case, &sources, &parsed, &names, &typed, trace);
    let complete = step(
        "complete types",
        complete_file_types(&sources, &parsed, &names, &types, &typed)
            .map_err(|error| admission_error(&sources, error)),
        trace,
    )?;
    raw?;
    trace.check(
        "complete retains original analysis and every raw fact",
        complete.is_same_analysis(&typed) && std::ptr::eq(complete.types(), &typed),
        (
            complete.is_same_analysis(&typed),
            std::ptr::eq(complete.types(), &typed),
        ),
    );
    diagnostics(
        "complete diagnostics",
        &sources,
        complete.types().diagnostics(),
        trace,
    )?;
    let owned = step(
        "ownership",
        check_ownership(&sources, &parsed, &names, complete.types()),
        trace,
    )?;
    let ownership_diagnostics = diagnostics(
        "ownership diagnostics",
        &sources,
        owned.diagnostics(),
        trace,
    );
    trace.stages.push(format!(
        "actual ownership deferred={:#?}; drops={:#?}",
        owned.deferred(),
        owned.drops()
    ));
    let completeness = gate(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    ownership_diagnostics?;
    completeness?;
    let (program, function) = step(
        "lower SSA with real main",
        crate::ssa::lower_scalar_file_with_entry(
            &sources,
            &parsed,
            &names,
            complete.types(),
            &owned,
            entry.id(),
        )
        .map_err(|error| lowering_error(&sources, error)),
        trace,
    )?;
    verify_and_render(&program, function, trace)
}

fn map_shape(
    typed: &TypedFile,
    ty: TypeId,
    constructor: IntrinsicTypeConstructor,
    pair: [TypeId; 2],
) -> bool {
    matches!(typed.types().get(ty), Some(TypeKind::Intrinsic { constructor: actual, arguments })
        if *actual == constructor && arguments.as_slice() == pair)
}

fn named_root(names: &NameResolution, parsed: &ParsedFile, name: &str) -> Option<SymbolId> {
    let matches: Vec<_> = names
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.name() == name
                && !symbol.is_synthetic()
                && symbol.span().source_id() == parsed.source_id()
                && symbol.kind()
                    == if name == "input" {
                        SymbolKind::ValueParameter
                    } else {
                        SymbolKind::Variable
                    }
        })
        .collect();
    match matches.as_slice() {
        [symbol] => Some(symbol.id()),
        _ => None,
    }
}

fn receiver_root(
    parsed: &ParsedFile,
    names: &NameResolution,
    receiver: ExpressionId,
) -> Option<SymbolId> {
    let leaf = ungroup(parsed, receiver)?;
    let span = parsed.ast().expressions().get(leaf).ok()?.span();
    names.references().iter().find_map(|reference| {
        if reference.span() != span {
            return None;
        }
        match reference.target() {
            ReferenceTarget::Symbol(root) => Some(*root),
            _ => None,
        }
    })
}

fn inspect_inputs(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    trace: &mut Trace,
) {
    let comparisons: Vec<_> = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(expression, _)| typed.null_comparison(expression))
        .collect();
    trace.stages.push(format!(
        concat!(
            "raw type table (including any Deferred)={:#?}; constructions={:#?}; puts={:#?}; ",
            "removes={:#?}; sizes={:#?}; consumes={:#?}; non-null uses={:#?}; comparisons={:#?}",
        ),
        typed.types(),
        typed.map_constructions(),
        typed.map_puts(),
        typed.map_removes(),
        typed.map_sizes(),
        typed.map_consumes(),
        typed.non_null_uses(),
        comparisons,
    ));
    let counts = [
        typed.map_consumes().len(),
        typed.map_removes().len(),
        typed.map_sizes().len(),
    ];
    trace.check(
        "case-specific consume/remove/size counts",
        counts == case.counts,
        (case.counts, counts),
    );
    let scalar = typed.types().builtin(case.scalar);
    let mut storage_ids = Vec::new();
    for &(name, _) in case.roots {
        let root = named_root(names, parsed, name);
        let storage = root.and_then(|root| typed.symbol_type(root));
        let inner = storage.and_then(|storage| match typed.types().get(storage) {
            Some(TypeKind::Nullable(inner)) => Some(*inner),
            _ => None,
        });
        trace.check(
            "real named nullable Map storage",
            root.is_some()
                && inner.zip(scalar).is_some_and(|(inner, scalar)| {
                    map_shape(
                        typed,
                        inner,
                        IntrinsicTypeConstructor::MutableMap,
                        [scalar, scalar],
                    )
                }),
            (name, root, storage, inner, scalar),
        );
        if let Some(storage) = storage {
            storage_ids.push(storage);
        }
    }
    trace.check(
        "same precise frontend nullable storage across declared sources",
        storage_ids.len() == case.roots.len()
            && storage_ids.windows(2).all(|pair| pair[0] == pair[1]),
        &storage_ids,
    );
    let source_root = named_root(names, parsed, "source");
    trace.check(
        "real null comparison root/storage and edge",
        match case.non_null_when_true {
            None => comparisons.is_empty(),
            Some(edge) => matches!(comparisons.as_slice(), [comparison]
                if Some(comparison.symbol()) == source_root
                    && typed.symbol_type(comparison.symbol()) == Some(comparison.nullable_type())
                    && comparison.non_null_when_true() == edge),
        },
        &comparisons,
    );
    for comparison in &comparisons {
        trace.stages.push(format!(
            "actual comparison source={:?}",
            parsed
                .ast()
                .expressions()
                .get(comparison.expression())
                .map(|node| origin(sources, node.span()))
        ));
    }
    for descriptor in typed.map_sizes() {
        let root = receiver_root(parsed, names, descriptor.receiver());
        let leaf = ungroup(parsed, descriptor.receiver());
        let proof = leaf.and_then(|leaf| typed.non_null_use(leaf));
        let is_view_case = case.id.starts_with('B');
        trace.check(
            "real size receiver/read type and nullable view input",
            typed.expression_type(descriptor.receiver()) == Some(descriptor.map_type())
                && (!is_view_case
                    || (root == source_root
                        && proof.is_some_and(|proof| {
                            Some(proof.symbol()) == root
                                && typed.symbol_type(proof.symbol()) == Some(proof.declared_type())
                                && proof.narrowed_type() == descriptor.map_type()
                                && typed.types().get(proof.declared_type())
                                    == Some(&TypeKind::Nullable(proof.narrowed_type()))
                        }))),
            (descriptor, root, leaf, proof),
        );
        trace.stages.push(format!(
            "actual size source={:?}",
            parsed
                .ast()
                .expressions()
                .get(descriptor.expression())
                .map(|node| origin(sources, node.span()))
        ));
    }
    for descriptor in typed.map_consumes() {
        let root = receiver_root(parsed, names, descriptor.receiver());
        let leaf = ungroup(parsed, descriptor.receiver());
        let proof = leaf.and_then(|leaf| typed.non_null_use(leaf));
        let pair = [descriptor.key_type(), descriptor.value_type()];
        trace.check(
            "real consume source/root/storage/read/result/KV",
            descriptor.source().is_some_and(|(source, storage)| {
                Some(source) == source_root
                    && Some(source) == root
                    && typed.symbol_type(source) == Some(storage)
                    && typed.types().get(storage)
                        == Some(&TypeKind::Nullable(descriptor.receiver_type()))
                    && proof.is_some_and(|proof| {
                        proof.symbol() == source
                            && proof.declared_type() == storage
                            && proof.narrowed_type() == descriptor.receiver_type()
                    })
            }) && typed.expression_type(descriptor.receiver()) == Some(descriptor.receiver_type())
                && typed.expression_type(descriptor.expression()) == Some(descriptor.result_type())
                && scalar.is_some_and(|scalar| pair == [scalar, scalar])
                && map_shape(
                    typed,
                    descriptor.receiver_type(),
                    IntrinsicTypeConstructor::MutableMap,
                    pair,
                )
                && map_shape(
                    typed,
                    descriptor.result_type(),
                    IntrinsicTypeConstructor::Map,
                    pair,
                ),
            (descriptor, root, leaf, proof),
        );
        trace.stages.push(format!(
            "actual consume call={:?}; receiver={:?}",
            parsed
                .ast()
                .expressions()
                .get(descriptor.expression())
                .map(|node| origin(sources, node.span())),
            parsed
                .ast()
                .expressions()
                .get(descriptor.receiver())
                .map(|node| origin(sources, node.span())),
        ));
    }
    for descriptor in typed.map_removes() {
        let outer = named_root(names, parsed, "outer");
        let outer_type = outer.and_then(|root| typed.symbol_type(root));
        let storage = storage_ids.first().copied();
        trace.check(
            "real nested Map put/remove and canonical frontend storage input",
            receiver_root(parsed, names, descriptor.receiver()) == outer
                && outer.is_some()
                && scalar == Some(descriptor.key_type())
                && outer_type.is_some_and(|ty| {
                    map_shape(
                        typed,
                        ty,
                        IntrinsicTypeConstructor::MutableMap,
                        [descriptor.key_type(), descriptor.value_type()],
                    )
                })
                && storage == Some(descriptor.result_type())
                && typed.types().get(descriptor.result_type())
                    == Some(&TypeKind::Nullable(descriptor.value_type()))
                && typed.expression_type(descriptor.expression()) == Some(descriptor.result_type())
                && typed.map_puts().len() == 1
                && typed.map_puts().iter().all(|put| {
                    receiver_root(parsed, names, put.receiver()) == outer
                        && put.key_type() == descriptor.key_type()
                        && put.value_type() == descriptor.value_type()
                        && typed.expression_type(put.value()) == Some(descriptor.value_type())
                }),
            (descriptor, outer, outer_type, storage, typed.map_puts()),
        );
        trace.stages.push(format!(
            "actual remove source={:?}",
            parsed
                .ast()
                .expressions()
                .get(descriptor.expression())
                .map(|node| origin(sources, node.span()))
        ));
    }
}
