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
