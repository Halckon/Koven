//! 同根双层正常遍历两源 × single/unit-basic；四次 fresh 观察后统一硬成功。
//! Map Borrow 独立记账；golden 是预测，复杂 owner/exit/cleanup 先完整观察，不预填拓扑。
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
            "mutable-map-nested",
            false,
            "dd0c91d0e149c0b0e527fed025f5cc38c77914d3efef4f6da39f25b041568afd",
            include_str!("../../../tests/fixtures/map-provider-nested-0331/mutable-map-nested.ko"),
            include_str!(
                "../../../tests/fixtures/map-provider-nested-0331/mutable-map-nested.stdout"
            ),
        ),
        (
            "map-borrow-nested",
            true,
            "76b3a5ce538956558e4d89aeb6b290721cd7b60a45e31ba3ae960a4753f76082",
            include_str!("../../../tests/fixtures/map-provider-nested-0331/map-borrow-nested.ko"),
            include_str!(
                "../../../tests/fixtures/map-provider-nested-0331/map-borrow-nested.stdout"
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
        "SPEC0331 NESTED INPUT case={label} unit={unit} path=provider.ko frozen_sha256={source_hash} text={text:?}"
    );
    let outcome = ast::Input::new(text, map).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query(
            "nested actual parent/child For and same-source AST anchors",
            &anchors,
        );
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
            "SPEC0331 NESTED SSA case={label} unit={unit}\n{}",
            crate::ssa::render::render_program(&program)
        );
        // 只复用无单 For/Name/Guard 假设的入口与源码函数可达查询。
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("nested AST identity", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "nested actual entry and four reachable functions",
            &reachable,
        );
        // 只读查询失败留在 report，不隐藏真实 verifier/LLVM 的原错误。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("nested SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("nested LLVM render/verify", true, llvm.len());
        println!("SPEC0331 NESTED LLVM case={label} unit={unit}\n{llvm}");
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
fn map_and_mutable_map_nested_borrow_sources_reach_verified_llvm() {
    let mut observations = Vec::new();
    for (label, map, hash, source, _golden) in sources() {
        for unit in [false, true] {
            observations.push(observe(label, map, hash, source, unit));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 NESTED OUTCOME case={} unit={} report={:#?}",
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
        "四格、八个真实For点必须完成同次frontend、完整validator/view、实际entry、SSA和LLVM；\
         Map Borrow与MutableMap分账；query失败不遮实际lower，UnsupportedNode不是成功oracle。\n{observations:#?}"
    );
}
