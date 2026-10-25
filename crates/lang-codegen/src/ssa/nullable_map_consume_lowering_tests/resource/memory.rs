//! Same-object original-single Unit observations. These are not compatibility expectations.

use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind},
};

use lang_frontend::{
    name_resolution::{
        SourceUnitId, SourceUnitInput, SymbolKind, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        CompilationUnitOwnership, NonNullAssertionTransferKind, check_compilation_unit_ownership,
        owned_compilation_unit_view,
    },
    type_checking::{
        CompilationUnitTypes, check_compilation_unit_types, complete_compilation_unit_types,
        standard_environments,
    },
};

use super::*;
use crate::ssa::map_original_observation_test_support::{Json, write_record};

mod records;
use records::*;

type Fields = Vec<(&'static str, Json)>;
type Catalog = Vec<(&'static str, usize)>;

#[derive(Clone, Copy)]
pub(super) struct Original<'a> {
    pub(super) id: &'a str,
    pub(super) path: &'a str,
    pub(super) text: &'a str,
    pub(super) sources: &'a SourceMap,
    pub(super) parsed: &'a ParsedFile,
}

struct CellState {
    trace: Trace,
    roots: Vec<Root>,
    plan_emitted: bool,
}

pub(super) struct Batch {
    whole: &'static str,
    expected_cells: usize,
    expected_roots: usize,
    sequences: BTreeMap<String, usize>,
    seen: BTreeSet<String>,
    legacy_seen: BTreeSet<String>,
    source_origins: BTreeMap<String, Vec<SourceOrigin>>,
    errors: Vec<String>,
    planned: usize,
    attempted: usize,
    terminals: usize,
}

impl Batch {
    pub(super) fn new(whole: &'static str, expected_cells: usize, expected_roots: usize) -> Self {
        let mut batch = Self {
            whole,
            expected_cells,
            expected_roots,
            sequences: BTreeMap::new(),
            seen: BTreeSet::new(),
            legacy_seen: BTreeSet::new(),
            source_origins: BTreeMap::new(),
            errors: Vec::new(),
            planned: 0,
            attempted: 0,
            terminals: 0,
        };
        batch.emit(
            "WHOLE_BEGIN",
            "",
            vec![
                ("expected_cells", Json::Number(expected_cells)),
                ("expected_function_roots", Json::Number(expected_roots)),
            ],
        );
        batch
    }

    fn emit(&mut self, event: &str, cell: &str, fields: Fields) {
        let sequence = self.sequences.entry(cell.to_owned()).or_default();
        if let Err(error) = write_record(
            event,
            self.whole,
            cell,
            "original-single",
            *sequence,
            &fields,
        ) {
            self.errors
                .push(format!("{cell}/{event}: stdout write failed: {error}"));
        }
        *sequence += 1;
    }

    pub(super) fn observe_parsed(
        &mut self,
        input: Original<'_>,
        assertion_count: usize,
        catalog: Catalog,
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
    ) {
        let Original {
            id,
            path,
            sources,
            parsed,
            ..
        } = input;
        let cell = format!("nullable:{id}:original-single");
        if !self.seen.insert(cell.clone()) {
            self.errors.push(format!("duplicate input {cell}"));
        }
        let actual_text = sources.source_text(parsed.source_id());
        if let Err(error) = &actual_text {
            self.errors
                .push(format!("{cell}: original source lookup failed: {error:?}"));
        }
        self.emit(
            "INPUT",
            &cell,
            vec![
                ("path", string(path)),
                (
                    "byte_len",
                    actual_text
                        .as_ref()
                        .map(|value| Json::Number(value.len()))
                        .unwrap_or(Json::Null),
                ),
                (
                    "source_text",
                    actual_text
                        .as_ref()
                        .map(|value| string(*value))
                        .unwrap_or(Json::Null),
                ),
                ("source_id", debug(parsed.source_id())),
                (
                    "lex_parse_status",
                    string("success_in_original_single_pipeline"),
                ),
                ("admission_kind", string("MemoryDocument")),
                ("source_map_same_object", Json::Bool(true)),
                ("parsed_same_object", Json::Bool(true)),
            ],
        );
        self.source_origins
            .insert(cell.clone(), source_origins(parsed));
        let mut state = CellState {
            trace: Trace::default(),
            roots: roots(sources, parsed),
            plan_emitted: false,
        };
        self.planned += state.roots.len();
        let result = catch_unwind(AssertUnwindSafe(|| {
            self.admit(
                &cell,
                input,
                assertion_count,
                (inspect, inspect_owned),
                &mut state,
            )
        }));
        let failed = match result {
            Ok(Ok(())) => None,
            Ok(Err(failure)) => {
                if failure.status == "observer_exception" {
                    self.errors
                        .push(format!("{cell}/{}: {}", failure.stage, failure.detail));
                }
                Some(failure)
            }
            Err(payload) => {
                let detail = panic_text(payload);
                self.errors
                    .push(format!("{cell}: observer panic: {detail}"));
                Some(internal("observer", detail))
            }
        };
        if !state.plan_emitted {
            self.plan(&cell, sources, parsed, &state.roots);
        }
        let failure_stage = failed.as_ref().map(|failure| failure.stage);
        if let Some(failure) = failed {
            self.emit(
                "ADMISSION",
                &cell,
                vec![
                    ("status", string(failure.status)),
                    ("kind", string(failure.kind)),
                    ("stage", string(failure.stage)),
                    ("detail", string(failure.detail)),
                    ("diagnostics", failure.diagnostics),
                    ("location", failure.location),
                    (
                        "roots_reached",
                        Json::Bool(state.roots.iter().any(|root| root.attempted)),
                    ),
                ],
            );
        }
        for root in &mut state.roots {
            if root.terminal {
                continue;
            }
            let mut fields = root.fields(sources);
            fields.extend([
                ("status", string("not_attempted")),
                ("reason", string(failure_stage.unwrap_or("ineligible root"))),
                ("lower_attempted", Json::Bool(false)),
                ("verify_reached", Json::Bool(false)),
                ("llvm_reached", Json::Bool(false)),
            ]);
            self.emit("ROOT_RESULT", &cell, fields);
            root.terminal = true;
            self.terminals += 1;
        }
        self.facts(&cell, &state.trace, &catalog);
        self.emit(
            "CELL_END",
            &cell,
            vec![
                ("expected_roots", Json::Number(state.roots.len())),
                (
                    "attempted_roots",
                    Json::Number(state.roots.iter().filter(|root| root.attempted).count()),
                ),
                (
                    "terminal_roots",
                    Json::Number(state.roots.iter().filter(|root| root.terminal).count()),
                ),
                (
                    "facts_failed",
                    Json::Number(state.trace.input_failures.len()),
                ),
                ("observation_only", Json::Bool(true)),
                ("cross_program_equivalence", Json::Bool(false)),
            ],
        );
    }

    fn plan(&mut self, cell: &str, sources: &SourceMap, parsed: &ParsedFile, roots: &[Root]) {
        self.emit(
            "ROOT_PLAN",
            cell,
            vec![
                (
                    "roots",
                    Json::Array(
                        roots
                            .iter()
                            .map(|root| Json::Object(root.fields(sources)))
                            .collect(),
                    ),
                ),
                ("source_inventory", source_inventory(sources, parsed)),
            ],
        );
    }

    fn admit<Inspect, InspectOwned>(
        &mut self,
        cell: &str,
        input: Original<'_>,
        assertion_count: usize,
        inspectors: (Inspect, InspectOwned),
        state: &mut CellState,
    ) -> Admission<()>
    where
        Inspect: FnOnce(
            &SourceMap,
            (SourceUnitId, &ParsedFile),
            &ValidatedCompilationUnitNames,
            &CompilationUnitTypes,
            &mut Trace,
        ),
        InspectOwned: FnOnce(
            &SourceMap,
            (SourceUnitId, &ParsedFile),
            &ValidatedCompilationUnitNames,
            &CompilationUnitTypes,
            &CompilationUnitOwnership,
            &mut Trace,
        ),
    {
        let Original {
            path,
            text,
            sources,
            parsed,
            ..
        } = input;
        let (inspect, inspect_owned) = inspectors;
        let CellState {
            trace,
            roots,
            plan_emitted,
        } = state;
        let inputs = [SourceUnitInput::memory_document(parsed.source_id(), parsed)];
        let same = std::ptr::eq(inputs[0].parsed(), parsed)
            && inputs[0].source_id() == parsed.source_id()
            && sources.source_name(parsed.source_id()).ok() == Some(path)
            && sources.source_text(parsed.source_id()).ok() == Some(text)
            && parsed.package().is_none();
        trace.check(
            "original MemoryDocument source identity",
            same,
            (parsed.source_id(), path),
        );
        identity(
            "source identity",
            same,
            "same borrowed ParsedFile, SourceMap, path, bytes and default package",
        )?;
        let (environment, types) = standard_environments();
        let index = internal_step("index", index_compilation_unit(sources, &inputs), trace)?;
        clean_diagnostics(sources, "index diagnostics", index.diagnostics(), trace)?;
        let names = internal_step(
            "names",
            resolve_compilation_unit_names(sources, &inputs, &index, &environment),
            trace,
        )?;
        clean_diagnostics(sources, "names diagnostics", names.diagnostics(), trace)?;
        let names = internal_step(
            "qualified names",
            names
                .validate()
                .map_err(|value| diagnostic_text(sources, value.diagnostics())),
            trace,
        )?;
        let [source_unit] = names.names().index().source_units() else {
            return Err(internal("source identity", "expected one real source unit"));
        };
        identity(
            "source unit identity",
            source_unit.source_id() == parsed.source_id(),
            source_unit,
        )?;
        for root in roots.iter_mut() {
            let candidates: Vec<_> = names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|declaration| {
                    declaration.kind() == SymbolKind::Function
                        && declaration.source_unit() == source_unit.id()
                        && declaration.root() == root.item
                        && declaration.name_span() == root.name_span
                        && declaration.name() == root.name
                        && !root.name_span.is_empty()
                        && names
                            .names()
                            .index()
                            .packages()
                            .get(declaration.package().index())
                            .is_some_and(|package| {
                                package
                                    .name()
                                    .is_some_and(|name| name.segments().is_empty())
                            })
                })
                .collect();
            let [declaration] = candidates.as_slice() else {
                return Err(internal(
                    "root identity",
                    format!("{root:?}; candidates={candidates:?}"),
                ));
            };
            root.declaration = Some(declaration.id());
            root.source_unit = Some(source_unit.id());
        }
        identity(
            "root selection",
            roots
                .iter()
                .filter(|root| root.name == "main" && root.parameters == 0 && root.eligible)
                .count()
                == 1,
            &roots,
        )?;
        self.plan(cell, sources, parsed, roots);
        *plan_emitted = true;
        let typed = internal_step(
            "raw types",
            check_compilation_unit_types(sources, &inputs, &names, &types),
            trace,
        )?;
        let raw = clean_diagnostics(sources, "raw type diagnostics", typed.diagnostics(), trace);
        inspect(sources, (source_unit.id(), parsed), &names, &typed, trace);
        self.emit(
            "FACT_RESULT",
            cell,
            vec![
                ("kind", string("source_qualified_identities")),
                ("declarations", debug(names.names().index().declarations())),
                (
                    "symbols",
                    Json::Array(
                        names
                            .names()
                            .source_units()
                            .iter()
                            .map(|unit| {
                                debug(
                                    unit.resolution()
                                        .symbols()
                                        .iter()
                                        .filter(|symbol| {
                                            !symbol.is_synthetic()
                                                && symbol.span().source_id() == parsed.source_id()
                                        })
                                        .collect::<Vec<_>>(),
                                )
                            })
                            .collect(),
                    ),
                ),
                (
                    "callable_signatures",
                    Json::Array(
                        roots
                            .iter()
                            .map(|root| {
                                debug((
                                    root.declaration,
                                    root.declaration
                                        .and_then(|id| typed.signatures().declaration(id)),
                                ))
                            })
                            .collect(),
                    ),
                ),
                (
                    "nominals",
                    Json::Array(
                        names
                            .names()
                            .index()
                            .declarations()
                            .iter()
                            .filter_map(|declaration| {
                                typed
                                    .signatures()
                                    .declaration(declaration.id())
                                    .and_then(|signature| signature.nominal())
                                    .map(debug)
                            })
                            .collect(),
                    ),
                ),
            ],
        );
        {
            let result = complete_compilation_unit_types(sources, &inputs, &names, &types, &typed);
            trace
                .admission_steps
                .push(("complete types".to_owned(), result.is_ok()));
            let complete = result.map_err(|error| complete_error(sources, error))?;
            let same = complete.is_same_analysis(&typed)
                && std::ptr::eq(complete.types(), &typed)
                && typed.is_compatible_with(sources, &inputs, &names, &types);
            trace.check(
                "same complete analysis and original facts",
                same,
                "same raw object and complete input chain",
            );
            identity(
                "complete analysis identity",
                same,
                "raw product or analysis mismatch",
            )?;
            clean_diagnostics(
                sources,
                "complete diagnostics",
                complete.types().diagnostics(),
                trace,
            )?;
        }
        raw?;
        let typed = internal_step(
            "qualified types",
            typed
                .validate()
                .map_err(|value| diagnostic_text(sources, value.diagnostics())),
            trace,
        )?;
        let owned = internal_step(
            "ownership",
            check_compilation_unit_ownership(sources, &inputs, &names, &types, &typed),
            trace,
        )?;
        let clean = clean_diagnostics(sources, "ownership diagnostics", owned.diagnostics(), trace);
        let complete_ownership = if owned.deferred().is_empty() {
            Ok(())
        } else {
            Err(AdmissionFailure {
                status: "admission_rejected",
                kind: "incomplete_capability",
                stage: "ownership completeness",
                detail: format!("explicit incomplete capability: {:?}", owned.deferred()),
                diagnostics: Json::Array(Vec::new()),
                location: Json::Null,
            })
        };
        trace.admission_steps.push((
            "ownership completeness".to_owned(),
            complete_ownership.is_ok(),
        ));
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
        let same = owned.is_compatible_with(&typed);
        trace.check(
            "ownership keeps original qualified types",
            same,
            owned.deferred(),
        );
        inspect_owned(
            sources,
            (source_unit.id(), parsed),
            &names,
            typed.types(),
            &owned,
            trace,
        );
        self.emit(
            "FACT_RESULT",
            cell,
            vec![
                ("kind", string("published_ownership_facts")),
                (
                    "drops",
                    Json::Array(
                        owned
                            .drops()
                            .iter()
                            .map(|drop| {
                                Json::Object(vec![
                                    ("point", debug(drop.point())),
                                    ("target", debug(drop.target())),
                                    (
                                        "value_origin",
                                        location(sources, drop.value_origin(), false),
                                    ),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("formed_owned_locals", debug(owned.formed_owned_locals())),
                ("non_null_assertions", debug(owned.non_null_assertions())),
                (
                    "deferred",
                    Json::Array(
                        owned
                            .deferred()
                            .iter()
                            .map(|deferred| {
                                let expression = deferred.expression();
                                let span = (expression.source_unit() == source_unit.id())
                                    .then(|| {
                                        parsed
                                            .ast()
                                            .expressions()
                                            .get(expression.expression())
                                            .ok()
                                            .map(|node| node.span())
                                    })
                                    .flatten();
                                Json::Object(vec![
                                    ("fact", debug(deferred)),
                                    (
                                        "location",
                                        span.map(|span| location(sources, span, true))
                                            .unwrap_or(Json::Null),
                                    ),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "typed_call_targets",
                    Json::Array(
                        typed
                            .types()
                            .calls()
                            .iter()
                            .map(|call| {
                                debug((call.expression(), call.target(), call.arguments().len()))
                            })
                            .collect(),
                    ),
                ),
            ],
        );
        clean?;
        complete_ownership?;
        identity(
            "ownership analysis identity",
            same,
            "typed/owned analysis mismatch",
        )?;
        let owned = internal_step(
            "qualified ownership",
            owned.validate().map_err(|value| {
                format!(
                    "{}; deferred={:?}",
                    diagnostic_text(sources, value.diagnostics()),
                    value.deferred()
                )
            }),
            trace,
        )?;
        trace.check("full ownership validator", true, "validate: Ok");
        let view = internal_step(
            "owned view",
            owned_compilation_unit_view(sources, &inputs, &names, &types, &typed, &owned),
            trace,
        )?;
        let same = std::ptr::eq(view.sources(), sources)
            && std::ptr::eq(view.inputs(), inputs.as_slice())
            && std::ptr::eq(view.names(), &names)
            && std::ptr::eq(view.types(), typed.types())
            && std::ptr::eq(view.ownership(), owned.ownership());
        trace.check(
            "owned view keeps all original references",
            same,
            "sources/inputs/names/types/ownership",
        );
        identity("owned view identity", same, "view references mismatch")?;
        self.emit(
            "ADMISSION",
            cell,
            vec![
                ("status", string("success")),
                ("stage", string("qualified_owned_view")),
                ("facts_failed", Json::Number(trace.input_failures.len())),
                ("full_ownership_validation", Json::Bool(true)),
                ("same_object_view", Json::Bool(same)),
            ],
        );
        for root in roots.iter_mut().filter(|root| root.eligible) {
            root.attempted = true;
            self.attempted += 1;
            let observed = catch_unwind(AssertUnwindSafe(|| {
                finish_root(sources, parsed, &view, root)
            }));
            let mut fields = root.fields(sources);
            match observed {
                Ok((result, integrity_error)) => {
                    fields.extend(result);
                    if let Some(error) = integrity_error {
                        self.errors.push(format!("{cell}: {error}"));
                    }
                }
                Err(payload) => {
                    let detail = panic_text(payload);
                    self.errors.push(format!("{cell}/{}: {detail}", root.name));
                    fields.extend([
                        ("status", string("observer_exception")),
                        ("detail", string(detail)),
                        ("lower_attempted", Json::Bool(true)),
                        ("verify_reached", Json::Null),
                        ("llvm_reached", Json::Null),
                    ]);
                }
            }
            self.emit("ROOT_RESULT", cell, fields);
            root.terminal = true;
            self.terminals += 1;
        }
        Ok(())
    }

    fn facts(&mut self, cell: &str, trace: &Trace, catalog: &Catalog) {
        let expected: Vec<_> = catalog
            .iter()
            .map(|(label, count)| {
                let actual = trace
                    .checks
                    .iter()
                    .filter(|check| check.label == *label)
                    .count();
                Json::Object(vec![
                    ("checkpoint", string(*label)),
                    ("expected", Json::Number(*count)),
                    ("reached", Json::Number(actual)),
                    ("not_reached", Json::Number(count.saturating_sub(actual))),
                ])
            })
            .collect();
        let checks = trace
            .checks
            .iter()
            .enumerate()
            .map(|(sequence, check)| {
                let mut fields = vec![
                    ("sequence", Json::Number(sequence)),
                    ("checkpoint", string(&check.label)),
                    ("passed", Json::Bool(check.passed)),
                ];
                if !check.passed || check.label.contains("formed-owned") {
                    fields.push(("detail", string(&check.detail)));
                }
                Json::Object(fields)
            })
            .collect();
        self.emit(
            "FACT_RESULT",
            cell,
            vec![
                ("kind", string("checkpoint_ledger")),
                ("catalog", Json::Array(expected)),
                ("checks", Json::Array(checks)),
                (
                    "admission_steps",
                    Json::Array(
                        trace
                            .admission_steps
                            .iter()
                            .map(|(stage, passed)| {
                                Json::Object(vec![
                                    ("stage", string(stage)),
                                    ("passed", Json::Bool(*passed)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("failed", strings(trace.input_failures.iter())),
                (
                    "facts_equivalent",
                    Json::Bool(
                        trace.input_failures.is_empty()
                            && catalog.iter().all(|(label, count)| {
                                trace
                                    .checks
                                    .iter()
                                    .filter(|check| check.label == *label)
                                    .count()
                                    == *count
                            }),
                    ),
                ),
            ],
        );
    }

    pub(super) fn legacy(&mut self, id: &str, trace: &Trace, outcome: &Outcome<LowerObservation>) {
        let cell = format!("nullable:{id}:original-single");
        self.legacy_seen.insert(cell.clone());
        let status = match outcome {
            Err(_) => "admission_rejected",
            Ok(value) => match value.outcome {
                LowerOutcome::VerifiedLlvm => "success",
                LowerOutcome::LowerRejected { .. } => "lower_rejected",
                LowerOutcome::VerifierRejected { .. } => "verify_rejected",
                LowerOutcome::LlvmRejected { .. } => "llvm_rejected",
            },
        };
        self.emit(
            "LEGACY_END",
            &cell,
            vec![
                ("observed_producer", string("Legacy")),
                ("status", string(status)),
                (
                    "program_inventory",
                    trace
                        .program_origins
                        .as_deref()
                        .map(inventory)
                        .unwrap_or(Json::Null),
                ),
                (
                    "source_coverage",
                    self.source_origins
                        .get(&cell)
                        .map(|expected| source_coverage(expected, trace.program_origins.as_deref()))
                        .unwrap_or(Json::Null),
                ),
                (
                    "detail",
                    if status == "success" {
                        Json::Null
                    } else {
                        debug(outcome)
                    },
                ),
                ("cross_program_equivalence", Json::Bool(false)),
            ],
        );
    }

    pub(super) fn finish(mut self, original_rows: usize) {
        if self.seen.len() != self.expected_cells
            || self.legacy_seen != self.seen
            || self.planned != self.expected_roots
            || self.terminals != self.planned
        {
            self.errors.push(format!(
                "incomplete capture: cells={}/{}, legacy={}, roots={}/{}, terminals={}",
                self.seen.len(),
                self.expected_cells,
                self.legacy_seen.len(),
                self.planned,
                self.expected_roots,
                self.terminals
            ));
        }
        self.emit(
            "WHOLE_END",
            "",
            vec![
                ("original_rows", Json::Number(original_rows)),
                ("unit_inputs", Json::Number(self.seen.len())),
                ("planned_function_roots", Json::Number(self.planned)),
                ("attempted_roots", Json::Number(self.attempted)),
                ("terminal_roots", Json::Number(self.terminals)),
                ("integrity_errors", strings(self.errors.iter())),
                ("legacy_assertions_completed", Json::Bool(true)),
            ],
        );
        assert!(
            self.errors.is_empty(),
            "original-single observation integrity failures: {:?}",
            self.errors
        );
    }
}

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(value) = payload.downcast_ref::<String>() {
        value.clone()
    } else if let Some(value) = payload.downcast_ref::<&str>() {
        (*value).to_owned()
    } else {
        "non-string panic payload".to_owned()
    }
}

pub(super) fn resource_catalog(removes: usize) -> Catalog {
    records::resource_catalog(removes)
}

pub(super) fn formed_catalog(id: &str, locals: usize) -> Catalog {
    records::formed_catalog(id, locals)
}
