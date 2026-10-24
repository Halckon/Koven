//! 独立 ordinary exact：原六源码 × MemoryDocument/rooted Unit 全采后断，不变异、不执行 native。
use super::{
    Observation, Report,
    fixture::{Fixture, failure, verify_failure},
    identity, input_label, pipeline, query_ssa,
};

fn observe(label: &'static str, text: &str, rooted: bool) -> Observation {
    let mut report = Report {
        identity_requested: true,
        ..Report::default()
    };
    let input = input_label(rooted);
    let outcome = Fixture::new(label, text).and_then(|fixture| {
        println!(
            "SPEC0331 EXIT IDENTITY INPUT case={label} input={input} path={:?} text={text:?}",
            fixture.path
        );
        let expected = identity::source_functions(&fixture);
        report.query("identity original source functions", &expected);
        let (program, entry) = if rooted {
            pipeline::unit(&fixture, &mut report)
        } else {
            pipeline::memory_document(&fixture, &mut report)
        }?;
        println!(
            "SPEC0331 EXIT IDENTITY RAW SSA case={label} input={input}\n{}",
            crate::ssa::render::render_program(&program)
        );
        let reachable = expected
            .as_ref()
            .map_err(|e| failure("identity AST prerequisite", e))
            .and_then(|functions| {
                identity::entry_sources(&fixture.sources, &program, entry, functions)
            });
        report.query(
            "identity actual entry and reachable source functions",
            &reachable,
        );
        let guard_operand = expected
            .and_then(|functions| identity::guard_and_operand(&fixture, &program, &functions));
        report.query(
            "identity existing actual Guard/operand query",
            &guard_operand,
        );
        let inputs = report.fe_identity.take().unwrap_or_else(|| {
            Err(failure(
                "identity same-run FE bridge missing",
                (label, input),
            ))
        });
        report.query(
            "identity FE original parameter/call/cleanup bridge",
            &inputs,
        );
        let entities = inputs
            .as_ref()
            .map_err(|e| failure("identity FE prerequisite", e))
            .and_then(|inputs| query_ssa::check(&program, inputs));
        report.query(
            "identity complete SSA entity/cleanup/all-incoming proof",
            &entities,
        );
        // Independent query errors never hide either original verifier or LLVM outcome.
        let verified = crate::ssa::verify::verify_program(&program)
            .map_err(|errors| verify_failure(&fixture.sources, errors));
        report.query("identity pipeline original SSA verifier", &verified);
        let rendered = crate::llvm::render_verified_program(&program)
            .map_err(|error| failure("LLVM render/verify", error));
        report.query(
            "identity pipeline original LLVM render/verify",
            &rendered
                .as_ref()
                .map(|s| s.len())
                .map_err(|e| failure(e.stage, &e.original)),
        );
        if let Ok(llvm) = &rendered {
            println!("SPEC0331 EXIT IDENTITY RAW LLVM case={label} input={input}\n{llvm}");
        }
        // This exact adds SSA identity evidence only; LLVM coverage remains render/verify.
        verified?;
        rendered?;
        Ok(())
    });
    Observation {
        label,
        rooted,
        report,
        outcome,
    }
}

#[test]
fn unconditional_map_exits_preserve_cleanup_and_carrier_identity() {
    let mut observations = Vec::new();
    for (label, source, _golden) in super::sources() {
        for rooted in [false, true] {
            observations.push(observe(label, source, rooted));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 EXIT IDENTITY OUTCOME case={} input={} report={:#?} result={:#?}",
            observation.label,
            input_label(observation.rooted),
            observation.report,
            observation.outcome
        );
    }
    assert!(
        observations.len() == 12
            && observations
                .iter()
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "原六源×MemoryDocument/rooted Unit必须完整采齐，FE/SSA身份查询和原verify/LLVM分别全绿。\n{observations:#?}"
    );
}
