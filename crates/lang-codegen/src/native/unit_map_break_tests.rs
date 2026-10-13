//! Ordinary single-source object/link/run. No instrumentation or replacement runtime.
use super::{assert_success, emit_link_and_run};

const NORMAL: &str = include_str!("../../tests/fixtures/map-conditional-break-0310/live-dead.ko");
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
