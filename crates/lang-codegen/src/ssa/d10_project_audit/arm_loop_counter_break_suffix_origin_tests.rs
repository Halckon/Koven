//! Exact source suffix closure after normal counter-break exits.
use super::arm_loop_counter_break_tests::check_with_suffix;
use super::*;
#[test]
fn counter_break_source_suffix_closes_both_original_exit_paths() {
    for own_rounds in [false, true] {
        for own_choose in [false, true] {
            for own_stop in [false, true] {
                let rounds = if own_rounds {
                    "own rounds:Int"
                } else {
                    "rounds:Int"
                };
                let choose = if own_choose {
                    "own choose:Boolean"
                } else {
                    "choose:Boolean"
                };
                let stop = if own_stop {
                    "own stop:Boolean"
                } else {
                    "stop:Boolean"
                };
                let params = ["source:String", rounds, choose, stop];
                for a in 0..4 {
                    for b in 0..4 {
                        for c in 0..4 {
                            for d in 0..4 {
                                if [a, b, c, d].iter().copied().collect::<BTreeSet<_>>().len() != 4
                                {
                                    continue;
                                }
                                check_with_suffix(
                                    &[params[a], params[b], params[c], params[d]].join(","),
                                    "stop",
                                    ";consume(source)",
                                    true,
                                );
                            }
                        }
                    }
                }
                if own_choose == own_stop {
                    let params = ["source:String", rounds, choose];
                    for a in 0..3 {
                        for b in 0..3 {
                            for c in 0..3 {
                                if [a, b, c].iter().copied().collect::<BTreeSet<_>>().len() != 3 {
                                    continue;
                                }
                                check_with_suffix(
                                    &[params[a], params[b], params[c]].join(","),
                                    "choose",
                                    ";other(source)",
                                    true,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn counter_break_source_suffix_keeps_noncanonical_consumption_outside() {
    for suffix in [
        ";consume((source))",
        ";consume(item=source)",
        ";consume(source);other(source)",
    ] {
        check_with_suffix(
            "source:String,rounds:Int,choose:Boolean,stop:Boolean",
            "stop",
            suffix,
            false,
        );
    }
}
