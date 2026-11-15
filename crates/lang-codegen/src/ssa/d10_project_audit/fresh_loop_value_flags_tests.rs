//! Source-backed Boolean Value/Shared ABI roles, without native execution.
use super::fresh_loop_flags_tests::check;

#[test]
fn value_boolean_flags_preserve_each_abi_role_and_scope() {
    for names in [
        ["source", "a", "b"],
        ["source", "b", "a"],
        ["a", "source", "b"],
        ["b", "source", "a"],
        ["a", "b", "source"],
        ["b", "a", "source"],
    ] {
        for owned in [[false, true], [true, false], [true, true]] {
            let parameters = names
                .map(|n| match n {
                    "source" => "source:String".to_string(),
                    "a" => format!("{}a:Boolean", if owned[0] { "own " } else { "" }),
                    _ => format!("{}b:Boolean", if owned[1] { "own " } else { "" }),
                })
                .join(",");
            for flags in [["a", "b"], ["b", "a"]] {
                for reverse in [false, true] {
                    for suffix in [false, true] {
                        check(&parameters, flags, reverse, suffix, true);
                    }
                }
            }
        }
    }
    for parameters in ["source:String,own a:Boolean", "own a:Boolean,source:String"] {
        for reverse in [false, true] {
            for suffix in [false, true] {
                check(parameters, ["a", "a"], reverse, suffix, true);
            }
        }
    }
}

#[test]
fn owned_flags_do_not_hide_extra_or_unused_abi_parameters() {
    for parameters in [
        "source:String,own a:Boolean,own b:Boolean,unused:Boolean",
        "source:String,own a:Boolean,b:Boolean",
    ] {
        let flags = if parameters.contains("unused") {
            ["a", "b"]
        } else {
            ["a", "a"]
        };
        check(parameters, flags, false, false, false);
    }
}

// Real probe confirms two original L0164 diagnostics at the borrow bindings.
#[test]
fn owned_string_source_keeps_original_route_boundary() {
    check(
        "own source:String,own a:Boolean,b:Boolean",
        ["a", "b"],
        false,
        false,
        false,
    );
}
