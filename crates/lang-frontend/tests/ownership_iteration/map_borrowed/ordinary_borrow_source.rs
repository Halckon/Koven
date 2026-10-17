//! SPEC-0331：普通 borrow-return Map call 不取得未经证明的 provider continuation。
//! 同一源码 single/unit 两观察；旧 P2 RED 不能冒充实际 ownership L0164。

#[path = "ordinary_borrow_source/contract.rs"]
mod contract;
use super::{
    Report,
    fixture::{Blocked, Fixture, LOGICAL_PATH, blocked, diagnostics, require_clean},
    pipeline::Entry,
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

const SOURCE: &str = r#"fun view(source: Map<Int, Int>): borrow Map<Int, Int> from source = source
fun scan(source: Map<Int, Int>): Unit {
  for ((key, value) in view(source)) {}
}
"#;

struct Single {
    names: NameResolution,
    raw: TypedFile,
    complete: Result<bool, FinalTypeAdmissionError>,
    owned: Result<Option<OwnershipCheckedFile>, String>,
}
struct Unit {
    names: ValidatedCompilationUnitNames,
    raw: CompilationUnitTypes,
    complete: Result<bool, FinalTypeAdmissionError>,
    owned: Result<Option<CompilationUnitOwnership>, String>,
}
enum Products {
    Single(Box<Single>),
    Unit(Box<Unit>),
}
struct Observed {
    fixture: Fixture,
    environment: TypeEnvironment,
    products: Products,
    report: Report,
}
struct Observation {
    entry: Entry,
    result: Result<Observed, Blocked>,
}

fn record_complete(
    fixture: &Fixture,
    complete: &Result<bool, FinalTypeAdmissionError>,
    report: &mut Report,
) {
    match complete {
        Ok(same) => report.record("complete original identity", *same, same),
        Err(FinalTypeAdmissionError::Diagnostics(items)) => report.record(
            "complete diagnostics",
            false,
            diagnostics(&fixture.sources, "complete", items),
        ),
        Err(FinalTypeAdmissionError::Internal(error)) => {
            report.record("complete INTERNAL", false, error)
        }
    }
}

fn analyze(entry: Entry) -> Result<Observed, Blocked> {
    let fixture = Fixture::new(SOURCE)?;
    let sites = contract::sites(&fixture)?;
    let (name_environment, environment) = standard_environments();
    let sources = &fixture.sources;
    let file = &fixture.file;
    let mut report = Report::default();
    report.record("lex/parse", true, file.source_id());
    report.note("actual source/borrow declaration sites", &sites);
    let products = match entry {
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
            contract::single(&fixture, &sites, &names, &raw, &mut report);
            report.note(
                "existing typed sequential plan count",
                raw.sequential_iterations().len(),
            );
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
                    report.record(
                        "ownership reached/identity",
                        owned.is_compatible_with(&names, &raw),
                        "original complete.types()",
                    );
                    contract::rejection(&fixture, &sites, owned.diagnostics(), &mut report);
                    report.record(
                        "ownership deferred",
                        owned.deferred().is_empty(),
                        owned.deferred(),
                    );
                    report.note("formed owned locals", owned.formed_owned_locals());
                    report.record(
                        "error atomic ordinary borrow origins",
                        owned.borrow_return_origins().is_empty(),
                        owned.borrow_return_origins(),
                    );
                    let borrow = owned.borrow_results();
                    report.record(
                        "error atomic ordinary borrow results",
                        borrow.bindings().is_empty()
                            && borrow.ends().is_empty()
                            && borrow.call_uses().is_empty()
                            && borrow.forwarded_source_loans().is_empty(),
                        borrow,
                    );
                    report.record(
                        "error atomic drops",
                        owned.drops().is_empty(),
                        owned.drops(),
                    );
                    report.note(
                        "existing ownership sequential plans (not Map oracle)",
                        owned.iterations(),
                    );
                }
                Ok(None) => report.record(
                    "OWNERSHIP_NOT_REACHED",
                    false,
                    "raw/complete admission failed",
                ),
                Err(error) => report.record("ownership INTERNAL", false, error),
            }
            Products::Single(Box::new(Single {
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
            contract::unit(&fixture, &sites, &names, &raw, &mut report);
            report.note(
                "existing typed sequential plan count",
                raw.sequential_iterations().len(),
            );
            let complete =
                complete_compilation_unit_types(sources, &inputs, &names, &environment, &raw)
                    .map(|complete| complete.is_same_analysis(&raw));
            record_complete(&fixture, &complete, &mut report);
            let (raw, owned) = if raw.diagnostics().is_empty() && matches!(complete, Ok(true)) {
                match raw.validate() {
                    Ok(typed) => {
                        report.record("basic type qualification", true, "original unit types");
                        let owned = check_compilation_unit_ownership(
                            sources,
                            &inputs,
                            &names,
                            &environment,
                            &typed,
                        )
                        .map(|owned| {
                            report.record(
                                "ownership reached/identity",
                                owned.is_compatible_with(&typed),
                                "original qualified unit",
                            );
                            contract::rejection(&fixture, &sites, owned.diagnostics(), &mut report);
                            report.record(
                                "ownership deferred",
                                owned.deferred().is_empty(),
                                owned.deferred(),
                            );
                            report.note("formed owned locals", owned.formed_owned_locals());
                            report.record(
                                "error atomic ordinary borrow origins",
                                owned.borrow_return_origins().is_empty(),
                                owned.borrow_return_origins(),
                            );
                            let borrow = owned.borrow_results();
                            report.record(
                                "error atomic ordinary borrow results",
                                borrow.bindings().is_empty()
                                    && borrow.ends().is_empty()
                                    && borrow.call_uses().is_empty()
                                    && borrow.forwarded_source_loans().is_empty(),
                                borrow,
                            );
                            report.record(
                                "error atomic drops",
                                owned.drops().is_empty(),
                                owned.drops(),
                            );
                            report.note(
                                "existing ownership sequential plans (not Map oracle)",
                                owned.iterations(),
                            );
                            match owned.validate() {
                                Ok(qualified) => {
                                    report.record(
                                        "ownership basic qualification",
                                        false,
                                        "unexpected acceptance",
                                    );
                                    qualified.into_ownership()
                                }
                                Err(owned) => {
                                    report.record(
                                        "ownership basic qualification",
                                        true,
                                        "rejected by actual ownership diagnostics",
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
                Ok(None) => report.record(
                    "OWNERSHIP_NOT_REACHED",
                    false,
                    "raw/complete/basic admission failed",
                ),
                Err(error) => report.record("ownership INTERNAL", false, error),
                Ok(Some(_)) => {}
            }
            Products::Unit(Box::new(Unit {
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
        products,
        report,
    })
}

impl Observation {
    fn assess(&self) -> (bool, String) {
        let label = format!("ordinary-borrow-return/{:?}", self.entry);
        let observed = match &self.result {
            Ok(observed) => observed,
            Err(error) => {
                return (
                    false,
                    format!(
                        "{label}: BLOCKED {}: {}\nSOURCE {:?}",
                        error.stage, error.detail, SOURCE
                    ),
                );
            }
        };
        let fixture = &observed.fixture;
        let retained_source = fixture.lexed.source_id() == fixture.file.source_id();
        let (identity, phase2, internal, reached) = match &observed.products {
            Products::Single(product) => (
                product.raw.is_compatible_with_names(&product.names)
                    && product
                        .owned
                        .as_ref()
                        .ok()
                        .and_then(Option::as_ref)
                        .is_none_or(|owned| owned.is_compatible_with(&product.names, &product.raw)),
                product.raw.diagnostics().is_empty() && matches!(product.complete, Ok(true)),
                matches!(product.complete, Err(FinalTypeAdmissionError::Internal(_)))
                    || product.owned.is_err(),
                matches!(product.owned, Ok(Some(_))),
            ),
            Products::Unit(product) => {
                let inputs = [SourceUnitInput::new(
                    "root",
                    LOGICAL_PATH,
                    fixture.file.source_id(),
                    &fixture.file,
                )];
                (
                    product.raw.is_compatible_with(
                        &fixture.sources,
                        &inputs,
                        &product.names,
                        &observed.environment,
                    ),
                    product.raw.diagnostics().is_empty() && matches!(product.complete, Ok(true)),
                    matches!(product.complete, Err(FinalTypeAdmissionError::Internal(_)))
                        || product.owned.is_err(),
                    matches!(product.owned, Ok(Some(_))),
                )
            }
        };
        let passed = retained_source && identity && observed.report.passed();
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
                SOURCE,
                observed.report.checks,
                observed.report.details.join("\n")
            ),
        )
    }
}

#[test]
fn direct_borrow_return_source_keeps_unproven_continuation_boundary() {
    let observations = [Entry::Single, Entry::Unit].map(|entry| Observation {
        entry,
        result: analyze(entry),
    });
    let assessments = observations
        .iter()
        .map(Observation::assess)
        .collect::<Vec<_>>();
    for (_, report) in &assessments {
        println!("{report}");
    }
    assert_eq!(assessments.len(), 2);
    assert!(
        assessments.iter().all(|(passed, _)| *passed),
        "SPEC-0331: both ordinary pipelines must complete P2 before ownership rejects the actual borrow-return source call with L0164 and its real declaration marker label. Earlier rejection is not ownership evidence.\n{}",
        assessments
            .iter()
            .map(|(_, report)| report.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
