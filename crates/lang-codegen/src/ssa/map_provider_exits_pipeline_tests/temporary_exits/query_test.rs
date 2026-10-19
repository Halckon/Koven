//! New exact: collect all twelve fresh pipelines before asserting; query Err still verifies LLVM.
use super::{
    super::{
        Report,
        fixture::{Failure, failure, verify_failure},
        identity, pipeline,
    },
    Case, Observation, ast, query_inputs, query_ssa, single, sources, unit,
};

fn observe(case: &Case, unit: bool) -> Observation {
    let mut report = Report::default();
    // SHA 是经仓外逐字复核的冻结输入标识；这里未做运行时哈希或 native 求值。
    println!(
        "SPEC0331 TEMPORARY EXITS IDENTITY INPUT case={} unit={unit} path=provider.ko frozen_sha256={} golden_sha256={} golden_bytes={} golden_status=predicted_only text={:?}",
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
                        .map_err(|e| failure("temporary exits identity AST for FE", e))
                        .and_then(|a| query_inputs::unit(&input, a, names, typed, owned));
                    report.query("temporary exits identity same-analysis FE bridge", &queried);
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
                        .map_err(|e| failure("temporary exits identity AST for FE", e))
                        .and_then(|a| query_inputs::single(&input, a, names, typed, owned));
                    report.query("temporary exits identity same-analysis FE bridge", &queried);
                    inputs = Some(queried);
                },
            )
        }?;
        println!(
            "SPEC0331 TEMPORARY EXITS IDENTITY SSA case={} unit={unit}\n{}",
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
        let inputs = inputs.unwrap_or_else(|| {
            Err(failure(
                "temporary exits identity same-analysis observer",
                "observer did not run",
            ))
        });
        let queried = inputs
            .as_ref()
            .map_err(|e| failure("temporary exits identity FE bridge for SSA", e))
            .and_then(|inputs| {
                super::super::fixture::need(
                    inputs.unit.is_some() == unit,
                    "temporary exits identity actual same-pipeline tag",
                    inputs.unit,
                )?;
                query_ssa::check(&program, entry, &input, inputs)
            });
        report.query(
            "temporary exits identity actual factory/owner/lease/Guard/all-edge/all-use closure",
            &queried,
        );
        // 查询失败只记 report；不能遮住真实 verifier/LLVM 的首断点。
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("temporary exits SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("temporary exits LLVM render/verify", true, llvm.len());
        println!(
            "SPEC0331 TEMPORARY EXITS IDENTITY LLVM case={} unit={unit}\n{llvm}",
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
fn temporary_map_exits_preserve_factory_owner_and_all_edge_identity() {
    let mut observations = Vec::new();
    for case in sources() {
        for unit in [false, true] {
            observations.push(observe(&case, unit));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 TEMPORARY EXITS IDENTITY OUTCOME case={} unit={} report={:#?}",
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
         query 失败不遮真实阶段，缺事实/UnsupportedNode 不算成功；本查询只证静态身份/全 edge/use，不声称动态次数、leak 或 native。\n{observations:#?}"
    );
}
