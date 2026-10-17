//! 真实阶段链；失败不越过 final type admission，也不使其余普通源码短路。
use super::{
    Report,
    cases::Case,
    fixture::{Blocked, Fixture, LOGICAL_PATH, blocked, diagnostics, require_clean, sites},
    ownership_facts, type_facts,
};
use lang_frontend::{
    name_resolution::{
        NameResolution, SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, OwnershipCheckedFile, check_compilation_unit_ownership,
        check_ownership,
    },
    type_checking::{
        CompilationUnitTypes, FinalTypeAdmissionError, TypeEnvironment, TypedFile,
        check_compilation_unit_types, check_types, complete_compilation_unit_types,
        complete_file_types, standard_environments,
    },
};

#[derive(Clone, Copy, Debug)]
pub(super) enum Entry {
    Single,
    Unit,
}

struct SingleAnalysis {
    names: NameResolution,
    raw: TypedFile,
    complete: Result<bool, FinalTypeAdmissionError>,
    owned: Result<Option<OwnershipCheckedFile>, String>,
}

struct UnitAnalysis {
    names: ValidatedCompilationUnitNames,
    raw: CompilationUnitTypes,
    complete: Result<bool, FinalTypeAdmissionError>,
    owned: Result<Option<CompilationUnitOwnership>, String>,
}

enum Analysis {
    Single(Box<SingleAnalysis>),
    Unit(Box<UnitAnalysis>),
}

struct Observed {
    fixture: Fixture,
    environment: TypeEnvironment,
    analysis: Analysis,
    report: Report,
}

pub(super) struct Observation<'a> {
    case: &'a Case,
    entry: Entry,
    result: Result<Observed, Blocked>,
}

fn record_complete(
    fixture: &Fixture,
    result: &Result<bool, FinalTypeAdmissionError>,
    report: &mut Report,
) {
    match result {
        Ok(same) => report.record("complete original identity", *same, same),
        Err(FinalTypeAdmissionError::Diagnostics(items)) => report.record(
            "complete language diagnostics",
            false,
            diagnostics(&fixture.sources, "complete", items),
        ),
        Err(FinalTypeAdmissionError::Internal(error)) => {
            report.record("complete INTERNAL", false, error);
        }
    }
}

pub(super) fn observe(case: &Case, entry: Entry) -> Observation<'_> {
    Observation {
        case,
        entry,
        result: analyze(case, entry),
    }
}

fn analyze(case: &Case, entry: Entry) -> Result<Observed, Blocked> {
    let fixture = Fixture::new(case.text)?;
    let sites = sites(&fixture, case)?;
    let (name_environment, environment) = standard_environments();
    let mut report = Report::default();
    report.record("lex/parse", true, fixture.file.source_id());
    report.note("actual AST sites", &sites);
    let sources = &fixture.sources;
    let file = &fixture.file;
    let analysis = match entry {
        Entry::Single => {
            let names = resolve_names(sources, file, &name_environment)
                .map_err(|error| blocked("single names infrastructure", error))?;
            require_clean(sources, "single names", names.diagnostics())?;
            report.record("names", true, names.source_id());
            let raw = check_types(sources, file, &names, &environment)
                .map_err(|error| blocked("single raw infrastructure", error))?;
            report.record(
                "raw diagnostics",
                raw.diagnostics().is_empty(),
                diagnostics(sources, "raw", raw.diagnostics()),
            );
            type_facts::single(case, &fixture, &sites, &names, &raw, &mut report);
            // raw 的恢复诊断也交 complete 记录；失败时绝不调用 ownership。
            let (complete, owned) =
                match complete_file_types(sources, file, &names, &environment, &raw) {
                    Ok(complete) => {
                        let same = complete.is_same_analysis(&raw);
                        let owned = if same && raw.diagnostics().is_empty() {
                            check_ownership(sources, file, &names, complete.types())
                                .map(Some)
                                .map_err(|error| format!("{error:?}"))
                        } else {
                            Ok(None)
                        };
                        (Ok(same), owned)
                    }
                    Err(error) => (Err(error), Ok(None)),
                };
            record_complete(&fixture, &complete, &mut report);
            match &owned {
                Ok(Some(owned)) => {
                    report.record("ownership reached", true, "original complete.types()");
                    ownership_facts::single(
                        case,
                        &fixture,
                        &sites,
                        &names,
                        &raw,
                        owned,
                        &mut report,
                    );
                }
                Ok(None) => report.record(
                    "OWNERSHIP_NOT_REACHED",
                    false,
                    "raw/complete admission failed",
                ),
                Err(error) => report.record("ownership INTERNAL", false, error),
            }
            Analysis::Single(Box::new(SingleAnalysis {
                names,
                raw,
                complete,
                owned,
            }))
        }
        Entry::Unit => {
            let inputs = [SourceUnitInput::new(
                "root",
                LOGICAL_PATH,
                file.source_id(),
                file,
            )];
            let index = index_compilation_unit(sources, &inputs)
                .map_err(|error| blocked("unit index infrastructure", error))?;
            require_clean(sources, "unit index", index.diagnostics())?;
            let names = resolve_compilation_unit_names(sources, &inputs, &index, &name_environment)
                .map_err(|error| blocked("unit names infrastructure", error))?;
            require_clean(sources, "unit names", names.diagnostics())?;
            let names = names
                .validate()
                .map_err(|error| blocked("unit names qualification", error.diagnostics()))?;
            report.record("names", true, names.names().source_units().len());
            let raw = check_compilation_unit_types(sources, &inputs, &names, &environment)
                .map_err(|error| blocked("unit raw infrastructure", error))?;
            report.record(
                "raw diagnostics",
                raw.diagnostics().is_empty(),
                diagnostics(sources, "raw", raw.diagnostics()),
            );
            type_facts::unit(case, &fixture, &sites, &names, &raw, &mut report);
            let complete =
                complete_compilation_unit_types(sources, &inputs, &names, &environment, &raw)
                    .map(|complete| complete.is_same_analysis(&raw));
            record_complete(&fixture, &complete, &mut report);
            let (raw, owned) = if raw.diagnostics().is_empty() && matches!(complete, Ok(true)) {
                match raw.validate() {
                    Ok(typed) => {
                        report.record("basic type qualification", true, "original typed product");
                        let owned = check_compilation_unit_ownership(
                            sources,
                            &inputs,
                            &names,
                            &environment,
                            &typed,
                        )
                        .map(|owned| {
                            report.record(
                                "ownership typed identity",
                                owned.is_compatible_with(&typed),
                                "original validated types",
                            );
                            ownership_facts::unit(
                                case,
                                &fixture,
                                &sites,
                                &names,
                                typed.types(),
                                &owned,
                                &mut report,
                            );
                            // validate 消费后解包同一真实产物，不重新分析或 clone facts。
                            match owned.validate() {
                                Ok(qualified) => {
                                    report.record(
                                        "ownership basic qualification",
                                        case.expected.is_success(),
                                        "accepted",
                                    );
                                    qualified.into_ownership()
                                }
                                Err(owned) => {
                                    report.record(
                                        "ownership basic qualification",
                                        !case.expected.is_success(),
                                        "rejected",
                                    );
                                    *owned
                                }
                            }
                        })
                        .map(Some)
                        .map_err(|error| format!("{error:?}"));
                        (typed.into_types(), owned)
                    }
                    Err(raw) => {
                        report.record("basic type qualification", false, raw.diagnostics());
                        (*raw, Ok(None))
                    }
                }
            } else {
                (raw, Ok(None))
            };
            match &owned {
                Ok(Some(_)) => report.record("ownership reached", true, "original qualified unit"),
                Ok(None) => report.record(
                    "OWNERSHIP_NOT_REACHED",
                    false,
                    "raw/complete/basic admission failed",
                ),
                Err(error) => report.record("ownership INTERNAL", false, error),
            }
            Analysis::Unit(Box::new(UnitAnalysis {
                names,
                raw,
                complete,
                owned,
            }))
        }
    };
    Ok(Observed {
        fixture,
        environment,
        analysis,
        report,
    })
}

impl Observation<'_> {
    pub(super) fn assess(&self) -> (bool, String) {
        let label = format!("{}/{:?}", self.case.label, self.entry);
        let observed = match &self.result {
            Ok(observed) => observed,
            Err(error) => {
                return (
                    false,
                    format!(
                        "{label}: BLOCKED {}: {}\nSOURCE {:?}",
                        error.stage, error.detail, self.case.text
                    ),
                );
            }
        };
        let fixture = &observed.fixture;
        let retained_source = fixture.lexed.source_id() == fixture.file.source_id();
        let (identity, phase2, internal, reached) = match &observed.analysis {
            Analysis::Single(analysis) => (
                analysis.raw.is_compatible_with_names(&analysis.names)
                    && analysis
                        .owned
                        .as_ref()
                        .ok()
                        .and_then(Option::as_ref)
                        .is_none_or(|owned| {
                            owned.is_compatible_with(&analysis.names, &analysis.raw)
                        }),
                analysis.raw.diagnostics().is_empty() && matches!(analysis.complete, Ok(true)),
                matches!(analysis.complete, Err(FinalTypeAdmissionError::Internal(_)))
                    || analysis.owned.is_err(),
                matches!(analysis.owned, Ok(Some(_))),
            ),
            Analysis::Unit(analysis) => {
                let inputs = [SourceUnitInput::new(
                    "root",
                    LOGICAL_PATH,
                    fixture.file.source_id(),
                    &fixture.file,
                )];
                (
                    analysis.raw.is_compatible_with(
                        &fixture.sources,
                        &inputs,
                        &analysis.names,
                        &observed.environment,
                    ),
                    analysis.raw.diagnostics().is_empty() && matches!(analysis.complete, Ok(true)),
                    matches!(analysis.complete, Err(FinalTypeAdmissionError::Internal(_)))
                        || analysis.owned.is_err(),
                    matches!(analysis.owned, Ok(Some(_))),
                )
            }
        };
        let passed = identity && retained_source && observed.report.passed();
        let status = if internal {
            "INTERNAL"
        } else if !phase2 {
            "PHASE_2_REJECTED / OWNERSHIP_NOT_REACHED"
        } else if !reached {
            "OWNERSHIP_NOT_REACHED"
        } else if passed {
            "TARGET_MET"
        } else {
            "CONTRACT_TARGET_FAILED"
        };
        (
            passed,
            format!(
                "{label}: {status} retained_source={retained_source} retained_analysis_identity={identity}\nSOURCE {:?}\nCHECKS {:#?}\nDETAILS {}",
                self.case.text,
                observed.report.checks,
                observed.report.details.join("\n")
            ),
        )
    }
}
