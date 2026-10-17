//! SPEC-0331：普通源码 P2 + ownership 联合成功目标。
//! 三个 exact 分别先收齐 8 / 4 / 4 次观察；不使用未来 Map plan API。

#[path = "map_borrowed/cases.rs"]
mod cases;
#[path = "map_borrowed/fixture.rs"]
mod fixture;
#[path = "map_borrowed/ownership_facts.rs"]
mod ownership_facts;
#[path = "map_borrowed/pipeline.rs"]
mod pipeline;
#[path = "map_borrowed/type_facts.rs"]
mod type_facts;

use cases::{CASES, Case};
use pipeline::{Entry, observe};

#[derive(Debug, Default)]
struct Report {
    checks: Vec<(&'static str, bool)>,
    details: Vec<String>,
}

impl Report {
    fn record(&mut self, name: &'static str, passed: bool, detail: impl std::fmt::Debug) {
        self.checks.push((name, passed));
        self.details.push(format!("{name}: {detail:?}"));
    }

    fn note(&mut self, name: &str, detail: impl std::fmt::Debug) {
        self.details.push(format!("{name}: {detail:?}"));
    }

    fn passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|(_, passed)| *passed)
    }
}

fn assert_cases(cases: &[Case], expected_observations: usize) {
    let observations = cases
        .iter()
        .flat_map(|case| [Entry::Single, Entry::Unit].map(|entry| observe(case, entry)))
        .collect::<Vec<_>>();
    // 保留所有真实阶段产物至本 exact 的全部观察结束；首个 RED 不掩盖后续路径。
    let assessments = observations
        .iter()
        .map(|item| item.assess())
        .collect::<Vec<_>>();
    for (_, report) in &assessments {
        println!("{report}");
    }
    assert_eq!(assessments.len(), expected_observations);
    assert!(
        assessments.iter().all(|(passed, _)| *passed),
        "SPEC-0331: all sources must pass ordinary lex/parse/names/raw/complete before their ownership target can be assessed. BLOCKED/INTERNAL and OWNERSHIP_NOT_REACHED are not ownership rejection evidence.\n{}",
        assessments
            .iter()
            .map(|(_, report)| report.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

#[test]
fn owned_borrowed_and_temporary_sources_preserve_bindings_and_cleanup() {
    assert_cases(&CASES[..4], 8);
}

#[test]
fn borrowed_move_only_key_and_value_reject_owned_delivery() {
    assert_cases(&CASES[4..6], 4);
}

#[test]
fn active_sources_reject_consume_and_owned_return() {
    assert_cases(&CASES[6..], 4);
}
