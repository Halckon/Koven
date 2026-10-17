//! SPEC-0331 联合前端已交付、Map provider 尚未 lower 的普通源码阶段门。
//! 只验证保守 whole-typed-set 拒绝；不构造/变异 facts，不进入 LLVM 或 native。

mod facts;
mod fixture;
mod single;
mod unit;

use fixture::{Fixture, SourceCase};
use lang_frontend::type_checking::{IntrinsicTypeConstructor, MapIterationKind};

const MAP: SourceCase = SourceCase {
    path: "map.ko",
    text: "fun scan(source: Map<Int, String>): Unit {\n  for ((key, value) in source) { val copied: Int = key; println(value) }\n}\n",
    constructor: Some((IntrinsicTypeConstructor::Map, MapIterationKind::Map)),
};
const MUTABLE: SourceCase = SourceCase {
    path: "mutable.ko",
    text: "fun scan(source: MutableMap<Int, String>): Unit {\n  for ((key, value) in source) { val copied: Int = key; println(value) }\n}\n",
    constructor: Some((
        IntrinsicTypeConstructor::MutableMap,
        MapIterationKind::MutableMap,
    )),
};
const WHOLE: [SourceCase; 3] = [
    SourceCase {
        path: "entry.ko",
        text: "fun entry(): Unit {}\n",
        constructor: None,
    },
    SourceCase {
        path: "z_map.ko",
        text: "fun unusedMap(source: Map<Int, String>): Unit {\n  for ((key, value) in source) { val copied: Int = key; println(value) }\n}\n",
        constructor: MAP.constructor,
    },
    SourceCase {
        path: "a_mutable.ko",
        text: "fun unusedMutable(source: MutableMap<Int, String>): Unit {\n  for ((key, value) in source) { val copied: Int = key; println(value) }\n}\n",
        constructor: MUTABLE.constructor,
    },
];

#[derive(Clone, Copy, Debug)]
enum Pathway {
    Single,
    Basic,
    Const,
}

#[derive(Debug, Default)]
struct Report {
    checks: Vec<(&'static str, bool)>,
    details: Vec<String>,
}

impl Report {
    fn check(&mut self, name: &'static str, passed: bool, detail: impl std::fmt::Debug) {
        self.checks.push((name, passed));
        self.details.push(format!("{name}: {detail:?}"));
    }

    fn passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|(_, passed)| *passed)
    }
}

#[derive(Debug)]
struct Observation {
    label: String,
    report: Report,
    result: Result<fixture::LowerOutcome, fixture::Stopped>,
}

impl Observation {
    fn passed(&self) -> bool {
        self.report.passed()
            && self
                .result
                .as_ref()
                .is_ok_and(fixture::LowerOutcome::matches)
    }
}

fn observe(
    label: &str,
    cases: &[SourceCase],
    pathway: Pathway,
    reverse_inputs: bool,
    entry_name: &str,
    whole_set: bool,
) -> Observation {
    let mut report = Report::default();
    let result = Fixture::new(cases).and_then(|fixture| match pathway {
        Pathway::Single => single::collect(&fixture, entry_name, whole_set, &mut report),
        Pathway::Basic | Pathway::Const => unit::collect(
            &fixture,
            pathway,
            reverse_inputs,
            entry_name,
            whole_set,
            &mut report,
        ),
    });
    Observation {
        label: format!("{label}/{pathway:?}/reverse={reverse_inputs}"),
        report,
        result,
    }
}

fn assert_observations(observations: &[Observation], expected: usize) {
    // 每条真实阶段链已结束，才输出和断言；首个失败不得遮住其他观察。
    for observation in observations {
        println!(
            "{}\n{:#?}\n{:#?}",
            observation.label, observation.result, observation.report
        );
        if let Err(stopped) = &observation.result {
            println!(
                "LOWER_NOT_ATTEMPTED stage={} original={}",
                stopped.stage, stopped.original
            );
        }
    }
    assert_eq!(observations.len(), expected);
    assert!(
        observations.iter().all(Observation::passed),
        "Map lower 阶段门必须是完整 frontend 成功后的精确 UnsupportedNode；\
         前置失败保留原 stage，代表 LOWER_NOT_ATTEMPTED，不能冒充能力拒绝。\n{observations:#?}",
    );
}

#[test]
fn map_and_mutable_map_reject_after_complete_owned_in_three_pathways() {
    let observations = [MAP, MUTABLE]
        .into_iter()
        .flat_map(|case| {
            [Pathway::Single, Pathway::Basic, Pathway::Const]
                .map(|pathway| observe(case.path, &[case], pathway, false, "scan", false))
        })
        .collect::<Vec<_>>();
    assert_observations(&observations, 6);
}

#[test]
fn whole_typed_source_set_rejects_unused_map_loops_in_actual_identity_order() {
    let mut observations = Vec::new();
    // 单文件也锁整个 typed file；entry 体为空，两个 Map 函数从未被 entry 调用。
    let text = WHOLE.iter().map(|case| case.text).collect::<String>();
    observations.push(single::collect_whole_file(&text));
    for pathway in [Pathway::Basic, Pathway::Const] {
        for reverse_inputs in [false, true] {
            observations.push(observe(
                "whole-source-set",
                &WHOLE,
                pathway,
                reverse_inputs,
                "entry",
                true,
            ));
        }
    }
    assert_observations(&observations, 5);
}
