//! SPEC-0331：固定两源 temporary Map 正常耗尽的普通 single/basic native 成功合同。
//! 同源四条 frontend→SSA verifier→LLVM pipeline 真绿后才能另行准入执行。
//! 原 helper 没有内置硬超时；本模块只复用普通 object/link/run，不扩展执行框架。

use super::{assert_result, run_unit};
use crate::{
    native_tests::emit_link_and_run, ssa::map_provider_exits_pipeline_tests::temporary_sources,
};

#[test]
fn single_map_provider_temporary_sources_exit_normally() {
    for (case, _map, _source_hash, source, expected) in temporary_sources() {
        let run = emit_link_and_run("provider.ko", source, "entry");
        assert_result(case, "single", &run, expected);
    }
}

#[test]
fn unit_map_provider_temporary_sources_exit_normally() {
    for (case, _map, _source_hash, source, expected) in temporary_sources() {
        let run = run_unit(case, source);
        assert_result(case, "unit", &run, expected);
    }
}
