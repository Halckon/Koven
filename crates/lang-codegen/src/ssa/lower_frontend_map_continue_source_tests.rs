//! Original SPEC-0311 source probes, now accepted after SPEC-0312 actual-main integration.
use super::super::unit_identity::{SourceScenario, check_source as check};

#[test]
fn map_continue_source_owned_after_use_without_map_now_lowers() {
    check(
        "fun main(): Unit { val keys = arrayOf(1, 2, 3); for (key in keys) { if (key == 2) { continue } }; keys.size }",
        SourceScenario::NamedAfterUse,
    );
}

#[test]
fn map_continue_source_owned_without_after_use_with_map_now_lowers() {
    check(
        "fun main(): Unit { val keys = arrayOf(1, 2, 3); var cache = mutableMapOf<Int, String>(); for (key in keys) { cache.put(key, \"head\".clone()); if (key == 2) { continue }; cache.put(9, \"tail\".clone()) }; cache.contains(9) }",
        SourceScenario::NamedLoopExit,
    );
}

#[test]
fn map_continue_source_temporary_with_map_lowers() {
    check(
        "fun main(): Unit { var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2, 3)) { cache.put(key, \"head\".clone()); if (key == 2) { continue }; cache.put(9, \"tail\".clone()) }; cache.contains(9) }",
        SourceScenario::Temporary,
    );
}

#[test]
fn map_continue_source_original_owned_fixture_now_lowers() {
    check(
        include_str!("../../tests/fixtures/map-conditional-continue-0311/providers-cleanup.ko"),
        SourceScenario::Providers,
    );
}
