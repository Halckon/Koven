//! Legacy single-file route retained only for the unchanged formed-local exact contract.

use lang_frontend::{
    name_resolution::{NameResolution, SymbolKind, resolve_names},
    ownership_checking::{NonNullAssertionTransferKind, OwnershipCheckedFile, check_ownership},
    type_checking::{TypedFile, check_types, complete_file_types, standard_environments},
};

use super::*;

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
