//! 两份本地 named 源的四次 fresh；复杂 owner/Drop 拓扑先采实际事实，不推定。
//! 仅普通 source → frontend/validator → SSA/LLVM；不执行 native。
// The input flag selects rooted Unit or MemoryDocument Unit, never Legacy lowering.
mod ast;
mod deep;
mod unit;

use super::{
    Report,
    fixture::{Failure, failure, verify_failure},
    identity, pipeline,
};

/// bool 只表示源中是否有循环后 size 读取，不代替实际 AST/typed 身份。
fn sources() -> [(&'static str, bool, &'static str, &'static str, &'static str); 2] {
    [
        (
            "local-formation",
            false,
            "707db786173cb3d8a458f378a92eb10847862fa82daa07cfc1d687966c7b2a6b",
            include_str!(
                "../../../tests/fixtures/map-provider-local-sources-0331/local-formation.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-local-sources-0331/local-formation.stdout"
            ),
        ),
        (
            "local-later-use",
            true,
            "48dcc464579648a53127944e0aef5eff2c30095969fb1295808887008dd0b3a4",
            include_str!(
                "../../../tests/fixtures/map-provider-local-sources-0331/local-later-use.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-local-sources-0331/local-later-use.stdout"
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
    later: bool,
    source_hash: &str,
    text: &str,
    unit: bool,
    deep_query: bool,
) -> Observation {
    let mut report = Report::default();
    // SHA 是仓外核验的冻结输入标识，不声称此处运行时计算哈希或执行 native。
    println!(
        "SPEC0331 LOCAL INPUT case={label} unit={unit} path=provider.ko frozen_sha256={source_hash} text={text:?}"
    );
    let outcome = ast::Input::new(text, later).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query(
            "local four scanners and eight independent AST sites",
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
                    if deep_query {
                        let query = anchors
                            .as_ref()
                            .map_err(|e| failure("local deep AST", e))
                            .and_then(|a| deep::unit(&input, a, names, typed, owned));
                        report.query("local independent unit last-use obligations", &query);
                    }
                },
            )
        } else {
            pipeline::memory_document_observed(
                &input.sources,
                &input.parsed,
                &mut report,
                |names, typed, owned, report| {
                    unit::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
                    if deep_query {
                        let query = anchors
                            .as_ref()
                            .map_err(|e| failure("local deep AST", e))
                            .and_then(|a| deep::unit(&input, a, names, typed, owned));
                        report.query("local independent unit last-use obligations", &query);
                    }
                },
            )
        }?;
        println!(
            "SPEC0331 LOCAL SSA case={label} unit={unit}\n{}",
            crate::ssa::render::render_program(&program)
        );
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("local AST identity", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "local actual entry and all ten reachable source functions",
            &reachable,
        );
        if deep_query {
            let query = anchors
                .as_ref()
                .map_err(|e| failure("local deep AST", e))
                .and_then(|a| deep::ssa(&input, a, &program));
            report.query(
                "local complete SSA transport and exact last-use consumers",
                &query,
            );
        }
        // 独立 query 失败仍保留原 SSA verifier / LLVM 错误；不替换其成功判据。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("local SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("local LLVM render/verify", true, llvm.len());
        println!("SPEC0331 LOCAL LLVM case={label} unit={unit}\n{llvm}");
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
fn owned_local_formation_and_later_use_sources_reach_verified_llvm() {
    let mut observations = Vec::new();
    for (label, later, source_hash, source, _predicted_golden) in sources() {
        for unit in [false, true] {
            observations.push(observe(label, later, source_hash, source, unit, false));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 LOCAL OUTCOME case={} unit={} report={:#?}",
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
        "两源×双端四次fresh必须同源frontend/完整validator/SSA/LLVM全成功；\
         32个FE site不代表32次lower，whole-set仅按真实首停记账。\n{observations:#?}"
    );
}

/// 同字节两源与完整 golden，供后续独立准入的普通 native 测试复用。
/// 保持 sources 私有，不改变首证 collector 的任何阶段或成功条件。
pub(crate) fn native_sources() -> [(&'static str, &'static str, &'static str); 2] {
    sources().map(|(label, _, _, source, golden)| (label, source, golden))
}

#[test]
fn local_sources_preserve_all_incoming_and_last_use_owner_identity() {
    let observations = sources()
        .into_iter()
        .flat_map(|(label, later, hash, source, _)| {
            [false, true].map(|unit| observe(label, later, hash, source, unit, true))
        })
        .collect::<Vec<_>>();
    assert!(
        observations.len() == 4
            && observations
                .iter()
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "ordinary sources must retain same-analysis FE owner topology, all SSA incoming and last-use identity, and independent SSA/LLVM success: {observations:#?}"
    );
}
