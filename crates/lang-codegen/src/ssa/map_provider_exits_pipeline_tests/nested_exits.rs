//! 同根双层 inner continue/break/return 的独立首证 collector。
//! 十二次 fresh 同源 pipeline 全部观察后统一硬成功；不执行 native，不预言复杂 cleanup 拓扑。
mod ast;
mod guards;
mod query_body;
mod query_callers;
mod query_graph;
mod query_inputs;
mod query_resources;
mod query_ssa;
mod query_tail;
mod query_test;
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
            label: "map-borrow-nested-continue",
            map: true,
            exit: Exit::Continue,
            source_hash: "58fd50d10b4f68127c146702aa2113be739a0504681b2d88877b2fa43843a964",
            golden_hash: "705c097f36e3a9f41f858997ab6a96f5f4e17e3db724083b2763fed28fa5594d",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-continue.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-continue.stdout"
            ),
        },
        Case {
            label: "map-borrow-nested-break",
            map: true,
            exit: Exit::Break,
            source_hash: "a46755450fa6e24db7a589366263d0153c28c805e06f42ca97cbe0764989be76",
            golden_hash: "583a0d8bfa98cddb8f1eb5bb19f583bfd6d20363c6eb5721ddb568d813f57722",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-break.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-break.stdout"
            ),
        },
        Case {
            label: "map-borrow-nested-return",
            map: true,
            exit: Exit::Return,
            source_hash: "e8f03c0e07445c9842772d07198fd2d42898b3ee9987538902b30751813762a6",
            golden_hash: "0f96d2872f26120fc4b29614689a76c8418b3ac4fc6c2bb63cfeebace937b6aa",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-return.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/map-borrow-nested-return.stdout"
            ),
        },
        Case {
            label: "mutable-map-nested-continue",
            map: false,
            exit: Exit::Continue,
            source_hash: "c1a738c7d4a7d73e75af8fd7339d98c55a5fcf4d30c657b9256828d827a7a4db",
            golden_hash: "705c097f36e3a9f41f858997ab6a96f5f4e17e3db724083b2763fed28fa5594d",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-continue.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-continue.stdout"
            ),
        },
        Case {
            label: "mutable-map-nested-break",
            map: false,
            exit: Exit::Break,
            source_hash: "62ade3dad99e15685359bc47e4251417f0424726b45edba9cfb04747b5f1ed40",
            golden_hash: "583a0d8bfa98cddb8f1eb5bb19f583bfd6d20363c6eb5721ddb568d813f57722",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-break.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-break.stdout"
            ),
        },
        Case {
            label: "mutable-map-nested-return",
            map: false,
            exit: Exit::Return,
            source_hash: "a244449378282b40055e5468b46507765f0b50f860b84b61dd9fecd999159835",
            golden_hash: "0f96d2872f26120fc4b29614689a76c8418b3ac4fc6c2bb63cfeebace937b6aa",
            text: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-return.ko"
            ),
            golden: include_str!(
                "../../../tests/fixtures/map-provider-nested-exits-0331/mutable-map-nested-return.stdout"
            ),
        },
    ]
}

#[derive(Debug)]
struct Observation {
    label: &'static str,
    rooted: bool,
    report: Report,
    outcome: Result<(), Failure>,
}

fn observe(case: &Case, rooted: bool) -> Observation {
    let mut report = Report::default();
    // SHA 是经仓外逐字复核的冻结输入标识；这里未做运行时哈希或 native 求值。
    println!(
        "SPEC0331 NESTED EXITS INPUT case={} rooted={rooted} path=provider.ko frozen_sha256={} golden_sha256={} golden_bytes={} golden_status=predicted_only text={:?}",
        case.label,
        case.source_hash,
        case.golden_hash,
        case.golden.len(),
        case.text
    );
    let outcome = ast::Input::new(case.text, case.map, case.exit).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query(
            "nested exits actual AST parent/child For, Guards, calls and caller identities",
            &anchors,
        );
        let (program, entry) = pipeline::unit_input_observed(
            &input.sources,
            &input.parsed,
            "provider.ko",
            &mut report,
            rooted,
            |names, typed, owned, report| {
                unit::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
            },
        )?;
        println!(
            "SPEC0331 NESTED EXITS SSA case={} rooted={rooted}\n{}",
            case.label,
            crate::ssa::render::render_program(&program)
        );
        // 无单层/固定 read 数假设的源码 origin 与真实可达函数身份查询。
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("nested exits AST identity", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "nested exits actual entry and reachable source functions/deinits",
            &reachable,
        );
        // 查询失败只记 report；不能遮住真实 verifier/LLVM 的首断点。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("nested exits SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("nested exits LLVM render/verify", true, llvm.len());
        println!(
            "SPEC0331 NESTED EXITS LLVM case={} rooted={rooted}\n{llvm}",
            case.label
        );
        Ok(())
    });
    Observation {
        label: case.label,
        rooted,
        report,
        outcome,
    }
}

#[test]
fn map_and_mutable_map_nested_inner_exits_reach_verified_llvm() {
    let mut observations = Vec::new();
    for case in sources() {
        for rooted in [false, true] {
            observations.push(observe(&case, rooted));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 NESTED EXITS OUTCOME case={} rooted={} report={:#?}",
            observation.label, observation.rooted, observation.report
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

/// 同字节六源与完整 golden，供后续独立准入的普通 native 测试复用。
/// 保持 Case/Exit 私有，不改变首证 collector 的任何阶段或成功条件。
pub(crate) fn native_sources() -> [(&'static str, &'static str, &'static str); 6] {
    sources().map(|case| (case.label, case.text, case.golden))
}
