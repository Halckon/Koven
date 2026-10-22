//! Ordinary source-qualified p/q chain; no test-side declaration or MapResult pre-registration.

use lang_frontend::{
    name_resolution::{
        SourceUnitId, SourceUnitInput, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, NonNullAssertionTransferKind, check_compilation_unit_ownership,
    },
    type_checking::{
        BuiltinType, CompilationUnitTypes, Copyability, IntrinsicTypeConstructor, NominalKind,
        ParameterMode, UnitCallTarget, UnitExpressionId, UnitTypeId, UnitTypeKind,
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use super::*;

pub(super) fn observe(case: &Case, trace: &mut Trace) -> Outcome<LowerObservation> {
    observe_sources(
        case.provider,
        case.consumer,
        |sources, file, names, typed, trace| inspect(case, sources, file, names, typed, trace),
        |_, _, _, _, _, _| {},
        case.counts[5],
        trace,
    )
}

pub(super) fn observe_sources(
    provider_text: &str,
    consumer_text: &str,
    inspect: impl FnOnce(
        &SourceMap,
        (SourceUnitId, &ParsedFile),
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &mut Trace,
    ),
    inspect_owned: impl FnOnce(
        &SourceMap,
        (SourceUnitId, &ParsedFile),
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
        &mut Trace,
    ),
    assertion_count: usize,
    trace: &mut Trace,
) -> Outcome<LowerObservation> {
    let mut sources = SourceMap::new();
    let provider = parsed(&mut sources, "p/provider.ko", provider_text, trace);
    let consumer = parsed(&mut sources, "q/consumer.ko", consumer_text, trace);
    let (provider, consumer) = (provider?, consumer?);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider.source_id(), &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer.source_id(), &consumer),
    ];
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
    let provider_units: Vec<_> = index
        .source_units()
        .iter()
        .filter(|unit| {
            unit.source_id() == provider.source_id()
                && unit.key().logical_path().expect("rooted source").as_str() == "p/provider.ko"
        })
        .collect();
    let consumer_units: Vec<_> = index
        .source_units()
        .iter()
        .filter(|unit| {
            unit.source_id() == consumer.source_id()
                && unit.key().logical_path().expect("rooted source").as_str() == "q/consumer.ko"
        })
        .collect();
    let ([provider_unit], [consumer_unit]) = (provider_units.as_slice(), consumer_units.as_slice())
    else {
        return Err(Failure {
            stage: "unique real provider and consumer".to_owned(),
            detail: format!("provider={provider_units:#?}; consumer={consumer_units:#?}"),
        });
    };
    gate(
        "two distinct source-qualified packages",
        index.source_units().len() == 2
            && provider_unit.id() != consumer_unit.id()
            && index.packages().iter().any(|package| {
                package.id() == provider_unit.package()
                    && package.name().expect("named package").segments() == ["p"]
            })
            && index.packages().iter().any(|package| {
                package.id() == consumer_unit.package()
                    && package.name().expect("named package").segments() == ["q"]
            }),
        (index.source_units(), index.packages()),
    )?;
    let entries: Vec<_> = index
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "entry"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == consumer_unit.id()
                && declaration.package() == consumer_unit.package()
                && declaration.name_span().source_id() == consumer.source_id()
        })
        .collect();
    let [entry] = entries.as_slice() else {
        return Err(Failure {
            stage: "unique real q.entry".to_owned(),
            detail: format!(
                "candidates={entries:#?}; declarations={:#?}",
                index.declarations()
            ),
        });
    };
    trace.stages.push(format!(
        "selected real q.entry={entry:#?}; sources={:#?}",
        index.source_units()
    ));
    let typed = step(
        "raw types",
        check_compilation_unit_types(&sources, &inputs, &names, &types),
        trace,
    )?;
    let raw = diagnostics("raw type diagnostics", &sources, typed.diagnostics(), trace);
    let mains: Vec<_> = index
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "main"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == provider_unit.id()
                && declaration.package() == provider_unit.package()
                && declaration.name_span().source_id() == provider.source_id()
        })
        .collect();
    let consumer_calls: Vec<_> = typed
        .calls()
        .iter()
        .filter(|call| call.expression().source_unit() == consumer_unit.id())
        .collect();
    trace.check(
        "real q.entry consumer call targets actual p.main",
        matches!((mains.as_slice(), consumer_calls.as_slice()), ([main], [call])
            if call.target() == UnitCallTarget::Declaration(main.id())
                && Some(call.return_type()) == typed.types().builtin(BuiltinType::Unit)
                && call.receiver().is_none() && call.arguments().is_empty()),
        (mains, consumer_calls),
    );
    inspect(
        &sources,
        (provider_unit.id(), &provider),
        &names,
        &typed,
        trace,
    );
    let complete = step(
        "complete types",
        complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
            .map_err(|error| admission_error(&sources, error)),
        trace,
    )?;
    raw?;
    trace.check(
        "same complete analysis and original facts",
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
    let clean = diagnostics(
        "ownership diagnostics",
        &sources,
        owned.diagnostics(),
        trace,
    );
    trace.stages.push(format!(
        "ownership deferred={:#?}; drops={:#?}; assertion plans={:#?}",
        owned.deferred(),
        owned.drops(),
        owned.non_null_assertions()
    ));
    let complete_ownership = gate(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
    );
    trace.check(
        "real qualified assertion ownership plans",
        owned.non_null_assertions().len() == assertion_count
            && owned.non_null_assertions().iter().all(|plan| {
                typed
                    .types()
                    .non_null_assertions()
                    .contains(plan.descriptor())
                    && plan.non_null_transfer() == NonNullAssertionTransferKind::Consume
            }),
        owned.non_null_assertions(),
    );
    inspect_owned(
        &sources,
        (provider_unit.id(), &provider),
        &names,
        typed.types(),
        &owned,
        trace,
    );
    clean?;
    complete_ownership?;
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
    trace
        .stages
        .push("attempt lower SSA with real q.entry".to_owned());
    Ok(LowerObservation {
        entry: selected_entry(&sources, entry.name(), entry.name_span()),
        outcome: finish_lower(
            &sources,
            crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                &sources,
                &inputs,
                &names,
                &types,
                &typed,
                &owned,
                entry.id(),
            ),
            trace,
        ),
    })
}

fn named(
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
    let values: Vec<_> = source
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.name() == name
                && symbol.kind() == SymbolKind::Variable
                && !symbol.is_synthetic()
                && symbol.span().source_id() == file.1.source_id()
        })
        .collect();
    match values.as_slice() {
        [symbol] => typed
            .body_symbol_types()
            .keys()
            .copied()
            .find(|root| root.source_unit() == file.0 && root.symbol() == symbol.id()),
        _ => None,
    }
}

fn receiver(
    names: &ValidatedCompilationUnitNames,
    file: (SourceUnitId, &ParsedFile),
    expression: UnitExpressionId,
) -> Option<UnitSymbolId> {
    if expression.source_unit() != file.0 {
        return None;
    }
    let leaf = super::super::ungroup(file.1, expression.expression())?;
    let span = file.1.ast().expressions().get(leaf).ok()?.span();
    names.names().references().iter().find_map(|reference| {
        if reference.source_unit() != file.0 || reference.span() != span {
            return None;
        }
        match reference.target() {
            UnitReferenceTarget::Symbol(root) => Some(*root),
            _ => None,
        }
    })
}

fn map_type(
    typed: &CompilationUnitTypes,
    key: Option<UnitTypeId>,
    value: Option<UnitTypeId>,
) -> Option<UnitTypeId> {
    typed.types().find(&UnitTypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::MutableMap,
        arguments: vec![key?, value?],
    })
}

fn inspect(
    case: &Case,
    sources: &SourceMap,
    file: (SourceUnitId, &ParsedFile),
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    trace: &mut Trace,
) {
    let declarations: Vec<_> = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "Resource"
                && declaration.kind() == SymbolKind::Classifier
                && declaration.source_unit() == file.0
                && declaration.name_span().source_id() == file.1.source_id()
        })
        .collect();
    let nominal = match declarations.as_slice() {
        [declaration] => typed
            .signatures()
            .declaration(declaration.id())
            .and_then(|value| value.nominal()),
        _ => None,
    };
    let resource = nominal.map(|nominal| nominal.ty());
    trace.check(
        "actual source-qualified Resource nominal and deinit owner/receiver",
        nominal.is_some_and(|nominal| {
            nominal.kind() == NominalKind::Class
                && nominal.has_deinit()
                && nominal.symbol().source_unit() == file.0
                && nominal.deinit().is_some_and(|deinit| {
                    deinit.owner() == nominal.declaration()
                        && deinit.item().source_unit() == file.0
                        && deinit.body().source_unit() == file.0
                        && Some(deinit.receiver_type()) == resource
                        && deinit.receiver_mode() == ParameterMode::Borrow
                })
        }),
        nominal,
    );
    let int = typed.types().builtin(BuiltinType::Int);
    let inner = map_type(typed, int, resource);
    let outer = map_type(typed, int, inner);
    let nullable = inner.and_then(|inner| typed.types().find(&UnitTypeKind::Nullable(inner)));
    trace.check(
        "precise Resource inner and Nullable Map classifications",
        [resource, inner, nullable]
            .into_iter()
            .all(|ty| ty.is_some_and(|ty| typed.is_resource_type(ty) == Some(true))),
        [resource, inner, nullable].map(|ty| (ty, ty.and_then(|ty| typed.is_resource_type(ty)))),
    );
    let root = |name| named(names, typed, file, name);
    let storage = |name| root(name).and_then(|root| typed.symbol_type(root));
    trace.check(
        "ordinary nullable declaration presence and exact storage",
        if case.has_ordinary {
            root("ordinary").is_some() && storage("ordinary") == nullable
        } else {
            root("ordinary").is_none()
        },
        (root("ordinary"), storage("ordinary"), nullable),
    );
    let counts = [
        typed.map_constructions().len(),
        typed.map_puts().len(),
        typed.map_removes().len(),
        typed.map_sizes().len(),
        typed.map_consumes().len(),
        typed.non_null_assertions().len(),
    ];
    trace.check(
        "resource-specific descriptor counts",
        counts == case.counts,
        (case.counts, counts),
    );
    trace.check(
        "all resource descriptors belong to the actual provider",
        typed
            .map_constructions()
            .iter()
            .all(|value| value.expression().source_unit() == file.0)
            && typed
                .map_puts()
                .iter()
                .all(|value| value.expression().source_unit() == file.0)
            && typed
                .map_removes()
                .iter()
                .all(|value| value.expression().source_unit() == file.0)
            && typed
                .map_sizes()
                .iter()
                .all(|value| value.expression().source_unit() == file.0)
            && typed
                .non_null_assertions()
                .iter()
                .all(|value| value.expression().source_unit() == file.0),
        file.0,
    );
    trace.stages.push(format!(
        "actual types={:#?}; constructions={:#?}; puts={:#?}; removes={:#?}; sizes={:#?}; assertions={:#?}",
        typed.types(), typed.map_constructions(), typed.map_puts(), typed.map_removes(),
        typed.map_sizes(), typed.non_null_assertions(),
    ));
    if case.counts[2] == 0 {
        return;
    }
    trace.check(
        "actual inner/outer/got roots and nested Map storage",
        root("inner").is_some()
            && storage("inner") == inner
            && root("outer").is_some()
            && storage("outer") == outer
            && root("got").is_some()
            && storage("got") == inner
            && outer.is_some_and(|ty| typed.is_resource_type(ty) == Some(true)),
        (
            root("inner"),
            storage("inner"),
            root("outer"),
            storage("outer"),
            root("got"),
            storage("got"),
        ),
    );
    trace.check(
        "actual inner and outer construction identities",
        typed
            .map_constructions()
            .iter()
            .filter(|value| Some(value.map_type()) == inner)
            .count()
            == 1
            && typed
                .map_constructions()
                .iter()
                .filter(|value| Some(value.map_type()) == outer)
                .count()
                == 1
            && typed.map_constructions().iter().all(|value| {
                Some(value.key_type()) == int
                    && typed.expression_type(value.expression()) == Some(value.map_type())
                    && ((Some(value.map_type()) == inner && Some(value.value_type()) == resource)
                        || (Some(value.map_type()) == outer && Some(value.value_type()) == inner))
            }),
        typed.map_constructions(),
    );
    let inner_puts: Vec<_> = typed
        .map_puts()
        .iter()
        .filter(|put| receiver(names, file, put.receiver()) == root("inner"))
        .collect();
    let outer_puts: Vec<_> = typed
        .map_puts()
        .iter()
        .filter(|put| receiver(names, file, put.receiver()) == root("outer"))
        .collect();
    trace.check(
        "real inner Resource put and outer inner-Map put",
        inner_puts.len() == 1
            && outer_puts.len() == 1
            && inner_puts.iter().all(|put| {
                Some(put.key_type()) == int
                    && Some(put.value_type()) == resource
                    && typed.expression_type(put.value()) == resource
            })
            && outer_puts.iter().all(|put| {
                Some(put.key_type()) == int
                    && Some(put.value_type()) == inner
                    && typed.expression_type(put.value()) == inner
                    && receiver(names, file, put.value()) == root("inner")
            }),
        (inner_puts, outer_puts),
    );
    for remove in typed.map_removes() {
        trace.check(
            "real outer remove payload/result and assertion operand",
            receiver(names, file, remove.receiver()) == root("outer")
                && Some(remove.key_type()) == int
                && Some(remove.value_type()) == inner
                && Some(remove.result_type()) == nullable
                && typed.expression_type(remove.expression()) == nullable
                && matches!(typed.non_null_assertions(), [assertion]
                    if assertion.operand() == remove.expression()
                        && Some(assertion.nullable_type()) == nullable
                        && Some(assertion.inner_type()) == inner
                        && assertion.copyability() == Copyability::MoveOnly),
            remove,
        );
        trace.stages.push(format!(
            "actual remove source={:?}",
            file.1
                .ast()
                .expressions()
                .get(remove.expression().expression())
                .map(|node| origin(sources, node.span()))
        ));
    }
    trace.check(
        "real got.size read",
        typed.map_sizes().iter().all(|size| {
            receiver(names, file, size.receiver()) == root("got")
                && Some(size.map_type()) == inner
                && typed.expression_type(size.receiver()) == inner
        }),
        typed.map_sizes(),
    );
}
