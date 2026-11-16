//! Two distinct original conditions sharing one complete ABI Boolean role.
use super::arm_loop_value_flags_tests::check_with_prefix;
#[test]
fn ordinary_arm_shared_condition_keeps_both_control_points() {
    for parameters in [
        "source:String,flag:Boolean",
        "flag:Boolean,source:String",
        "source:String,own flag:Boolean",
        "own flag:Boolean,source:String",
    ] {
        for reverse in [false, true] {
            for prefix in [None, Some("before(source);")] {
                check_with_prefix(parameters, ["flag", "flag"], reverse, true, prefix);
            }
        }
    }
}
#[test]
fn ordinary_arm_shared_condition_does_not_hide_unused_abi() {
    for parameters in [
        "source:String,flag:Boolean,choose:Boolean",
        "source:String,own flag:Boolean,own choose:Boolean",
    ] {
        check_with_prefix(parameters, ["flag", "flag"], false, false, None);
    }
}
