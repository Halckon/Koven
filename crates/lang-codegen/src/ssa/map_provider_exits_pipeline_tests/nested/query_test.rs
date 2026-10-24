//! 新 exact 独立四次 fresh；查询 Err 只入 report，原 validator/SSA/LLVM 错误继续保全。
use super::{
    super::{
        Report,
        fixture::{Failure, failure, verify_failure},
        identity, pipeline,
    },
    Observation, ast, query_inputs, query_ssa, unit,
};

fn observe(label: &'static str, map: bool, hash: &str, text: &str, rooted: bool) -> Observation {
    let mut report = Report::default();
    println!(
        "SPEC0331 NESTED IDENTITY INPUT case={label} rooted={rooted} path=provider.ko frozen_sha256={hash} text={text:?}"
    );
    let outcome = ast::Input::new(text, map).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query(
            "nested identity actual parent/child same-root AST anchors",
            &anchors,
        );
        let mut inputs: Option<Result<query_inputs::Inputs, Failure>> = None;
        let (program, entry) = pipeline::unit_input_observed(
            &input.sources,
            &input.parsed,
            "provider.ko",
            &mut report,
            rooted,
            |names, typed, owned, report| {
                unit::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
                let queried = anchors
                    .as_ref()
                    .map_err(|e| failure("nested identity AST for FE", e))
                    .and_then(|a| query_inputs::unit(&input, a, names, typed, owned));
                report.query(
                    "nested identity same-analysis FE source/caller/component/cleanup bridge",
                    &queried,
                );
                inputs = Some(queried);
            },
        )?;
        println!(
            "SPEC0331 NESTED IDENTITY SSA case={label} rooted={rooted}\n{}",
            crate::ssa::render::render_program(&program)
        );
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("nested identity actual AST roots", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "nested identity actual returned entry and reachable functions",
            &reachable,
        );
        let inputs = inputs.unwrap_or_else(|| {
            Err(failure(
                "nested identity same-analysis FE observer",
                "observer did not run",
            ))
        });
        let queried = inputs
            .as_ref()
            .map_err(|e| failure("nested identity FE bridge for SSA", e))
            .and_then(|inputs| query_ssa::check(&program, entry, &input, inputs));
        report.query(
            "nested identity two distinct leases all incoming current outer resume and complete uses",
            &queried,
        );
        // 不使用 query 的 ?：保持原 verifier/LLVM 的真实到达与首错误。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("nested identity original SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check(
            "nested identity original LLVM render/verify",
            true,
            llvm.len(),
        );
        println!("SPEC0331 NESTED IDENTITY LLVM case={label} rooted={rooted}\n{llvm}");
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
fn nested_map_sources_preserve_caller_and_all_edge_identity() {
    let mut observations = Vec::new();
    for (label, map, hash, text, _golden) in super::sources() {
        for rooted in [false, true] {
            observations.push(observe(label, map, hash, text, rooted));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 NESTED IDENTITY OUTCOME case={} rooted={} report={:#?}",
            observation.label, observation.rooted, observation.report
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
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "两份原源×MemoryDocument/rooted Unit四fresh、八site必须完成同次FE、完整validator、真实caller/两lease/四component全部incoming/uses、SSA和LLVM；\
         query失败与原阶段错误分账，未运行不算通过，不推断动态次数。\n{observations:#?}"
    );
}
