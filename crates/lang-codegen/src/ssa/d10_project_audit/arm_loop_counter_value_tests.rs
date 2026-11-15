//! Boolean value condition changes no counter arithmetic or source ownership policy.
use super::arm_loop_counter_tests::check;
#[test]
fn counter_boolean_value_keeps_original_condition_transport() {
    for signature in [
        "source:String,rounds:Int,own choose:Boolean",
        "source:String,own choose:Boolean,rounds:Int",
        "rounds:Int,source:String,own choose:Boolean",
        "rounds:Int,own choose:Boolean,source:String",
        "own choose:Boolean,source:String,rounds:Int",
        "own choose:Boolean,rounds:Int,source:String",
    ] {
        check(signature, true);
    }
}
#[test]
fn counter_boolean_value_does_not_authorize_unused_parameters() {
    for signature in [
        "source:String,own rounds:Int,own choose:Boolean,extra:Boolean",
        "source:String,rounds:Int,own choose:Boolean,extra:Boolean",
    ] {
        check(signature, false);
    }
}
