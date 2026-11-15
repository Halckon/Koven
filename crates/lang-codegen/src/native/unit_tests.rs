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
fn unit_object_atomically_replaces_links_and_runs_across_packages() {
    let analysis = analyze_unit();
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("program.o");
    let executable = directory.join("program");
    fs::write(&object, b"previous object").expect("seed output");

    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &object,
    )
    .expect("validated compilation unit emits one atomic native object");

    crate::test_support::assert_native_object(&fs::read(&object).expect("object bytes"));
    assert_no_sibling_temporary(&directory.0);
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked executable must launch");
    assert!(run.status.success(), "{run:?}");

    let argv_object = directory.join("argv.o");
    let argv_executable = directory.join("argv");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        NativeUnitEntry::BorrowedArguments(analysis.declaration("q", "argvEntry")),
        &argv_object,
    )
    .expect("borrowed Array<String> unit entry emits one native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&argv_object)
        .arg("-o")
        .arg(&argv_executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&argv_executable)
        .args(["first", "second"])
        .output()
        .expect("argv executable must launch");
    assert!(run.status.success(), "{run:?}");
}

#[test]
fn direct_member_receivers_link_run_with_source_order_and_unique_class_drop() {
    let analysis = analyze_sources(
        "package p\n\
         value class Counter(val item: Int) {\n\
             fun add(own delta: Int): Int {\n\
                 println(\"value-borrow-body\")\n\
                 return item + delta\n\
             }\n\
             own fun consume(own delta: Int): Int {\n\
                 println(\"value-own-body\")\n\
                 return item + delta\n\
             }\n\
         }\n\
         class Resource {\n\
             fun ping(): Int {\n\
                 println(\"class-borrow-body\")\n\
                 return 41\n\
             }\n\
             own fun finish(): Int {\n\
                 println(\"class-own-body\")\n\
                 return 42\n\
             }\n\
         }\n\
         fun makeCounter(): Counter {\n\
             println(\"receiver\")\n\
             return Counter(1)\n\
         }\n\
         fun makeDelta(): Int {\n\
             println(\"argument\")\n\
             return 40\n\
         }\n\
         fun makeReusableCounter(): Counter = Counter(20)\n\
         fun makeResource(): Resource {\n\
             println(\"resource\")\n\
             return Resource()\n\
         }",
        "package q\n\
         fun entry(): Unit {\n\
             val answer = p.makeCounter().add(p.makeDelta())\n\
             if (answer != 41) { error(\"bad borrowed value receiver\") }\n\
             val counter = p.makeReusableCounter()\n\
             val first = counter.consume(1)\n\
             val second = counter.consume(2)\n\
             if (first != 21 || second != 22) { error(\"bad copied value receiver\") }\n\
             val resource = p.makeResource()\n\
             val observed = resource.ping()\n\
             val finished = resource.finish()\n\
             if (observed != 41) { error(\"bad borrowed class receiver\") }\n\
             if (finished != 42) { error(\"bad moved class receiver\") }\n\
         }",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("q", "entry");
    let (program, _) = lower_scalar_unit_with_entry(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
    )
    .expect("direct source member receivers must lower to verified SSA");
    let module = &program.modules[0];
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains("q.entry"))
        .expect("entry function exists");
    let ping = module
        .functions
        .iter()
        .find(|function| function.name.contains("Resource.ping"))
        .expect("Borrow member exists");
    let finish = module
        .functions
        .iter()
        .find(|function| function.name.contains("Resource.finish"))
        .expect("Value member exists");
    let (ping_call, ping_loan) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match instruction.operation {
            Operation::DirectCall {
                callee,
                receiver: Some(EntityId::Loan(loan)),
                ..
            } if callee == ping.id() => Some((index, loan)),
            _ => None,
        })
        .expect("class Borrow receiver call exists");
    let (finish_call, moved_owner) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match instruction.operation {
            Operation::DirectCall {
                callee,
                receiver: Some(EntityId::Value(owner)),
                ..
            } if callee == finish.id() => Some((index, owner)),
            _ => None,
        })
        .expect("class Value receiver call exists");
    let borrowed_place = entry
        .instructions
        .iter()
        .find_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::BorrowBegin { place, .. }, [EntityId::Loan(result)])
                    if *result == ping_loan =>
                {
                    Some(*place)
                }
                _ => None,
            },
        )
        .expect("Borrow receiver loan has a source place");
    let borrowed_owner = entry
        .instructions
        .iter()
        .find_map(
            |instruction| match (&instruction.operation, instruction.results.as_slice()) {
                (Operation::RootPlace { owner }, [EntityId::Place(result)])
                    if *result == borrowed_place =>
                {
                    Some(*owner)
                }
                _ => None,
            },
        )
        .expect("Borrow receiver place has an owner");
    let borrow_end = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == ping_loan)
        })
        .expect("class Borrow receiver ends before the Value call");
    assert_eq!(borrowed_owner, moved_owner, "the same class owner is moved");
    assert!(ping_call < borrow_end && borrow_end < finish_call);
    assert!(!entry.instructions.iter().any(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if owner == moved_owner)
    }));
    let EntityId::Value(callee_owner) = finish.blocks[0].parameters[0] else {
        panic!("Value member receiver is an owned value");
    };
    assert_eq!(
        finish
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::Drop { owner } if owner == callee_owner)
            })
            .count(),
        1,
        "the Value receiver callee owns exactly one class drop"
    );
    let directory = TestDirectory::create();
    let object = directory.join("receivers.o");
    let executable = directory.join("receivers");

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
    .expect("direct source member receivers must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked receiver executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"receiver\nargument\nvalue-borrow-body\nvalue-own-body\nvalue-own-body\nresource\nclass-borrow-body\nclass-own-body\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn inout_class_payload_mutation_is_observed_by_a_later_borrow() {
    let analysis = analyze_sources(
        "package p\n\
         class Cell(var item: Int) {\n\
             inout fun set(own next: Int): Unit = item = next\n\
             fun isUpdated(): Boolean = item == 42\n\
         }\n\
         fun rhs(): Int {\n\
             println(\"rhs\")\n\
             return 42\n\
         }",
        "package q\n\
         import p.Cell\n\
         fun entry(): Unit {\n\
             val cell = Cell(1)\n\
             val ignored = cell.set(p.rhs())\n\
             val updated = cell.isUpdated()\n\
             if (!updated) { error(\"payload mutation was not observed\") }\n\
             println(\"observed\")\n\
         }",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("q", "entry");
    let (program, _) = lower_scalar_unit_with_entry(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        entry_declaration,
    )
    .expect("Inout class payload mutation must lower to verified SSA");
    let module = &program.modules[0];
    let entry = module
        .functions
        .iter()
        .find(|function| function.name.contains("q.entry"))
        .expect("entry function");
    let setter = module
        .functions
        .iter()
        .find(|function| function.name.contains("Cell.set"))
        .expect("Inout setter");
    let getter = module
        .functions
        .iter()
        .find(|function| function.name.contains("Cell.isUpdated"))
        .expect("Borrow getter");
    let receiver_owner = |callee| {
        let loan = entry
            .instructions
            .iter()
            .find_map(|instruction| match instruction.operation {
                Operation::DirectCall {
                    callee: actual,
                    receiver: Some(EntityId::Loan(loan)),
                    ..
                } if actual == callee => Some(loan),
                _ => None,
            })
            .expect("receiver call loan");
        let place = entry
            .instructions
            .iter()
            .find_map(|instruction| {
                match (&instruction.operation, instruction.results.as_slice()) {
                    (Operation::BorrowBegin { place, .. }, [EntityId::Loan(result)])
                        if *result == loan =>
                    {
                        Some(*place)
                    }
                    _ => None,
                }
            })
            .expect("receiver loan place");
        entry
            .instructions
            .iter()
            .find_map(|instruction| {
                match (&instruction.operation, instruction.results.as_slice()) {
                    (Operation::RootPlace { owner }, [EntityId::Place(result)])
                        if *result == place =>
                    {
                        Some(*owner)
                    }
                    _ => None,
                }
            })
            .expect("receiver root owner")
    };
    assert_eq!(receiver_owner(setter.id()), receiver_owner(getter.id()));
    let EntityId::Loan(setter_receiver) = setter.blocks[0].parameters[0] else {
        panic!("setter receiver loan");
    };
    assert!(setter.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::HeapFieldReplace {
            receiver,
            field: 0,
            ..
        } if receiver == setter_receiver
    )));
    let EntityId::Loan(getter_receiver) = getter.blocks[0].parameters[0] else {
        panic!("getter receiver loan");
    };
    assert!(getter.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::HeapFieldRead {
            receiver,
            field: 0
        } if receiver == getter_receiver
    )));

    let directory = TestDirectory::create();
    let object = directory.join("inout-payload.o");
    let executable = directory.join("inout-payload");
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
    .expect("Inout payload source must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked Inout payload executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"rhs\nobserved\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn move_only_class_payload_replacement_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         class Cell(var item: String) {\n\
             inout fun set(own next: String): Unit {\n\
                 val ignored: Unit = (this.item = next)\n\
             }\n\
         }\n\
         fun entry(): Unit {\n\
             val old = \"o\" + \"ld\"\n\
             val cell = Cell(old)\n\
             val next = \"n\" + \"ew\"\n\
             val ignored = cell.set(next)\n\
             if (true) { println(\"done\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("move-only-field.o");
    let executable = directory.join("move-only-field");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("p", "entry"),
        &object,
    )
    .expect("MoveOnly field replacement must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked MoveOnly field replacement executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"done\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn stateless_object_borrow_receiver_links_and_runs_without_runtime_storage() {
    let analysis = analyze_sources(
        "package p\n\
         object Registry { fun message(): String = \"object\" }\n\
         fun entry(): Unit { println(Registry.message()) }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("object-receiver.o");
    let executable = directory.join("object-receiver");

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
    .expect("stateless object Borrow receiver must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked object receiver executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"object\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn inline_inout_read_only_receivers_link_and_run() {
    let analysis = analyze_sources(
        "package p\n\
         value class Counter(var item: Int) {\n\
             inout fun read(): Int = item\n\
         }\n\
         enum class Signal {\n\
             Ready;\n\
             inout fun code(): Int = 9\n\
         }\n\
         fun entry(): Unit {\n\
             var counter = Counter(7)\n\
             var signal = Signal.Ready\n\
             val actual = counter.read() + signal.code()\n\
             if (actual == 16) { println(\"inline-inout\") } else { error(\"wrong receiver\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("inline-inout-receiver.o");
    let executable = directory.join("inline-inout-receiver");

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
    .expect("inline Inout receivers must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked inline Inout receiver executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"inline-inout\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn copyable_inline_inout_mutation_is_observed_after_writeback() {
    let analysis = analyze_sources(
        "package p\n\
         fun next(): Int = 7\n\
         value class Counter(var item: Int) {\n\
             inout fun set(own nextValue: Int): Unit { item = nextValue }\n\
             fun read(): Int = item\n\
         }\n\
         fun entry(): Unit {\n\
             var counter = Counter(1)\n\
             val ignored = counter.set(next())\n\
             if (counter.read() == 7) { println(\"inline-writeback\") } else { error(\"stale value\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("inline-inout-writeback.o");
    let executable = directory.join("inline-inout-writeback");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("p", "entry"),
        &object,
    )
    .expect("Copyable inline Inout write-back must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked inline write-back executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"inline-writeback\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn move_only_inline_inout_takes_the_owner_back_without_double_drop() {
    let analysis = analyze_sources(
        "package p\n\
         fun seed(): String = \"owned\" + \"-resource\"\n\
         value class Resource(val owner: String) {\n\
             inout fun inspect(): Int = 7\n\
         }\n\
         fun entry(): Unit {\n\
             var resource = Resource(seed())\n\
             if (resource.inspect() == 7) { println(\"move-only-writeback\") } else { error(\"wrong receiver\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("move-only-inline-inout.o");
    let executable = directory.join("move-only-inline-inout");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("p", "entry"),
        &object,
    )
    .expect("MoveOnly inline Inout write-back must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked MoveOnly inline Inout executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"move-only-writeback\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn move_only_inline_owner_mutates_a_copyable_field_natively() {
    let analysis = analyze_sources(
        "package p\n\
         fun seed(): String = \"owned\" + \"-resource\"\n\
         fun next(): Int = 7\n\
         value class Resource(val owner: String, var generation: Int) {\n\
             inout fun set(nextValue: Int): Unit { generation = nextValue }\n\
             fun read(): Int = generation\n\
         }\n\
         fun entry(): Unit {\n\
             var resource = Resource(seed(), 1)\n\
             val ignored = resource.set(next())\n\
             if (resource.read() == 7) { println(\"move-only-copyable-field\") } else { error(\"stale field\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("move-only-copyable-field.o");
    let executable = directory.join("move-only-copyable-field");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("p", "entry"),
        &object,
    )
    .expect("MoveOnly inline owner Copyable field mutation must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked MoveOnly inline field mutation executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"move-only-copyable-field\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn move_only_inline_replacement_collects_nested_field_drop_glue() {
    let analysis = analyze_sources(
        "package p\n\
         value class Payload(val text: String)\n\
         value class Resource(val marker: Int, var item: Payload) {\n\
             inout fun reset(): Unit { item = Payload(\"n\" + \"ew\") }\n\
         }\n\
         fun entry(): Unit {\n\
             var resource = Resource(1, Payload(\"o\" + \"ld\"))\n\
             val ignored = resource.reset()\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("nested-inline-replacement.o");
    let executable = directory.join("nested-inline-replacement");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("p", "entry"),
        &object,
    )
    .expect("nested MoveOnly inline field replacement must collect recursive drop glue");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("nested inline replacement executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}













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
