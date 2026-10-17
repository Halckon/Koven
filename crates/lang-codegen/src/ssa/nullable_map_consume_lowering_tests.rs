//! SPEC-0291: ordinary nullable Map sources, complete FE, ownership, SSA and LLVM.
//! Each exact collects its entire layer before asserting. No object emission or native run.
//! First-red input oracles do not claim SSA owner/proof/cleanup identity coverage.

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::Diagnostic,
    lexer::lex,
    parser::{Expression, ParsedFile, parse_file},
    source::{SourceMap, Span},
    type_checking::{FinalTypeAdmissionError, FinalTypeAdmissionInternalError},
};

use super::model::{FunctionId, Program};
use sources::{CASES, Case};

mod resource;
mod single;
mod sources;
mod unit;

#[derive(Debug)]
struct Failure {
    stage: String,
    detail: String,
}

type Outcome<T> = Result<T, Failure>;

#[derive(Debug, Default)]
struct Trace {
    stages: Vec<String>,
    input_failures: Vec<String>,
}

impl Trace {
    fn check(&mut self, label: &str, valid: bool, detail: impl std::fmt::Debug) {
        self.stages
            .push(format!("input {label}: valid={valid}; actual={detail:#?}"));
        if !valid {
            self.input_failures.push(label.to_owned());
        }
    }
}

#[derive(Debug)]
struct Observation {
    label: String,
    trace: Trace,
    outcome: Outcome<()>,
}

fn step<T, E: std::fmt::Debug>(stage: &str, result: Result<T, E>, trace: &mut Trace) -> Outcome<T> {
    match result {
        Ok(value) => {
            trace.stages.push(format!("{stage}: reached"));
            Ok(value)
        }
        Err(error) => {
            let detail = format!("{error:#?}");
            trace.stages.push(format!("{stage}: failed; {detail}"));
            Err(Failure {
                stage: stage.to_owned(),
                detail,
            })
        }
    }
}

fn gate(stage: &str, valid: bool, detail: impl std::fmt::Debug) -> Outcome<()> {
    if valid {
        Ok(())
    } else {
        Err(Failure {
            stage: stage.to_owned(),
            detail: format!("{detail:#?}"),
        })
    }
}

fn origin(sources: &SourceMap, span: Span) -> String {
    format!(
        "span={span:?}; source={:?}; text={:?}",
        sources.source_name(span.source_id()),
        sources.slice(span),
    )
}

fn diagnostic_text(sources: &SourceMap, diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{diagnostic:#?}; {}",
                origin(sources, diagnostic.primary_span())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn diagnostics(
    stage: &str,
    sources: &SourceMap,
    values: &[Diagnostic],
    trace: &mut Trace,
) -> Outcome<()> {
    let detail = diagnostic_text(sources, values);
    trace
        .stages
        .push(format!("{stage}: count={}; {detail}", values.len()));
    gate(stage, values.is_empty(), detail)
}

fn admission_error(sources: &SourceMap, error: FinalTypeAdmissionError) -> String {
    match &error {
        FinalTypeAdmissionError::Diagnostics(values) => diagnostic_text(sources, values),
        FinalTypeAdmissionError::Internal(
            FinalTypeAdmissionInternalError::MissingRequiredFact { span, .. },
        ) => format!("{error:#?}; {}", origin(sources, *span)),
        _ => format!("{error:#?}"),
    }
}

fn parsed(
    sources: &mut SourceMap,
    name: &str,
    text: &str,
    trace: &mut Trace,
) -> Outcome<ParsedFile> {
    let source = step("source", sources.add_source(name, text), trace)?;
    let lexed = step("lex", lex(sources, source), trace)?;
    let lexical = diagnostics("lex diagnostics", sources, lexed.diagnostics(), trace);
    let parsed = step("parse", parse_file(sources, &lexed), trace)?;
    let syntax = diagnostics("parse diagnostics", sources, parsed.diagnostics(), trace);
    lexical?;
    syntax?;
    trace
        .stages
        .push(format!("parsed source={source:?}; logical path={name}"));
    Ok(parsed)
}

fn ungroup(parsed: &ParsedFile, mut expression: ExpressionId) -> Option<ExpressionId> {
    loop {
        match parsed.ast().expressions().get(expression).ok()?.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            Expression::Name => return Some(expression),
            _ => return None,
        }
    }
}

fn lowering_error(sources: &SourceMap, error: super::LoweringError) -> String {
    let actual_origin = error.span.map(|span| origin(sources, span));
    format!("{error:#?}; actual source origin={actual_origin:?}")
}

fn verify_and_render(program: &Program, entry: FunctionId, trace: &mut Trace) -> Outcome<()> {
    // Record the real program even if its verifier rejects it; never mutate it.
    trace.stages.push(format!(
        "actual selected SSA entry={entry:?}; actual modules={:#?}",
        program.modules,
    ));
    step("verify SSA", super::verify::verify_program(program), trace)?;
    trace
        .stages
        .push(format!("verified SSA:\n{}", super::render_program(program)));
    let llvm = step(
        "in-memory LLVM with selected entry",
        crate::llvm::render_verified_program_with_entry(program, entry),
        trace,
    )?;
    trace
        .stages
        .push(format!("actual LLVM bytes={}; LLVM:\n{llvm}", llvm.len()));
    Ok(())
}

fn collect_layer(cases: &[Case], expected: usize) {
    let mut observations = Vec::with_capacity(expected);
    for case in cases {
        for unit in [false, true] {
            // A1, A2 and every other entry receive independent sources and fresh environments.
            let mut trace = Trace::default();
            let outcome = if unit {
                unit::observe(case, &mut trace)
            } else {
                single::observe(case, &mut trace)
            };
            observations.push(Observation {
                label: format!(
                    "{}/{}:{}",
                    case.id,
                    if unit { "unit" } else { "single" },
                    case.label
                ),
                trace,
                outcome,
            });
        }
    }
    for observation in &observations {
        println!(
            "NULLABLE MAP {}\n{}",
            observation.label,
            observation.trace.stages.join("\n")
        );
        println!("input failures={:?}", observation.trace.input_failures);
        match &observation.outcome {
            Ok(()) => println!("pipeline outcome=verified LLVM"),
            Err(failure) => println!("failed stage={}; detail={}", failure.stage, failure.detail),
        }
    }
    // These assertions happen only after every ordinary observation in this layer returned.
    assert_eq!(observations.len(), expected);
    assert!(
        observations
            .iter()
            .all(|value| value.outcome.is_ok() && value.trace.input_failures.is_empty()),
        "all {expected} ordinary observations must pass input oracles and reach verified LLVM; see observations above"
    );
}

#[test]
fn nullable_map_storage_and_remove_canonical_sources_reach_verified_llvm() {
    collect_layer(&CASES[..3], 6);
}

#[test]
fn nullable_map_if_read_views_reach_verified_llvm() {
    collect_layer(&CASES[3..5], 4);
}

#[test]
fn nullable_map_if_owned_consumes_reach_verified_llvm() {
    collect_layer(&CASES[5..], 6);
}
