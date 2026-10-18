//! 两份原字节 temporary source × single/unit；四次完整观察后统一硬成功断言。
//! 每份 entry 有两个真实 For；golden 仅留给后续独立准入的 native，不是执行结果。
mod ast;
mod single;
mod unit;

use super::{
    Report,
    fixture::{Failure, failure, verify_failure},
    identity, pipeline,
};

pub(crate) fn sources() -> [(&'static str, bool, &'static str, &'static str, &'static str); 2] {
    [
        (
            "map-temporary",
            true,
            "17546f80b9cadebf9d1ca24f679e1a9a9ad6dc9076101acfd96fd3007ab40016",
            include_str!("../../../tests/fixtures/map-provider-temporary-0331/map-temporary.ko"),
            include_str!(
                "../../../tests/fixtures/map-provider-temporary-0331/map-temporary.stdout"
            ),
        ),
        (
            "mutable-map-temporary",
            false,
            "64d53cc0799ec7b68303f8aa1bfc5b5e4db4d04a73ddf3b24c54655b4289c5dd",
            include_str!(
                "../../../tests/fixtures/map-provider-temporary-0331/mutable-map-temporary.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-temporary-0331/mutable-map-temporary.stdout"
            ),
        ),
    ]
}

#[derive(Debug)]
struct Observation {
    label: &'static str,
    unit: bool,
    report: Report,
    outcome: Result<(), Failure>,
}

fn observe(
    label: &'static str,
    map: bool,
    source_hash: &str,
    text: &str,
    unit: bool,
) -> Observation {
    let mut report = Report::default();
    // SHA 是仓外核验的冻结输入标识；不声称这里进行了运行时哈希或 native 求值。
    println!(
        "SPEC0331 TEMPORARY INPUT case={label} unit={unit} path=provider.ko frozen_sha256={source_hash} text={text:?}"
    );
    let outcome = ast::Input::new(text, map).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query("temporary actual two Call-source For AST anchors", &anchors);
        let (program, entry) = if unit {
            pipeline::unit_observed(
                &input.sources,
                &input.parsed,
                "provider.ko",
                &mut report,
                |names, typed, owned, report| {
                    unit::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
                },
            )
        } else {
            pipeline::single_observed(
                &input.sources,
                &input.parsed,
                &mut report,
                |names, typed, owned, report| {
                    single::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
                },
            )
        }?;
        println!(
            "SPEC0331 TEMPORARY SSA case={label} unit={unit}\n{}",
            crate::ssa::render::render_program(&program)
        );
        // 只复用无单 For/Name/Guard 假设的入口与源码函数可达查询。
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("temporary AST identity", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "temporary actual entry and four reachable functions",
            &reachable,
        );
        // 只读查询失败留在 report，不隐藏真实 verifier/LLVM 的原错误。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("temporary SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("temporary LLVM render/verify", true, llvm.len());
        println!("SPEC0331 TEMPORARY LLVM case={label} unit={unit}\n{llvm}");
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
fn map_and_mutable_map_temporary_sources_reach_verified_llvm() {
    let mut observations = Vec::new();
    for (label, map, hash, source, _golden) in sources() {
        for unit in [false, true] {
            observations.push(observe(label, map, hash, source, unit));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 TEMPORARY OUTCOME case={} unit={} report={:#?}",
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
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "四格、八个真实For点必须完成同源frontend、完整validator/view、实际entry、SSA和LLVM；\
         query失败与原阶段失败分账，UnsupportedNode不是成功oracle。\n{observations:#?}"
    );
}
