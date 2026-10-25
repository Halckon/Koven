//! Original single-source bytes admitted as one same-object MemoryDocument Unit.

use lang_frontend::{
    name_resolution::{
        DeclarationId, SourceUnitId, SourceUnitInput, SourceUnitInputKey, SourceUnitKey,
        SymbolKind, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, NonNullAssertionTransferKind, OwnedCompilationUnitView,
        check_compilation_unit_ownership, owned_compilation_unit_view,
    },
    type_checking::{
        CompilationUnitTypes, UnitCallTarget, check_compilation_unit_types,
        complete_compilation_unit_types, standard_environments,
    },
};

use super::*;

pub(super) fn observe(
    case: &Case,
    strong_contract: bool,
    trace: &mut Trace,
) -> Outcome<LowerObservation> {
    observe_source(
        case.path,
        case.single,
        |sources, file, names, typed, trace| {
            unit::inspect(case, sources, file, names, typed, trace)
        },
        |_, _, _, _, _, _| {},
        case.counts[5],
        |view, entry, program, lowered_entry, trace| {
            if strong_contract {
                contract::inspect(view, entry, program, lowered_entry, trace);
            }
        },
        trace,
    )
}

pub(super) fn observe_source(
    path: &str,
    text: &str,
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
    inspect_lower: impl FnOnce(
        &OwnedCompilationUnitView<'_, '_>,
        DeclarationId,
        &Program,
        FunctionId,
        &mut Trace,
    ),
    trace: &mut Trace,
) -> Outcome<LowerObservation> {
    let mut sources = SourceMap::new();
    let parsed = parsed(&mut sources, path, text, trace)?;
    let inputs = [SourceUnitInput::memory_document(
        parsed.source_id(),
        &parsed,
    )];
    gate(
        "same-object original MemoryDocument",
        std::ptr::eq(inputs[0].parsed(), &parsed)
            && inputs[0].source_id() == parsed.source_id()
            && matches!(inputs[0].key(), SourceUnitInputKey::MemoryDocument)
            && sources.source_name(parsed.source_id()).ok() == Some(path)
            && sources.source_text(parsed.source_id()).ok() == Some(text)
            && parsed.package().is_none(),
        (&inputs, path),
    )?;
    let (environment, types) = standard_environments();
    let index = step("index", index_compilation_unit(&sources, &inputs), trace)?;
    diagnostics("index diagnostics", &sources, index.diagnostics(), trace)?;
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
    let [source_unit] = index.source_units() else {
        return Err(Failure {
            stage: "one original MemoryDocument".to_owned(),
            detail: format!("{:#?}", index.source_units()),
        });
    };
    gate(
        "original default-package source unit",
        source_unit.source_id() == parsed.source_id()
            && source_unit.key() == &SourceUnitKey::MemoryDocument
            && index
                .packages()
                .get(source_unit.package().index())
                .is_some_and(|package| {
                    package
                        .name()
                        .is_some_and(|name| name.segments().is_empty())
                }),
        source_unit,
    )?;
    let entries: Vec<_> = index
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "main"
                && declaration.kind() == SymbolKind::Function
                && declaration.source_unit() == source_unit.id()
                && declaration.package() == source_unit.package()
                && declaration.name_span().source_id() == parsed.source_id()
        })
        .collect();
    let [entry] = entries.as_slice() else {
        return Err(Failure {
            stage: "unique original main declaration".to_owned(),
            detail: format!("{entries:#?}"),
        });
    };
    let entry_span = parsed
        .ast()
        .items()
        .get(entry.root())
        .expect("real main item")
        .span();
    trace
        .stages
        .push(format!("selected real MemoryDocument main={entry:#?}"));
    let typed = step(
        "raw types",
        check_compilation_unit_types(&sources, &inputs, &names, &types),
        trace,
    )?;
    let raw = diagnostics("raw type diagnostics", &sources, typed.diagnostics(), trace);
    inspect(&sources, (source_unit.id(), &parsed), &names, &typed, trace);
    {
        let complete = step(
            "complete types",
            complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed)
                .map_err(|error| admission_error(&sources, error)),
            trace,
        )?;
        gate(
            "same complete analysis and original facts",
            complete.is_same_analysis(&typed)
                && std::ptr::eq(complete.types(), &typed)
                && typed.is_compatible_with(&sources, &inputs, &names, &types),
            "complete types borrow the original raw analysis",
        )?;
        diagnostics(
            "complete diagnostics",
            &sources,
            complete.types().diagnostics(),
            trace,
        )?;
    }
    raw?;
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
        owned.non_null_assertions(),
    ));
    gate(
        "ownership completeness",
        owned.deferred().is_empty(),
        owned.deferred(),
    )?;
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
    gate(
        "ownership analysis identity",
        owned.is_compatible_with(&typed),
        owned.deferred(),
    )?;
    inspect_owned(
        &sources,
        (source_unit.id(), &parsed),
        &names,
        typed.types(),
        &owned,
        trace,
    );
    clean?;
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
        "owned view",
        owned_compilation_unit_view(&sources, &inputs, &names, &types, &typed, &owned),
        trace,
    )?;
    gate(
        "owned view keeps all original references",
        std::ptr::eq(view.sources(), &sources)
            && std::ptr::eq(view.inputs(), inputs.as_slice())
            && std::ptr::eq(view.names(), &names)
            && std::ptr::eq(view.types(), typed.types())
            && std::ptr::eq(view.ownership(), owned.ownership()),
        "sources/inputs/names/types/ownership",
    )?;
    trace
        .stages
        .push("attempt lower SSA with original MemoryDocument main".to_owned());
    let result = crate::ssa::unit_lower::lower_owned_unit_with_entry(&view, entry.id());
    if let Ok((program, lowered_entry)) = &result {
        trace.check(
            "selected SSA entry retains exact original Function Item origin",
            program
                .module(lowered_entry.module())
                .and_then(|module| module.function(*lowered_entry))
                .is_some_and(|function| {
                    function.origin == crate::ssa::model::Origin::Source(entry_span)
                }),
            (entry.id(), lowered_entry, entry_span),
        );
        inspect_dependencies(&view, &parsed, program, trace);
        inspect_lower(&view, entry.id(), program, *lowered_entry, trace);
    }
    Ok(LowerObservation {
        entry: selected_entry(&sources, entry.name(), entry.name_span()),
        outcome: finish_lower(&sources, result, trace),
    })
}

// The frozen P3/K6 inputs contain a real run(Boolean) dependency of main.
// Keep the declaration and source-call identities, not merely a successful empty entry.
fn inspect_dependencies(
    view: &OwnedCompilationUnitView<'_, '_>,
    parsed: &ParsedFile,
    program: &Program,
    trace: &mut Trace,
) {
    use crate::ssa::model::{Operation, Origin};

    for declaration in view
        .names()
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| declaration.kind() == SymbolKind::Function)
    {
        let span = parsed
            .ast()
            .items()
            .get(declaration.root())
            .expect("original function item")
            .span();
        let functions: Vec<_> = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| function.origin == Origin::Source(span))
            .collect();
        trace.check(
            "original function declaration has exactly one real emitted body",
            functions.len() == 1,
            (declaration, &functions),
        );
        let [target] = functions.as_slice() else {
            continue;
        };
        for call in view
            .types()
            .calls()
            .iter()
            .filter(|call| call.target() == UnitCallTarget::Declaration(declaration.id()))
        {
            let call_span = parsed
                .ast()
                .expressions()
                .get(call.expression().expression())
                .expect("original source call")
                .span();
            let actual: Vec<_> = program
                .modules
                .iter()
                .flat_map(|module| &module.functions)
                .flat_map(|function| &function.instructions)
                .filter(|instruction| {
                    instruction.origin == Origin::Source(call_span)
                        && matches!(instruction.operation, Operation::DirectCall { callee, .. }
                            if callee == target.id())
                })
                .collect();
            trace.check(
                "real source call reaches its exact source-qualified dependency",
                actual.len() == 1,
                (call, target.id(), actual),
            );
        }
    }
}
