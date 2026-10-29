//! SPEC-0331：六个 clean 非法 Map pattern 与一个 nominal 同名邻控。
//! 单个普通 exact 先采齐 single/rooted Unit/MemoryDocument Unit 二十一路，再核完整原始诊断和 AST 身份。

use crate::map_binding_analysis as analysis;
#[path = "map_binding_controls/facts.rs"]
mod facts;

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticDetail, Severity},
    type_checking::{
        FinalTypeAdmissionError, FinalTypeAdmissionInternalError, IntrinsicTypeConstructor,
    },
};

use analysis::{Analysis, Entry, Observation, diagnostics, observe};
use facts::{Sites, facts, sites};

struct Case {
    id: &'static str,
    text: &'static str,
    constructor: Option<IntrinsicTypeConstructor>,
    destructuring: bool,
    pattern: &'static str,
    markers: &'static [&'static str],
}

const CASES: [Case; 7] = [
    Case {
        id: "B1 Map name",
        text: "fun scan(m: Map<Int, String>): Unit { for (entry in m) {} }",
        constructor: Some(IntrinsicTypeConstructor::Map),
        destructuring: false,
        pattern: "entry",
        markers: &["entry"],
    },
    Case {
        id: "B2 MutableMap discard",
        text: "fun scan(m: MutableMap<Int, String>): Unit { for (_ in m) {} }",
        constructor: Some(IntrinsicTypeConstructor::MutableMap),
        destructuring: false,
        pattern: "_",
        markers: &["_"],
    },
    Case {
        id: "B3 Map one component",
        text: "fun scan(m: Map<Int, String>): Unit { for ((key) in m) {} }",
        constructor: Some(IntrinsicTypeConstructor::Map),
        destructuring: true,
        pattern: "(key)",
        markers: &["key"],
    },
    Case {
        id: "B4 MutableMap three components",
        text: "fun scan(m: MutableMap<Int, String>): Unit { for ((key, value, extra) in m) {} }",
        constructor: Some(IntrinsicTypeConstructor::MutableMap),
        destructuring: true,
        pattern: "(key, value, extra)",
        markers: &["key", "value", "extra"],
    },
    Case {
        id: "B5 Map left discard",
        text: "fun scan(m: Map<Int, String>): Unit { for ((_, value) in m) {} }",
        constructor: Some(IntrinsicTypeConstructor::Map),
        destructuring: true,
        pattern: "(_, value)",
        markers: &["_", "value"],
    },
    Case {
        id: "B6 MutableMap right discard",
        text: "fun scan(m: MutableMap<Int, String>): Unit { for ((key, _) in m) {} }",
        constructor: Some(IntrinsicTypeConstructor::MutableMap),
        destructuring: true,
        pattern: "(key, _)",
        markers: &["key", "_"],
    },
    Case {
        id: "N1 nominal Map",
        text: "class Map {}\nfun scan(m: Map): Unit { for ((key, value) in m) {} }",
        constructor: None,
        destructuring: true,
        pattern: "(key, value)",
        markers: &["key", "value"],
    },
];

fn raw_matches(case: &Case, sites: &Sites, raw: &[Diagnostic]) -> bool {
    let [diagnostic] = raw else { return false };
    let (code, primary) = if case.constructor.is_some() {
        ("L0160", sites.binding_span)
    } else {
        ("L0159", sites.source_span)
    };
    let label_matches = match sites.nominal {
        Some((_, declaration_span)) => {
            matches!(diagnostic.details(), [DiagnosticDetail::Label(label)] if label.span() == declaration_span)
        }
        None => true,
    };
    diagnostic.severity() == Severity::Error
        && diagnostic.code().to_string() == code
        && diagnostic.primary_span() == primary
        && label_matches
}

fn assess(case: &Case, observation: &Observation) -> (bool, String) {
    let label = format!("{} / {:?}", case.id, observation.entry);
    let mut report = format!(
        "{label}\nSOURCE {:?}\n{}\n",
        case.text,
        observation.trace.join("\n")
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
    // 在任何 AST/facts oracle 前保存完整 complete 结果，后续失败不能吞掉 Internal。
    match complete {
        Err(FinalTypeAdmissionError::Diagnostics(items)) => {
            report.push_str(&diagnostics(&observed.fixture.sources, "complete", items));
        }
        Err(FinalTypeAdmissionError::Internal(error)) => {
            report.push_str(&format!("complete INTERNAL: {error:#?}\n"));
            if let FinalTypeAdmissionInternalError::MissingRequiredFact { span, kind } = error {
                report.push_str(&format!(
                    "stage=complete kind={kind:?} span={span:?} slice={:?}\n",
                    observed.fixture.sources.slice(*span)
                ));
            }
        }
        Ok(same_analysis) => report.push_str(&format!(
            "complete ACCEPTED same_analysis={same_analysis}\n"
        )),
    }
    let sites = match sites(&observed.fixture, case) {
        Ok(sites) => sites,
        Err(error) => {
            report.push_str(&format!(
                "\nBLOCKED stage={} detail={}\n",
                error.stage, error.detail
            ));
            return (false, report);
        }
    };
    report.push_str(&format!("\nAST {sites:#?}\n"));
    let facts = match facts(observed, &sites, case) {
        Ok(facts) => facts,
        Err(error) => {
            report.push_str(&format!(
                "BLOCKED stage={} detail={}\n",
                error.stage, error.detail
            ));
            return (false, report);
        }
    };
    let retained = observed.fixture.lexed.source_id() == observed.fixture.file.source_id();
    let exact_raw = raw_matches(case, &sites, raw);
    // Diagnostic 的全值相等包含 severity、message、完整 primary 和有序 details。
    let same_diagnostics = matches!(complete, Err(FinalTypeAdmissionError::Diagnostics(items)) if items.as_slice() == raw);
    let passed = retained
        && exact_raw
        && same_diagnostics
        && facts.source_matches
        && facts.bindings_match
        && facts.plans_absent;
    report.push_str(&format!(
        "retained_lex_parse={retained} exact_raw={exact_raw} complete_same_diagnostics={same_diagnostics} source_matches={} binding_names_match={} no_partial_plans={}\nFACTS {}\nTARGET_MATCH={passed}\n",
        facts.source_matches, facts.bindings_match, facts.plans_absent, facts.details.join("\n"),
    ));
    (passed, report)
}

#[test]
fn invalid_map_bindings_and_nominal_lookalike_preserve_diagnostics_in_both_entries() {
    let mut observations = Vec::new();
    for case in &CASES {
        for entry in [Entry::Single, Entry::Unit, Entry::MemoryUnit] {
            observations.push((case, observe(case.text, entry)));
        }
    }
    // 二十一次真实分析全部结束后才评估、完整输出，再作唯一的集合目标断言。
    let assessments = observations
        .iter()
        .map(|(case, observation)| assess(case, observation))
        .collect::<Vec<_>>();
    for (_, report) in &assessments {
        println!("{report}");
    }
    assert_eq!(
        assessments.len(),
        21,
        "seven source fixtures × three entries"
    );
    assert!(
        assessments.iter().all(|(passed, _)| *passed),
        "SPEC-0331: clean invalid Map bindings require exact full-binding L0160; nominal Map requires source L0159. Complete must retain the same full raw Diagnostic. BLOCKED, INTERNAL, arbitrary Err and partial plans cannot satisfy this target.\n{}",
        assessments
            .iter()
            .map(|(_, report)| report.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
