//! 新 exact 独立十二次 fresh；查询 Err 只入 report，原 validator/SSA/LLVM 错误继续保全。
use super::{
    super::{
        Report,
        fixture::{Failure, failure, verify_failure},
        identity, pipeline,
    },
    Case, Observation, ast, query_inputs, query_ssa, single, unit,
};

fn observe(case: &Case, unit: bool) -> Observation {
    let (label, map, hash, text) = (case.label, case.map, case.source_hash, case.text);
    let mut report = Report::default();
    println!(
        "SPEC0331 NESTED EXITS IDENTITY INPUT case={label} unit={unit} path=provider.ko frozen_sha256={hash} text={text:?}"
    );
    let outcome = ast::Input::new(text, map, case.exit).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query(
            "nested exits identity actual parent/child same-root AST anchors",
            &anchors,
        );
        let mut inputs: Option<Result<query_inputs::Inputs, Failure>> = None;
        let (program, entry) = if unit {
            pipeline::unit_observed(
                &input.sources,
                &input.parsed,
                "provider.ko",
                &mut report,
                |names, typed, owned, report| {
                    unit::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
                    let queried = anchors
                        .as_ref()
                        .map_err(|e| failure("nested exits identity AST for FE", e))
                        .and_then(|a| query_inputs::unit(&input, a, names, typed, owned));
                    report.query(
                        "nested exits identity same-analysis FE source/caller/component/cleanup bridge",
                        &queried,
                    );
                    inputs = Some(queried);
                },
            )
        } else {
            pipeline::single_observed(
                &input.sources,
                &input.parsed,
                &mut report,
                |names, typed, owned, report| {
                    single::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
                    let queried = anchors
                        .as_ref()
                        .map_err(|e| failure("nested exits identity AST for FE", e))
                        .and_then(|a| query_inputs::single(&input, a, names, typed, owned));
                    report.query(
                        "nested exits identity same-analysis FE source/caller/component/cleanup bridge",
                        &queried,
                    );
                    inputs = Some(queried);
                },
            )
        }?;
        println!(
            "SPEC0331 NESTED EXITS IDENTITY SSA case={label} unit={unit}\n{}",
            crate::ssa::render::render_program(&program)
        );
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("nested exits identity actual AST roots", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "nested exits identity actual returned entry and reachable functions",
            &reachable,
        );
        let inputs = inputs.unwrap_or_else(|| {
            Err(failure(
                "nested exits identity same-analysis FE observer",
                "observer did not run",
            ))
        });
        let queried = inputs
            .as_ref()
            .map_err(|e| failure("nested exits identity FE bridge for SSA", e))
            .and_then(|inputs| query_ssa::check(&program, entry, &input, inputs));
        report.query(
            "nested exits identity caller two Guard identities both leases all incoming full cleanup and complete uses",
            &queried,
        );
        // 不使用 query 的 ?：保持原 verifier/LLVM 的真实到达与首错误。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("nested exits identity original SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check(
            "nested exits identity original LLVM render/verify",
            true,
            llvm.len(),
        );
        println!("SPEC0331 NESTED EXITS IDENTITY LLVM case={label} unit={unit}\n{llvm}");
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
fn nested_inner_exits_preserve_full_caller_guard_and_edge_identity() {
    let mut observations = Vec::new();
    for case in super::sources() {
        for unit in [false, true] {
            observations.push(observe(&case, unit));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 NESTED EXITS IDENTITY OUTCOME case={} unit={} report={:#?}",
            observation.label, observation.unit, observation.report
        );
        match &observation.outcome {
            Ok(()) => println!("stage=LLVM render/verify result=ok"),
            Err(error) => println!("stage={} original={}", error.stage, error.original),
        }
    }
    assert!(
        observations.len() == 12
            && observations
                .iter()
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "六份原源×single/unit十二fresh、二十四site必须完成同次FE、完整validator、真实caller/两Guard/两lease/四component全部incoming/uses、SSA和LLVM；\
         query失败与原阶段错误分账，未运行不算通过，不推断动态次数。\n{observations:#?}"
    );
}
