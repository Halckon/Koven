//! Compact original-input record encoding and exact source diagnostic serialization.
use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail},
    parser::Expression,
    source::{SourceMap, Span},
};
use std::io::Write;

/// Small JSON values shared with the nullable original-input observer.
#[derive(Debug)]
pub(crate) enum Json {
    String(String),
    Number(usize),
    Bool(bool),
    Null,
    Array(Vec<Json>),
    Object(Vec<(&'static str, Json)>),
}

impl From<&str> for Json {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}
impl From<String> for Json {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}
impl From<usize> for Json {
    fn from(value: usize) -> Self {
        Self::Number(value)
    }
}
impl From<bool> for Json {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

fn quoted(output: &mut String, value: &str) {
    use std::fmt::Write;
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            '\u{0}'..='\u{1f}' => {
                write!(output, "\\u{:04x}", character as u32).unwrap();
            }
            _ => output.push(character),
        }
    }
    output.push('"');
}

fn json(output: &mut String, value: &Json) {
    match value {
        Json::String(value) => quoted(output, value),
        Json::Number(value) => output.push_str(&value.to_string()),
        Json::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Json::Null => output.push_str("null"),
        Json::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(',');
                }
                json(output, value);
            }
            output.push(']');
        }
        Json::Object(fields) => object(output, fields),
    }
}

fn object(output: &mut String, fields: &[(&str, Json)]) {
    output.push('{');
    for (index, (key, value)) in fields.iter().enumerate() {
        if index != 0 {
            output.push(',');
        }
        quoted(output, key);
        output.push(':');
        json(output, value);
    }
    output.push('}');
}

/// One complete physical record, outside libtest's successful-test capture.
/// Caller owns sequence and IO integrity; no global state or panic-hook changes.
pub(crate) fn write_record(
    event: &str,
    whole: &str,
    cell: &str,
    layout: &str,
    sequence: usize,
    fields: &[(&str, Json)],
) -> std::io::Result<()> {
    let mut output = String::from("\nMAP_ORIGINAL_OBS_V1 ");
    let header = [
        ("event", event.into()),
        ("whole", whole.into()),
        ("cell", cell.into()),
        ("layout", layout.into()),
        ("sequence", sequence.into()),
        ("producer", "Unit".into()),
    ];
    object(&mut output, &header);
    output.pop();
    for (key, value) in fields {
        output.push(',');
        quoted(&mut output, key);
        output.push(':');
        json(&mut output, value);
    }
    output.push_str("}\n");
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()
}

pub(super) fn span_json(sources: &SourceMap, span: Span, include_text: bool) -> Json {
    let path = sources
        .source_name(span.source_id())
        .expect("span belongs to original source map");
    let text = sources
        .slice(span)
        .expect("span is a valid original UTF-8 range");
    let mut fields = vec![
        ("source_id", format!("{:?}", span.source_id()).into()),
        ("logical_path", path.into()),
        ("start", span.start().into()),
        ("end", span.end().into()),
    ];
    if include_text {
        fields.push(("text", text.into()));
    }
    Json::Object(fields)
}

pub(super) fn diagnostic_json(sources: &SourceMap, diagnostic: &Diagnostic) -> Json {
    Json::Object(vec![
        ("code", diagnostic.code().to_string().into()),
        ("message", diagnostic.message().into()),
        ("severity", format!("{:?}", diagnostic.severity()).into()),
        ("span", span_json(sources, diagnostic.primary_span(), true)),
        (
            "details",
            Json::Array(
                diagnostic
                    .details()
                    .iter()
                    .map(|detail| match detail {
                        DiagnosticDetail::Label(label) => Json::Object(vec![
                            ("kind", "label".into()),
                            ("message", label.message().into()),
                            ("span", span_json(sources, label.span(), true)),
                        ]),
                        DiagnosticDetail::Note(text) => Json::Object(vec![
                            ("kind", "note".into()),
                            ("message", text.as_str().into()),
                        ]),
                        DiagnosticDetail::Help(text) => Json::Object(vec![
                            ("kind", "help".into()),
                            ("message", text.as_str().into()),
                        ]),
                    })
                    .collect(),
            ),
        ),
    ])
}

/// Serialize only the actual same-analysis ownership/AST inventory, without changing phases.
pub(super) fn ownership_fields(analysis: &super::UnitSource) -> Vec<(&'static str, Json)> {
    let owned = analysis.ownership();
    vec![
        ("fact", "unit_ownership_inventory".into()),
        ("iteration_count", owned.iterations().len().into()),
        ("drop_count", owned.drops().len().into()),
        ("loan_count", owned.loans().len().into()),
        ("receiver_count", owned.receiver_facts().len().into()),
        (
            "receivers",
            Json::Array(
                owned
                    .receiver_facts()
                    .iter()
                    .map(|receiver| {
                        Json::Object(vec![
                            ("call", format!("{:?}", receiver.call()).into()),
                            ("kind", format!("{:?}", receiver.kind()).into()),
                            ("source", format!("{:?}", receiver.source()).into()),
                            (
                                "begin",
                                span_json(analysis.sources(), receiver.begin_span(), false),
                            ),
                            (
                                "end",
                                span_json(analysis.sources(), receiver.end_span(), false),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "returns",
            Json::Array(
                analysis
                    .parsed()
                    .ast()
                    .expressions()
                    .iter()
                    .filter_map(|(id, node)| {
                        if let Expression::Return { value, .. } = node.payload() {
                            Some(Json::Object(vec![
                                ("expression", format!("{id:?}").into()),
                                ("span", span_json(analysis.sources(), node.span(), false)),
                                (
                                    "operand",
                                    value.map_or(Json::Null, |value| format!("{value:?}").into()),
                                ),
                                (
                                    "operand_span",
                                    value.map_or(Json::Null, |value| {
                                        span_json(
                                            analysis.sources(),
                                            analysis
                                                .parsed()
                                                .ast()
                                                .expressions()
                                                .get(value)
                                                .unwrap()
                                                .span(),
                                            false,
                                        )
                                    }),
                                ),
                            ]))
                        } else {
                            None
                        }
                    })
                    .collect(),
            ),
        ),
        (
            "legacy_closure_phi_equivalence",
            "unproved_not_published_by_unit".into(),
        ),
        (
            "iterations",
            Json::Array(
                owned
                    .iterations()
                    .iter()
                    .map(|plan| {
                        Json::Object(vec![
                            (
                                "statement",
                                format!("{:?}", plan.descriptor().statement()).into(),
                            ),
                            (
                                "source_access",
                                format!("{:?}", plan.source_access()).into(),
                            ),
                            (
                                "exits",
                                Json::Array(
                                    plan.exits()
                                        .iter()
                                        .map(|exit| {
                                            Json::Object(vec![
                                                ("kind", format!("{:?}", exit.kind()).into()),
                                                ("point", format!("{:?}", exit.point()).into()),
                                                ("action_count", exit.actions().len().into()),
                                            ])
                                        })
                                        .collect(),
                                ),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "drops",
            Json::Array(
                owned
                    .drops()
                    .iter()
                    .map(|drop| {
                        Json::Object(vec![
                            ("point", format!("{:?}", drop.point()).into()),
                            ("target", format!("{:?}", drop.target()).into()),
                            (
                                "value_origin",
                                span_json(analysis.sources(), drop.value_origin(), false),
                            ),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "loans",
            Json::Array(
                owned
                    .loans()
                    .iter()
                    .map(|loan| {
                        Json::Object(vec![
                            ("call", format!("{:?}", loan.call()).into()),
                            ("argument", format!("{:?}", loan.argument()).into()),
                            ("kind", format!("{:?}", loan.kind()).into()),
                            (
                                "begin",
                                span_json(analysis.sources(), loan.begin_span(), false),
                            ),
                            ("end", span_json(analysis.sources(), loan.end_span(), false)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]
}
