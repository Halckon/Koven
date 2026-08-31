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

    assert_eq!(
        &fs::read(&object).expect("object bytes")[..4],
        b"\xcf\xfa\xed\xfe"
    );
    assert_no_sibling_temporary(&directory.0);
    let linked = Command::new("/usr/bin/clang")
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
    let linked = Command::new("/usr/bin/clang")
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
    let linked = Command::new("/usr/bin/clang")
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
    let linked = Command::new("/usr/bin/clang")
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
    let linked = Command::new("/usr/bin/clang")
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
             val action: move (borrow String) -> Unit = move { message -> p.inspect(message) }\n\
             val invoked = action(\"unsupported-borrow\")\n\
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
