//! One real source-prefix use, then the unchanged complete ordinary arm loop.
use super::arm_loop_value_flags_tests::check_with_prefix;
#[test]
fn ordinary_arm_prefix_preserves_source_across_call_and_zero_iteration() {
    let names = ["source", "flag", "choose"];
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        for owned in [[false, false], [false, true], [true, false], [true, true]] {
            let parameters = order
                .map(|i| {
                    format!(
                        "{}{}:{}",
                        if i > 0 && owned[i - 1] { "own " } else { "" },
                        names[i],
                        if i == 0 { "String" } else { "Boolean" }
                    )
                })
                .join(",");
            for reverse in [false, true] {
                check_with_prefix(
                    &parameters,
                    ["flag", "choose"],
                    reverse,
                    true,
                    Some("before(source);"),
                );
            }
        }
    }
}
#[test]
fn ordinary_arm_prefix_rejects_unproven_argument_syntax() {
    for prefix in ["before((source));", "before(item=source);"] {
        check_with_prefix(
            "source:String,own flag:Boolean,choose:Boolean",
            ["flag", "choose"],
            false,
            false,
            Some(prefix),
        );
    }
}
