//! Original stop Value identity is retained without a fabricated Read or End.
use super::arm_loop_counter_break_tests::check;
use super::*;
#[test]
fn counter_break_stop_value_preserves_exit_and_checked_paths() {
    let parameters = [
        "source:String",
        "rounds:Int",
        "choose:Boolean",
        "own stop:Boolean",
    ];
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
#[test]
fn counter_break_stop_value_keeps_unclaimed_abi_outside() {
    for signature in [
        "source:String,own rounds:Int,choose:Boolean,own stop:Boolean,extra:Boolean",
        "source:String,own rounds:Int,own choose:Boolean,own stop:Boolean,extra:Boolean",
        "source:String,rounds:Int,choose:Boolean,own stop:Boolean,extra:Boolean",
    ] {
        check(signature, false);
    }
}
