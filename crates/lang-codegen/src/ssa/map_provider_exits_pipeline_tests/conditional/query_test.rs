//! 四次 fresh MemoryDocument/rooted Unit 流水线；身份查询只读同次公开产物，原阶段错误全部保留。
#[test]
fn conditional_map_exits_preserve_all_edge_identity() {
    let mut observations = Vec::new();
    for (label, map, hash, text, _golden) in super::sources() {
        for rooted in [false, true] {
            observations.push(super::observe(label, map, hash, text, rooted, true));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 CONDITIONAL IDENTITY OUTCOME case={} input={} report={:#?} result={:#?}",
            observation.label,
            super::input_label(observation.rooted),
            observation.report,
            observation.outcome,
        );
    }
    assert!(
        observations.len() == 4
            && observations
                .iter()
                .all(|observation| { observation.report.passed() && observation.outcome.is_ok() }),
        "原两源×MemoryDocument/rooted Unit四fresh必须完成同次FE、完整validator、独立全edge身份query、SSA和LLVM；\
         查询失败不隐藏原流水线阶段错误，未运行不作通过。\n{observations:#?}"
    );
}
