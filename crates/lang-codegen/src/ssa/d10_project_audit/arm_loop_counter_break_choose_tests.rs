//! Independent choose and stop roles retain their original conditional modes.
use super::arm_loop_counter_break_tests::check;
use super::*;
#[test]
fn counter_break_choose_value_keeps_stop_role_independent() {
    for stop in ["stop:Boolean", "own stop:Boolean"] {
        let parameters = ["source:String", "rounds:Int", "own choose:Boolean", stop];
        for a in 0..4 {
            for b in 0..4 {
                for c in 0..4 {
                    for d in 0..4 {
                        if [a, b, c, d].iter().copied().collect::<BTreeSet<_>>().len() != 4 {
                            continue;
                        }
                        check(
                            &[parameters[a], parameters[b], parameters[c], parameters[d]].join(","),
                            true,
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn counter_break_choose_value_keeps_rounds_and_extra_parameters_outside() {
    for signature in [
        "source:String,own rounds:Int,own choose:Boolean,stop:Boolean",
        "source:String,own rounds:Int,own choose:Boolean,own stop:Boolean",
        "source:String,rounds:Int,own choose:Boolean,stop:Boolean,extra:Boolean",
        "source:String,rounds:Int,own choose:Boolean,own stop:Boolean,extra:Boolean",
    ] {
        check(signature, false);
    }
}
