//! SPEC-0331：完整 for pattern 恢复必须继续访问独立 body，且不级联污染 binding。
//! 三个真实源的 single/unit 六次观察先全部收齐，再比较完整有序 Diagnostic。

use crate::map_binding_analysis as analysis;
#[path = "pattern_recovery_neighbors/facts.rs"]
mod facts;
#[path = "pattern_recovery_neighbors/sites.rs"]
mod sites;

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
    type_checking::{FinalTypeAdmissionError, FinalTypeAdmissionInternalError},
};

use analysis::{Analysis, Blocked, Entry, Observation, blocked, diagnostics, observe};
use facts::facts;
use sites::{Body, Sites, sites};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    IndependentBody,
    SequentialArity,
    PoisonedBinding,
}

impl Case {
    fn text(self) -> &'static str {
        match self {
            Self::IndependentBody => {
                "fun scan(m: Map<Int, String>): Unit { for ((key) in m) { val n: Int = 1; val bad = n.fooBar() } }"
            }
            Self::SequentialArity => {
                "value class Pair(val a: Int, val b: Boolean)\nfun run(xs: List<Pair>) { for ((a, b, c) in xs) {} }"
            }
            Self::PoisonedBinding => {
                "fun scan(m: Map<Int, String>): Unit { for ((key) in m) { val derived = key } }"
            }
        }
    }

    fn pattern(self) -> &'static str {
        match self {
            Self::SequentialArity => "(a, b, c)",
            _ => "(key)",
        }
    }

    fn markers(self) -> &'static [&'static str] {
        match self {
            Self::SequentialArity => &["a", "b", "c"],
            _ => &["key"],
        }
    }
}

fn expected(
    observed: &analysis::Observed,
    sites: &Sites,
    case: Case,
) -> Result<(Vec<Diagnostic>, Vec<Diagnostic>), Blocked> {
    let sources = &observed.fixture.sources;
    let catalog = DiagnosticCodeCatalog::try_new(&["L0080", "L0118", "L0160"])
        .map_err(|error| blocked("diagnostic oracle", error))?;
    let (code, message) = match case {
        Case::SequentialArity => (
            "L0118",
            "value class destructuring must bind every component exactly once",
        ),
        _ => (
            "L0160",
            "Map iteration requires exactly two named bindings (key, value)",
        ),
    };
    let mut pattern = Diagnostic::new(
        sources,
        Severity::Error,
        catalog
            .resolve(code)
            .map_err(|error| blocked("diagnostic oracle", error))?,
        message,
        sites.pattern,
    )
    .map_err(|error| blocked("diagnostic oracle", error))?;
    if let Some((_, declaration)) = sites.nominal {
        pattern
            .add_label(sources, declaration, "expected 2 bindings")
            .map_err(|error| blocked("diagnostic oracle", error))?;
    }
    let raw = vec![pattern];
    let mut complete = raw.clone();
    if let Body::Independent { name_span, .. } = sites.body {
        complete.push(
            Diagnostic::new(
                sources,
                Severity::Error,
                catalog
                    .resolve("L0080")
                    .map_err(|error| blocked("diagnostic oracle", error))?,
                "receiver has no such member",
                name_span,
            )
            .map_err(|error| blocked("diagnostic oracle", error))?,
        );
    }
    Ok((raw, complete))
}

fn assess(case: Case, observation: &Observation) -> (bool, String) {
    let mut report = format!(
        "{case:?} / {:?}\nSOURCE {:?}\n{}\n",
        observation.entry,
        case.text(),
        observation.trace.join("\n"),
    );
    let observed = match &observation.result {
        Ok(observed) => observed,
        Err(error) => {
            report.push_str(&format!(
                "BLOCKED stage={} detail={}\n",
                error.stage, error.detail
            ));
            return (false, report);
        }
    };
    let (raw, complete) = match &observed.analysis {
        Analysis::Single(analysis) => (analysis.raw.diagnostics(), &analysis.complete),
        Analysis::Unit(analysis) => (analysis.raw.diagnostics(), &analysis.complete),
    };
    // 原始顺序和完整 payload 先保留；AST/facts oracle 失败不能吞 complete 的 Internal。
    match complete {
        Err(FinalTypeAdmissionError::Diagnostics(items)) => {
            report.push_str(&diagnostics(&observed.fixture.sources, "complete", items));
        }
        Err(FinalTypeAdmissionError::Internal(error)) => {
            report.push_str(&format!("complete INTERNAL: {error:#?}\n"));
            if let FinalTypeAdmissionInternalError::MissingRequiredFact { span, kind } = error {
                report.push_str(&format!(
                    "stage=complete kind={kind:?} span={span:?} slice={:?}\n",
                    observed.fixture.sources.slice(*span),
                ));
            }
        }
        Ok(same) => report.push_str(&format!("complete ACCEPTED same_analysis={same}\n")),
    }
    let assessed = (|| {
        let sites = sites(&observed.fixture, case)?;
        report.push_str(&format!("\nAST {sites:#?}\n"));
        let facts = facts(observed, &sites, case)?;
        report.push_str(&format!("FACTS {}\n", facts.details.join("\n")));
        let (expected_raw, expected_complete) = expected(observed, &sites, case)?;
        let exact_raw = raw == expected_raw.as_slice();
        let exact_complete = matches!(complete,
            Err(FinalTypeAdmissionError::Diagnostics(items)) if items.as_slice() == expected_complete.as_slice());
        let retained_root = matches!(complete,
            Err(FinalTypeAdmissionError::Diagnostics(items)) if items.first() == raw.first());
        let unchanged = case == Case::IndependentBody
            || matches!(complete,
            Err(FinalTypeAdmissionError::Diagnostics(items)) if items.as_slice() == raw);
        let retained_input =
            observed.fixture.lexed.source_id() == observed.fixture.file.source_id();
        let passed = facts.valid
            && exact_raw
            && exact_complete
            && retained_root
            && unchanged
            && retained_input;
        report.push_str(&format!(
            "exact_raw={exact_raw} exact_complete={exact_complete} retained_full_root={retained_root} unchanged_when_required={unchanged} retained_input={retained_input} facts_valid={}\nTARGET_MATCH={passed}\n",
            facts.valid,
        ));
        Ok::<_, Blocked>(passed)
    })();
    match assessed {
        Ok(passed) => (passed, report),
        Err(error) => {
            report.push_str(&format!(
                "BLOCKED stage={} detail={}\n",
                error.stage, error.detail
            ));
            (false, report)
        }
    }
}

#[test]
fn for_pattern_recovery_preserves_independent_body_roots_in_both_entries() {
    let mut observations = Vec::new();
    for case in [
        Case::IndependentBody,
        Case::SequentialArity,
        Case::PoisonedBinding,
    ] {
        for entry in [Entry::Single, Entry::Unit] {
            observations.push((case, observe(case.text(), entry)));
        }
    }
    // 六次真实流水线全部返回之后，才评估全部 oracle、打印完整集合并统一断言。
    let assessments = observations
        .iter()
        .map(|(case, observation)| assess(*case, observation))
        .collect::<Vec<_>>();
    for (_, report) in &assessments {
        println!("{report}");
    }
    assert!(
        assessments.len() == 6 && assessments.iter().all(|(passed, _)| *passed),
        "SPEC-0331: all six ordinary observations must preserve complete ordered diagnostics; independent body adds exactly AST fooBar L0080 beside original L0160, arity and poisoned uses retain their original full diagnostic without cascades.\n{}",
        assessments
            .iter()
            .map(|(_, report)| report.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
