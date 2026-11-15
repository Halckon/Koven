//! Original rounds Value seeds the checked counter without a fabricated Read.
use super::arm_loop_counter_break_tests::check;
use super::*;
#[test]
fn counter_break_rounds_value_preserves_initial_and_conditional_roles() {
    for choose in ["choose:Boolean", "own choose:Boolean"] {
        for stop in ["stop:Boolean", "own stop:Boolean"] {
            let parameters = ["source:String", "own rounds:Int", choose, stop];
            for a in 0..4 {
                for b in 0..4 {
                    for c in 0..4 {
                        for d in 0..4 {
                            if [a, b, c, d].iter().copied().collect::<BTreeSet<_>>().len() != 4 {
                                continue;
                            }
                            check(
                                &[parameters[a], parameters[b], parameters[c], parameters[d]]
                                    .join(","),
                                true,
                            );
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn counter_break_rounds_value_keeps_unclaimed_abi_outside() {
    for choose in ["choose:Boolean", "own choose:Boolean"] {
        for stop in ["stop:Boolean", "own stop:Boolean"] {
            check(
                &format!("source:String,own rounds:Int,{choose},{stop},extra:Boolean"),
                false,
            );
        }
    }
}
