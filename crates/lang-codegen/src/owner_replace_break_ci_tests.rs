//! CI-only identity, edge corruption and physical String Drop counter proof.
//! Compile locally; never execute these tests or their subprocesses locally.
use super::{OwnedProducts, UnitSource, owner_replace_break_downstream_tests::SOURCE};
use crate::ssa::{model::SsaTypeKind, verify::verify_program};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::{const_owned_compilation_unit_view, owned_compilation_unit_view},
};
use std::{fs, os::unix::process::ExitStatusExt, process::Command};

#[test]
fn ci_only_bounded_owner_foreign_analysis_products_cannot_construct_unit() {
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                SOURCE.replace("run(true)", "run(ON)")
            )
        } else {
            SOURCE.to_owned()
        };
        let source = UnitSource::analyze("local-owner.ko", &text).unwrap();
        for name in ["local-owner.ko", "foreign-owner.ko"] {
            let foreign = UnitSource::analyze(name, &text).unwrap();
            let inputs = [SourceUnitInput::memory_document(
                source.parsed.source_id(),
                &source.parsed,
            )];
            match (&source.products, &foreign.products) {
                (OwnedProducts::Basic(typed, own), OwnedProducts::Basic(_, foreign_owned)) => {
                    assert!(own.ownership().owner_observations().iter().any(|o| {
                        o.replace_break_observation_v1()
                            .and_then(|b| b.suffix_consumer_v1())
                            .is_some()
                    }));
                    assert!(
                        owned_compilation_unit_view(
                            &source.sources,
                            &inputs,
                            &source.names,
                            &source.environment,
                            typed,
                            foreign_owned
                        )
                        .is_err()
                    );
                }
                (
                    OwnedProducts::Constant(typed, own),
                    OwnedProducts::Constant(_, foreign_owned),
                ) => {
                    assert!(own.ownership().owner_observations().iter().any(|o| {
                        o.replace_break_observation_v1()
                            .and_then(|b| b.suffix_consumer_v1())
                            .is_some()
                    }));
                    assert!(
                        const_owned_compilation_unit_view(
                            &source.sources,
                            &inputs,
                            &source.names,
                            &source.environment,
                            typed,
                            foreign_owned
                        )
                        .is_err()
                    );
                }
                _ => panic!("matching actual capabilities"),
            }
        }
    }
}

#[test]
fn ci_only_bounded_owner_missing_foreign_and_reversed_incoming_roles_fail_closed() {
    use crate::ssa::OwnerFault::{ForeignIncoming, MissingIncoming, ReversedIncomingRoles};
    for fault in [MissingIncoming, ForeignIncoming, ReversedIncomingRoles] {
        let source = UnitSource::analyze("broken-owner-edge.ko", SOURCE).unwrap();
        let result = crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()));
        assert!(
            result.is_err(),
            "selected plan must reject a missing, unrelated or wrongly assigned predecessor"
        );
    }
}

#[derive(Clone, Copy)]
enum Mutation {
    None,
    DuplicateDrop,
    OmitDrop,
}

#[test]
fn ci_only_bounded_owner_literal_drop_counts_cover_both_edges_and_units() {
    for constant in [false, true] {
        for flag in [false, true] {
            let llvm = llvm_for_edge(constant, flag);
            let output = counted(&llvm.0, llvm.1, flag, Mutation::None);
            assert!(output.status.success(), "{output:?}");
            assert!(
                output.stdout.is_empty() && output.stderr.is_empty(),
                "{output:?}"
            );
        }
    }
}

#[test]
fn ci_only_bounded_owner_counter_rejects_actual_duplicate_and_missing_drop_calls() {
    for flag in [false, true] {
        let llvm = llvm_for_edge(false, flag);
        for mutation in [Mutation::DuplicateDrop, Mutation::OmitDrop] {
            let output = counted(&llvm.0, llvm.1, flag, mutation);
            assert_eq!(
                output.status.signal(),
                Some(6),
                "counter must intentionally abort: {output:?}"
            );
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("bounded-owner-count-mismatch"),
                "failure must come from the count oracle: {output:?}"
            );
        }
    }
}

fn llvm_for_edge(constant: bool, flag: bool) -> (String, usize) {
    // Change only the caller. The strictly bounded run function remains byte-for-byte unchanged.
    let (run, _) = SOURCE.split_once("\nfun entry").unwrap();
    let text = if constant {
        format!("const val FLAG: Boolean = {flag}\n{run}\nfun entry(): Unit {{ run(FLAG) }}")
    } else {
        format!("{run}\nfun entry(): Unit {{ run({flag}) }}")
    };
    let source = UnitSource::analyze("counted-b-owner.ko", &text).unwrap();
    source.assert_frontend_clean();
    assert_eq!(
        matches!(source.products, OwnedProducts::Constant(..)),
        constant
    );
    let ((program, entry), drops) =
        crate::ssa::observe_owner_drops(|| source.lower_entry(source.entry()).unwrap());
    assert_eq!(
        drops.iter().filter(|drop| drop.3.is_some()).count(),
        1,
        "the counted program must use the real B consumer"
    );
    verify_program(&program).unwrap();
    let string = program.modules[0]
        .types
        .iter()
        .position(|ty| *ty == SsaTypeKind::StringOwner)
        .unwrap();
    (
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap(),
        string,
    )
}

fn counted(llvm: &str, string: usize, flag: bool, mutation: Mutation) -> std::process::Output {
    let mut instrumented = String::new();
    let mut parameter = None;
    let mut destructors = 0;
    let mut sites = 0;
    for line in llvm.lines() {
        let line = line
            .replace("@malloc(", "@counted_malloc(")
            .replace("@free(", "@counted_free(");
        let drop_call = line.contains(&format!("call void @koven.drop.t{string}("));
        if drop_call {
            sites += 1;
        }
        if !(drop_call && matches!(mutation, Mutation::OmitDrop)) {
            instrumented.push_str(&line);
            instrumented.push('\n');
        }
        if drop_call && matches!(mutation, Mutation::DuplicateDrop) {
            instrumented.push_str(&line);
            instrumented.push('\n');
        }
        if line.starts_with("define ") && line.contains(&format!("@koven.drop.t{string}(")) {
            parameter = Some(
                line.split_once('(')
                    .unwrap()
                    .1
                    .split_once(')')
                    .unwrap()
                    .0
                    .to_owned(),
            );
        } else if parameter.is_some() && line.ends_with(':') {
            let owner = parameter.take().unwrap();
            let bits = usize::BITS;
            instrumented.push_str(&format!("  %count.bytes = extractvalue {owner}, 0\n  %count.length = extractvalue {owner}, 1\n  %count.capacity = extractvalue {owner}, 2\n  call void @counted_owner_drop(ptr %count.bytes, i{bits} %count.length, i{bits} %count.capacity)\n"));
            destructors += 1;
        }
    }
    assert_eq!(
        destructors, 1,
        "instrument the real String destructor exactly once"
    );
    assert_eq!(
        sites, 2,
        "exactly detached P and selected-owner physical sites"
    );
    let bits = usize::BITS;
    instrumented.push_str(&format!(
        "declare void @counted_owner_drop(ptr, i{bits}, i{bits})\n"
    ));
    let directory = crate::native_tests::TestDirectory::create();
    let ir = directory.join("bounded-owner.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("bounded-owner");
    fs::write(&ir, instrumented).unwrap();
    fs::write(
        &counter,
        format!("#define EXPECT_Q {}\n{COUNTER}", u8::from(flag)),
    )
    .unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg(&counter)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "LLVM/C counter must link before execution: {linked:?}"
    );
    Command::new(executable).output().unwrap()
}

const COUNTER: &str = r#"
#include <assert.h>
#include <stddef.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
static unsigned p_drops, q_drops, allocations, releases;
void counted_owner_drop(const void *bytes, size_t length, size_t capacity) {
    /* The bounded shape has literals, not heap-allocated Strings or Resource. */
    assert(capacity == 0);
    if (length == 5 && memcmp(bytes, "outer", 5) == 0) ++p_drops;
    else if (length == 4 && memcmp(bytes, "next", 4) == 0) ++q_drops;
    else abort();
}
void *counted_malloc(size_t size) { ++allocations; return malloc(size); }
void counted_free(void *value) { ++releases; free(value); }
__attribute__((destructor)) static void verify_counts(void) {
    if (p_drops != 1 || q_drops != EXPECT_Q) {
        fputs("bounded-owner-count-mismatch\n", stderr);
        abort();
    }
    assert(allocations == 0 && releases == 0);
}
"#;
