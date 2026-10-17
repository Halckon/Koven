//! 原四格完整 pipeline 的独立 ordinary query；不安装变异，不额外创建源。
use super::{
    Observation, Report,
    fixture::{Fixture, failure, verify_failure},
    identity, pipeline, query_llvm, query_ssa,
};

fn observe(label: &'static str, text: &str, unit: bool) -> Observation {
    let mut report = Report {
        identity_requested: true,
        ..Report::default()
    };
    let outcome = Fixture::new(label, text).and_then(|fixture| {
        println!(
            "SPEC0331 PROVIDER IDENTITY INPUT case={label} unit={unit} path={:?} text={text:?}",
            fixture.path
        );
        let expected = identity::source_functions(&fixture);
        report.query("identity original source function AST", &expected);
        let (program, entry) = if unit {
            pipeline::unit(&fixture, &mut report)
        } else {
            pipeline::single(&fixture, &mut report)
        }?;
        println!(
            "SPEC0331 PROVIDER IDENTITY RAW SSA case={label} unit={unit}\n{}",
            crate::ssa::render::render_program(&program)
        );
        let origin = expected.and_then(|expected| {
            identity::entry_sources(&fixture.sources, &program, entry, &expected)
        });
        report.query(
            "identity actual returned entry and reachable source functions",
            &origin,
        );
        let inputs = report
            .fe_identity
            .take()
            .unwrap_or_else(|| Err(failure("identity FE bridge missing", (label, unit))));
        report.query(
            "identity FE original parameter/call/cleanup bridge",
            &inputs,
        );
        let shape = inputs
            .as_ref()
            .map_err(|e| failure("identity FE prerequisite", e))
            .and_then(|inputs| query_ssa::check(&program, inputs));
        report.query(
            "independent SSA provider entity and full CFG identity",
            &shape,
        );
        // Queries cannot short-circuit the original lower/SSA/LLVM observations.
        let verified = crate::ssa::verify::verify_program(&program)
            .map_err(|errors| verify_failure(&fixture.sources, errors));
        report.query("identity pipeline SSA verifier", &verified);
        let rendered = crate::llvm::render_verified_program(&program)
            .map_err(|error| failure("LLVM render/verify", error));
        report.query(
            "identity pipeline LLVM render/verify",
            &rendered
                .as_ref()
                .map(|s| s.len())
                .map_err(|e| failure(e.stage, &e.original)),
        );
        if let Ok(llvm) = &rendered {
            println!("SPEC0331 PROVIDER IDENTITY RAW LLVM case={label} unit={unit}\n{llvm}");
            let identity = match (&inputs, &shape) {
                (Ok(inputs), Ok(shape)) => query_llvm::check(&program, inputs, shape, llvm),
                _ => Err(failure(
                    "LLVM identity anchors unavailable",
                    (&inputs, &shape),
                )),
            };
            report.query(
                "independent LLVM actual operands phi and checked-slot CFG",
                &identity,
            );
        } else {
            report.check(
                "independent LLVM actual operands phi and checked-slot CFG",
                false,
                "render failed: no artifact to inspect",
            );
        }
        verified?;
        rendered?;
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
fn normal_map_provider_preserves_source_component_cursor_identity() {
    let mut observations = Vec::new();
    for (label, source, _golden) in super::sources() {
        for unit in [false, true] {
            observations.push(observe(label, source, unit));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 PROVIDER IDENTITY OUTCOME case={} unit={} report={:#?} result={:#?}",
            observation.label, observation.unit, observation.report, observation.outcome
        );
    }
    assert!(
        observations.len() == 4
            && observations
                .iter()
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "原两source×single/basic必须完整FE/lower/verify/LLVM及独立实体query全绿；不以verifier替代身份，不跳过失败格。\n{observations:#?}"
    );
}
