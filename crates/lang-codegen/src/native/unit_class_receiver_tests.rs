//! Class and inline receiver ownership native object/link/run coverage.
use super::*;

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
