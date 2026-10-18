//! 新 Group whole-set 候选首证；不修改旧 stagegate6/whole5 的源码或 oracle。
//! 五次 fresh 实际管线全收集后断言；FE 红不能变成 Unsupported，查询红不能阻止 lower。
mod ast;
mod cleanup;
mod facts;
mod fixture;
mod single;
mod unit;

use super::{Pathway, Report, SourceCase};
use fixture::{Fixture, LowerOutcome, Stopped};
use lang_frontend::type_checking::{IntrinsicTypeConstructor, MapIterationKind};

const FOR_DIRECT: &str = "for ((key, value) in source) { val copied: Int = key; println(value) }";
const FOR_GROUP: &str = "for ((key, value) in (source)) { val copied: Int = key; println(value) }";
const CASES: [SourceCase; 5] = [
    SourceCase {
        path: "entry.ko",
        text: include_str!("../../../tests/fixtures/map-group-boundary-0331/entry.ko"),
        constructor: None,
    },
    SourceCase {
        path: "z_direct_map.ko",
        text: include_str!("../../../tests/fixtures/map-group-boundary-0331/z_direct_map.ko"),
        constructor: Some((IntrinsicTypeConstructor::Map, MapIterationKind::Map)),
    },
    SourceCase {
        path: "a_direct_mutable.ko",
        text: include_str!("../../../tests/fixtures/map-group-boundary-0331/a_direct_mutable.ko"),
        constructor: Some((
            IntrinsicTypeConstructor::MutableMap,
            MapIterationKind::MutableMap,
        )),
    },
    SourceCase {
        path: "y_group_map.ko",
        text: include_str!("../../../tests/fixtures/map-group-boundary-0331/y_group_map.ko"),
        constructor: Some((IntrinsicTypeConstructor::Map, MapIterationKind::Map)),
    },
    SourceCase {
        path: "b_group_mutable.ko",
        text: include_str!("../../../tests/fixtures/map-group-boundary-0331/b_group_mutable.ko"),
        constructor: Some((
            IntrinsicTypeConstructor::MutableMap,
            MapIterationKind::MutableMap,
        )),
    },
];

#[derive(Debug)]
struct Observation {
    label: String,
    report: Report,
    result: Result<LowerOutcome, Stopped>,
}

impl Observation {
    fn passed(&self) -> bool {
        self.report.passed() && self.result.as_ref().is_ok_and(LowerOutcome::matches)
    }
}

fn observe(pathway: Pathway, reverse_inputs: bool) -> Observation {
    let mut report = Report::default();
    let fixture = match pathway {
        Pathway::Single => {
            let text = CASES.iter().map(|case| case.text).collect::<String>();
            println!("SPEC0331 GROUP INPUT single path=whole.ko text={text:?}");
            Fixture::single_whole(&text)
        }
        Pathway::Basic | Pathway::Const => {
            for case in CASES {
                println!(
                    "SPEC0331 GROUP INPUT pathway={pathway:?} reverse={reverse_inputs} path={} text={:?}",
                    case.path, case.text
                );
            }
            Fixture::new(&CASES)
        }
    };
    let result = fixture.and_then(|fixture| match pathway {
        Pathway::Single => single::collect(&fixture, "entry", &mut report),
        Pathway::Basic | Pathway::Const => {
            unit::collect(&fixture, pathway, reverse_inputs, "entry", &mut report)
        }
    });
    Observation {
        label: format!("group-whole-set/{pathway:?}/reverse={reverse_inputs}"),
        report,
        result,
    }
}

#[test]
fn grouped_sources_reject_whole_set_at_first_actual_group_identity() {
    let mut observations = vec![observe(Pathway::Single, false)];
    for pathway in [Pathway::Basic, Pathway::Const] {
        for reverse in [false, true] {
            observations.push(observe(pathway, reverse));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 GROUP OUTCOME {}\n{:#?}\n{:#?}",
            observation.label, observation.result, observation.report
        );
        if let Err(stopped) = &observation.result {
            println!(
                "LOWER_NOT_ATTEMPTED stage={} original={}",
                stopped.stage, stopped.original
            );
        }
    }
    assert!(
        observations.len() == 5 && observations.iter().all(Observation::passed),
        "新 Group 候选必须五路 fresh 完整 FE/ownership/validator/view 后，\
         对真实 Group 集合按当轮 typed identity 精确 whole-For UnsupportedNode；\
         未形成事实保持真实红，不把前置错误、MissingFact 或任意成功算作 U。\n{observations:#?}"
    );
}
