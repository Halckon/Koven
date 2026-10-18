//! Single 原始 names/raw/complete/ownership/完整 validator → 真实 lower。
use super::{
    Observation, Report, facts,
    fixture::{Fixture, LowerOutcome, Stopped, clean, stopped},
};
use lang_frontend::{
    name_resolution::{SymbolKind, resolve_names},
    ownership_checking::{check_ownership, validate_iteration_facts},
    type_checking::{check_types, complete_file_types, standard_environments},
};

pub(super) fn collect_whole_file(text: &str) -> Observation {
    let mut report = Report::default();
    let result = Fixture::single_whole(text)
        .and_then(|fixture| collect(&fixture, "entry", true, &mut report));
    Observation {
        label: "whole-file/Single".to_owned(),
        report,
        result,
    }
}

pub(super) fn collect(
    fixture: &Fixture,
    entry_name: &str,
    whole_set: bool,
    report: &mut Report,
) -> Result<LowerOutcome, Stopped> {
    let [file] = fixture.files.as_slice() else {
        return Err(stopped(
            "single fixture infrastructure",
            "expected one source",
        ));
    };
    let sources = &fixture.sources;
    let parsed = &file.parsed;
    report.check("single lex/parse clean", true, parsed.source_id());
    let (name_environment, environment) = standard_environments();
    let names = resolve_names(sources, parsed, &name_environment)
        .map_err(|error| stopped("single names infrastructure", error))?;
    clean(sources, "single names diagnostics", names.diagnostics())?;
    let entries = names
        .symbols()
        .iter()
        .filter(|symbol| {
            symbol.name() == entry_name
                && !symbol.is_synthetic()
                && symbol.kind() == SymbolKind::Function
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        return Err(stopped("single real entry identity", entries));
    };
    report.check("single names and real entry", true, entry);
    let expected_entry = fixture.function_location(entry.span());
    report.check(
        "single real entry full source origin",
        expected_entry.is_some(),
        &expected_entry,
    );
    if whole_set {
        report.check(
            "single entry empty and calls no Map function",
            facts::empty_entry(file, entry.span()),
            entry,
        );
    }
    let raw = check_types(sources, parsed, &names, &environment)
        .map_err(|error| stopped("single raw infrastructure", error))?;
    report.check(
        "single raw diagnostics",
        raw.diagnostics().is_empty(),
        raw.diagnostics(),
    );
    // raw 有诊断仍走 complete 留下原根因；任何前置失败都不调用 ownership/lower。
    let complete = complete_file_types(sources, parsed, &names, &environment, &raw)
        .map_err(|error| stopped("single complete admission", error))?;
    let same = complete.is_same_analysis(&raw) && std::ptr::eq(complete.types(), &raw);
    report.check(
        "single complete original analysis",
        same,
        raw.is_compatible_with_names(&names),
    );
    if !same {
        return Err(stopped(
            "single complete identity",
            "not the original raw product",
        ));
    }
    clean(sources, "single raw diagnostics", raw.diagnostics())?;
    let owned = check_ownership(sources, parsed, &names, complete.types())
        .map_err(|error| stopped("single ownership infrastructure", error))?;
    clean(sources, "single ownership diagnostics", owned.diagnostics())?;
    if !owned.deferred().is_empty() {
        return Err(stopped("single ownership deferred", owned.deferred()));
    }
    report.check(
        "single ownership original identity",
        owned.is_compatible_with(&names, &raw),
        owned.source_id(),
    );
    report.check(
        "single ownership clean and not deferred",
        true,
        owned.deferred(),
    );
    facts::single(fixture, file, &names, &raw, &owned, whole_set, report);
    // 完整公共 validator 独立于上述只读 oracle，必须成功才允许进入 lower。
    validate_iteration_facts(parsed, &names, complete.types(), &owned)
        .map_err(|error| stopped("single full iteration validator", error))?;
    report.check(
        "single full iteration validator",
        true,
        "validate_iteration_facts: Ok",
    );
    // query check 为 false 仍调用真实 lower；不能用手造错误或 query 返回值替代阶段门。
    let lowered = crate::ssa::lower_scalar_file_with_entry(
        sources,
        parsed,
        &names,
        complete.types(),
        &owned,
        entry.id(),
    );
    report.check(
        "single lower actually attempted",
        true,
        lowered.as_ref().err(),
    );
    Ok(LowerOutcome::new(sources, expected_entry, lowered))
}
