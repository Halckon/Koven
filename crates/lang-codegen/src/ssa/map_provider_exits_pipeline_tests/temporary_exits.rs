//! temporary Call source continue/break/return 的独立首证 collector。
//! 十二次 fresh 同源 pipeline 全部观察后统一硬成功；不执行 native，不预言复杂 cleanup 拓扑。
mod ast;
mod calls_single;
mod calls_unit;
mod guards;
mod single;
mod unit;

use super::{
    Report,
    fixture::{Failure, failure, verify_failure},
    identity, pipeline,
};
use ast::Exit;

struct Case {
    label: &'static str,
    map: bool,
    exit: Exit,
    source_hash: &'static str,
    golden_hash: &'static str,
    text: &'static str,
    golden: &'static str,
}

fn sources() -> [Case; 6] {
    [
        Case {
            label: "map-temporary-continue",
            map: true,
            exit: Exit::Continue,
            source_hash: "a5071340c9ff411a2b3bbd718845972a792abc0330160f099b229b31b5e17dc2",
            golden_hash: "45604a95bf7d814e739b2f0df7ffddd7b08c19d672bb55b552918e2dd63a767b",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-continue.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-continue.stdout"
            ),
        },
        Case {
            label: "map-temporary-break",
            map: true,
            exit: Exit::Break,
            source_hash: "322e3067889445a2d726c062e46c4c91ab2b964f8093ffd72c038a2ae234c420",
            golden_hash: "bd12c9d7fbcf2d28a05a9979a6e73fd2df5bf7a3f0883cbd9ab4d5a340cbd957",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-break.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-break.stdout"
            ),
        },
        Case {
            label: "map-temporary-return",
            map: true,
            exit: Exit::Return,
            source_hash: "4405c5c5249842eba5a53ed010ebe0232d3409bc374aac60418add5613238212",
            golden_hash: "487425b0aaec53ffb4462fdc3bfac85613eb107f666f57ba6abd5c364cb8fc98",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-return.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/map-temporary-return.stdout"
            ),
        },
        Case {
            label: "mutable-map-temporary-continue",
            map: false,
            exit: Exit::Continue,
            source_hash: "db2355c3c578ab306115f6ac26fdfd9f0bfb5fbf0a5f3680c997143262d147d2",
            golden_hash: "f0e831442c4c142840327c92903e47b86dfdb16d4c303ea9945cbb2276027fc1",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-continue.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-continue.stdout"
            ),
        },
        Case {
            label: "mutable-map-temporary-break",
            map: false,
            exit: Exit::Break,
            source_hash: "aae92d25d99f39637b01de43f418bc3099f94c04e32add62f2caa08dfec94811",
            golden_hash: "543c9ca03eb1954d929ceb4c5617eaebc162be68c2fd53bc14f0697eec99475f",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-break.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-break.stdout"
            ),
        },
        Case {
            label: "mutable-map-temporary-return",
            map: false,
            exit: Exit::Return,
            source_hash: "e90cee83581136e06a39f97035b6f89004c86ea7b56fd47d0d7b73e63083a79b",
            golden_hash: "721e19210c52871e2e609a9fa0e590e76f7a720e6f4fa9798f951722ba5827e2",
            text: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-return.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-temporary-exits-0331/mutable-map-temporary-return.stdout"
            ),
        },
    ]
}

#[derive(Debug)]
struct Observation {
    label: &'static str,
    unit: bool,
    report: Report,
    outcome: Result<(), Failure>,
}

fn observe(case: &Case, unit: bool) -> Observation {
    let mut report = Report::default();
    // SHA 是经仓外逐字复核的冻结输入标识；这里未做运行时哈希或 native 求值。
    println!(
        "SPEC0331 TEMPORARY EXITS INPUT case={} unit={unit} path=provider.ko frozen_sha256={} golden_sha256={} golden_bytes={} golden_status=predicted_only text={:?}",
        case.label,
        case.source_hash,
        case.golden_hash,
        case.golden.len(),
        case.text
    );
    let outcome = ast::Input::new(case.text, case.map, case.exit).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query(
            "temporary exits actual two Call-source For sites, Guards, calls and caller identities",
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
            "SPEC0331 TEMPORARY EXITS SSA case={} unit={unit}\n{}",
            case.label,
            crate::ssa::render::render_program(&program)
        );
        // 无单层/固定 read 数假设的源码 origin 与真实可达函数身份查询。
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("temporary exits AST identity", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "temporary exits actual entry and reachable source functions/deinits",
            &reachable,
        );
        // 查询失败只记 report；不能遮住真实 verifier/LLVM 的首断点。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("temporary exits SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("temporary exits LLVM render/verify", true, llvm.len());
        println!(
            "SPEC0331 TEMPORARY EXITS LLVM case={} unit={unit}\n{llvm}",
            case.label
        );
        Ok(())
    });
    Observation {
        label: case.label,
        unit,
        report,
        outcome,
    }
}

#[test]
fn map_and_mutable_map_temporary_exits_reach_verified_llvm() {
    let mut observations = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            observations.push(observe(&case, unit));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 TEMPORARY EXITS OUTCOME case={} unit={} report={:#?}",
            observation.label, observation.unit, observation.report
        );
        match &observation.outcome {
            Ok(()) => println!("stage=LLVM render/verify result=ok"),
            Err(error) => println!("stage={} original={}", error.stage, error.original),
        }
    }
    // 不以缺事实或 UnsupportedNode 为成功；不因首红丢掉其余十一格。
    assert!(
        observations.len() == 12
            && observations
                .iter()
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "十二格、二十四真实 For 必须完成同次 frontend、完整 validator/view、真实 lower、SSA 与 LLVM；\
         query 失败不遮真实阶段，缺事实/UnsupportedNode 不算成功；本首证不宣称全实体/全 edge 闭包或 native。\n{observations:#?}"
    );
}
