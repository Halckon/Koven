//! Original Value Int initialization has no fabricated Read, loan, or End.
use super::arm_loop_counter_tests::check;
#[test]
fn counter_value_rounds_preserves_original_initial_and_backedge_identity() {
    for signature in [
        "source:String,own rounds:Int,choose:Boolean",
        "source:String,choose:Boolean,own rounds:Int",
        "own rounds:Int,source:String,choose:Boolean",
        "own rounds:Int,choose:Boolean,source:String",
        "choose:Boolean,source:String,own rounds:Int",
        "choose:Boolean,own rounds:Int,source:String",
    ] {
        for value_choice in [false, true] {
            let signature = if value_choice {
                signature.replace("choose:Boolean", "own choose:Boolean")
            } else {
                signature.into()
            };
            check(&signature, true);
        }
    }
}
#[test]
fn counter_value_rounds_keeps_extra_abi_outside() {
    for signature in [
        "source:String,own rounds:Int,choose:Boolean,extra:Boolean",
        "source:String,own rounds:Int,own choose:Boolean,extra:Boolean",
    ] {
        check(signature, false);
    }
}
