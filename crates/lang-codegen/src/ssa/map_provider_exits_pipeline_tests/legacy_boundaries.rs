//! Explicit test-only Legacy boundary probe, never selected as a fallback.
//! Sole consumers: the exact pre-lower L0135 conflict and the historical
//! `result.consume()` UnsupportedNode / Unit-success differential. Retire this
//! probe only when those two contracts have an explicitly approved replacement.
use super::{
    Report,
    fixture::{Failure, at, clean, complete_failure, failure, lower_failure},
};
use crate::ssa::{
    lower_scalar_file_with_entry,
    model::{FunctionId, Program},
};
use lang_frontend::{
    name_resolution::{NameResolution, SymbolKind, resolve_names},
    ownership_checking::{OwnershipCheckedFile, check_ownership, validate_iteration_facts},
    parser::{Item, NameMarker, ParsedFile},
    source::SourceMap,
    type_checking::{TypedFile, check_types, complete_file_types, standard_environments},
};
type Lowered = (Program, FunctionId);

pub(super) fn observe(
    sources: &SourceMap,
    parsed: &ParsedFile,
    entry_name: &str,
    report: &mut Report,
    observe: impl FnOnce(&NameResolution, &TypedFile, &OwnershipCheckedFile, &mut Report),
) -> Result<Lowered, Failure> {
    report.check("single lex/parse clean", true, parsed.source_id());
    let (name_environment, environment) = standard_environments();
    let names = resolve_names(sources, parsed, &name_environment)
        .map_err(|error| failure("single names infrastructure", error))?;
    clean(sources, "single names diagnostics", names.diagnostics())?;
    let entries = names.symbols().iter().filter(|symbol| {
        symbol.name() == entry_name && !symbol.is_synthetic() && symbol.kind() == SymbolKind::Function
            && symbol.span().source_id() == parsed.source_id()
            && parsed.roots().iter().any(|root| {
                parsed.ast().items().get(*root).is_ok_and(|node| {
                    matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. } if *name == symbol.span())
                })
            })
    }).collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        return Err(failure("single real entry selection", entries));
    };
    report.check("single actual source entry symbol", true, entry);
    let raw = check_types(sources, parsed, &names, &environment)
        .map_err(|error| failure("single raw type infrastructure", error))?;
    report.check(
        "single raw diagnostics clean",
        raw.diagnostics().is_empty(),
        raw.diagnostics(),
    );
    // raw 即使有诊断仍执行完整准入，保存 complete 原始根因，不冒充 lower 拒绝。
    let complete = complete_file_types(sources, parsed, &names, &environment, &raw)
        .map_err(|error| complete_failure(sources, error))?;
    let same = complete.is_same_analysis(&raw)
        && std::ptr::eq(complete.types(), &raw)
        && raw.is_compatible_with_names(&names);
    report.check(
        "single complete keeps original raw/names analysis",
        same,
        raw.source_id(),
    );
    if !same {
        return Err(failure(
            "single complete analysis identity",
            "original raw identity mismatch",
        ));
    }
    clean(sources, "single raw diagnostics", raw.diagnostics())?;
    let owned = check_ownership(sources, parsed, &names, complete.types())
        .map_err(|error| failure("single ownership infrastructure", error))?;
    clean(sources, "single ownership diagnostics", owned.diagnostics())?;
    if !owned.deferred().is_empty() {
        return Err(failure(
            "single ownership deferred",
            owned
                .deferred()
                .iter()
                .map(|deferred| {
                    let location = parsed
                        .ast()
                        .expressions()
                        .get(deferred.expression())
                        .map(|node| super::fixture::location(sources, node.span()));
                    (deferred, location)
                })
                .collect::<Vec<_>>(),
        ));
    }
    let same = owned.is_compatible_with(&names, &raw);
    report.check(
        "single ownership original analysis and no deferred",
        same,
        owned.source_id(),
    );
    if !same {
        return Err(failure(
            "single ownership analysis identity",
            "raw/names mismatch",
        ));
    }
    observe(&names, &raw, &owned, report);
    // 完整公共 validator 独立于只读 input query；失败保留原 reason/span。
    validate_iteration_facts(parsed, &names, complete.types(), &owned).map_err(|error| {
        at(
            sources,
            "single full iteration validator",
            &error,
            error.span(),
        )
    })?;
    report.check(
        "single full iteration validator",
        true,
        "validate_iteration_facts: Ok",
    );
    report.check("single lower actually attempted", true, entry.id());
    lower_scalar_file_with_entry(
        sources,
        parsed,
        &names,
        complete.types(),
        &owned,
        entry.id(),
    )
    .map_err(|error| lower_failure(sources, error))
}
