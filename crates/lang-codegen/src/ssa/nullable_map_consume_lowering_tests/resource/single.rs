//! Ordinary single-file analysis with a Resource-specific read-only input oracle.

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, ReferenceTarget, SymbolId, SymbolKind, resolve_names},
    ownership_checking::{NonNullAssertionTransferKind, OwnershipCheckedFile, check_ownership},
    type_checking::{
        BuiltinType, Copyability, IntrinsicTypeConstructor, NominalKind, ParameterMode, TypeId,
        TypeKind, TypedFile, check_types, complete_file_types, standard_environments,
    },
};

use super::*;

pub(super) fn observe(case: &Case, trace: &mut Trace) -> Outcome<LowerObservation> {
    observe_source(
        case.path,
        case.single,
        |sources, parsed, names, typed, trace| inspect(case, sources, parsed, names, typed, trace),
        |_, _, _, _, _, _| {},
        case.counts[5],
        trace,
    )
}

pub(super) fn observe_source(
    path: &str,
    text: &str,
    inspect: impl FnOnce(&SourceMap, &ParsedFile, &NameResolution, &TypedFile, &mut Trace),
    inspect_owned: impl FnOnce(
        &SourceMap,
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
        &mut Trace,
    ),
    assertion_count: usize,
    trace: &mut Trace,
) -> Outcome<LowerObservation> {
    let mut sources = SourceMap::new();
    let parsed = parsed(&mut sources, path, text, trace)?;
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
    let raw = diagnostics("raw type diagnostics", &sources, typed.diagnostics(), trace);
    inspect(&sources, &parsed, &names, &typed, trace);
    let complete = step(
        "complete types",
        complete_file_types(&sources, &parsed, &names, &types, &typed)
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
    let owned = step(
        "ownership",
        check_ownership(&sources, &parsed, &names, complete.types()),
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
        "real assertion ownership plans",
        owned.non_null_assertions().len() == assertion_count
            && owned.non_null_assertions().iter().all(|plan| {
                typed.non_null_assertions().contains(plan.descriptor())
                    && plan.non_null_transfer() == NonNullAssertionTransferKind::Consume
            }),
        owned.non_null_assertions(),
    );
    inspect_owned(&sources, &parsed, &names, complete.types(), &owned, trace);
    clean?;
    complete_ownership?;
    trace
        .stages
        .push("attempt lower SSA with real main".to_owned());
    Ok(LowerObservation {
        entry: selected_entry(&sources, entry.name(), entry.span()),
        outcome: finish_lower(
            &sources,
            crate::ssa::lower_scalar_file_with_entry(
                &sources,
                &parsed,
                &names,
                complete.types(),
                &owned,
                entry.id(),
            ),
            trace,
        ),
    })
}

fn named(
    names: &NameResolution,
    parsed: &ParsedFile,
    name: &str,
    kind: SymbolKind,
) -> Option<SymbolId> {
    let values: Vec<_> = names
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.name() == name
                && symbol.kind() == kind
                && !symbol.is_synthetic()
                && symbol.span().source_id() == parsed.source_id()
        })
        .collect();
    match values.as_slice() {
        [symbol] => Some(symbol.id()),
        _ => None,
    }
}

fn receiver(
    names: &NameResolution,
    parsed: &ParsedFile,
    expression: ExpressionId,
) -> Option<SymbolId> {
    let leaf = super::super::ungroup(parsed, expression)?;
    let span = parsed.ast().expressions().get(leaf).ok()?.span();
    names.references().iter().find_map(|reference| {
        if reference.span() != span {
            return None;
        }
        match reference.target() {
            ReferenceTarget::Symbol(symbol) => Some(*symbol),
            _ => None,
        }
    })
}

fn map_type(typed: &TypedFile, key: Option<TypeId>, value: Option<TypeId>) -> Option<TypeId> {
    typed.types().find(&TypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::MutableMap,
        arguments: vec![key?, value?],
    })
}

fn inspect(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    trace: &mut Trace,
) {
    let resource_symbol = named(names, parsed, "Resource", SymbolKind::Classifier);
    let nominals: Vec<_> = typed
        .nominals()
        .iter()
        .filter(|nominal| Some(nominal.id().symbol()) == resource_symbol)
        .collect();
    let resource = match nominals.as_slice() {
        [nominal] => typed.types().find(&TypeKind::Nominal {
            nominal: nominal.id(),
            arguments: Vec::new(),
        }),
        _ => None,
    };
    trace.check(
        "actual Resource nominal and deinit owner/receiver",
        resource_symbol.is_some()
            && matches!(nominals.as_slice(), [nominal]
            if nominal.kind() == NominalKind::Class && nominal.has_deinit()
                && nominal.deinit().is_some_and(|deinit| deinit.owner() == nominal.id()
                    && Some(deinit.receiver_type()) == resource
                    && deinit.receiver_mode() == ParameterMode::Borrow)),
        &nominals,
    );
    let int = typed.types().builtin(BuiltinType::Int);
    let inner = map_type(typed, int, resource);
    let outer = map_type(typed, int, inner);
    let nullable = inner.and_then(|inner| typed.types().find(&TypeKind::Nullable(inner)));
    trace.check(
        "precise Resource inner and Nullable Map classifications",
        [resource, inner, nullable]
            .into_iter()
            .all(|ty| ty.is_some_and(|ty| typed.is_resource_type(ty) == Some(true))),
        [resource, inner, nullable].map(|ty| (ty, ty.and_then(|ty| typed.is_resource_type(ty)))),
    );
    let root = |name| named(names, parsed, name, SymbolKind::Variable);
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
        .filter(|put| receiver(names, parsed, put.receiver()) == root("inner"))
        .collect();
    let outer_puts: Vec<_> = typed
        .map_puts()
        .iter()
        .filter(|put| receiver(names, parsed, put.receiver()) == root("outer"))
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
                    && receiver(names, parsed, put.value()) == root("inner")
            }),
        (inner_puts, outer_puts),
    );
    for remove in typed.map_removes() {
        trace.check(
            "real outer remove payload/result and assertion operand",
            receiver(names, parsed, remove.receiver()) == root("outer")
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
            parsed
                .ast()
                .expressions()
                .get(remove.expression())
                .map(|node| origin(sources, node.span()))
        ));
    }
    trace.check(
        "real got.size read",
        typed.map_sizes().iter().all(|size| {
            receiver(names, parsed, size.receiver()) == root("got")
                && Some(size.map_type()) == inner
                && typed.expression_type(size.receiver()) == inner
        }),
        typed.map_sizes(),
    );
}
