//! SPEC-0291: ordinary nullable Map sources, complete FE, ownership, SSA and LLVM.
//! Collectors gather their entire layer before asserting; input oracles alone do not prove SSA identity.
//! Resource identity contracts are explicit, and resource::runtime is separate CI-only
//! object/native/count/Abort coverage rather than part of the ordinary collectors.

use crate::test_support::diagnostics::diagnostic;
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

mod consume_identity;
mod read_cross_join {
    //! B3 first observation: the original nullable owner remains live after its If.
    //! This independent source does not change the frozen A/B/C source partitions.
    //! Full raw ownership/SSA observations precede independent owner/proof identity review.

    use super::*;

    pub(super) const CROSS_JOIN: Case = Case {
        id: "B3",
        label: "if_size_keep_nullable_owner",
        path: "b3_if_size_keep_nullable_owner.ko",
        single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source != null) { val count = source.size }
      val kept = source
      return
    }
    "#,
        provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source != null) { val count = source.size }
      val kept = source
      return
    }
    "#,
        consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
        counts: [0, 0, 1],
        non_null_when_true: Some(true),
        scalar: lang_frontend::type_checking::BuiltinType::Int,
        roots: &[("source", true), ("kept", true)],
    };

    #[test]
    fn nullable_map_if_size_keeps_owner_across_join() {
        collect_layer(std::slice::from_ref(&CROSS_JOIN), 2);
    }
}
mod read_identity;
mod resource;
pub(in crate::ssa) mod single;
mod sources {
    //! Exact approved eight sources; no synthetic pre-registration or source rewriting.

    use lang_frontend::type_checking::BuiltinType;

    pub(super) struct Case {
        pub id: &'static str,
        pub label: &'static str,
        pub path: &'static str,
        pub single: &'static str,
        pub provider: &'static str,
        pub consumer: &'static str,
        pub counts: [usize; 3],
        pub non_null_when_true: Option<bool>,
        pub scalar: BuiltinType,
        pub roots: &'static [(&'static str, bool)],
    }

    // A root's bool selects p/provider.ko (true) or q/consumer.ko (false) in unit mode.
    // Single mode resolves every listed root in its one actual source.
    pub(super) const CASES: &[Case] = &[
        Case {
            id: "A1",
            label: "null_decl",
            path: "a1_null_decl.ko",
            single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = null
      println("null done")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = null
      println("null done")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 0, 0],
            non_null_when_true: None,
            scalar: BuiltinType::Int,
            roots: &[("source", true)],
        },
        Case {
            id: "A2",
            label: "present_decl",
            path: "a2_present_decl.ko",
            single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      println("present done")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      println("present done")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 0, 0],
            non_null_when_true: None,
            scalar: BuiltinType::Int,
            roots: &[("source", true)],
        },
        Case {
            id: "A3",
            label: "remove_canonical",
            path: "a3_remove_canonical.ko",
            single: r#"fun main(): Unit {
      val directNull: MutableMap<Int, Int>? = null
      val directPresent: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      var outer = mutableMapOf<Int, MutableMap<Int, Int>>()
      outer.put(1, mutableMapOf<Int, Int>())
      val removed: MutableMap<Int, Int>? = outer.remove(1)
      println("canonical done")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val directNull: MutableMap<Int, Int>? = null
      val directPresent: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      var outer = mutableMapOf<Int, MutableMap<Int, Int>>()
      outer.put(1, mutableMapOf<Int, Int>())
      val removed: MutableMap<Int, Int>? = outer.remove(1)
      println("canonical done")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 1, 0],
            non_null_when_true: None,
            scalar: BuiltinType::Int,
            roots: &[
                ("directNull", true),
                ("directPresent", true),
                ("removed", true),
            ],
        },
        Case {
            id: "B1",
            label: "if_group_size",
            path: "b1_if_group_size.ko",
            single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if ((source) != null) { val count = ((source)).size }
      println("read done")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if ((source) != null) { val count = ((source)).size }
      println("read done")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 0, 1],
            non_null_when_true: Some(true),
            scalar: BuiltinType::Int,
            roots: &[("source", true)],
        },
        Case {
            id: "B2",
            label: "if_else_var_size",
            path: "b2_if_else_var_size.ko",
            single: r#"fun main(): Unit {
      var source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source == null) {} else { val count = source.size }
      println("else read done")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      var source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source == null) {} else { val count = source.size }
      println("else read done")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [0, 0, 1],
            non_null_when_true: Some(false),
            scalar: BuiltinType::Int,
            roots: &[("source", true)],
        },
        Case {
            id: "C1",
            label: "val_empty",
            path: "c1_val_empty.ko",
            single: r#"fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source != null) {
        val result = source.consume()
        if (result.size == 0) { println("consumed empty") }
      }
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      val source: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      if (source != null) {
        val result = source.consume()
        if (result.size == 0) { println("consumed empty") }
      }
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [1, 0, 1],
            non_null_when_true: Some(true),
            scalar: BuiltinType::Int,
            roots: &[("source", true)],
        },
        Case {
            id: "C2",
            label: "var_string_group_discard",
            path: "c2_var_string_group_discard.ko",
            single: r#"fun main(): Unit {
      var seed = mutableMapOf<String, String>()
      seed.put("key".clone(), "old".clone())
      seed.put("key".clone(), "value".clone())
      var source: MutableMap<String, String>? = seed
      if (source != null) { ((source)).consume() }
      println("consumed strings")
    }
    "#,
            provider: r#"package p
    fun main(): Unit {
      var seed = mutableMapOf<String, String>()
      seed.put("key".clone(), "old".clone())
      seed.put("key".clone(), "value".clone())
      var source: MutableMap<String, String>? = seed
      if (source != null) { ((source)).consume() }
      println("consumed strings")
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit { p.main() }
    "#,
            counts: [1, 0, 0],
            non_null_when_true: Some(true),
            scalar: BuiltinType::String,
            roots: &[("source", true)],
        },
        Case {
            id: "C3",
            label: "nullable_input_both_edges",
            path: "c3_nullable_input_both_edges.ko",
            single: r#"fun run(own input: MutableMap<Int, Int>?): Unit {
      val source: MutableMap<Int, Int>? = input
      if (source == null) {} else {
        val result = source.consume()
        if (result.size == 0) { println("input consumed") }
      }
    }
    fun main(): Unit {
      val present: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      run(present)
      val absent: MutableMap<Int, Int>? = null
      run(absent)
    }
    "#,
            provider: r#"package p
    fun run(own input: MutableMap<Int, Int>?): Unit {
      val source: MutableMap<Int, Int>? = input
      if (source == null) {} else {
        val result = source.consume()
        if (result.size == 0) { println("input consumed") }
      }
    }
    "#,
            consumer: r#"package q
    fun entry(): Unit {
      val present: MutableMap<Int, Int>? = mutableMapOf<Int, Int>()
      p.run(present)
      val absent: MutableMap<Int, Int>? = null
      p.run(absent)
    }
    "#,
            counts: [1, 0, 1],
            non_null_when_true: Some(false),
            scalar: BuiltinType::Int,
            roots: &[
                ("input", true),
                ("source", true),
                ("present", false),
                ("absent", false),
            ],
        },
    ];
}
mod unit;

/// Share the frozen R1 fixture without loading its module more than once.
pub(in crate::ssa) fn registration_r1_source()
-> (&'static str, &'static str, &'static str, &'static str) {
    resource::registration_r1_source()
}

/// Reuse the exact C1/C2/C3 sources in ordinary native tests without loading them twice.
pub(crate) fn ordinary_nullable_consume_sources() -> [(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
); 3] {
    [&CASES[5], &CASES[6], &CASES[7]].map(|case| {
        (
            case.id,
            case.path,
            case.single,
            case.provider,
            case.consumer,
        )
    })
}

/// The unchanged ordinary B/C inputs shared by CI-only backend guard checks.
#[cfg(test)]
pub(in crate::ssa) fn nullable_map_guard_sources() -> [(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
); 6] {
    [
        &CASES[3],
        &CASES[4],
        &read_cross_join::CROSS_JOIN,
        &CASES[5],
        &CASES[6],
        &CASES[7],
    ]
    .map(|case| {
        (
            case.id,
            case.path,
            case.single,
            case.provider,
            case.consumer,
        )
    })
}

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
        diagnostic!(
            "NULLABLE MAP {}\n{}",
            observation.label,
            observation.trace.stages.join("\n")
        );
        diagnostic!("input failures={:?}", observation.trace.input_failures);
        match &observation.outcome {
            Ok(()) => diagnostic!("pipeline outcome=verified LLVM"),
            Err(failure) => {
                diagnostic!("failed stage={}; detail={}", failure.stage, failure.detail)
            }
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
