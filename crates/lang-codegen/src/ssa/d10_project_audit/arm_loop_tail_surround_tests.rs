//! Complete source lifetime on each side of the original tail-break CFG.
use super::arm_loop_tail_break_tests::check;
#[test]
fn tail_break_surrounding_source_calls_keep_occurrence_identity() {
    let names = ["source", "flag", "choose"];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        for modes in [[false, false], [false, true], [true, false], [true, true]] {
            let parameters = order
                .map(|i| {
                    format!(
                        "{}{}:{}",
                        if i > 0 && modes[i - 1] { "own " } else { "" },
                        names[i],
                        if i == 0 { "String" } else { "Boolean" }
                    )
                })
                .join(",");
            for (prefix, suffix) in [
                ("before(source);", ""),
                ("before(source);", ";after(source)"),
                ("after(source);", ";after(source)"),
            ] {
                check(&parameters, prefix, suffix, true);
            }
        }
    }
}
#[test]
fn tail_break_surrounding_calls_reject_unproved_segments() {
    for (prefix, suffix) in [
        ("before((source));", ""),
        ("before(item=source);", ""),
        ("before(source);after(source);", ""),
        ("before(source);", ";after(source);after(source)"),
    ] {
        check(
            "source:String,flag:Boolean,choose:Boolean",
            prefix,
            suffix,
            false,
        );
    }
}
