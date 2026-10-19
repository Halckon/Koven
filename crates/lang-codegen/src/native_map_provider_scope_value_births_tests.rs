//! Only the fixed Int storage group is registered in this bounded repair.
mod int_storage {
    use super::super::{assert_result, run_unit};
    use crate::{
        native_tests::emit_link_and_run,
        ssa::map_provider_exits_pipeline_tests::scope_int_storage_sources,
    };
    fn run(unit: bool) {
        for c in scope_int_storage_sources() {
            let output = if unit {
                run_unit(c.label, c.source)
            } else {
                emit_link_and_run("provider.ko", c.source, "entry")
            };
            assert_result(
                c.label,
                if unit { "unit" } else { "single" },
                &output,
                c.golden,
            );
        }
    }
    #[test]
    fn single_original_int_storage_keeps_full_golden() {
        run(false);
    }
    #[test]
    fn unit_original_int_storage_keeps_full_golden() {
        run(true);
    }
}
