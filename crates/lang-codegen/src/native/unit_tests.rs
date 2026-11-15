#[path = "unit_borrow_result_tests.rs"]
mod borrow_result_tests;
#[allow(unused_imports)]
pub(crate) use boxed_enum_tests::container_remove_at_tests;
#[path = "unit_companion_boundary_tests.rs"]
pub(crate) mod companion_boundary;
#[allow(unused_imports)]
pub(crate) use map_consume_tests::container_insert_at_tests;
#[allow(unused_imports)]
pub(crate) use map_consume_tests::container_remove_first_tests;
#[allow(unused_imports)]
pub(crate) use map_consume_tests::container_remove_last_tests;
#[allow(unused_imports)]
pub(crate) use map_operand_last_use_tests::borrow_string_tests;
#[allow(unused_imports)]
pub(crate) use test_directory::container_append_tests;
#[allow(unused_imports)]
pub(crate) use test_directory::container_clear_tests;
#[path = "unit_container_size_tests.rs"]
pub(crate) mod container_size_tests;
#[path = "unit_handoff_contract_tests.rs"]
mod handoff_contracts;
#[path = "unit_map_consume_tests.rs"]
pub(crate) mod map_consume_tests;
#[allow(unused_imports)]
pub(crate) use map_loop_owner_tests::map_key_return_tests;
#[path = "unit_map_loop_owner_tests.rs"]
pub(crate) mod map_loop_owner_tests;
#[allow(unused_imports)]
pub(crate) use test_directory::map_word_frequency_tests;
#[path = "unit_owned_null_cleanup_tests.rs"]
pub(crate) mod owned_null_cleanup_tests;
#[allow(unused_imports)]
pub(crate) use boxed_enum_tests::string_clone_tests;
#[allow(unused_imports)]
pub(crate) use map_consume_tests::temporary_null_cleanup_tests;
#[allow(unused_imports)]
pub(crate) use map_operand_last_use_tests::runtime_global_tests;
#[allow(unused_imports)]
pub(crate) use test_directory::receiver_two_phase_tests;

use std::{fs, path::Path, process::Command};

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        DeclarationId, SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    ownership_checking::{ValidatedCompilationUnitOwnership, check_compilation_unit_ownership},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        TypeEnvironment, ValidatedCompilationUnitTypes, check_compilation_unit_types,
        standard_environments,
    },
};

use super::{NativeObjectErrorKind, NativeUnitEntry, emit_native_unit_object};
use crate::ssa::{
    model::{EntityId, Operation},
    unit_lower::lower_scalar_unit_with_entry,
};

#[path = "unit_interface_delegation_tests.rs"]
mod interface_delegation_tests;

#[path = "unit_class_receiver_tests.rs"]
mod class_receiver_tests;

#[path = "unit_test_directory.rs"]
pub(crate) mod test_directory;
#[allow(unused_imports)]
pub(crate) use boxed_enum_tests::for_atomic;
#[allow(unused_imports)]
pub(crate) use boxed_enum_tests::iteration_boundaries;
use test_directory::TestDirectory;
#[allow(unused_imports)]
pub(crate) use test_directory::iteration_conditional_receivers;
#[path = "unit_for_report_resource_tests.rs"]
pub(crate) mod iteration_report_resources;
#[allow(unused_imports)]
pub(crate) use owned_null_cleanup_tests::iteration_resources;
#[allow(unused_imports)]
pub(crate) use owned_null_cleanup_tests::sequential_iteration;

#[allow(unused_imports)]
pub(crate) use map_operand_last_use_tests::non_null_assertion_tests;

#[path = "unit_constant_tests.rs"]
pub(crate) mod constants;

#[path = "unit_boxed_enum_tests.rs"]
pub(crate) mod boxed_enum_tests;

#[allow(unused_imports)]
pub(crate) use map_operand_last_use_tests::analysis_support;
use map_operand_last_use_tests::analysis_support::{
    UnitAnalysis, analyze_sources, analyze_unit, parsed,
};























#[test]
fn direct_slot_generic_string_replacement_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         class Cell<T>(var item: T) {\n\
             inout fun set(own replacement: T): Unit { this.item = replacement }\n\
         }\n\
         fun entry(): Unit {\n\
             val old = \"old\" + \"-value\"\n\
             val replacement = \"new\" + \"-value\"\n\
             val holder = Cell<String>(old)\n\
             val ignored = holder.set(replacement)\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("direct-slot-generic.o");
    let executable = directory.join("direct-slot-generic");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("direct-slot generic String replacement must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked direct-slot generic executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn nested_generic_wrapper_replacement_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         class Wrapper<T>(val item: T)\n\
         class Holder<T>(var wrapped: Wrapper<T>) {\n\
             inout fun set(own replacement: Wrapper<T>): Unit { this.wrapped = replacement }\n\
         }\n\
         fun entry(): Unit {\n\
             val holder = Holder<String>(Wrapper<String>(\"old\" + \"-value\"))\n\
             val ignored = holder.set(Wrapper<String>(\"new\" + \"-value\"))\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("nested-generic-wrapper.o");
    let executable = directory.join("nested-generic-wrapper");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("nested generic Wrapper<T> replacement must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked nested-generic executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn generic_pointer_nullable_field_replacement_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         class Node\n\
         class Holder<T>(var item: T?) {\n\
             inout fun set(own replacement: T?): Unit { this.item = replacement }\n\
         }\n\
         fun entry(): Unit {\n\
             val holder = Holder<Node>(Node())\n\
             val cleared = holder.set(null)\n\
             val next: Node? = Node()\n\
             val replaced = holder.set(next)\n\
             if (true) { println(\"nullable-field\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("generic-nullable-field.o");
    let executable = directory.join("generic-nullable-field");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("generic pointer nullable field must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked generic nullable executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"nullable-field\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn enum_borrow_and_value_receivers_link_and_run() {
    let analysis = analyze_sources(
        "package p\n\
         enum class Signal {\n\
             Ready;\n\
             fun code(): Int = 7\n\
         }\n\
         enum class Owned {\n\
             Full(text: String);\n\
             own fun consume(): Int = 8\n\
         }\n\
         fun entry(): Unit {\n\
             val signal = Signal.Ready\n\
             val borrowed = signal.code()\n\
             val consumed = Owned.Full(\"owned\").consume()\n\
             if (borrowed + consumed == 15) { println(\"enum-receiver\") }\n\
             else { error(\"wrong enum receiver result\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("enum-receiver.o");
    let executable = directory.join("enum-receiver");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("enum receiver program must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked enum receiver executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"enum-receiver\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn move_only_empty_enum_case_links_and_runs_with_full_case_drop_glue() {
    let analysis = analyze_sources(
        "package p\n\
         enum class Owned {\n\
             Empty, Full(text: String);\n\
             own fun score(): Int = 9\n\
         }\n\
         fun empty(): Owned = (Owned.Empty)\n\
         fun consume(own input: Owned): Int = 7\n\
         fun entry(): Unit {\n\
             val local = (Owned.Empty)\n\
             val receiver = Owned.Empty\n\
             val fromLocal = consume(local)\n\
             val fromReturn = consume(empty())\n\
             val fromReceiver = receiver.score()\n\
             val groupedFull = (Owned.Full(\"payload\"))\n\
             val full = groupedFull.score()\n\
             if (fromLocal + fromReturn + fromReceiver + full == 32) {\n\
                 println(\"empty-enum-owner\")\n\
             } else { error(\"wrong empty enum owner result\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("empty-enum-owner.o");
    let executable = directory.join("empty-enum-owner");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
        &object,
    )
    .expect("MoveOnly empty and Full enum cases must emit one native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked MoveOnly empty enum executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"empty-enum-owner\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}



#[test]
fn unit_object_failures_preserve_targets_and_cleanup_sibling_temporary() {
    let analysis = analyze_unit();
    let foreign = analyze_unit();
    let inputs = analysis.inputs();
    let reversed_inputs = [inputs[1], inputs[0]];
    let directory = TestDirectory::create();
    let object = directory.join("preserved.o");
    fs::write(&object, b"preserve me").expect("seed output");

    for (entry, expected) in [
        (
            NativeUnitEntry::NoArguments(analysis.declaration("q", "invalidEntry")),
            NativeObjectErrorKind::InvalidEntry,
        ),
        (
            NativeUnitEntry::NoArguments(analysis.declaration("q", "unsupportedBorrow")),
            NativeObjectErrorKind::UnsupportedSource,
        ),
        (
            NativeUnitEntry::BorrowedArguments(analysis.declaration("q", "entry")),
            NativeObjectErrorKind::InvalidEntry,
        ),
        (
            NativeUnitEntry::NoArguments(analysis.declaration("q", "argvEntry")),
            NativeObjectErrorKind::InvalidEntry,
        ),
    ] {
        let error = emit_native_unit_object(
            &analysis.sources,
            &inputs,
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            entry,
            &object,
        )
        .expect_err("invalid unit must fail before replacing its target");
        assert_eq!(error.kind(), expected);
        assert_eq!(fs::read(&object).expect("preserved output"), b"preserve me");
        assert_no_sibling_temporary(&directory.0);
    }

    let unsupported_entry = analysis.declaration("q", "unsupportedBorrow");
    let forward_error = emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        unsupported_entry,
        &object,
    )
    .expect_err("unsupported unit must fail in canonical input order");
    let reversed_error = emit_native_unit_object(
        &analysis.sources,
        &reversed_inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        unsupported_entry,
        &object,
    )
    .expect_err("input permutation must preserve the unsupported diagnostic");
    assert_eq!(forward_error.kind(), reversed_error.kind());
    assert_eq!(forward_error.span(), reversed_error.span());
    assert_eq!(fs::read(&object).expect("preserved output"), b"preserve me");
    assert_no_sibling_temporary(&directory.0);

    let error = emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &foreign.typed,
        &foreign.owned,
        analysis.declaration("q", "invalidEntry"),
        &object,
    )
    .expect_err("analysis mismatch must precede incidental entry-shape validation");
    assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
    assert_eq!(fs::read(&object).expect("preserved output"), b"preserve me");
    assert_no_sibling_temporary(&directory.0);

    let blocked_output = directory.join("blocked");
    fs::create_dir(&blocked_output).expect("commit target directory");
    let error = emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &blocked_output,
    )
    .expect_err("failed atomic commit must surface as backend error");
    assert_eq!(error.kind(), NativeObjectErrorKind::Backend);
    assert!(blocked_output.is_dir());
    assert_no_sibling_temporary(&directory.0);
}

fn assert_no_sibling_temporary(directory: &Path) {
    assert!(
        fs::read_dir(directory)
            .expect("read test directory")
            .all(|entry| !entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".koven-unit-object-"))
    );
}

#[allow(unused_imports)]
pub(crate) use map_operand_last_use_tests::bitwise_tests;

#[allow(unused_imports)]
pub(crate) use iteration_report_resources::resource_deinit_tests;

#[allow(unused_imports)]
pub(crate) use owned_null_cleanup_tests::resource_exchange_tests;

#[allow(unused_imports)]
pub(crate) use test_directory::field_resource_replace_tests;

#[allow(unused_imports)]
pub(crate) use test_directory::owned_map_borrow_tests;

#[path = "unit_map_operand_last_use_tests.rs"]
pub(crate) mod map_operand_last_use_tests;
#[allow(unused_imports)]
pub(crate) use test_directory::owned_source_continue_tests;
