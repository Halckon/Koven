//! Test-only observations of frozen original inputs. Never replaces a Legacy oracle.
use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
};

use super::model::{Operation, Origin, Program};
use crate::native_tests::boxed_enum_tests::unit_source::{
    FrontendReport, SourceRejectionKind, UnitSource,
};
use lang_frontend::{
    name_resolution::{DeclarationId, SourceUnitKey, SymbolKind},
    parser::{Expression, Item},
    source::{SourceMap, Span},
};

mod record;
pub(crate) use record::{Json, write_record};
use record::{diagnostic_json, span_json};

pub(super) struct Input<'a> {
    id: &'a str,
    text: &'a str,
    roots: &'a [&'a str],
    trusted_range: bool,
}
impl<'a> Input<'a> {
    pub(super) fn new(id: &'a str, text: &'a str, roots: &'a [&'a str]) -> Self {
        Self {
            id,
            text,
            roots,
            trusted_range: false,
        }
    }
    pub(super) fn trusted_range(id: &'a str, text: &'a str) -> Self {
        Self {
            id,
            text,
            roots: &["main"],
            trusted_range: true,
        }
    }
    fn path(&self) -> &'static str {
        if self.trusted_range {
            "trusted.ko"
        } else {
            "lowering.ko"
        }
    }
}

struct Root {
    name: String,
    declaration: DeclarationId,
    span: Span,
    parameters: usize,
}

type Portable = fn(&str, &UnitSource, &Program, &str) -> Option<bool>;

pub(super) struct Observation {
    ledger: &'static str,
    whole: String,
    sequences: BTreeMap<String, usize>,
    expected_cells: usize,
    completed_cells: usize,
    expected_roots: usize,
    completed_roots: usize,
    lower_calls: usize,
    verify_calls: usize,
    llvm_calls: usize,
    faults: Vec<String>,
}

pub(super) fn observe(ledger: &'static str, whole: &str, inputs: &[Input<'_>]) -> Observation {
    observe_with(ledger, whole, inputs, |_, _, _, _| None)
}

pub(super) fn observe_with(
    ledger: &'static str,
    whole: &str,
    inputs: &[Input<'_>],
    portable: Portable,
) -> Observation {
    let mut result = Observation {
        ledger,
        whole: whole
            .strip_prefix("lang_codegen::")
            .unwrap_or(whole)
            .to_owned(),
        sequences: BTreeMap::new(),
        expected_cells: inputs.len(),
        completed_cells: 0,
        expected_roots: inputs.iter().map(|input| input.roots.len()).sum(),
        completed_roots: 0,
        lower_calls: 0,
        verify_calls: 0,
        llvm_calls: 0,
        faults: Vec::new(),
    };
    result.record(
        "WHOLE_BEGIN",
        "",
        vec![
            ("expected_cells", result.expected_cells.into()),
            ("expected_roots", result.expected_roots.into()),
            (
                "ci_only",
                matches!(ledger, "M01" | "M04" | "M08" | "M19").into(),
            ),
        ],
    );
    for input in inputs {
        let roots_before = result.completed_roots;
        let observed = catch_unwind(AssertUnwindSafe(|| result.cell(input, portable)));
        if let Err(payload) = observed {
            result.exception(input.id, "cell_observer", panic_text(payload));
            let completed = result.completed_roots - roots_before;
            for root in input.roots.iter().skip(completed) {
                result.record(
                    "ROOT_RESULT",
                    input.id,
                    vec![
                        ("root_name", (*root).into()),
                        ("root_identity", Json::Null),
                        ("outcome", "not_reached".into()),
                        ("stage", "lower".into()),
                        ("blocker", "cell_observer_exception".into()),
                    ],
                );
                result.completed_roots += 1;
            }
            result.record(
                "CELL_END",
                input.id,
                vec![
                    ("outcome", "observer_exception".into()),
                    ("expected_roots", input.roots.len().into()),
                    (
                        "completed_roots",
                        (result.completed_roots - roots_before).into(),
                    ),
                ],
            );
            result.completed_cells += 1;
        }
    }
    result
}

impl Observation {
    fn record(&mut self, event: &str, cell: &str, mut fields: Vec<(&str, Json)>) {
        fields.push(("ledger_id", self.ledger.into()));
        fields.push(("admission_kind", "MemoryDocument".into()));
        fields.push((
            "observed_producer",
            if event == "LEGACY_END" {
                "Legacy"
            } else {
                "Unit"
            }
            .into(),
        ));
        let sequence = self.sequences.entry(cell.to_owned()).or_default();
        let write = write_record(
            event,
            &self.whole,
            cell,
            "original-single",
            *sequence,
            &fields,
        );
        *sequence += 1;
        if let Err(error) = write {
            self.faults
                .push(format!("record_io_error {event}/{cell}: {error}"));
        }
    }

    fn exception(&mut self, cell: &str, stage: &str, detail: String) {
        self.faults.push(format!("{cell}/{stage}: {detail}"));
        self.record(
            "FACT_RESULT",
            cell,
            vec![
                ("outcome", "observer_exception".into()),
                ("stage", stage.into()),
                ("detail", detail.into()),
            ],
        );
    }

    fn blocked(&mut self, input: &Input<'_>, blocker: &str) {
        self.record(
            "ROOT_PLAN",
            input.id,
            vec![
                ("actual_manifest", "unavailable".into()),
                (
                    "expected_roots",
                    Json::Array(input.roots.iter().map(|name| (*name).into()).collect()),
                ),
                ("blocker", blocker.into()),
            ],
        );
        for root in input.roots {
            self.record(
                "ROOT_RESULT",
                input.id,
                vec![
                    ("root_name", (*root).into()),
                    ("root_identity", Json::Null),
                    ("outcome", "not_reached".into()),
                    ("stage", "lower".into()),
                    ("blocker", blocker.into()),
                ],
            );
            self.completed_roots += 1;
        }
    }

    fn cell(&mut self, input: &Input<'_>, portable: Portable) {
        if self.sequences.contains_key(input.id) {
            self.exception(input.id, "input_identity", "duplicate cell ID".to_owned());
        }
        self.record(
            "INPUT",
            input.id,
            vec![
                ("logical_path", input.path().into()),
                ("source_text", input.text.into()),
                ("byte_len", input.text.len().into()),
                (
                    "expected_roots",
                    Json::Array(input.roots.iter().map(|name| (*name).into()).collect()),
                ),
            ],
        );
        let roots_before = self.completed_roots;
        let calls_before = (self.lower_calls, self.verify_calls, self.llvm_calls);
        let analysis = catch_unwind(AssertUnwindSafe(|| {
            UnitSource::analyze_with_type_environment(
                input.path(),
                input.text,
                |sources, source, types| {
                    if input.trusted_range {
                        let range = types.authorize_range_source(sources, source);
                        let extension = types.authorize_range_extension_source(sources, source);
                        self.record(
                            "FACT_RESULT",
                            input.id,
                            vec![
                                ("fact", "original_range_authorizations".into()),
                                ("source_id", format!("{source:?}").into()),
                                ("logical_path", sources.source_name(source).unwrap().into()),
                                ("range_ok", range.is_ok().into()),
                                ("range_extension_ok", extension.is_ok().into()),
                            ],
                        );
                        range.expect("original range source authorization");
                        extension.expect("original range extension authorization");
                    }
                },
            )
        }));
        match analysis {
            Err(payload) => {
                let detail = panic_text(payload);
                self.exception(input.id, "unit_source_analyze", detail);
                self.record(
                    "ADMISSION",
                    input.id,
                    vec![
                        ("outcome", "observer_exception".into()),
                        ("stage", "unit_source_analyze".into()),
                    ],
                );
                self.blocked(input, "unit_source_analyze_exception");
            }
            Ok(Err(rejected)) => {
                self.report(input.id, &rejected.sources, &rejected.report);
                let outcome = match &rejected.kind {
                    SourceRejectionKind::Diagnostics => "admission_rejected",
                    SourceRejectionKind::IncompleteBasicOwnership(facts) => {
                        self.record(
                            "FACT_RESULT",
                            input.id,
                            vec![
                                ("fact", "incomplete_basic_ownership".into()),
                                (
                                    "deferred",
                                    Json::Array(
                                        facts
                                            .owned
                                            .deferred()
                                            .iter()
                                            .map(|fact| {
                                                Json::Object(vec![
                                                    (
                                                        "expression",
                                                        format!("{:?}", fact.expression()).into(),
                                                    ),
                                                    (
                                                        "reason",
                                                        format!("{:?}", fact.reason()).into(),
                                                    ),
                                                ])
                                            })
                                            .collect(),
                                    ),
                                ),
                            ],
                        );
                        "incomplete_basic_ownership"
                    }
                };
                let stage = rejected
                    .report
                    .stages
                    .last()
                    .map_or("unknown", |(stage, _)| *stage);
                self.record(
                    "ADMISSION",
                    input.id,
                    vec![
                        ("outcome", outcome.into()),
                        (
                            "stage",
                            if stage == "types" {
                                "types_substage_unknown"
                            } else {
                                stage
                            }
                            .into(),
                        ),
                    ],
                );
                self.blocked(input, outcome);
            }
            Ok(Ok(analysis)) => {
                self.report(input.id, analysis.sources(), &analysis.report);
                self.record("ADMISSION", input.id, vec![("outcome", "success".into())]);
                let manifest = catch_unwind(AssertUnwindSafe(|| self.roots(input, &analysis)));
                match manifest {
                    Ok(roots) => {
                        for root in roots {
                            let mut stage = "lower";
                            let observed = catch_unwind(AssertUnwindSafe(|| {
                                self.root(input, &analysis, &root, portable, &mut stage)
                            }));
                            if let Err(payload) = observed {
                                self.exception(input.id, stage, panic_text(payload));
                                self.record(
                                    "ROOT_RESULT",
                                    input.id,
                                    vec![
                                        ("root_name", root.name.into()),
                                        ("outcome", "observer_exception".into()),
                                        ("stage", stage.into()),
                                    ],
                                );
                            }
                            self.completed_roots += 1;
                        }
                    }
                    Err(payload) => {
                        self.exception(input.id, "root_identity", panic_text(payload));
                        self.blocked(input, "root_identity_exception");
                    }
                }
            }
        }
        self.record(
            "CELL_END",
            input.id,
            vec![
                ("expected_roots", input.roots.len().into()),
                (
                    "completed_roots",
                    (self.completed_roots - roots_before).into(),
                ),
                ("lower_calls", (self.lower_calls - calls_before.0).into()),
                ("verify_calls", (self.verify_calls - calls_before.1).into()),
                ("llvm_calls", (self.llvm_calls - calls_before.2).into()),
            ],
        );
        self.completed_cells += 1;
    }

    fn report(&mut self, cell: &str, sources: &SourceMap, report: &FrontendReport) {
        for (stage, count) in &report.stages {
            self.record(
                "FACT_RESULT",
                cell,
                vec![
                    ("fact", "frontend_stage".into()),
                    ("stage", (*stage).into()),
                    ("diagnostic_count", (*count).into()),
                    (
                        "type_substage",
                        if *stage == "types" {
                            "types_substage_unknown"
                        } else {
                            "not_applicable"
                        }
                        .into(),
                    ),
                ],
            );
        }
        for (stage, diagnostic) in &report.diagnostics {
            self.record(
                "FACT_RESULT",
                cell,
                vec![
                    ("fact", "diagnostic".into()),
                    ("stage", (*stage).into()),
                    ("diagnostic", diagnostic_json(sources, diagnostic)),
                ],
            );
        }
    }

    fn roots(&mut self, input: &Input<'_>, analysis: &UnitSource) -> Vec<Root> {
        let index = analysis.names().names().index();
        let [source] = index.source_units() else {
            panic!("one original memory source")
        };
        assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
        assert_eq!(source.source_id(), analysis.parsed().source_id());
        assert_eq!(
            analysis.sources().source_text(source.source_id()).unwrap(),
            input.text
        );
        assert_eq!(
            analysis.sources().source_name(source.source_id()).unwrap(),
            input.path()
        );
        let mut roots = Vec::new();
        let mut declarations = index.declarations().iter().collect::<Vec<_>>();
        declarations.sort_by_key(|declaration| declaration.name_span().start());
        for declaration in declarations {
            assert_eq!(declaration.source_unit(), source.id());
            assert_eq!(declaration.package(), source.package());
            assert!(analysis.parsed().roots().contains(&declaration.root()));
            let span = analysis.declaration_span(declaration.id());
            let callable = analysis
                .types()
                .signatures()
                .declaration(declaration.id())
                .and_then(|signature| signature.callable());
            let eligible = declaration.kind() == SymbolKind::Function
                && callable.is_some_and(|signature| signature.type_parameters().is_empty());
            self.record(
                "ROOT_PLAN",
                input.id,
                vec![
                    ("root_name", declaration.name().into()),
                    ("declaration_id", format!("{:?}", declaration.id()).into()),
                    (
                        "source_unit_id",
                        format!("{:?}", declaration.source_unit()).into(),
                    ),
                    ("ast_item_id", format!("{:?}", declaration.root()).into()),
                    ("kind", format!("{:?}", declaration.kind()).into()),
                    ("span", span_json(analysis.sources(), span, false)),
                    (
                        "name_span",
                        span_json(analysis.sources(), declaration.name_span(), false),
                    ),
                    ("eligible", eligible.into()),
                    (
                        "parameter_count",
                        callable
                            .map_or(Json::Null, |signature| signature.parameters().len().into()),
                    ),
                    (
                        "type_parameter_count",
                        callable.map_or(Json::Null, |signature| {
                            signature.type_parameters().len().into()
                        }),
                    ),
                    (
                        "coverage_gap",
                        if eligible {
                            Json::Null
                        } else {
                            "no_independent_non_generic_function_entry".into()
                        },
                    ),
                ],
            );
            if eligible {
                roots.push(Root {
                    name: declaration.name().to_owned(),
                    declaration: declaration.id(),
                    span,
                    parameters: callable.unwrap().parameters().len(),
                });
            }
        }
        assert_eq!(
            roots
                .iter()
                .map(|root| root.name.as_str())
                .collect::<Vec<_>>(),
            input.roots,
            "frozen original function-root names and order"
        );
        for (id, item) in analysis.parsed().ast().items().iter() {
            if matches!(item.payload(), Item::Deinit { .. }) {
                self.record(
                    "ROOT_PLAN",
                    input.id,
                    vec![
                        ("ast_item_id", format!("{id:?}").into()),
                        ("kind", "deinit".into()),
                        ("span", span_json(analysis.sources(), item.span(), false)),
                        ("eligible", false.into()),
                        ("coverage_gap", "no_public_function_entry".into()),
                    ],
                );
            }
        }
        for (id, expression) in analysis.parsed().ast().expressions().iter() {
            if matches!(expression.payload(), Expression::Lambda { .. }) {
                self.record(
                    "ROOT_PLAN",
                    input.id,
                    vec![
                        ("expression_id", format!("{id:?}").into()),
                        ("kind", "lambda".into()),
                        (
                            "span",
                            span_json(analysis.sources(), expression.span(), false),
                        ),
                        ("eligible", false.into()),
                        ("coverage_gap", "no_public_function_entry".into()),
                    ],
                );
            }
        }
        self.record("FACT_RESULT", input.id, record::ownership_fields(analysis));
        roots
    }

    fn root(
        &mut self,
        input: &Input<'_>,
        analysis: &UnitSource,
        root: &Root,
        portable: Portable,
        stage: &mut &'static str,
    ) {
        let name = root.name.as_str();
        let declaration = root.declaration;
        let span = root.span;
        let parameters = root.parameters;
        self.lower_calls += 1;
        let (program, entry) = match analysis.lower_entry(declaration) {
            Ok(result) => result,
            Err(error) => {
                self.record(
                    "ROOT_RESULT",
                    input.id,
                    vec![
                        ("root_name", name.into()),
                        ("declaration_id", format!("{declaration:?}").into()),
                        ("outcome", "lower_rejected".into()),
                        ("stage", "lower".into()),
                        ("kind", format!("{:?}", error.kind).into()),
                        (
                            "span",
                            error.span.map_or(Json::Null, |span| {
                                span_json(analysis.sources(), span, true)
                            }),
                        ),
                        ("verify", "not_reached_no_program_returned".into()),
                        ("llvm", "not_reached".into()),
                    ],
                );
                return;
            }
        };
        *stage = "root_program_identity";
        let function = program
            .module(entry.module())
            .and_then(|module| module.function(entry))
            .expect("real entry function");
        assert_eq!(function.origin, Origin::Source(span));
        self.program_inventory(input.id, name, analysis.sources(), &program);
        self.dependent_coverage(input.id, name, analysis, &program);
        *stage = "verify";
        self.verify_calls += 1;
        if let Err(errors) = super::verify::verify_program(&program) {
            self.record(
                "ROOT_RESULT",
                input.id,
                vec![
                    ("root_name", name.into()),
                    ("outcome", "verify_rejected".into()),
                    ("stage", "verify".into()),
                    ("lower", "success".into()),
                    (
                        "errors",
                        Json::Array(
                            errors
                                .errors
                                .iter()
                                .map(|error| {
                                    Json::Object(vec![
                                        ("kind", format!("{:?}", error.kind).into()),
                                        ("location", format!("{:?}", error.location).into()),
                                        (
                                            "origin",
                                            error.origin.as_ref().map_or(Json::Null, |origin| {
                                                span_json(analysis.sources(), origin.span(), true)
                                            }),
                                        ),
                                    ])
                                })
                                .collect(),
                        ),
                    ),
                    ("llvm", "not_reached".into()),
                ],
            );
            return;
        }
        *stage = "llvm";
        self.llvm_calls += 1;
        let with_entry = name == "main" && parameters == 0;
        let llvm = if with_entry {
            crate::llvm::render_verified_program_with_entry(&program, entry)
        } else {
            crate::llvm::render_verified_program(&program)
        };
        let mode = if with_entry {
            "original_main_entry"
        } else {
            "no_entry_full_program"
        };
        match llvm {
            Err(error) => self.record(
                "ROOT_RESULT",
                input.id,
                vec![
                    ("root_name", name.into()),
                    ("outcome", "llvm_rejected".into()),
                    ("stage", "llvm".into()),
                    ("lower", "success".into()),
                    ("verify", "success".into()),
                    ("llvm_mode", mode.into()),
                    ("kind", llvm_error_kind(&error).into()),
                    ("detail", format!("{error:?}").into()),
                ],
            ),
            Ok(llvm) => {
                *stage = "portable_oracle";
                let portable = catch_unwind(AssertUnwindSafe(|| {
                    portable(input.id, analysis, &program, name)
                }));
                let (portable_status, detail) = match portable {
                    Ok(None) => ("not_applicable", Json::Null),
                    Ok(Some(true)) => ("success", Json::Null),
                    Ok(Some(false)) => ("portable_oracle_mismatch", Json::Null),
                    Err(payload) => {
                        let detail = panic_text(payload);
                        self.exception(input.id, "portable_oracle", detail.clone());
                        ("observer_exception", detail.into())
                    }
                };
                self.record(
                    "ROOT_RESULT",
                    input.id,
                    vec![
                        ("root_name", name.into()),
                        ("declaration_id", format!("{declaration:?}").into()),
                        ("entry_function", format!("{entry:?}").into()),
                        (
                            "outcome",
                            match portable_status {
                                "portable_oracle_mismatch" => "portable_oracle_mismatch",
                                "observer_exception" => "observer_exception",
                                _ => "success",
                            }
                            .into(),
                        ),
                        (
                            "stage",
                            if portable_status == "observer_exception" {
                                "portable_oracle"
                            } else {
                                "complete"
                            }
                            .into(),
                        ),
                        ("lower", "success".into()),
                        ("verify", "success".into()),
                        ("llvm", "success".into()),
                        ("llvm_mode", mode.into()),
                        ("llvm_bytes", llvm.len().into()),
                        ("portable_oracle", portable_status.into()),
                        ("portable_detail", detail),
                    ],
                );
            }
        }
    }

    fn program_inventory(
        &mut self,
        cell: &str,
        root: &str,
        sources: &SourceMap,
        program: &Program,
    ) {
        let functions = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .map(|function| {
                let (kind, reason) = match &function.origin {
                    Origin::Source(_) => ("source", Json::Null),
                    Origin::Synthetic { reason, .. } => ("synthetic", reason.as_str().into()),
                };
                Json::Object(vec![
                    ("function_id", format!("{:?}", function.id).into()),
                    ("name", function.name.as_str().into()),
                    ("origin_kind", kind.into()),
                    ("origin", span_json(sources, function.origin.span(), false)),
                    ("synthetic_reason", reason),
                    ("blocks", function.blocks.len().into()),
                    ("instructions", function.instructions.len().into()),
                    (
                        "direct_calls",
                        Json::Array(
                            function
                                .instructions
                                .iter()
                                .filter_map(|instruction| {
                                    if let Operation::DirectCall { callee, .. } =
                                        instruction.operation
                                    {
                                        Some(Json::Object(vec![
                                            (
                                                "instruction_id",
                                                format!("{:?}", instruction.id).into(),
                                            ),
                                            ("callee", format!("{callee:?}").into()),
                                            (
                                                "origin",
                                                span_json(
                                                    sources,
                                                    instruction.origin.span(),
                                                    false,
                                                ),
                                            ),
                                        ]))
                                    } else {
                                        None
                                    }
                                })
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        self.record(
            "FACT_RESULT",
            cell,
            vec![
                ("root_name", root.into()),
                ("fact", "actual_program_dependencies".into()),
                ("function_count", functions.len().into()),
                ("functions", Json::Array(functions)),
                (
                    "coverage_scope",
                    "this_root_program_only_not_merged_whole_file".into(),
                ),
            ],
        );
    }

    fn dependent_coverage(
        &mut self,
        cell: &str,
        root: &str,
        analysis: &UnitSource,
        program: &Program,
    ) {
        let mut candidates = Vec::new();
        for (id, item) in analysis.parsed().ast().items().iter() {
            let kind = match item.payload() {
                Item::Function {
                    type_parameters, ..
                } if !type_parameters.is_empty() => "generic_template",
                Item::Deinit { .. } => "deinit",
                Item::Classifier(_) => "classifier",
                _ => continue,
            };
            candidates.push((kind, format!("{id:?}"), item.span()));
        }
        for (id, expression) in analysis.parsed().ast().expressions().iter() {
            if matches!(expression.payload(), Expression::Lambda { .. }) {
                candidates.push(("lambda", format!("{id:?}"), expression.span()));
            }
        }
        let coverage = candidates
            .into_iter()
            .map(|(kind, identity, span)| {
                let functions = program
                    .modules
                    .iter()
                    .flat_map(|module| &module.functions)
                    .filter(|function| function.origin == Origin::Source(span))
                    .map(|function| format!("{:?}", function.id).into())
                    .collect::<Vec<_>>();
                Json::Object(vec![
                    ("kind", kind.into()),
                    ("ast_identity", identity.into()),
                    ("span", span_json(analysis.sources(), span, false)),
                    (
                        "coverage_gap",
                        if functions.is_empty() {
                            "no_exact_source_function_in_this_program".into()
                        } else {
                            Json::Null
                        },
                    ),
                    ("exact_origin_functions", Json::Array(functions)),
                    ("independent_entry", false.into()),
                ])
            })
            .collect();
        self.record(
            "FACT_RESULT",
            cell,
            vec![
                ("root_name", root.into()),
                ("fact", "dependent_source_coverage".into()),
                ("dependencies", Json::Array(coverage)),
                (
                    "scope",
                    "exact_source_origins_only_not_independent_template_or_classifier_execution"
                        .into(),
                ),
            ],
        );
    }

    /// Call only after all original assertions have returned normally, never inside catch_unwind.
    pub(super) fn finish(mut self) {
        self.record(
            "LEGACY_END",
            "",
            vec![("outcome", "original_assertions_completed".into())],
        );
        if self.completed_cells != self.expected_cells
            || self.completed_roots != self.expected_roots
        {
            self.faults
                .push("cell/root terminal count mismatch".to_owned());
        }
        self.record(
            "WHOLE_END",
            "",
            vec![
                ("expected_cells", self.expected_cells.into()),
                ("completed_cells", self.completed_cells.into()),
                ("expected_roots", self.expected_roots.into()),
                ("completed_roots", self.completed_roots.into()),
                ("lower_calls", self.lower_calls.into()),
                ("verify_calls", self.verify_calls.into()),
                ("llvm_calls", self.llvm_calls.into()),
                ("integrity_errors", self.faults.len().into()),
            ],
        );
        assert!(
            self.faults.is_empty(),
            "original-input observation integrity: {:?}",
            self.faults
        );
    }
}

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn llvm_error_kind(error: &crate::llvm::LlvmAdapterError) -> &'static str {
    use crate::llvm::LlvmAdapterError;
    match error {
        LlvmAdapterError::InvalidSsa(_) => "InvalidSsa",
        LlvmAdapterError::InvalidEntry(_) => "InvalidEntry",
        LlvmAdapterError::Debug(_) => "Debug",
        LlvmAdapterError::InvalidLayout(_) => "InvalidLayout",
        LlvmAdapterError::Object(_) => "Object",
        LlvmAdapterError::Target(_) => "Target",
        LlvmAdapterError::Build(_) => "Build",
        LlvmAdapterError::Unsupported(_) => "Unsupported",
        LlvmAdapterError::Verify(_) => "Verify",
    }
}
