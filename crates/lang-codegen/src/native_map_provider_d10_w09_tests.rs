//! D10 原 w09 的两条 native 正例；只复用既有 object/link/run helper。
//! helper 没有内置硬超时。执行须在同源 SSA/LLVM 门通过后独立准入。

use super::{assert_result, run_unit_with_entry};
use crate::native_tests::emit_link_and_run;

const PATH: &str = "w09_map_temp_source.ko";
const SOURCE: &str =
    include_str!("../tests/fixtures/d10-unit-migration-0341/w09_map_temp_source.ko");

#[test]
fn single_original_w09_temporary_map_carry_exits_normally() {
    let run = emit_link_and_run(PATH, SOURCE, "main");
    assert_result(PATH, "single", &run, "map-temp\n");
}

#[test]
fn unit_original_w09_temporary_map_carry_exits_normally() {
    let run = run_unit_with_entry(PATH, PATH, SOURCE, "main");
    assert_result(PATH, "unit", &run, "map-temp\n");
}
