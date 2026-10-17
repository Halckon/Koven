//! SPEC-0291 CI-only live Map chain observations of real ordinary source.
//!
//! These tests intentionally execute raw LLVM and a read-only instrumented copy.
//! They are not ordinary local regression tests; no ignore or environment skip.

#[path = "native_ordered_map_tests/fixtures.rs"]
mod fixtures;
#[path = "native_ordered_map_tests/probe.rs"]
mod probe;
#[path = "native_ordered_map_tests/runner.rs"]
mod runner;

use std::fmt::Write;

use crate::llvm::ordered_map_test_support::OrderedMapLayout;

use fixtures::{Fixture, Key};

fn observer(fixture: &Fixture, layout: &OrderedMapLayout) -> String {
    let mut text = String::from(
        "#include <stdint.h>\n#include <stddef.h>\n\
         typedef struct { int64_t integer; const char *bytes; size_t length; int64_t value; int slot; } ExpectedEntry;\n\
         typedef struct { uint64_t step, capacity, tombstones; size_t size; const ExpectedEntry *entries; } Snapshot;\n",
    );
    writeln!(
        text,
        "enum {{ KEY_STRING={}, POINTER_BITS={}, S_STRIDE={},",
        u8::from(fixture.string_keys),
        layout.pointer_bits,
        layout.slot_stride
    )
    .unwrap();
    for (name, offset) in [
        "H_BUFFER",
        "H_SIZE",
        "H_CAPACITY",
        "H_TOMBSTONES",
        "H_FIRST",
        "H_LAST",
    ]
    .into_iter()
    .zip(layout.header_offsets)
    {
        writeln!(text, "{name}={offset},").unwrap();
    }
    for (name, offset) in ["S_STATE", "S_KEY", "S_VALUE", "S_PREV", "S_NEXT"]
        .into_iter()
        .zip(layout.slot_offsets)
    {
        writeln!(text, "{name}={offset},").unwrap();
    }
    let strings = layout.string_offsets.unwrap_or([0, 0]);
    writeln!(
        text,
        "STRING_DATA={}, STRING_LENGTH={} }};",
        strings[0], strings[1]
    )
    .unwrap();
    for (index, snapshot) in fixture.snapshots.iter().enumerate() {
        if snapshot.entries.is_empty() {
            continue;
        }
        writeln!(text, "static const ExpectedEntry ENTRIES_{index}[] = {{").unwrap();
        for entry in &snapshot.entries {
            let (integer, bytes, length) = match entry.key {
                Key::Int(integer) => (integer, "NULL".to_owned(), 0),
                Key::String(value) => {
                    let mut bytes = String::from("\"");
                    for byte in value.as_bytes() {
                        write!(bytes, "\\x{byte:02x}").unwrap();
                    }
                    bytes.push('"');
                    (0, bytes, value.len())
                }
            };
            let slot = entry.slot.map_or(-1, |value| i32::try_from(value).unwrap());
            writeln!(
                text,
                "{{ {integer}, {bytes}, {length}, {}, {slot} }},",
                entry.value
            )
            .unwrap();
        }
        text.push_str("};\n");
    }
    text.push_str("static const Snapshot EXPECTED[] = {\n");
    for (index, snapshot) in fixture.snapshots.iter().enumerate() {
        let entries = if snapshot.entries.is_empty() {
            "NULL".to_owned()
        } else {
            format!("ENTRIES_{index}")
        };
        writeln!(
            text,
            "{{ {}, {}, {}, {}, {entries} }},",
            snapshot.step,
            snapshot.capacity,
            snapshot.tombstones,
            snapshot.entries.len()
        )
        .unwrap();
    }
    text.push_str(
        "};\nstatic const size_t SNAPSHOT_COUNT = sizeof(EXPECTED) / sizeof(EXPECTED[0]);\n",
    );
    text.push_str(include_str!("native_ordered_map_tests/observer.c"));
    text
}

fn run(fixture: Fixture) {
    assert!(!fixture.snapshots.is_empty());
    let artifacts = runner::Artifacts::new(fixture.name);
    artifacts.write("fixture.ko", fixture.source);
    artifacts.write(
        "expected.txt",
        format!(
            "{:#?}\nstdout:\n{}stderr:\n{}",
            fixture.snapshots,
            fixture.stdout(),
            fixture.stderr()
        ),
    );
    let product = probe::lower(&fixture);
    let raw = artifacts.write("raw.ll", &product.raw);
    artifacts.check_compiler();
    // Do not even instrument until the real unmodified artifact has passed.
    artifacts.compile_and_run("raw", &raw, None, &fixture.stdout(), "");
    let probe = probe::instrument(&product);
    artifacts.write("identity.txt", &probe.identity);
    let ir = artifacts.write("probe.ll", &probe.llvm);
    let observer = artifacts.write("observer.c", observer(&fixture, &probe.layout));
    artifacts.compile_and_run(
        "probe",
        &ir,
        Some(&observer),
        &fixture.stdout(),
        &fixture.stderr(),
    );
    artifacts.finish();
}

#[test]
fn ordered_map_empty_singleton_and_endpoint_deletions() {
    let cases = fixtures::empty_and_endpoints();
    assert_eq!(
        cases.iter().map(|fixture| fixture.name).collect::<Vec<_>>(),
        ["int_readonly.ko", "int_empty.ko", "int_endpoints.ko"]
    );
    for fixture in cases {
        run(fixture);
    }
}

#[test]
fn ordered_map_overwrite_and_reinsert_preserve_order() {
    run(fixtures::overwrite_reinsert());
}

#[test]
fn ordered_map_collision_tombstone_reuse_preserves_order() {
    run(fixtures::collision());
}

#[test]
fn ordered_map_overwrite_at_growth_boundary_preserves_order() {
    run(fixtures::overwrite_growth());
}

#[test]
fn ordered_map_multiple_rehashes_preserve_order() {
    run(fixtures::multiple_rehashes());
}

#[test]
fn ordered_map_empty_tombstone_rehash_resets_endpoints() {
    run(fixtures::empty_tombstone_growth());
}

#[test]
fn ordered_map_string_key_cfg_preserves_order() {
    let cases = fixtures::string_cases();
    assert_eq!(
        cases.iter().map(|fixture| fixture.name).collect::<Vec<_>>(),
        [
            "string_readonly.ko",
            "string_empty.ko",
            "string_endpoints.ko",
            "string_overwrite.ko",
            "string_collision.ko",
            "string_growth.ko",
            "string_rehash.ko",
            "string_empty_growth.ko",
            "string_empty_key.ko",
        ]
    );
    for fixture in cases {
        run(fixture);
    }
}
