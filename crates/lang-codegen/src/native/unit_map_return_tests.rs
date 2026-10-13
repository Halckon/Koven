//! SPEC-0313: ordinary source through original object/link/run helpers.
use super::{assert_success, emit_link_and_run};

#[test]
fn single_map_return_native_cardinality() {
    let run = emit_link_and_run(
        "map-return-cardinality.ko",
        include_str!("../../tests/fixtures/map-conditional-return-0313/live-dead.ko"),
        "main",
    );
    let expected = concat!(
        "head\ntail\nhead\ntail\nhead\nreturn\n",
        "dead\n",
        "head\nreturn\n",
        "head\ntail\nhead\ntail\nhead\ntail\ndead\n",
        "tail absent\nafter present\nlive\n",
        "head\ntail\ntail present\nafter present\nlive\n",
        "head\nreturn\n",
        "head\ntail\nhead\nreturn\n",
        "dead\n",
        "present\nreturn\n",
        "present\ntail removed\nmissing\nreturn\n",
        "head absent\nafter missing\nlive\n",
        "present\nreturn\n",
        "present\ntail removed\nmissing\ntail removed\nmissing\ntail removed\nhead absent\nafter missing\nlive\n",
    );
    assert_success(&run, expected.as_bytes());
}

#[test]
fn single_map_return_native_distinct_owners() {
    let run = emit_link_and_run(
        "map-return-distinct.ko",
        include_str!("../../tests/fixtures/map-conditional-return-0313/distinct-owners.ko"),
        "main",
    );
    assert_success(
        &run,
        concat!(
            "parameter empty\nparameter no tail\nparameter missing\n",
            "return\n",
            "return\n",
            "first no last\nfirst no tail\nsecond tail\nfirst after\nsecond after removed\n",
            "second removed\nreturn\n",
            "second removed\nsecond removed\nsecond removed\nreturn\n",
        )
        .as_bytes(),
    );
}

#[test]
fn single_map_return_native_providers() {
    let run = emit_link_and_run(
        "map-return-providers.ko",
        include_str!(
            "../../tests/fixtures/map-conditional-return-0313/borrowed-temporary-providers.ko"
        ),
        "main",
    );
    assert_success(
        &run,
        b"tail\nreturn\ntail\nreturn\ntail\nreturn\ntail\nreturn\ntail\nreturn\ntail\nreturn\n",
    );
}

#[test]
fn single_map_return_native_operand_cleanup() {
    let run = emit_link_and_run(
        "map-return-cleanup.ko",
        include_str!("../../tests/fixtures/map-conditional-return-0313/operand-cleanup.ko"),
        "main",
    );
    assert_success(&run, concat!(
        "first\noperand\nlater\nearlier\nreturned\n",
        "fallthrough\nlater\nearlier\nfallthrough\nlater\nearlier\ntail kept\nexhausted\ncompleted\n",
        "copy returned\ncopy exhausted\nremoved operand\nupdated returned\nupdated exhausted\n",
    ).as_bytes());
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
        if (key == 1) {{ return }}
        cache.put(9, "tail".clone())
        println("tail")
    }}
    {after}
    println("done")
}}
"#
    );
    let (run, _) = super::super::resource_exchange_tests::run_unit(
        &provider,
        "package q\nfun entry(): Unit { p.main(); println(\"caller\") }",
        constants,
    );
    assert_success(&run, b"head\ncaller\n");
}
#[test]
fn unit_map_return_native_basic_live() {
    unit_source(true, false);
}
#[test]
fn unit_map_return_native_basic_dead() {
    unit_source(false, false);
}
#[test]
fn unit_map_return_native_constant_live() {
    unit_source(true, true);
}
#[test]
fn unit_map_return_native_constant_dead() {
    unit_source(false, true);
}
