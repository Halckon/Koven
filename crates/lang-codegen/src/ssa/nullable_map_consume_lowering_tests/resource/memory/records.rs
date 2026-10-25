//! Record values, source-origin inventories and isolated per-root SSA/LLVM observations.

use super::{
    Catalog, Fields, Json, ParsedFile, ProgramOrigin, SourceMap, Span, Trace, program_origins,
};
use crate::ssa::model::Origin;
use lang_frontend::{
    ast::ItemId,
    diagnostic::{Diagnostic, DiagnosticDetail},
    name_resolution::{DeclarationId, SourceUnitId},
    ownership_checking::OwnedCompilationUnitView,
    parser::{Expression, Item, NameMarker},
    type_checking::{FinalTypeAdmissionError, FinalTypeAdmissionInternalError},
};

pub(super) fn string(value: impl Into<String>) -> Json {
    Json::String(value.into())
}
pub(super) fn debug(value: impl std::fmt::Debug) -> Json {
    string(format!("{value:?}"))
}
pub(super) fn strings(values: impl IntoIterator<Item = impl Into<String>>) -> Json {
    Json::Array(values.into_iter().map(string).collect())
}
pub(super) fn location(sources: &SourceMap, span: Span, with_text: bool) -> Json {
    let mut fields = vec![
        ("source_id", debug(span.source_id())),
        ("start", Json::Number(span.start())),
        ("end", Json::Number(span.end())),
        (
            "path",
            sources
                .source_name(span.source_id())
                .map(string)
                .unwrap_or(Json::Null),
        ),
        (
            "path_lookup_error",
            sources
                .source_name(span.source_id())
                .err()
                .map(debug)
                .unwrap_or(Json::Null),
        ),
    ];
    if with_text {
        fields.push((
            "text",
            sources.slice(span).map(string).unwrap_or(Json::Null),
        ));
        fields.push((
            "text_lookup_error",
            sources.slice(span).err().map(debug).unwrap_or(Json::Null),
        ));
    }
    Json::Object(fields)
}

#[derive(Debug)]
pub(super) struct Root {
    pub(super) item: ItemId,
    pub(super) span: Span,
    pub(super) name_span: Span,
    pub(super) name: String,
    pub(super) parameters: usize,
    pub(super) parameter_syntax: String,
    pub(super) eligible: bool,
    pub(super) declaration: Option<DeclarationId>,
    pub(super) source_unit: Option<SourceUnitId>,
    pub(super) terminal: bool,
    pub(super) attempted: bool,
}

impl Root {
    pub(super) fn fields(&self, sources: &SourceMap) -> Fields {
        vec![
            (
                "root_id",
                string(format!(
                    "{}@{}..{}",
                    self.name,
                    self.span.start(),
                    self.span.end()
                )),
            ),
            ("root_name", string(&self.name)),
            (
                "declaration_id",
                self.declaration.map(debug).unwrap_or(Json::Null),
            ),
            (
                "source_unit_id",
                self.source_unit.map(debug).unwrap_or(Json::Null),
            ),
            ("item", debug(self.item)),
            ("item_span", location(sources, self.span, false)),
            ("name_span", location(sources, self.name_span, true)),
            ("parameters", Json::Number(self.parameters)),
            ("parameter_syntax", string(&self.parameter_syntax)),
            ("eligible", Json::Bool(self.eligible)),
            (
                "llvm_mode",
                string(if self.name == "main" {
                    "with_entry"
                } else {
                    "no_entry"
                }),
            ),
            (
                "consumer_roles",
                strings(if self.name == "main" {
                    vec!["explicit_main", "all_real_function_roots"]
                } else {
                    vec!["all_real_function_roots"]
                }),
            ),
        ]
    }
}

pub(super) fn roots(sources: &SourceMap, parsed: &ParsedFile) -> Vec<Root> {
    let mut roots = Vec::new();
    for &item in parsed.roots() {
        let Ok(node) = parsed.ast().items().get(item) else {
            continue;
        };
        let Item::Function {
            name: NameMarker::Present(name),
            parameters,
            type_parameters,
            extension_receiver,
            ..
        } = node.payload()
        else {
            continue;
        };
        roots.push(Root {
            item,
            span: node.span(),
            name_span: *name,
            name: sources
                .slice(*name)
                .unwrap_or("<source lookup failed>")
                .to_owned(),
            parameters: parameters.len(),
            parameter_syntax: format!("{parameters:?}"),
            eligible: type_parameters.is_empty() && extension_receiver.is_none(),
            declaration: None,
            source_unit: None,
            terminal: false,
            attempted: false,
        });
    }
    roots.sort_by_key(|root| (root.span.start(), root.span.end()));
    roots
}

#[derive(Debug)]
pub(super) struct SourceOrigin {
    span: Span,
    kind: &'static str,
    id: String,
    independent_entry: bool,
}

pub(super) fn source_origins(parsed: &ParsedFile) -> Vec<SourceOrigin> {
    let items = parsed.ast().items().iter().filter_map(|(id, node)| {
        let kind = match node.payload() {
            Item::Function { .. } => "function",
            Item::Classifier(_) => "nominal",
            Item::Deinit { .. } => "deinit",
            _ => return None,
        };
        Some(SourceOrigin {
            span: node.span(),
            kind,
            id: format!("{id:?}"),
            independent_entry: kind == "function" && parsed.roots().contains(&id),
        })
    });
    let lambdas = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Expression::Lambda { .. }))
        .map(|(id, node)| SourceOrigin {
            span: node.span(),
            kind: "lambda",
            id: format!("{id:?}"),
            independent_entry: false,
        });
    let mut entries: Vec<_> = items.chain(lambdas).collect();
    entries.sort_by_key(|entry| (entry.span.start(), entry.span.end()));
    entries
}

pub(super) fn source_inventory(sources: &SourceMap, parsed: &ParsedFile) -> Json {
    Json::Array(source_origins(parsed).into_iter().map(|entry| Json::Object(vec![
        ("kind", string(entry.kind)), ("id", string(entry.id)),
        ("span", location(sources, entry.span, false)),
        ("independent_entry", Json::Bool(entry.independent_entry)),
        ("coverage_gap", if entry.independent_entry { Json::Null } else {
            string("no legal independent Function entry; source facts do not prove lowering coverage")
        }),
    ])).collect())
}

pub(super) fn source_coverage(expected: &[SourceOrigin], actual: Option<&[ProgramOrigin]>) -> Json {
    Json::Array(expected.iter().map(|entry| {
        let emitted = actual.map(|values| values.iter().filter(|value| {
            value.span == entry.span && value.synthetic.is_none()
        }).map(|value| value.id.clone()).collect::<Vec<_>>());
        let anchors = actual.map(|values| values.iter().filter(|value| {
            value.span == entry.span && value.synthetic.is_some()
        }).map(|value| value.id.clone()).collect::<Vec<_>>());
        let state = match emitted.as_deref() {
            None => "program_unavailable",
            Some([]) => "not_emitted_at_exact_source_origin",
            Some([_]) => "one_exact_source_origin",
            Some(_) => "multiple_exact_source_origins",
        };
        Json::Object(vec![
            ("kind", string(entry.kind)), ("source_identity", string(&entry.id)),
            ("source_id", debug(entry.span.source_id())),
            ("start", Json::Number(entry.span.start())), ("end", Json::Number(entry.span.end())),
            ("state", string(state)),
            ("exact_function_origins", emitted.map(strings).unwrap_or(Json::Null)),
            ("synthetic_function_anchors", anchors.map(strings).unwrap_or(Json::Null)),
            ("coverage_gap", if entry.independent_entry && state == "one_exact_source_origin" {
                Json::Null
            } else {
                string("not proof of independent lowering or equivalence to the original whole-file Program")
            }),
        ])
    }).collect())
}

pub(super) fn inventory(origins: &[ProgramOrigin]) -> Json {
    Json::Array(
        origins
            .iter()
            .map(|value| {
                Json::Object(vec![
                    ("function_id", string(&value.id)),
                    ("name", string(&value.name)),
                    ("source_id", debug(value.span.source_id())),
                    ("start", Json::Number(value.span.start())),
                    ("end", Json::Number(value.span.end())),
                    (
                        "path",
                        value.path.as_ref().map(string).unwrap_or(Json::Null),
                    ),
                    (
                        "path_lookup_error",
                        value.path.as_ref().err().map(string).unwrap_or(Json::Null),
                    ),
                    (
                        "synthetic_reason",
                        value.synthetic.as_ref().map(string).unwrap_or(Json::Null),
                    ),
                    ("direct_calls", strings(value.direct_calls.iter())),
                ])
            })
            .collect(),
    )
}

pub(super) fn finish_root(
    sources: &SourceMap,
    parsed: &ParsedFile,
    view: &OwnedCompilationUnitView<'_, '_>,
    root: &Root,
) -> (Fields, Option<String>) {
    let Some(declaration) = root.declaration else {
        return (
            vec![
                ("status", string("observer_exception")),
                ("detail", string("missing real declaration")),
            ],
            Some("qualified root lost declaration identity".to_owned()),
        );
    };
    let mut fields = vec![("lower_attempted", Json::Bool(true))];
    let (program, entry) =
        match crate::ssa::unit_lower::lower_owned_unit_with_entry(view, declaration) {
            Ok(value) => value,
            Err(error) => {
                fields.extend([
                    ("status", string("lower_rejected")),
                    ("stage", string("lower")),
                    ("kind", debug(error.kind)),
                    ("detail", debug(error)),
                    (
                        "location",
                        error
                            .span
                            .map(|span| location(sources, span, true))
                            .unwrap_or(Json::Null),
                    ),
                    ("verify_reached", Json::Bool(false)),
                    ("llvm_reached", Json::Bool(false)),
                    ("program_available", Json::Bool(false)),
                    (
                        "source_coverage",
                        source_coverage(&source_origins(parsed), None),
                    ),
                    (
                        "internal_verifier_details",
                        string("not exposed by the public lowering result"),
                    ),
                ]);
                return (fields, None);
            }
        };
    let candidates: Vec<_> = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == Origin::Source(root.span))
        .collect();
    let same = matches!(candidates.as_slice(), [function] if function.id == entry);
    let integrity = (!same).then(|| {
        format!(
            "{}: returned entry {entry:?} does not uniquely match {:?}",
            root.name, root.span
        )
    });
    let actual_origins = program_origins(sources, &program);
    fields.extend([
        ("entry", debug(entry)),
        ("entry_matches_selected_source", Json::Bool(same)),
        ("program_available", Json::Bool(true)),
        ("program_inventory", inventory(&actual_origins)),
        (
            "source_coverage",
            source_coverage(&source_origins(parsed), Some(&actual_origins)),
        ),
        ("verify_reached", Json::Bool(true)),
    ]);
    if let Err(error) = crate::ssa::verify::verify_program(&program) {
        fields.extend([
            ("status", string("verify_rejected")),
            ("stage", string("verify")),
            ("detail", debug(error)),
            ("llvm_reached", Json::Bool(false)),
        ]);
        return (fields, integrity);
    }
    let llvm = if root.name == "main" {
        crate::llvm::render_verified_program_with_entry(&program, entry)
    } else {
        crate::llvm::render_verified_program(&program)
    };
    fields.push(("llvm_reached", Json::Bool(true)));
    match llvm {
        Ok(llvm) => fields.extend([
            (
                "status",
                string(if same {
                    "success"
                } else {
                    "observer_exception"
                }),
            ),
            ("stage", string("LLVM")),
            ("llvm_byte_len", Json::Number(llvm.len())),
        ]),
        Err(error) => fields.extend([
            ("status", string("llvm_rejected")),
            ("stage", string("LLVM")),
            ("detail", debug(error)),
        ]),
    }
    (fields, integrity)
}

fn common_catalog() -> Catalog {
    vec![
        ("original MemoryDocument source identity", 1),
        ("same complete analysis and original facts", 1),
        ("real qualified assertion ownership plans", 1),
        ("ownership keeps original qualified types", 1),
        ("full ownership validator", 1),
        ("owned view keeps all original references", 1),
    ]
}

pub(super) fn resource_catalog(removes: usize) -> Catalog {
    let mut catalog = common_catalog();
    catalog.extend([
        (
            "actual source-qualified Resource nominal and deinit owner/receiver",
            1,
        ),
        ("precise Resource inner and Nullable Map classifications", 1),
        (
            "ordinary nullable declaration presence and exact storage",
            1,
        ),
        ("resource-specific descriptor counts", 1),
        ("all resource descriptors belong to the actual provider", 1),
        (
            "actual inner/outer/got roots and nested Map storage",
            usize::from(removes > 0),
        ),
        (
            "actual inner and outer construction identities",
            usize::from(removes > 0),
        ),
        (
            "real inner Resource put and outer inner-Map put",
            usize::from(removes > 0),
        ),
        (
            "real outer remove payload/result and assertion operand",
            removes,
        ),
        ("real got.size read", usize::from(removes > 0)),
    ]);
    catalog
}

pub(super) fn formed_catalog(id: &str, locals: usize) -> Catalog {
    let mut catalog = common_catalog();
    catalog.extend([
        ("one exact provider source-qualified name resolution", 1),
        ("formation exact source identity", 2),
        (
            "exact ordinary Function Item identities and zero-argument main",
            2,
        ),
        ("exact number of local declarations", 2),
        (
            "declaration NameMarker selects one exact variable symbol",
            2 * locals,
        ),
        (
            "nearest callable scope matches exact Function Item or Lambda expression",
            2 * locals,
        ),
        ("exact local initializer syntax", 2 * locals),
        ("exact control/source arrangement", 2),
        ("same source-qualified body symbol identity", locals),
        (
            "formation exact qualified descriptor counts, no result pre-registration",
            1,
        ),
        ("exact qualified local storage and declared type", locals),
        (
            "initializer has exact qualified payload and source identity",
            locals,
        ),
        (
            "exact formed-owned local history from ordinary source-qualified symbols",
            1,
        ),
        ("ordinary formed-owned local facts are sorted and unique", 1),
        (
            "uninvoked action has no name references",
            if id == "K4" { 2 } else { 0 },
        ),
        (
            "shadowed source identities and lexical scopes are distinct",
            if id == "K5" { 2 } else { 0 },
        ),
    ]);
    catalog
}

pub(super) struct AdmissionFailure {
    pub(super) status: &'static str,
    pub(super) kind: &'static str,
    pub(super) stage: &'static str,
    pub(super) detail: String,
    pub(super) diagnostics: Json,
    pub(super) location: Json,
}

pub(super) type Admission<T> = Result<T, AdmissionFailure>;

pub(super) fn internal(stage: &'static str, error: impl std::fmt::Debug) -> AdmissionFailure {
    AdmissionFailure {
        status: "observer_exception",
        kind: "producer_or_observer_internal",
        stage,
        detail: format!("{error:?}"),
        diagnostics: Json::Array(Vec::new()),
        location: Json::Null,
    }
}

pub(super) fn internal_step<T, E: std::fmt::Debug>(
    stage: &'static str,
    result: Result<T, E>,
    trace: &mut Trace,
) -> Admission<T> {
    trace
        .admission_steps
        .push((stage.to_owned(), result.is_ok()));
    result.map_err(|error| internal(stage, error))
}

fn diagnostic_records(sources: &SourceMap, values: &[Diagnostic]) -> Json {
    Json::Array(
        values
            .iter()
            .map(|value| {
                Json::Object(vec![
                    ("code", string(value.code().to_string())),
                    ("message", string(value.message())),
                    ("severity", debug(value.severity())),
                    ("location", location(sources, value.primary_span(), true)),
                    (
                        "related",
                        Json::Array(
                            value
                                .details()
                                .iter()
                                .map(|detail| match detail {
                                    DiagnosticDetail::Label(label) => Json::Object(vec![
                                        ("kind", string("label")),
                                        ("message", string(label.message())),
                                        ("location", location(sources, label.span(), true)),
                                    ]),
                                    DiagnosticDetail::Note(text) => Json::Object(vec![
                                        ("kind", string("note")),
                                        ("message", string(text.as_str())),
                                    ]),
                                    DiagnosticDetail::Help(text) => Json::Object(vec![
                                        ("kind", string("help")),
                                        ("message", string(text.as_str())),
                                    ]),
                                })
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect(),
    )
}

pub(super) fn clean_diagnostics(
    sources: &SourceMap,
    stage: &'static str,
    values: &[Diagnostic],
    trace: &mut Trace,
) -> Admission<()> {
    trace
        .admission_steps
        .push((stage.to_owned(), values.is_empty()));
    if values.is_empty() {
        return Ok(());
    }
    Err(AdmissionFailure {
        status: "admission_rejected",
        kind: "source_diagnostics",
        stage,
        detail: "real source diagnostics".to_owned(),
        diagnostics: diagnostic_records(sources, values),
        location: Json::Null,
    })
}

pub(super) fn identity(
    stage: &'static str,
    valid: bool,
    detail: impl std::fmt::Debug,
) -> Admission<()> {
    if valid {
        Ok(())
    } else {
        Err(internal(stage, detail))
    }
}

pub(super) fn complete_error(
    sources: &SourceMap,
    error: FinalTypeAdmissionError,
) -> AdmissionFailure {
    match error {
        FinalTypeAdmissionError::Diagnostics(values) => AdmissionFailure {
            status: "admission_rejected",
            kind: "source_diagnostics",
            stage: "complete types",
            detail: "real final-admission diagnostics".to_owned(),
            diagnostics: diagnostic_records(sources, &values),
            location: Json::Null,
        },
        FinalTypeAdmissionError::Internal(error) => {
            let mut failure = internal("complete types", &error);
            if let FinalTypeAdmissionInternalError::MissingRequiredFact { span, .. } = error {
                failure.location = location(sources, span, true);
            }
            failure
        }
    }
}
