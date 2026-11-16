//! Original source survives either tail-break or zero-iteration exit until one exact call.
use super::arm_loop_tail_break_tests::check;
#[test]
fn tail_break_suffix_keeps_original_source_across_both_exit_paths() {
    for signature in [
        "source:String,flag:Boolean,choose:Boolean",
        "source:String,choose:Boolean,flag:Boolean",
        "flag:Boolean,source:String,choose:Boolean",
        "flag:Boolean,choose:Boolean,source:String",
        "choose:Boolean,source:String,flag:Boolean",
        "choose:Boolean,flag:Boolean,source:String",
    ] {
        check(signature, "", ";after(source)", true);
    }
}
#[test]
fn tail_break_suffix_rejects_unproved_argument_and_surrounding_work() {
    for (prefix, suffix) in [
        ("", ";after((source))"),
        ("", ";after(item=source)"),
        ("consume(source);other(source);", ";after(source)"),
        ("", ";after(source);consume(source)"),
    ] {
        check(
            "source:String,flag:Boolean,choose:Boolean",
            prefix,
            suffix,
            false,
        );
    }
}
