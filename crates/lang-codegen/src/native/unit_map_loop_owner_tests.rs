//! SPEC-0300: unmodified ordinary-source native paths, without allocator instrumentation.
use super::resource_exchange_tests::run_unit;
use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};

const ORIGINAL: &str =
    include_str!("../../tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko");
const NORMAL: &str = include_str!("../../tests/fixtures/map-loop-owner-0300/normal-exhaustion.ko");
const EXPECTED: &str = concat!(
    "dead\npresent\ndead\npresent\nmissing\nmissing\ndead\n",
    "retained\ntail present\ntail removed\nlive\npresent\nremoved\ntail present\ntail removed\nlive\npresent\nmissing\nmissing\nremoved\ntail present\ntail removed\nlive\n",
    "dead\nput\ndead\nput\nput\nput\ndead\n",
    "empty\ntail present\nlive\nput\nretained\ntail present\nlive\nput\nput\nput\nretained\ntail present\nlive\n",
);

#[test]
fn single_map_loop_original_native_repeats_remove() {
    let run = emit_link_and_run("map-loop-original.ko", ORIGINAL, "main");
    assert_success(&run, b"present\nmissing\n");
}

#[test]
fn unit_map_loop_original_native_is_independent() {
    assert_units(ORIGINAL, "present\nmissing\n");
}

#[test]
fn single_map_loop_normal_native_zero_one_many_dead_and_live() {
    let run = emit_link_and_run("map-loop-normal.ko", NORMAL, "main");
    assert_success(&run, EXPECTED.as_bytes());
}

#[test]
fn unit_map_loop_normal_native_zero_one_many_dead_and_live() {
    assert_units(NORMAL, EXPECTED);
}

fn assert_units(source: &str, expected: &str) {
    let provider = format!("package p\n{source}");
    let consumer = "package q\nfun entry(): Unit { p.main() }";
    for constants in [false, true] {
        let (run, _) = run_unit(&provider, consumer, constants);
        assert_success(&run, expected.as_bytes());
    }
}

const DISTINCT: &str = include_str!("../../tests/fixtures/map-loop-owner-0300/distinct-owners.ko");
const DISTINCT_EXPECTED: &str = concat!(
    "parameter empty\nparameter missing\n",
    "parameter present\nparameter removed\nparameter present\nparameter removed\n",
    "first present\nsecond present\nfirst tail\nsecond tail\n",
    "second removed\nfirst present\nsecond empty\nfirst tail\nsecond tail\n",
    "second removed\nsecond missing\nsecond missing\nfirst present\nsecond empty\nfirst tail\nsecond tail\n",
);

#[test]
fn single_map_loop_native_parameters_and_distinct_owners() {
    let run = emit_link_and_run("map-loop-distinct.ko", DISTINCT, "main");
    assert_success(&run, DISTINCT_EXPECTED.as_bytes());
}

#[test]
fn unit_map_loop_native_parameters_and_distinct_owners() {
    assert_units(DISTINCT, DISTINCT_EXPECTED);
}

#[test]
fn single_map_loop_native_other_sequential_providers() {
    for provider in ["listOf", "mutableListOf"] {
        let source = ORIGINAL.replace("arrayOf", provider);
        let run = emit_link_and_run("map-loop-provider.ko", &source, "main");
        assert_success(&run, b"present\nmissing\n");
    }
}

mod conditional_break {
    //! Ordinary single-source object/link/run. No instrumentation or replacement runtime.
    use super::{assert_success, emit_link_and_run};

    const NORMAL: &str =
        include_str!("../../tests/fixtures/map-conditional-break-0310/live-dead.ko");
    const DISTINCT: &str =
        include_str!("../../tests/fixtures/map-conditional-break-0310/distinct-owners.ko");

    #[test]
    fn single_map_break_native_zero_first_later_and_exhaustion() {
        let run = emit_link_and_run("map-break-live-dead.ko", NORMAL, "main");
        let expected = concat!(
            "dead\npresent\ndead\npresent\nmissing\nmissing\ndead\npresent\nmissing\nmissing\ndead\n",
            "retained\ntail present\ntail removed\nlive\npresent\nremoved\ntail present\ntail removed\nlive\n",
            "present\nmissing\nmissing\nremoved\ntail present\ntail removed\nlive\npresent\nmissing\nmissing\nremoved\ntail present\ntail removed\nlive\n",
            "dead\nput\ndead\nput\nput\nput\ndead\nput\nput\nput\ndead\n",
            "empty\ntail present\nlive\nput\nretained\ntail present\nlive\n",
            "put\nput\nput\nretained\ntail present\nlive\nput\nput\nput\nretained\ntail present\nlive\n",
        );
        assert_success(&run, expected.as_bytes());
    }

    #[test]
    fn single_map_break_native_own_seed_and_same_type_owners() {
        let run = emit_link_and_run("map-break-distinct.ko", DISTINCT, "main");
        let expected = concat!(
            "parameter empty\nparameter missing\nparameter present\nparameter removed\nparameter present\nparameter removed\n",
            "first present\nsecond present\nfirst tail\nsecond tail\n",
            "second removed\nfirst present\nsecond empty\nfirst tail\nsecond tail\n",
            "second removed\nsecond missing\nfirst present\nsecond empty\nfirst tail\nsecond tail\n",
        );
        assert_success(&run, expected.as_bytes());
    }

    #[test]
    fn single_map_break_native_temporary_sequential_sources() {
        for provider in ["arrayOf", "listOf", "mutableListOf"] {
            let source = format!(
                "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in {provider}(1, 2, 3)) {{ cache.put(key, \"value\".clone()); if (key == 2) {{ break }} }}; if (cache.contains(2)) {{ println(\"current\") }} else {{ println(\"lost\") }}; if (cache.contains(3)) {{ println(\"overran\") }} else {{ println(\"stopped\") }} }}"
            );
            let run = emit_link_and_run("map-break-provider.ko", &source, "main");
            assert_success(&run, b"current\nstopped\n");
        }
    }

    fn unit_live(constants: bool) {
        let provider = r#"package p
fun main(): Unit {
    var cache = mutableMapOf<Int, String>()
    for (key in arrayOf(1, 2, 3)) {
        cache.put(key, "value".clone())
        if (key == 2) { break }
    }
    if (cache.contains(2)) { println("current") } else { println("lost") }
    if (cache.contains(3)) { println("overran") } else { println("stopped") }
}
"#;
        let (run, _) = super::super::resource_exchange_tests::run_unit(
            provider,
            "package q\nfun entry(): Unit { p.main() }",
            constants,
        );
        assert_success(&run, b"current\nstopped\n");
    }

    #[test]
    fn unit_map_break_native_basic_live_control() {
        unit_live(false);
    }

    #[test]
    fn unit_map_break_native_constant_live_control() {
        unit_live(true);
    }
}

mod conditional_continue {
    //! SPEC-0311: ordinary source/object/link/run, with no IR or runtime replacement.
    use super::{assert_success, emit_link_and_run};

    const LIVE_DEAD: &str =
        include_str!("../../tests/fixtures/map-conditional-continue-0311/live-dead.ko");
    const DISTINCT: &str =
        include_str!("../../tests/fixtures/map-conditional-continue-0311/distinct-owners.ko");
    const PROVIDERS: &str = include_str!(
        "../../tests/fixtures/map-conditional-continue-0311/borrowed-temporary-providers.ko"
    );

    #[test]
    fn single_map_continue_native_zero_one_many_never_all_mixed() {
        let run = emit_link_and_run("map-continue-live-dead.ko", LIVE_DEAD, "main");
        // One literal segment per source invocation; expectations follow the source
        // operations, independently of compiler output or any allocation counters.
        let expected = concat!(
            // putDead: zero; one/all; three/never.
            "dead\n",
            "head\ndead\n",
            "head\ntail\nhead\ntail\nhead\ntail\ndead\n",
            // putLive: zero; one/never; three/all; three/mixed.
            "tail absent\nafter present\nlive\n",
            "head\ntail\ntail present\nafter present\nlive\n",
            "head\nhead\nhead\ntail absent\nafter present\nlive\n",
            "head\ntail\nhead\nhead\ntail\ntail present\nafter present\nlive\n",
            // removeDead: zero; one/all; three/mixed.
            "dead\n",
            "present\ndead\n",
            "present\ntail removed\nmissing\nmissing\ntail removed\ndead\n",
            // removeLive: zero; three/all; three/never.
            "head absent\nafter missing\nlive\n",
            "present\nmissing\nmissing\nhead retained\nafter removed\nlive\n",
            "present\ntail removed\nmissing\ntail removed\nmissing\ntail removed\nhead absent\nafter missing\nlive\n",
        );
        assert_success(&run, expected.as_bytes());
    }

    #[test]
    fn single_map_continue_native_own_seed_and_same_type_owners() {
        let run = emit_link_and_run("map-continue-distinct.ko", DISTINCT, "main");
        let expected = concat!(
            // Own seed: zero, one continue, three with a middle continue.
            "parameter empty\nparameter no tail\nparameter missing\n",
            "parameter present\nparameter no tail\nparameter missing\n",
            "parameter present\nparameter tail\nparameter removed\n",
            // Distinct same-typed maps: zero, one continue, final continue.
            "first no last\nfirst no tail\nsecond tail\nfirst after\nsecond after removed\n",
            "second removed\nfirst no last\nfirst no tail\nsecond no tail\nfirst after\nsecond after missing\n",
            "second removed\nsecond removed\nsecond removed\nfirst last\nfirst tail\nsecond no tail\nfirst after\nsecond after missing\n",
        );
        assert_success(&run, expected.as_bytes());
    }

    #[test]
    fn single_map_continue_native_borrowed_and_temporary_providers() {
        let run = emit_link_and_run("map-continue-providers.ko", PROVIDERS, "main");
        let expected = concat!(
            "tail\ntail\nborrowedArray\nsource kept\n",
            "tail\ntail\ntemporaryArray\n",
            "tail\ntail\nborrowedList\nsource kept\n",
            "tail\ntail\ntemporaryList\n",
            "tail\ntail\nborrowedMutableList\nsource kept\n",
            "tail\ntail\ntemporaryMutableList\n",
        );
        assert_success(&run, expected.as_bytes());
    }

    fn unit_source(live: bool, constants: bool) {
        let after = if live {
            "if (cache.contains(9)) { println(\"current\") } else { println(\"lost\") }"
        } else {
            ""
        };
        let provider = format!(
            r#"package p
fun main(): Unit {{
    var cache = mutableMapOf<Int, String>()
    for (key in arrayOf(1, 2, 3)) {{
        cache.put(key, "head".clone())
        println("head")
        if (key == 1) {{ continue }}
        cache.put(9, "tail".clone())
        println("tail")
    }}
    {after}
    println("done")
}}
"#,
        );
        let (run, _) = super::super::resource_exchange_tests::run_unit(
            &provider,
            "package q\nfun entry(): Unit { p.main() }",
            constants,
        );
        assert_success(
            &run,
            if live {
                b"head\nhead\ntail\nhead\ntail\ncurrent\ndone\n"
            } else {
                b"head\nhead\ntail\nhead\ntail\ndone\n"
            },
        );
    }

    // Each unit mode/liveness cell remains independently selectable and observable.
    #[test]
    fn unit_map_continue_native_basic_live_source_red() {
        unit_source(true, false);
    }

    #[test]
    fn unit_map_continue_native_basic_dead_source_red() {
        unit_source(false, false);
    }

    #[test]
    fn unit_map_continue_native_constant_live_source_red() {
        unit_source(true, true);
    }

    #[test]
    fn unit_map_continue_native_constant_dead_source_red() {
        unit_source(false, true);
    }
}

#[path = "unit_map_return_tests.rs"]
pub(crate) mod conditional_return;

mod owned_source_return {
    //! Independent public unit native path; no source/IR instrumentation.
    use super::super::resource_exchange_tests::run_unit;
    use crate::native_tests::boxed_enum_tests::assert_success;

    fn run(factory: &str, ty: &str, own: bool, live: bool, constants: bool) {
        let parameter = if own {
            format!("own keys: {ty}<Int>, ")
        } else {
            String::new()
        };
        let seed = if own {
            String::new()
        } else {
            format!("val keys = {factory}(1, 2, 3);")
        };
        let after = if live {
            "if (keys.size == 3) { println(\"live\") }"
        } else {
            ""
        };
        let provider = format!(
            "package p\nfun work({parameter}stop: Int): Unit {{ {seed} for (key in keys) {{ if (key == stop) {{ return }}; println(\"tail\") }}; {after} }}"
        );
        let argument = if own {
            format!("{factory}(1, 2, 3), ")
        } else {
            String::new()
        };
        let consumer = format!(
            "package q\nfun entry(): Unit {{ p.work({argument}2); println(\"returned\"); p.work({argument}9); println(\"exhausted\") }}"
        );
        let (output, _) = run_unit(&provider, &consumer, constants);
        let expected = format!(
            "tail\nreturned\ntail\ntail\ntail\n{}exhausted\n",
            if live { "live\n" } else { "" }
        );
        assert_success(&output, expected.as_bytes());
    }

    #[test]
    fn owned_source_return_unit_native_array_local_dead_basic() {
        run("arrayOf", "Array", false, false, false);
    }

    #[test]
    fn owned_source_return_unit_native_array_local_dead_constant() {
        run("arrayOf", "Array", false, false, true);
    }

    #[test]
    fn owned_source_return_unit_native_array_local_live_basic() {
        run("arrayOf", "Array", false, true, false);
    }

    #[test]
    fn owned_source_return_unit_native_array_local_live_constant() {
        run("arrayOf", "Array", false, true, true);
    }

    #[test]
    fn owned_source_return_unit_native_array_own_dead_basic() {
        run("arrayOf", "Array", true, false, false);
    }

    #[test]
    fn owned_source_return_unit_native_array_own_dead_constant() {
        run("arrayOf", "Array", true, false, true);
    }

    #[test]
    fn owned_source_return_unit_native_array_own_live_basic() {
        run("arrayOf", "Array", true, true, false);
    }

    #[test]
    fn owned_source_return_unit_native_array_own_live_constant() {
        run("arrayOf", "Array", true, true, true);
    }

    #[test]
    fn owned_source_return_unit_native_list_local_dead_basic() {
        run("listOf", "List", false, false, false);
    }

    #[test]
    fn owned_source_return_unit_native_list_local_dead_constant() {
        run("listOf", "List", false, false, true);
    }

    #[test]
    fn owned_source_return_unit_native_list_local_live_basic() {
        run("listOf", "List", false, true, false);
    }

    #[test]
    fn owned_source_return_unit_native_list_local_live_constant() {
        run("listOf", "List", false, true, true);
    }

    #[test]
    fn owned_source_return_unit_native_list_own_dead_basic() {
        run("listOf", "List", true, false, false);
    }

    #[test]
    fn owned_source_return_unit_native_list_own_dead_constant() {
        run("listOf", "List", true, false, true);
    }

    #[test]
    fn owned_source_return_unit_native_list_own_live_basic() {
        run("listOf", "List", true, true, false);
    }

    #[test]
    fn owned_source_return_unit_native_list_own_live_constant() {
        run("listOf", "List", true, true, true);
    }

    #[test]
    fn owned_source_return_unit_native_mutable_list_local_dead_basic() {
        run("mutableListOf", "MutableList", false, false, false);
    }

    #[test]
    fn owned_source_return_unit_native_mutable_list_local_dead_constant() {
        run("mutableListOf", "MutableList", false, false, true);
    }

    #[test]
    fn owned_source_return_unit_native_mutable_list_local_live_basic() {
        run("mutableListOf", "MutableList", false, true, false);
    }

    #[test]
    fn owned_source_return_unit_native_mutable_list_local_live_constant() {
        run("mutableListOf", "MutableList", false, true, true);
    }

    #[test]
    fn owned_source_return_unit_native_mutable_list_own_dead_basic() {
        run("mutableListOf", "MutableList", true, false, false);
    }

    #[test]
    fn owned_source_return_unit_native_mutable_list_own_dead_constant() {
        run("mutableListOf", "MutableList", true, false, true);
    }

    #[test]
    fn owned_source_return_unit_native_mutable_list_own_live_basic() {
        run("mutableListOf", "MutableList", true, true, false);
    }

    #[test]
    fn owned_source_return_unit_native_mutable_list_own_live_constant() {
        run("mutableListOf", "MutableList", true, true, true);
    }
}

pub(crate) mod map_key_return_tests {
    //! SPEC-0297: ordinary-source key returns through both public native entries.
    //!
    //! These tests observe source results, continuation and Resource deinit ordering.
    //! They do not instrument allocation or claim complete memory-lifetime evidence.

    use super::super::resource_exchange_tests::run_unit;
    use crate::native_tests::emit_link_and_run;

    const RESOURCE: &str = r#"
class Resource(val n: Int) {
    deinit() {
        if (n == 7) { println("value-drop") } else { println("prefix-drop") }
    }
}
"#;

    #[derive(Clone, Copy)]
    enum Entry {
        Single,
        Unit,
    }

    fn assert_source(entry: Entry, name: &str, source: &str, expected: &str) {
        let run = match entry {
            Entry::Single => emit_link_and_run(name, source, "exercise"),
            Entry::Unit => {
                let provider = format!("package p\n{source}");
                let consumer = "package q\nfun entry(): Unit { p.exercise() }";
                run_unit(&provider, consumer, false).0
            }
        };
        assert!(run.status.success(), "{name}: {run:?}");
        assert_eq!(run.stdout, expected.as_bytes(), "{name}");
        assert!(run.stderr.is_empty(), "{name}: {run:?}");
    }

    #[test]
    fn single_map_key_return_scalar_shapes_preserve_normal_results() {
        scalar_shapes(Entry::Single);
    }

    #[test]
    fn unit_map_key_return_scalar_shapes_preserve_normal_results() {
        scalar_shapes(Entry::Unit);
    }

    fn scalar_shapes(entry: Entry) {
        for operation in ["get", "contains", "remove"] {
            for (shape, key, expected) in [
                (
                    "mixed",
                    "if (flag) { println(\"left\"); return } else { println(\"right\"); 1 }",
                    "right\nnormal\nprefix-drop\nleft\nprefix-drop\ndone\n",
                ),
                (
                    "reversed",
                    "if (flag) { println(\"left\"); 1 } else { println(\"right\"); return }",
                    "right\nprefix-drop\nleft\nnormal\nprefix-drop\ndone\n",
                ),
                (
                    "both",
                    "if (flag) { println(\"left\"); return } else { println(\"right\"); return }",
                    "right\nprefix-drop\nleft\nprefix-drop\ndone\n",
                ),
                ("direct", "return", "prefix-drop\nprefix-drop\ndone\n"),
            ] {
                let result = match operation {
                    "contains" => "if (!result) { error(\"missing contains result\") }",
                    _ => "if (result != 7) { error(\"wrong nullable result\") }",
                };
                let remaining = if operation == "remove" {
                    "if (m.size != 0) { error(\"remove did not commit\") }"
                } else {
                    "if (m.size != 1) { error(\"query changed receiver\") }\n    if (m.get(1) != 7) { error(\"query lost value\") }"
                };
                let source = format!(
                    r#"{RESOURCE}
fun inspect(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    var m = mutableMapOf<Int, Int>()
    m.put(1, 7)
    val result = m.{operation}({key})
    {result}
    {remaining}
    println("normal")
}}
fun exercise(): Unit {{ inspect(false); inspect(true); println("done") }}
"#
                );
                assert_source(
                    entry,
                    &format!("map-key-{operation}-{shape}"),
                    &source,
                    expected,
                );
            }
        }
    }

    #[test]
    fn single_map_key_return_string_owners_and_resources_follow_selected_edge() {
        string_owners(Entry::Single);
    }

    #[test]
    fn unit_map_key_return_string_owners_and_resources_follow_selected_edge() {
        string_owners(Entry::Unit);
    }

    fn string_owners(entry: Entry) {
        for operation in ["get", "contains", "remove"] {
            for (form, key) in [
                ("named", "key"),
                ("grouped", "(key)"),
                ("temporary", "\"key\".clone()"),
            ] {
                let (value_type, value, check, expected) = match operation {
                    "get" => (
                        "Int",
                        "7",
                        "if (result != 7) { error(\"wrong get result\") }\n    if (m.size != 1) { error(\"get changed receiver\") }",
                        "key-evaluated\nnormal\nprefix-drop\nreturning\nprefix-drop\ndone\n",
                    ),
                    "contains" => (
                        "Resource",
                        "Resource(7)",
                        "if (!result) { error(\"missing contains result\") }\n    if (m.size != 1) { error(\"contains changed receiver\") }",
                        "key-evaluated\nnormal\nvalue-drop\nprefix-drop\nreturning\nvalue-drop\nprefix-drop\ndone\n",
                    ),
                    "remove" => (
                        "Resource",
                        "Resource(7)",
                        "val taken = result!!\n    if (taken.n != 7) { error(\"wrong removed resource\") }\n    if (m.size != 0) { error(\"resource still in receiver\") }",
                        "key-evaluated\nnormal\nvalue-drop\nprefix-drop\nreturning\nvalue-drop\nprefix-drop\ndone\n",
                    ),
                    _ => unreachable!(),
                };
                // A control tail delivers its String owner before the outer Borrow.
                // Named/grouped keys are intentionally not reused after this move.
                let source = format!(
                    r#"{RESOURCE}
fun inspect(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    var m = mutableMapOf<String, {value_type}>()
    m.put("key".clone(), {value})
    val key = "key".clone()
    val result = m.{operation}(if (flag) {{ println("returning"); return }} else {{ println("key-evaluated"); {key} }})
    {check}
    println("normal")
}}
fun exercise(): Unit {{ inspect(false); inspect(true); println("done") }}
"#
                );
                assert_source(
                    entry,
                    &format!("map-key-{operation}-{form}"),
                    &source,
                    expected,
                );
            }
        }
    }

    #[test]
    fn single_map_key_return_without_suffix_cleans_existing_receiver() {
        without_suffix(Entry::Single);
    }

    #[test]
    fn unit_map_key_return_without_suffix_cleans_existing_receiver() {
        without_suffix(Entry::Unit);
    }

    fn without_suffix(entry: Entry) {
        for operation in ["get", "contains", "remove"] {
            let (value_type, value, expected) = if operation == "get" {
                ("Int", "7", "prefix-drop\nprefix-drop\ndone\n")
            } else {
                (
                    "Resource",
                    "Resource(7)",
                    "value-drop\nprefix-drop\nvalue-drop\nprefix-drop\ndone\n",
                )
            };
            let source = format!(
                r#"{RESOURCE}
fun direct(): Unit {{
    val prefix = Resource(0)
    var m = mutableMapOf<String, {value_type}>()
    m.put("key".clone(), {value})
    m.{operation}(return)
}}
fun both(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    var m = mutableMapOf<String, {value_type}>()
    m.put("key".clone(), {value})
    m.{operation}(if (flag) {{ return }} else {{ return }})
}}
fun exercise(): Unit {{ direct(); both(false); println("done") }}
"#
            );
            assert_source(
                entry,
                &format!("map-key-{operation}-no-suffix"),
                &source,
                expected,
            );
        }
    }

    #[test]
    fn single_map_key_return_borrowed_receivers_remain_owned_by_caller() {
        borrowed_receivers(Entry::Single);
    }

    #[test]
    fn unit_map_key_return_borrowed_receivers_remain_owned_by_caller() {
        borrowed_receivers(Entry::Unit);
    }

    fn borrowed_receivers(entry: Entry) {
        // remove requires Inout; a Borrow receiver is only legal for these queries.
        for operation in ["get", "contains"] {
            let check = if operation == "get" {
                "if (result != 7) { error(\"wrong borrowed get\") }"
            } else {
                "if (!result) { error(\"wrong borrowed contains\") }"
            };
            let source = format!(
                r#"{RESOURCE}
fun inspect(m: MutableMap<String, Int>, own flag: Boolean): Unit {{
    val prefix = Resource(0)
    val result = m.{operation}(if (flag) {{ println("returning"); return }} else {{ "key".clone() }})
    {check}
    println("normal")
}}
fun exercise(): Unit {{
    var m = mutableMapOf<String, Int>()
    m.put("key".clone(), 7)
    inspect(m, true)
    if (m.get("key") != 7) {{ error("return changed caller map") }}
    inspect(m, false)
    m.put("key".clone(), 9)
    if (m.remove("key") != 9) {{ error("caller cannot mutate after query") }}
    println("caller-kept")
}}
"#
            );
            assert_source(
                entry,
                &format!("map-key-{operation}-borrow-receiver"),
                &source,
                "returning\nprefix-drop\nnormal\nprefix-drop\ncaller-kept\n",
            );
        }
    }

    #[test]
    fn single_map_key_return_normal_borrow_controls_preserve_named_keys() {
        normal_borrow_controls(Entry::Single);
    }

    #[test]
    fn unit_map_key_return_normal_borrow_controls_preserve_named_keys() {
        normal_borrow_controls(Entry::Unit);
    }

    fn normal_borrow_controls(entry: Entry) {
        let source = r#"
fun exercise(): Unit {
    var m = mutableMapOf<String, Int>()
    m.put("key".clone(), 0)
    val key = "key".clone()
    if (m.get(key) != 0) { error("get lost present zero") }
    if (!m.contains((key))) { error("contains lost entry") }
    if (m.remove(key) != 0) { error("remove lost present zero") }
    if (m.get((key)) != null) { error("missing get must be null") }
    if (m.contains(key)) { error("removed entry still present") }
    if (m.remove((key)) != null) { error("missing remove must be null") }
    if (m.get("missing".clone()) != null) { error("temporary missing get") }
    if (m.contains("missing".clone())) { error("temporary missing contains") }
    if (m.remove("missing".clone()) != null) { error("temporary missing remove") }
    println(key)
    println("done")
}
"#;
        assert_source(
            entry,
            "map-key-normal-borrow-controls",
            source,
            "key\ndone\n",
        );
    }

    #[test]
    fn single_map_key_return_no_map_control_preserves_resource_cleanup() {
        no_map_control(Entry::Single);
    }

    #[test]
    fn unit_map_key_return_no_map_control_preserves_resource_cleanup() {
        no_map_control(Entry::Unit);
    }

    fn no_map_control(entry: Entry) {
        let source = format!(
            r#"{RESOURCE}
fun inspect(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    val value = if (flag) {{ println("returning"); return }} else {{ 7 }}
    if (value != 7) {{ error("wrong ordinary result") }}
    println("normal")
}}
fun exercise(): Unit {{ inspect(false); inspect(true); println("done") }}
"#
        );
        assert_source(
            entry,
            "map-key-no-map-control",
            &source,
            "normal\nprefix-drop\nreturning\nprefix-drop\ndone\n",
        );
    }

    #[test]
    fn single_map_key_return_temporary_receivers_evaluate_once_and_cleanup() {
        temporary_receivers(Entry::Single, false);
    }

    #[test]
    fn unit_map_key_return_temporary_receivers_evaluate_once_and_cleanup() {
        temporary_receivers(Entry::Unit, false);
    }

    fn temporary_receivers(entry: Entry, grouped: bool) {
        let receiver = if grouped { "(make())" } else { "make()" };
        for operation in ["get", "contains"] {
            for readonly in [true, false] {
                let (receiver_type, make_body, check, receiver_drop) = match (operation, readonly) {
                    ("get", true) => (
                        "Map<String, Int>",
                        "return mapOf<String, Int>()",
                        "if (result != null) { error(\"readonly get must be missing\") }",
                        "",
                    ),
                    ("contains", true) => (
                        "Map<String, Resource>",
                        "return mapOf<String, Resource>()",
                        "if (result) { error(\"readonly contains must be false\") }",
                        "",
                    ),
                    ("get", false) => (
                        "MutableMap<String, Int>",
                        "var result = mutableMapOf<String, Int>()\n    result.put(\"key\".clone(), 7)\n    return result",
                        "if (result != 7) { error(\"owned get lost present value\") }",
                        "",
                    ),
                    ("contains", false) => (
                        "MutableMap<String, Resource>",
                        "var result = mutableMapOf<String, Resource>()\n    result.put(\"key\".clone(), Resource(7))\n    return result",
                        "if (!result) { error(\"owned contains lost entry\") }",
                        "value-drop\n",
                    ),
                    _ => unreachable!(),
                };
                let source = format!(
                    r#"{RESOURCE}
fun make(): {receiver_type} {{
    println("receiver-evaluated")
    {make_body}
}}
fun inspect(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    val result = {receiver}.{operation}(if (flag) {{ println("returning"); return }} else {{ println("key-evaluated"); "key".clone() }})
    {check}
    println("normal")
}}
fun exercise(): Unit {{ inspect(false); inspect(true); println("done") }}
"#
                );
                let expected = format!(
                    "receiver-evaluated\nkey-evaluated\n{receiver_drop}normal\nprefix-drop\nreceiver-evaluated\nreturning\n{receiver_drop}prefix-drop\ndone\n"
                );
                assert_source(
                    entry,
                    &format!(
                        "map-key-{operation}-temporary-receiver-readonly-{readonly}-grouped-{grouped}"
                    ),
                    &source,
                    &expected,
                );
            }
        }
    }

    #[test]
    fn single_map_key_return_grouped_temporary_and_control_keys_keep_cleanup() {
        grouped_key_expressions(Entry::Single);
    }

    #[test]
    fn unit_map_key_return_grouped_temporary_and_control_keys_keep_cleanup() {
        grouped_key_expressions(Entry::Unit);
    }

    fn grouped_key_expressions(entry: Entry) {
        for operation in ["get", "contains", "remove"] {
            let (value_type, value, check, value_drop) = match operation {
                "get" => (
                    "Int",
                    "7",
                    "if (result != 7) { error(\"wrong grouped get\") }\n    if (m.size != 1) { error(\"grouped get changed receiver\") }",
                    "",
                ),
                "contains" => (
                    "Resource",
                    "Resource(7)",
                    "if (!result) { error(\"wrong grouped contains\") }\n    if (m.size != 1) { error(\"grouped contains changed receiver\") }",
                    "value-drop\n",
                ),
                "remove" => (
                    "Resource",
                    "Resource(7)",
                    "val taken = result!!\n    if (taken.n != 7) { error(\"wrong grouped removed resource\") }\n    if (m.size != 0) { error(\"grouped remove did not commit\") }",
                    "value-drop\n",
                ),
                _ => unreachable!(),
            };
            for (form, key, calls, expected) in [
                (
                    "temporary",
                    "(\"key\".clone())",
                    "inspect(false)",
                    format!("normal\n{value_drop}prefix-drop\ndone\n"),
                ),
                (
                    "control",
                    "(if (flag) { println(\"returning\"); return } else { println(\"key-evaluated\"); \"key\".clone() })",
                    "inspect(false); inspect(true)",
                    format!(
                        "key-evaluated\nnormal\n{value_drop}prefix-drop\nreturning\n{value_drop}prefix-drop\ndone\n"
                    ),
                ),
            ] {
                let source = format!(
                    r#"{RESOURCE}
fun inspect(own flag: Boolean): Unit {{
    val prefix = Resource(0)
    var m = mutableMapOf<String, {value_type}>()
    m.put("key".clone(), {value})
    val result = m.{operation}({key})
    {check}
    println("normal")
}}
fun exercise(): Unit {{ {calls}; println("done") }}
"#
                );
                assert_source(
                    entry,
                    &format!("map-key-{operation}-grouped-{form}"),
                    &source,
                    &expected,
                );
            }
        }
    }

    #[test]
    fn single_map_key_return_grouped_temporary_receivers_keep_outer_owner() {
        temporary_receivers(Entry::Single, true);
    }

    #[test]
    fn unit_map_key_return_grouped_temporary_receivers_keep_outer_owner() {
        temporary_receivers(Entry::Unit, true);
    }
}
