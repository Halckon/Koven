//! Same-round strict Unit pipeline for the original memory and p/q input layouts.

use lang_frontend::{
    name_resolution::{
        SourceUnitId, SourceUnitInput, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget, check_compilation_unit_ownership,
        owned_compilation_unit_view,
    },
    parser::{Item, NameMarker},
    type_checking::{
        CompilationUnitTypes, IntrinsicTypeConstructor, UnitCallReceiverOrigin, UnitExpressionId,
        UnitTypeId, UnitTypeKind, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

use super::*;

mod layout;
pub(super) use layout::Layout;

#[derive(Clone, Copy)]
pub(super) struct Context<'a> {
    pub case: &'a Case,
    pub layout: Layout,
    pub sources: &'a SourceMap,
}

pub(super) fn observe(case: &Case, trace: &mut Trace) -> Outcome<()> {
    observe_impl(case, trace, Layout::Rooted, false, false)
}

pub(super) fn observe_read_identity(case: &Case, trace: &mut Trace) -> Outcome<()> {
    observe_impl(case, trace, Layout::Rooted, true, false)
}

pub(super) fn observe_consume_identity(case: &Case, trace: &mut Trace) -> Outcome<()> {
    observe_impl(case, trace, Layout::Rooted, false, true)
}

pub(super) fn observe_impl(
    case: &Case,
    trace: &mut Trace,
    layout: Layout,
    identity: bool,
    consume_identity: bool,
) -> Outcome<()> {
    let mut sources = SourceMap::new();
    let parsed_files = match layout {
        Layout::Memory => vec![parsed(&mut sources, case.path, case.single, trace)?],
        Layout::Rooted => {
            // Collect both independent parse outcomes before propagating either failure.
            let provider = parsed(&mut sources, "p/provider.ko", case.provider, trace);
            let consumer = parsed(&mut sources, "q/consumer.ko", case.consumer, trace);
            vec![provider?, consumer?]
        }
    };
    let provider = &parsed_files[0];
    let consumer = &parsed_files[match layout {
        Layout::Memory => 0,
        Layout::Rooted => 1,
    }];
    let inputs = match layout {
        Layout::Memory => vec![SourceUnitInput::memory_document(
            provider.source_id(),
            provider,
        )],
        Layout::Rooted => vec![
            SourceUnitInput::new("root", "p/provider.ko", provider.source_id(), provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer.source_id(), consumer),
        ],
    };
    let (environment, types) = standard_environments();
    let index = step("index", index_compilation_unit(&sources, &inputs), trace)?;
    let names = step(
        "names",
        resolve_compilation_unit_names(&sources, &inputs, &index, &environment),
        trace,
    )?;
    diagnostics("names diagnostics", &sources, names.diagnostics(), trace)?;
    let names = step(
        "qualified names",
        names
            .validate()
            .map_err(|value| diagnostic_text(&sources, value.diagnostics())),
        trace,
    )?;
    let index = names.names().index();
    let actual_sources = index.source_units();
    let provider_units: Vec<_> = actual_sources
        .iter()
        .filter(|unit| {
            unit.source_id() == provider.source_id()
                && layout.provider_matches(case, &sources, &names, unit)
        })
        .collect();
    let consumer_units: Vec<_> = actual_sources
        .iter()
        .filter(|unit| {
            unit.source_id() == consumer.source_id()
                && layout.consumer_matches(case, &sources, &names, unit)
        })
        .collect();
    let ([provider_unit], [consumer_unit]) = (provider_units.as_slice(), consumer_units.as_slice())
    else {
        return Err(Failure {
            stage: "unique real provider and consumer source units".to_owned(),
            detail: format!(
                "provider={provider_units:#?}; consumer={consumer_units:#?}; all={actual_sources:#?}"
            ),
        });
    };
    gate(
        "exact original source-unit layout and packages",
        match layout {
            Layout::Memory => {
                actual_sources.len() == 1
                    && inputs.len() == 1
                    && provider_unit.id() == consumer_unit.id()
                    && provider.package().is_none()
            }
            Layout::Rooted => {
                actual_sources.len() == 2
                    && inputs.len() == 2
                    && provider_unit.id() != consumer_unit.id()
                    && provider_unit.package() != consumer_unit.package()
            }
        },
        (layout, actual_sources, index.packages()),
    )?;
    let entries: Vec<_> = index
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == layout.entry_name()
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == consumer_unit.id()
                && declaration.package() == consumer_unit.package()
                && declaration.name_span().source_id() == consumer.source_id()
                && consumer.roots().contains(&declaration.root())
                && consumer.ast().items().get(declaration.root()).is_ok_and(|node| {
                    matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                        if *name == declaration.name_span())
                })
        })
        .collect();
    let [entry] = entries.as_slice() else {
        return Err(Failure {
            stage: format!("unique real {} entry", layout.entry_name()),
            detail: format!(
                "candidates={entries:#?}; declarations={:#?}",
                index.declarations()
            ),
        });
    };
    trace.stages.push(format!(
        "selected real {layout:?} entry={entry:#?}; sources={actual_sources:#?}"
    ));
    let typed = step(
        "raw types",
        check_compilation_unit_types(&sources, &inputs, &names, &types),
        trace,
    )?;
    let raw = diagnostics("raw type diagnostics", &sources, typed.diagnostics(), trace);
    let roles = [
        (provider_unit.id(), provider),
        (consumer_unit.id(), consumer),
    ];
    let files = match layout {
        Layout::Memory => vec![roles[0]],
        Layout::Rooted => roles.to_vec(),
    };
    inspect_inputs(case, &sources, &roles, &names, &typed, trace);
    let complete = step(
        "complete types",
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .map_err(|error| admission_error(&sources, error)),
        trace,
    )?;
    raw?;
    let same = complete.is_same_analysis(&typed)
        && std::ptr::eq(complete.types(), &typed)
        && typed.is_compatible_with(&sources, &inputs, &names, &types);
    trace.check(
        "complete retains original analysis and every raw fact",
        same,
        (
            complete.is_same_analysis(&typed),
            std::ptr::eq(complete.types(), &typed),
        ),
    );
    gate("complete original analysis identity", same, &typed)?;
    diagnostics(
        "complete diagnostics",
        &sources,
        complete.types().diagnostics(),
        trace,
    )?;
    // Validate the same raw product once; never re-run type checking or replace its fact table.
    let typed = step(
        "qualified types",
        typed
            .validate()
            .map_err(|value| diagnostic_text(&sources, value.diagnostics())),
        trace,
    )?;
    let owned = step(
        "ownership",
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed),
        trace,
    )?;
    let ownership_diagnostics = diagnostics(
        "ownership diagnostics",
        &sources,
        owned.diagnostics(),
        trace,
    );
    trace.stages.push(format!(
        "actual ownership deferred={:#?}; drops={:#?}; receiver facts={:#?}",
        owned.deferred(),
        owned.drops(),
        owned.receiver_facts(),
    ));
    let completeness = gate(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    for descriptor in typed.types().map_consumes() {
        let actual = owned.receiver_fact(descriptor.expression());
        trace.check(
            "real unit consume receiver Move fact",
            actual.is_some_and(|fact| {
                fact.call() == descriptor.expression()
                    && fact.source() == UnitCallReceiverOrigin::Expression(descriptor.receiver())
                    && fact.receiver_type() == descriptor.receiver_type()
                    && fact.kind() == UnitReceiverOwnershipKind::Move
                    && matches!(fact.target(), UnitReceiverOwnershipTarget::Place(place)
                        if place.is_root() && descriptor.source().is_some_and(|(root, _)| root == place.root()))
            }),
            (descriptor, actual),
        );
    }
    ownership_diagnostics?;
    completeness?;
    gate(
        "ownership original typed identity",
        owned.is_compatible_with(&typed),
        &owned,
    )?;
    let owned = step(
        "qualified ownership",
        owned.validate().map_err(|value| {
            format!(
                "{}; deferred={:#?}",
                diagnostic_text(&sources, value.diagnostics()),
                value.deferred()
            )
        }),
        trace,
    )?;
    let view = step(
        "sealed owned unit",
        owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned),
        trace,
    )?;
    gate(
        "sealed view retains every original analysis reference",
        std::ptr::eq(view.sources(), &sources)
            && std::ptr::eq(view.inputs(), inputs.as_slice())
            && std::ptr::eq(view.names(), &names)
            && std::ptr::eq(view.types(), typed.types())
            && std::ptr::eq(view.ownership(), owned.ownership()),
        (layout, entry.id()),
    )?;
    let (program, function) = step(
        "lower SSA with original explicit Unit entry",
        crate::ssa::unit_lower::lower_owned_unit_with_entry(&view, entry.id())
            .map_err(|error| lowering_error(&sources, error)),
        trace,
    )?;
    let entry_span = consumer
        .ast()
        .items()
        .get(entry.root())
        .ok()
        .map(|node| node.span());
    let lowered_entries: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|candidate| {
            entry_span
                .is_some_and(|span| candidate.origin == crate::ssa::model::Origin::Source(span))
        })
        .collect();
    trace.check(
        "selected Unit declaration is actual lowered entry",
        matches!(lowered_entries.as_slice(), [actual] if actual.id == function),
        (entry.id(), entry_span, function, &lowered_entries),
    );
    let context = Context {
        case,
        layout,
        sources: &sources,
    };
    if consume_identity {
        let facts = consume_identity::unit_facts(
            &context,
            &files,
            &names,
            typed.types(),
            owned.ownership(),
            trace,
        );
        return consume_identity::finish(&program, function, facts, trace);
    }
    // Preserve the complete ordinary pipeline even when the independent query fails.
    let queried = if identity {
        read_identity::inspect_unit(
            &context,
            &files,
            &names,
            typed.types(),
            owned.ownership(),
            &program,
            trace,
        )
    } else {
        Ok(())
    };
    let rendered = verify_and_render(&program, function, trace);
    rendered?;
    queried
}

fn map_shape(
    typed: &CompilationUnitTypes,
    ty: UnitTypeId,
    constructor: IntrinsicTypeConstructor,
    pair: [UnitTypeId; 2],
) -> bool {
    matches!(typed.types().get(ty), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
        if *actual == constructor && arguments.as_slice() == pair)
}

fn named_root(
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    file: (SourceUnitId, &ParsedFile),
    name: &str,
) -> Option<UnitSymbolId> {
    let source = names
        .names()
        .source_units()
        .iter()
        .find(|source| source.source_unit() == file.0)?;
    let matches: Vec<_> = source
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.name() == name
                && !symbol.is_synthetic()
                && symbol.span().source_id() == file.1.source_id()
                && symbol.kind()
                    == if name == "input" {
                        SymbolKind::ValueParameter
                    } else {
                        SymbolKind::Variable
                    }
        })
        .collect();
    match matches.as_slice() {
        [symbol] => typed
            .body_symbol_types()
            .keys()
            .copied()
            .chain(names.names().references().iter().filter_map(
                |reference| match reference.target() {
                    UnitReferenceTarget::Symbol(root) => Some(*root),
                    _ => None,
                },
            ))
            .find(|root| root.source_unit() == file.0 && root.symbol() == symbol.id()),
        _ => None,
    }
}

fn receiver_root(
    files: &[(SourceUnitId, &ParsedFile)],
    names: &ValidatedCompilationUnitNames,
    receiver: UnitExpressionId,
) -> Option<UnitSymbolId> {
    let parsed = files
        .iter()
        .find(|file| file.0 == receiver.source_unit())?
        .1;
    let leaf = ungroup(parsed, receiver.expression())?;
    let span = parsed.ast().expressions().get(leaf).ok()?.span();
    names.names().references().iter().find_map(|reference| {
        if reference.source_unit() != receiver.source_unit() || reference.span() != span {
            return None;
        }
        match reference.target() {
            UnitReferenceTarget::Symbol(root) => Some(*root),
            UnitReferenceTarget::Declaration(declaration) => {
                names.names().declaration_symbol(*declaration)
            }
            _ => None,
        }
    })
}

fn leaf(
    files: &[(SourceUnitId, &ParsedFile)],
    receiver: UnitExpressionId,
) -> Option<UnitExpressionId> {
    let parsed = files
        .iter()
        .find(|file| file.0 == receiver.source_unit())?
        .1;
    ungroup(parsed, receiver.expression())
        .map(|expression| UnitExpressionId::new(receiver.source_unit(), expression))
}

fn expression_origin(
    sources: &SourceMap,
    files: &[(SourceUnitId, &ParsedFile)],
    expression: UnitExpressionId,
) -> Option<String> {
    let parsed = files
        .iter()
        .find(|file| file.0 == expression.source_unit())?
        .1;
    Some(origin(
        sources,
        parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .ok()?
            .span(),
    ))
}

fn inspect_inputs(
    case: &Case,
    sources: &SourceMap,
    files: &[(SourceUnitId, &ParsedFile); 2],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    trace: &mut Trace,
) {
    let comparisons = typed.null_comparisons();
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
    for &(name, provider) in case.roots {
        let file = files[usize::from(!provider)];
        let root = named_root(names, typed, file, name);
        let storage = root.and_then(|root| typed.symbol_type(root));
        let inner = storage.and_then(|storage| match typed.types().get(storage) {
            Some(UnitTypeKind::Nullable(inner)) => Some(*inner),
            _ => None,
        });
        trace.check(
            "real source-qualified nullable Map storage",
            root.is_some()
                && inner.zip(scalar).is_some_and(|(inner, scalar)| {
                    map_shape(
                        typed,
                        inner,
                        IntrinsicTypeConstructor::MutableMap,
                        [scalar, scalar],
                    )
                }),
            (file.0, name, root, storage, inner, scalar),
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
    let source_root = named_root(names, typed, files[0], "source");
    trace.check(
        "real null comparison source/root/storage and edge",
        match case.non_null_when_true {
            None => comparisons.is_empty(),
            Some(edge) => matches!(comparisons, [comparison]
                if comparison.expression().source_unit() == files[0].0
                    && Some(comparison.symbol()) == source_root
                    && typed.symbol_type(comparison.symbol()) == Some(comparison.nullable_type())
                    && comparison.non_null_when_true() == edge),
        },
        comparisons,
    );
    for comparison in comparisons {
        trace.stages.push(format!(
            "actual comparison source={:?}",
            expression_origin(sources, files, comparison.expression()),
        ));
    }
    for descriptor in typed.map_sizes() {
        let root = receiver_root(files, names, descriptor.receiver());
        let leaf = leaf(files, descriptor.receiver());
        let proof = leaf.and_then(|leaf| typed.non_null_use(leaf));
        let is_view_case = case.id.starts_with('B');
        trace.check(
            "real size source/receiver/read type and nullable view input",
            descriptor.expression().source_unit() == files[0].0
                && descriptor.receiver().source_unit() == files[0].0
                && typed.expression_type(descriptor.receiver()) == Some(descriptor.map_type())
                && (!is_view_case
                    || (root == source_root
                        && proof.is_some_and(|proof| {
                            Some(proof.symbol()) == root
                                && typed.symbol_type(proof.symbol()) == Some(proof.declared_type())
                                && proof.narrowed_type() == descriptor.map_type()
                                && typed.types().get(proof.declared_type())
                                    == Some(&UnitTypeKind::Nullable(proof.narrowed_type()))
                        }))),
            (descriptor, root, leaf, proof),
        );
        trace.stages.push(format!(
            "actual size source={:?}",
            expression_origin(sources, files, descriptor.expression()),
        ));
    }
    for descriptor in typed.map_consumes() {
        let root = receiver_root(files, names, descriptor.receiver());
        let leaf = leaf(files, descriptor.receiver());
        let proof = leaf.and_then(|leaf| typed.non_null_use(leaf));
        let pair = [descriptor.key_type(), descriptor.value_type()];
        trace.check(
            "real consume source/root/storage/read/result/KV",
            descriptor.expression().source_unit() == files[0].0
                && descriptor.receiver().source_unit() == files[0].0
                && descriptor.source().is_some_and(|(source, storage)| {
                    source.source_unit() == files[0].0
                        && Some(source) == source_root
                        && Some(source) == root
                        && typed.symbol_type(source) == Some(storage)
                        && typed.types().get(storage)
                            == Some(&UnitTypeKind::Nullable(descriptor.receiver_type()))
                        && proof.is_some_and(|proof| {
                            proof.symbol() == source
                                && proof.declared_type() == storage
                                && proof.narrowed_type() == descriptor.receiver_type()
                        })
                })
                && typed.expression_type(descriptor.receiver()) == Some(descriptor.receiver_type())
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
            expression_origin(sources, files, descriptor.expression()),
            expression_origin(sources, files, descriptor.receiver()),
        ));
    }
    for descriptor in typed.map_removes() {
        let outer = named_root(names, typed, files[0], "outer");
        let outer_type = outer.and_then(|root| typed.symbol_type(root));
        let storage = storage_ids.first().copied();
        trace.check(
            "real nested Map put/remove and canonical frontend storage input",
            descriptor.expression().source_unit() == files[0].0
                && descriptor.receiver().source_unit() == files[0].0
                && receiver_root(files, names, descriptor.receiver()) == outer
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
                    == Some(&UnitTypeKind::Nullable(descriptor.value_type()))
                && typed.expression_type(descriptor.expression()) == Some(descriptor.result_type())
                && typed.map_puts().len() == 1
                && typed.map_puts().iter().all(|put| {
                    put.expression().source_unit() == files[0].0
                        && receiver_root(files, names, put.receiver()) == outer
                        && put.key_type() == descriptor.key_type()
                        && put.value_type() == descriptor.value_type()
                        && typed.expression_type(put.value()) == Some(descriptor.value_type())
                }),
            (descriptor, outer, outer_type, storage, typed.map_puts()),
        );
        trace.stages.push(format!(
            "actual remove source={:?}",
            expression_origin(sources, files, descriptor.expression()),
        ));
    }
}
