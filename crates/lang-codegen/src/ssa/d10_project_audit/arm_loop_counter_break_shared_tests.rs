//! Two distinct original condition expressions may share one complete ABI role.
use super::arm_loop_counter_break_tests::check_with_stop;
use super::*;
#[test]
fn counter_break_shared_flag_preserves_both_condition_identities() {
    for rounds in ["rounds:Int", "own rounds:Int"] {
        for choose in ["choose:Boolean", "own choose:Boolean"] {
            let parameters = ["source:String", rounds, choose];
            for a in 0..3 {
                for b in 0..3 {
                    for c in 0..3 {
                        if [a, b, c].iter().copied().collect::<BTreeSet<_>>().len() != 3 {
                            continue;
                        }
                        check_with_stop(
                            &[parameters[a], parameters[b], parameters[c]].join(","),
                            "choose",
                            true,
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn counter_break_shared_flag_rejects_unclaimed_boolean_abi() {
    for rounds in ["rounds:Int", "own rounds:Int"] {
        for choose in ["choose:Boolean", "own choose:Boolean"] {
            check_with_stop(
                &format!("source:String,{rounds},{choose},unused:Boolean"),
                "choose",
                false,
            );
        }
    }
}
