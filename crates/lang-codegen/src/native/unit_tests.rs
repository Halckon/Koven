use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

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

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[path = "unit_non_null_tests.rs"]
mod non_null_assertion_tests;

#[path = "unit_constant_tests.rs"]
mod constants;

struct UnitAnalysis {
    sources: SourceMap,
    provider_source: SourceId,
    provider: ParsedFile,
    consumer_source: SourceId,
    consumer: ParsedFile,
    names: ValidatedCompilationUnitNames,
    environment: TypeEnvironment,
    typed: ValidatedCompilationUnitTypes,
    owned: ValidatedCompilationUnitOwnership,
}

impl UnitAnalysis {
    fn inputs(&self) -> [SourceUnitInput<'_>; 2] {
        [
            SourceUnitInput::new(
                "root",
                "p/provider.ko",
                self.provider_source,
                &self.provider,
            ),
            SourceUnitInput::new(
                "root",
                "q/consumer.ko",
                self.consumer_source,
                &self.consumer,
            ),
        ]
    }

    fn declaration(&self, package: &str, name: &str) -> DeclarationId {
        self.names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.name() == name
                    && self.names.names().index().packages()[declaration.package().index()]
                        .name()
                        .segments()
                        .iter()
                        .map(String::as_str)
                        .eq(package.split('.'))
            })
            .expect("fixture declaration exists")
            .id()
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-unit-native-test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("test directory must be creatable");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("owned test directory must be removable");
    }
}

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
fn interface_default_and_super_static_calls_link_and_run() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base { fun read(): Int = 7 }\n\
         interface Derived: Base {\n\
             fun inherited(): Int = super<Base>.read()\n\
         }\n\
         class Child: Derived {}\n\
         fun entry(): Unit {\n\
             val actual = Child().inherited()\n\
             if (actual == 7) { println(\"default-super\") } else { error(\"wrong default\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("interface-default.o");
    let executable = directory.join("interface-default");

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
    .expect("interface default and super<I> calls must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked interface-default executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"default-super\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn interface_default_abstract_requirement_override_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Readable {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         class Child: Readable { override fun read(): Int = 7 }\n\
         fun entry(): Unit {\n\
             val actual = Child().throughRequirement()\n\
             if (actual == 7) { println(\"abstract-override\") }\
             else { error(\"wrong override\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("abstract-override.o");
    let executable = directory.join("abstract-override");

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
    .expect("abstract requirement must resolve to its concrete override");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked abstract override executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"abstract-override\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn inherited_and_unrelated_defaults_satisfy_abstract_requirements_natively() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived: Base { fun read(): Int = 7 }\n\
         class Child: Derived {}\n\
         interface Required {\n\
             fun otherRead(): Int\n\
             fun throughRequirement(): Int = this.otherRead()\n\
         }\n\
         interface Provided { fun otherRead(): Int = 5 }\n\
         class OtherChild: Required, Provided {}\n\
         fun entry(): Unit {\n\
             val actual = Child().throughRequirement() + OtherChild().throughRequirement()\n\
             if (actual == 12) { println(\"inherited-defaults\") }\
             else { error(\"wrong inherited default\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("inherited-defaults.o");
    let executable = directory.join("inherited-defaults");

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
    .expect("inherited effective defaults must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked inherited default executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"inherited-defaults\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn list_inherited_owner_recipe_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Host<Y>: Derived<List<Y>> {}\n\
         fun entry(): Unit {\n\
             val actual = Host<Int>().throughRequirement()\n\
             if (actual == 7) { println(\"list-inherited-owner\") }\
             else { error(\"wrong inherited owner recipe\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("list-inherited-owner.o");
    let executable = directory.join("list-inherited-owner");
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
    .expect("List inherited owner recipe must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked List inherited-owner executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"list-inherited-owner\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn parameter_independent_class_inherited_owner_recipe_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base<A> {\n\
             fun read(): Int\n\
             fun throughRequirement(): Int = this.read()\n\
         }\n\
         interface Derived<B>: Base<String> { fun read(): Int = 7 }\n\
         class Wrapper<T>(val marker: Int)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         fun entry(): Unit {\n\
             val actual = Host<Int>().throughRequirement()\n\
             if (actual == 7) { println(\"class-inherited-owner\") }\
             else { error(\"wrong class inherited owner recipe\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("class-inherited-owner.o");
    let executable = directory.join("class-inherited-owner");
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
    .expect("parameter-independent class inherited owner recipe must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked class inherited-owner executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"class-inherited-owner\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn dependent_class_inherited_owner_recipe_value_roundtrip_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base<A> { fun read(): Int }\n\
         interface Derived<B>: Base<String> {\n\
             fun read(): Int = 7\n\
             fun echo(own input: B): B = input\n\
         }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<Y>: Derived<Wrapper<Y>> {}\n\
         fun entry(): Unit {\n\
             val result = Host<Int>().echo(Wrapper<Int>(7))\n\
             if (result.item == 7) { println(\"dependent-class-inherited-owner\") }\
             else { error(\"wrong dependent class inherited owner recipe\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("dependent-class-inherited-owner.o");
    let executable = directory.join("dependent-class-inherited-owner");
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
    .expect("dependent class inherited owner recipe must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked dependent class inherited-owner executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"dependent-class-inherited-owner\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn borrow_only_interface_delegation_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Readable { fun read(): Int = 1 }\n\
         interface Derived: Readable { fun read(): Int = 2 }\n\
         class DefaultReader: Derived {}\n\
         class OverrideReader: Readable { override fun read(): Int = 7 }\n\
         class DefaultHost(val delegate: DefaultReader): Readable by delegate {}\n\
         class OverrideHost(val tag: Int, val delegate: OverrideReader): Readable by delegate {}\n\
         fun entry(): Unit {\n\
             val inherited = DefaultHost(DefaultReader()).read()\n\
             val overridden = OverrideHost(0, OverrideReader()).read()\n\
             if (inherited + overridden == 9) { println(\"borrow-delegate\") }\
             else { error(\"wrong delegate\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("borrow-delegate.o");
    let executable = directory.join("borrow-delegate");

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
    .expect("bodyful Borrow-only interface delegation must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked Borrow delegation executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"borrow-delegate\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn same_requirement_delegation_chain_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Readable { fun read(): Int = 1 }\n\
         class Reader: Readable {\n\
             override fun read(): Int {\n\
                 println(\"chain-endpoint\")\n\
                 return 7\n\
             }\n\
         }\n\
         class Middle(val tag: Int, val reader: Reader): Readable by reader {}\n\
         class Host(val tag: Int, val middle: Middle): Readable by middle {}\n\
         class Outer(val tag: Int, val host: Host): Readable by host {}\n\
         fun entry(): Unit {\n\
             val actual = Outer(0, Host(1, Middle(2, Reader()))).read()\n\
             if (actual == 7) { println(\"delegate-chain\") }\
             else { error(\"wrong chain endpoint\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("delegate-chain.o");
    let executable = directory.join("delegate-chain");

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
    .expect("same-requirement delegation chain must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked delegation-chain executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"chain-endpoint\ndelegate-chain\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn identity_changing_delegation_chain_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Base { fun read(): Int = 1 }\n\
         interface Derived: Base { fun read(): Int = 2 }\n\
         class Reader: Derived {\n\
             override fun read(): Int {\n\
                 println(\"replacement-endpoint\")\n\
                 return 7\n\
             }\n\
         }\n\
         class Middle(val reader: Reader): Derived by reader {}\n\
         class Host(val middle: Middle): Base by middle {}\n\
         fun entry(): Unit {\n\
             val actual = Host(Middle(Reader())).read()\n\
             if (actual == 7) { println(\"identity-chain\") }\
             else { error(\"wrong replacement endpoint\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("identity-chain.o");
    let executable = directory.join("identity-chain");

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
    .expect("identity-changing delegation chain must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked identity-chain executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"replacement-endpoint\nidentity-chain\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn generic_interface_owner_and_callable_delegation_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Mapper<T> {\n\
             fun <R> map(own input: R): R {\n\
                 println(\"generic-default\")\n\
                 return input\n\
             }\n\
         }\n\
         class DefaultMapper: Mapper<String> {}\n\
         class OverrideMapper: Mapper<String> {\n\
             override fun <R> map(own input: R): R {\n\
                 println(\"generic-override\")\n\
                 return input\n\
             }\n\
         }\n\
         class DefaultHost(val delegate: DefaultMapper): Mapper<String> by delegate {}\n\
         class OverrideHost(val delegate: OverrideMapper): Mapper<String> by delegate {}\n\
         fun entry(): Unit {\n\
             val inherited = DefaultHost(DefaultMapper()).map<Long>(7L)\n\
             val overridden = OverrideHost(OverrideMapper()).map<Long>(9L)\n\
             if (inherited + overridden == 16L) { println(\"generic-delegate\") }\
             else { error(\"wrong generic delegate\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("generic-delegate.o");
    let executable = directory.join("generic-delegate");

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
    .expect("generic interface owner/callable delegation must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked generic-delegation executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"generic-default\ngeneric-override\ngeneric-delegate\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn parameter_independent_generic_nominal_delegation_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class Reader<T>(val marker: Int): Readable {\n\
             override fun read(): Int = this.readMarker()\n\
             fun readMarker(): Int {\n\
                 println(\"generic-runtime-reader\")\n\
                 return this.marker\n\
             }\n\
         }\n\
         class Host<T>(val delegate: Reader<Int>): Readable by delegate {}\n\
         fun entry(): Unit {\n\
             val actual = Host<String>(Reader<Int>(7)).read()\n\
             if (actual == 7) { println(\"generic-runtime-layout\") }\
             else { error(\"wrong generic runtime layout\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("generic-runtime-layout.o");
    let executable = directory.join("generic-runtime-layout");

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
    .expect("parameter-independent generic nominal layout must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked generic-runtime executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"generic-runtime-reader\ngeneric-runtime-layout\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn nested_generic_nominal_delegation_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Readable { fun read(): Int }\n\
         class Reader<T>(val item: T): Readable {\n\
             override fun read(): Int {\n\
                 println(\"nested-generic-reader\")\n\
                 return 7\n\
             }\n\
         }\n\
         class Wrapper<T>(val item: T)\n\
         class Host<T>(val delegate: Reader<Wrapper<T>>): Readable by delegate {}\n\
         fun entry(): Unit {\n\
             val actual = Host<String>(Reader<Wrapper<String>>(Wrapper<String>(\"payload\"))).read()\n\
             if (actual == 7) { println(\"nested-generic-delegate\") }\
             else { error(\"wrong nested generic delegation\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("nested-generic-delegate.o");
    let executable = directory.join("nested-generic-delegate");

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
    .expect("nested generic delegation must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked nested-generic delegation executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"nested-generic-reader\nnested-generic-delegate\n"
    );
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
fn inout_interface_default_links_and_runs() {
    let analysis = analyze_sources(
        "package p\n\
         interface Mutable { inout fun probe(): Int = 7 }\n\
         class Counter(var count: Int): Mutable { fun read(): Int = count }\n\
         fun entry(): Unit {\n\
             val counter = Counter(5)\n\
             val defaultValue = counter.probe()\n\
             val actual = counter.read() + defaultValue\n\
             if (actual == 12) { println(\"inout-default\") }\
             else { error(\"wrong receiver default\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("inout-default.o");
    let executable = directory.join("inout-default");

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
    .expect("Inout interface default must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked Inout-default executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"inout-default\n");
    assert!(run.stderr.is_empty(), "{run:?}");
    assert_no_sibling_temporary(&directory.0);
}

#[test]
fn value_interface_default_move_only_and_copyable_specializations_link_and_run() {
    let analysis = analyze_sources(
        "package p\n\
         interface Finishable {\n\
             own fun finish(): Int = 40\n\
             own fun relay(): Int = finish()\n\
             own fun forward(): Int = this.relay()\n\
             own fun choose(flag: Boolean): Int {\n\
                 if (flag) { return finish() }\n\
                 return finish()\n\
             }\n\
         }\n\
         class Resource: Finishable {}\n\
         value class Counter(val item: Int): Finishable {}\n\
         fun entry(): Unit {\n\
             val counter = Counter(1)\n\
             val actual = Resource().forward() + counter.forward() + counter.forward()\n\
                 + Resource().choose(true) + counter.choose(false)\n\
             if (actual == 200) { println(\"value-default\") }\
             else { error(\"wrong Value receiver default\") }\n\
         }",
        "package q\nfun unused(): Unit {}",
    );
    let inputs = analysis.inputs();
    let entry_declaration = analysis.declaration("p", "entry");
    let directory = TestDirectory::create();
    let object = directory.join("value-default.o");
    let executable = directory.join("value-default");

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
    .expect("MoveOnly and Copyable Value interface defaults must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked Value-default executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"value-default\n");
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

fn analyze_unit() -> UnitAnalysis {
    analyze_sources(
        "package p\n\
         value class Token(val item: Int)\n\
         class Bundle(val text: String, val count: Int)\n\
         fun make(number: Int): String = if (number == 5) {\n\
             \"provider\" + \"!\"\n\
         } else {\n\
             \"fallback\" + \"!\"\n\
         }\n\
         fun inspect(message: String): Unit {}\n\
         fun makeBundle(): Bundle = Bundle(count = 7, text = \"bundle\" + \"!\")\n\
         fun count(own bundle: Bundle): Int = bundle.count\n\
         fun boxed(): Box<Token> = Box(Token(9))\n\
         fun inspectBox(own resource: Box<Token>): Int = 1\n\
         fun buildRc(): Rc<Int> = Rc(40)\n\
         fun useRc(own owner: Rc<Int>): Int {\n\
             val retained = owner.share()\n\
             val copied = retained.value\n\
             return copied + owner.value\n\
         }",
        "package q\n\
         import p.make as build\n\
         import p.inspect\n\
         fun exercise(flag: Boolean): Unit {\n\
             val bundle = p.makeBundle()\n\
             val boxed = p.boxed()\n\
             val shared = p.buildRc()\n\
             if (flag) { return }\n\
             val counted = p.count(bundle)\n\
             val inspected = p.inspectBox(boxed)\n\
             val used = p.useRc(shared)\n\
         }\n\
         fun entry(): Unit {\n\
             val offset = 2\n\
             val action: move (borrow Int) -> String = move { item ->\n\
                 if (item == 3) {\n\
                     build(item + offset)\n\
                 } else {\n\
                     \"unused\" + \"!\"\n\
                 }\n\
             }\n\
             val message = action(3)\n\
             val seen = inspect(message)\n\
             val ownedAction: move (own String) -> Unit = move { owned -> inspect(owned) }\n\
             val ownedInvoked = ownedAction(\"native-owned\")\n\
             val early = exercise(true)\n\
             val normal = exercise(false)\n\
         }\n\
         fun argvEntry(args: Array<String>): Unit {}\n\
         fun invalidEntry(number: Int): Unit {}\n\
         fun unsupportedBorrow(): Unit {\n\
             val action: move (borrow p.Bundle) -> Unit = move { message -> }\n\
             val invoked = action(p.makeBundle())\n\
         }",
    )
}

fn analyze_sources(provider_text: &str, consumer_text: &str) -> UnitAnalysis {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider_text);
    let (consumer_source, consumer) = parsed(&mut sources, "q/consumer.ko", consumer_text);
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .expect("name resolution succeeds")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .expect("type checking succeeds")
        .validate()
        .expect("valid types");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .expect("ownership checking succeeds")
        .validate()
        .expect("valid ownership");
    UnitAnalysis {
        sources,
        provider_source,
        provider,
        consumer_source,
        consumer,
        names,
        environment,
        typed,
        owned,
    }
}

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
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
