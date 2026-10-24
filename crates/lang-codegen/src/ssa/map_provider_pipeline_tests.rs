//! SPEC-0331 的普通 Map provider 成功合同；两种真实 Unit 输入的四格全收齐后断言。
//! 不执行 native，不以当前 lower 的 UnsupportedNode 债务作为通过条件。

mod facts;
mod fixture;
mod identity;
mod pipeline;
mod query_inputs;
mod query_llvm;
mod query_owner;
mod query_ssa;
mod query_test;

use fixture::{Failure, Fixture};

/// 同字节源码和拟议 stdout，供后续独立获准的 native 测试复用。
/// 本 exact 只消费源码；golden 是期望，不是执行结果。
pub(crate) fn sources() -> [(&'static str, &'static str, &'static str); 2] {
    [
        (
            "map",
            include_str!("../../tests/fixtures/map-provider-0331/map.ko"),
            include_str!("../../tests/fixtures/map-provider-0331/map.stdout"),
        ),
        (
            "mutable-map",
            include_str!("../../tests/fixtures/map-provider-0331/mutable-map.ko"),
            include_str!("../../tests/fixtures/map-provider-0331/mutable-map.stdout"),
        ),
    ]
}

#[derive(Debug, Default)]
struct Report {
    identity_requested: bool,
    fe_identity: Option<Result<query_inputs::Inputs, Failure>>,
    checks: Vec<(&'static str, bool, String)>,
}

impl Report {
    fn check(&mut self, name: &'static str, passed: bool, detail: impl std::fmt::Debug) {
        self.checks.push((name, passed, format!("{detail:?}")));
    }

    fn query<T: std::fmt::Debug>(&mut self, name: &'static str, result: &Result<T, Failure>) {
        self.check(name, result.is_ok(), result);
    }

    fn passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|(_, passed, _)| *passed)
    }
}

#[derive(Debug)]
struct Observation {
    label: &'static str,
    unit: bool,
    report: Report,
    outcome: Result<(), Failure>,
}

fn observe(label: &'static str, text: &str, unit: bool) -> Observation {
    let mut report = Report::default();
    let outcome = Fixture::new(label, text).and_then(|fixture| {
        // false 是 MemoryDocument，true 是原 logical-path Unit；各自保持原始单源字节。
        println!(
            "SPEC0331 PROVIDER INPUT case={label} unit={unit} path={:?} text={text:?}",
            fixture.path
        );
        let expected_functions = identity::source_functions(&fixture);
        report.query("source function AST identity", &expected_functions);
        let lowered = if unit {
            pipeline::unit(&fixture, &mut report)
        } else {
            pipeline::single(&fixture, &mut report)
        }?;
        let (program, entry) = lowered;
        println!(
            "SPEC0331 PROVIDER SSA case={label} unit={unit}\n{}",
            super::render::render_program(&program)
        );
        // 只读 query 的失败留在独立 report，不吞 lower 已到达的 verify/LLVM 原错误。
        let identity = expected_functions.and_then(|expected| {
            identity::entry_sources(&fixture.sources, &program, entry, &expected)
        });
        report.query("returned entry and reachable source functions", &identity);
        super::verify::verify_program(&program)
            .map_err(|errors| fixture::verify_failure(&fixture.sources, errors))?;
        report.check("SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|error| fixture::failure("LLVM render/verify", error))?;
        report.check("LLVM render/verify", true, llvm.len());
        println!("SPEC0331 PROVIDER LLVM case={label} unit={unit}\n{llvm}");
        Ok(())
    });
    Observation {
        label,
        unit,
        report,
        outcome,
    }
}

#[test]
fn map_and_mutable_map_normal_sources_reach_verified_llvm() {
    let mut observations = Vec::new();
    for (label, source, _golden) in sources() {
        for unit in [false, true] {
            observations.push(observe(label, source, unit));
        }
    }
    // 所有真实观察结束后只有这一个硬成功断言，首红不能短路另外三格。
    for observation in &observations {
        println!(
            "SPEC0331 PROVIDER OUTCOME case={} unit={} report={:#?}",
            observation.label, observation.unit, observation.report
        );
        match &observation.outcome {
            Ok(()) => println!("stage=LLVM render/verify result=ok"),
            Err(error) => println!("stage={} original={}", error.stage, error.original),
        }
    }
    assert!(
        observations.len() == 4
            && observations
                .iter()
                .all(|observation| { observation.report.passed() && observation.outcome.is_ok() }),
        "四格必须是同源完整 frontend、真实 entry/CFG 可达身份、SSA 与 LLVM 全成功；\
         原阶段失败保持失败，UnsupportedNode 不是成功 oracle。\n{observations:#?}"
    );
}
