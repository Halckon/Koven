#[path = "native_runtime_constructor_tests.rs"]
pub(crate) mod runtime_constructor_tests;

#[path = "native_field_replace_tests.rs"]
mod field_replace_tests;

#[path = "native_root_primitive_tests.rs"]
mod root_primitive_tests;

#[path = "native_return_control_tests.rs"]
pub(crate) mod return_control_tests;

#[path = "native_string_clone_tests.rs"]
mod string_clone_tests;

#[path = "native_numeric_literal_tests.rs"]
mod numeric_literal_tests;

#[path = "native_constant_tests.rs"]
mod constant_tests;

#[path = "native_non_null_assertion_tests.rs"]
mod non_null_assertion_tests;

#[path = "native_sequential_for_tests.rs"]
mod sequential_for_tests;

#[path = "native_boxed_enum_tests.rs"]
pub(crate) mod boxed_enum_tests;

#[path = "native_map_tests.rs"]
mod map_tests;

#[path = "native_ordered_map_tests.rs"]
mod ordered_map_tests;

#[path = "native_borrow_result_tests.rs"]
mod borrow_result_tests;

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::{
    diagnostic::DiagnosticDetail, lexer::lex, name_resolution::SymbolKind, parser::parse_file,
    source::SourceMap, type_checking::standard_environments,
};

use super::{NativeObjectErrorKind, NativeUnitEntry};
use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
use crate::{
    llvm::{
        LlvmAdapterError,
        layout::{LayoutFailure, LayoutQuantity, TargetLayoutError},
    },
    native::map_backend_error,
    ssa::model::{Program, SsaTypeKind, TypeOrigin},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

pub(crate) struct TestDirectory(PathBuf);

impl TestDirectory {
    pub(crate) fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-native-facade-test-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("test directory must be creatable");
        Self(path)
    }

    pub(crate) fn snapshot(&self) -> std::collections::BTreeMap<String, Vec<u8>> {
        fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                (
                    entry.file_name().into_string().unwrap(),
                    fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }

    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("owned test directory must be removable");
    }
}

pub(crate) fn emit_link_and_run(source_name: &str, text: &str, entry_name: &str) -> Output {
    let analysis = UnitSource::analyze(source_name, text)
        .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();

    let directory = TestDirectory::create();
    let object = directory.join("program.o");
    let executable = directory.join("program");
    analysis
        .emit(
            analysis.declaration(entry_name, SymbolKind::Function),
            &object,
        )
        .expect("accepted source must emit a native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    Command::new(&executable)
        .output()
        .expect("linked executable must launch")
}

#[test]
fn resolved_unit_entry_emits_the_existing_native_wrapper_object() {
    let analysis = UnitSource::analyze("facade.ko", "fun bootstrap(): Unit {}\n")
        .expect("original source must pass strict Unit admission");
    let directory = TestDirectory::create();
    let object = directory.join("bootstrap.o");

    analysis
        .emit(
            analysis.declaration("bootstrap", SymbolKind::Function),
            &object,
        )
        .expect("resolved entry must emit object");

    crate::test_support::assert_native_object(&fs::read(&object).expect("object bytes"));
    let symbols = Command::new("/usr/bin/nm")
        .arg("-gj")
        .arg(&object)
        .output()
        .expect("nm must launch");
    assert!(symbols.status.success(), "{symbols:?}");
    let names = String::from_utf8(symbols.stdout).expect("nm output must be UTF-8");
    assert_eq!(
        names
            .lines()
            .filter(|name| *name == crate::test_support::main_symbol())
            .count(),
        1
    );
}

#[test]
#[cfg(unix)]
fn borrowed_argv_entry_accepts_utf8_boundaries_and_rejects_invalid_sequences_before_call() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let analysis = UnitSource::analyze("argv.ko", "fun main(args: Array<String>): Unit {}\n")
        .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let directory = TestDirectory::create();
    let object = directory.join("argv.o");
    let executable = directory.join("argv");
    analysis
        .emit(
            NativeUnitEntry::BorrowedArguments(analysis.declaration("main", SymbolKind::Function)),
            &object,
        )
        .expect("borrowed argv entry object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");

    for arguments in [
        vec![],
        vec![b"".to_vec(), b"ascii".to_vec()],
        vec![
            "\u{80}\u{7ff}\u{800}\u{ffff}\u{10000}\u{10ffff}"
                .as_bytes()
                .to_vec(),
        ],
    ] {
        let status = Command::new(&executable)
            .args(arguments.into_iter().map(OsString::from_vec))
            .status()
            .expect("linked executable must launch");
        assert!(
            status.success(),
            "valid UTF-8 argv must reach and return from main"
        );
    }

    for invalid in [
        vec![0xc0, 0x80],
        vec![0xc1, 0xbf],
        vec![0xe0, 0x9f, 0x80],
        vec![0xed, 0xa0, 0x80],
        vec![0xf0, 0x8f, 0xbf, 0xbf],
        vec![0xf4, 0x90, 0x80, 0x80],
        vec![0xe2, 0x82],
        vec![0xff],
    ] {
        let status = Command::new(&executable)
            .arg(OsString::from_vec(invalid))
            .status()
            .expect("linked executable must launch");
        assert_eq!(
            status.code(),
            Some(1),
            "invalid UTF-8 must fail operationally"
        );
    }

    let observing = UnitSource::analyze(
        "argv-observe.ko",
        "fun main(args: Array<String>): Unit {\n\
             val zero: Int = 0\n\
             val one: Int = 1\n\
             val two: Int = 2\n\
             val three: Int = 3\n\
             if (true) { println(args[zero]) }\n\
             if (true) { println(args[one]) }\n\
             if (true) { println(args[two]) }\n\
             println(args[three])\n\
         }\n",
    )
    .expect("original source must pass strict Unit admission");
    observing.assert_frontend_clean();
    let observing_object = directory.join("argv-observe.o");
    let observing_executable = directory.join("argv-observe");
    observing
        .emit(
            NativeUnitEntry::BorrowedArguments(observing.declaration("main", SymbolKind::Function)),
            &observing_object,
        )
        .expect("observing borrowed argv entry object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&observing_object)
        .arg("-o")
        .arg(&observing_executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let output = Command::new(&observing_executable)
        .args(["first", "", "你好", "last"])
        .output()
        .expect("linked executable must launch");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, "first\n\n你好\nlast\n".as_bytes());
}

#[test]
fn entry_shape_and_analysis_identity_fail_before_object_emission() {
    let directory = TestDirectory::create();

    for (index, (source, entry_name, kind)) in [
        (
            "fun parameterized(value: Int): Unit {}",
            "parameterized",
            SymbolKind::Function,
        ),
        (
            "fun <T> generic(): Unit {}",
            "generic",
            SymbolKind::Function,
        ),
        ("fun nonunit(): Int = 1", "nonunit", SymbolKind::Function),
        ("class Value {}", "Value", SymbolKind::Classifier),
    ]
    .into_iter()
    .enumerate()
    {
        let analysis = UnitSource::analyze("invalid-entry.ko", source)
            .expect("original source must pass strict Unit admission");
        let output = directory.join(&format!("invalid-{index}.o"));
        let error = analysis
            .emit(analysis.declaration(entry_name, kind), &output)
            .expect_err("invalid entry must fail");
        assert_eq!(error.kind(), NativeObjectErrorKind::InvalidEntry);
        assert!(!output.exists());
    }

    let one_symbol = UnitSource::analyze("one.ko", "fun only(): Unit {}")
        .expect("original source must pass strict Unit admission");
    let two_symbols = UnitSource::analyze("two.ko", "fun first(): Unit {}\nfun foreign(): Unit {}")
        .expect("original source must pass strict Unit admission");
    let unknown_output = directory.join("unknown.o");
    let error = one_symbol
        .emit(
            two_symbols.declaration("foreign", SymbolKind::Function),
            &unknown_output,
        )
        .expect_err("unknown symbol id must fail");
    assert_eq!(error.kind(), NativeObjectErrorKind::InvalidEntry);
    assert!(!unknown_output.exists());

    let first = UnitSource::analyze("first.ko", "fun bootstrap(): Unit {}")
        .expect("original source must pass strict Unit admission");
    let second = UnitSource::analyze("second.ko", "fun bootstrap(): Unit {}")
        .expect("original source must pass strict Unit admission");
    let output = directory.join("foreign.o");
    let error = first
        .emit_with_products_from(
            &second,
            &second,
            first.declaration("bootstrap", SymbolKind::Function),
            &output,
        )
        .expect_err("mixed analysis must fail");
    assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
    assert!(!output.exists());
}

#[test]
fn frontend_diagnostics_and_unsupported_source_do_not_write_objects() {
    let directory = TestDirectory::create();
    let diagnostic_output = directory.join("diagnostic.o");
    fs::write(directory.join("keep"), b"unchanged sibling").unwrap();
    let before = directory.snapshot();
    let Err(diagnostic) = UnitSource::analyze("diagnostic.ko", "fun broken(): Unit { missing }")
    else {
        panic!("name diagnostics must reject before an owned Unit source exists");
    };
    assert!(
        diagnostic
            .report
            .diagnostics
            .iter()
            .any(|(stage, _)| *stage == "names")
    );
    assert_eq!(directory.snapshot(), before);
    assert!(!diagnostic_output.exists());

    let unsupported = UnitSource::analyze(
        "unsupported.ko",
        "fun closure(): Unit { val callback: ()->Unit = { val nested: ()->Unit = {}\nnested() }\ncallback() }",
    ).expect("original source must pass strict Unit admission");
    unsupported.assert_frontend_clean();
    let unsupported_output = directory.join("unsupported.o");
    let error = unsupported
        .emit(
            unsupported.declaration("closure", SymbolKind::Function),
            &unsupported_output,
        )
        .expect_err("unsupported source must fail");
    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
    assert!(!unsupported_output.exists());
}

#[test]
fn standard_error_emits_object_while_interpolated_message_fails_before_writing() {
    let directory = TestDirectory::create();
    let canonical = UnitSource::analyze(
        "error-entry.ko",
        "fun abortEntry(): Unit { error(\"fatal\") }",
    )
    .expect("original source must pass strict Unit admission");
    canonical.assert_frontend_clean();
    let object = directory.join("error.o");
    canonical
        .emit(
            canonical.declaration("abortEntry", SymbolKind::Function),
            &object,
        )
        .expect("canonical standard error must emit an object");
    assert!(object.is_file());

    let unsupported = UnitSource::analyze(
        "interpolated-error.ko",
        "fun abortEntry(): Unit { error(\"${1}\") }",
    )
    .expect("original source must pass strict Unit admission");
    unsupported.assert_frontend_clean();
    let rejected = directory.join("interpolated.o");
    let error = unsupported
        .emit(
            unsupported.declaration("abortEntry", SymbolKind::Function),
            &rejected,
        )
        .expect_err("interpolated String remains unsupported");
    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
    assert!(!rejected.exists());
}

#[test]
fn standard_println_links_and_writes_exact_utf8_stdout() {
    let analysis = UnitSource::analyze(
        "println-entry.ko",
        "fun output(): Unit { if (true) { println(\"Hello, World!\") }\nif (true) { println(\"你好\") } }",
    ).expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let directory = TestDirectory::create();
    let object = directory.join("println.o");
    let executable = directory.join("println");
    analysis
        .emit(
            analysis.declaration("output", SymbolKind::Function),
            &object,
        )
        .expect("standard println must emit an object");
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
    assert_eq!(run.stdout, "Hello, World!\n你好\n".as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");

    let unsupported = UnitSource::analyze(
        "interpolated-println.ko",
        "fun output(): Unit { println(\"${1}\") }",
    )
    .expect("original source must pass strict Unit admission");
    let rejected = directory.join("interpolated-println.o");
    let error = unsupported
        .emit(
            unsupported.declaration("output", SymbolKind::Function),
            &rejected,
        )
        .expect_err("interpolated println must fail before writing an object");
    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
    assert!(!rejected.exists());
}

#[test]
fn dynamic_strings_cross_borrow_value_and_return_boundaries_with_exact_bytes() {
    let run = emit_link_and_run(
        "dynamic-string.ko",
        r#"fun borrowLine(text: String): Unit { println(text) }
        fun takeAndReturn(own text: String): String = text
        fun makeDynamic(): String = "前\0" + "后"
        fun stringEntry(): Unit {
            val emptyOutput = println("")
            val left = "A"
            val borrowed = borrowLine(left)
            val returned = takeAndReturn("B")
            val joined = left + returned
            val joinedOutput = println(joined + "你好\0!")
            if (joined == "AB") { println("equal") }
            if (joined != "AC") { println("different") }
            val dynamicOutput = println(makeDynamic())
            val leftOutput = println(left)
        }"#,
        "stringEntry",
    );

    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        b"\nA\nAB\xe4\xbd\xa0\xe5\xa5\xbd\0!\nequal\ndifferent\n\xe5\x89\x8d\0\xe5\x90\x8e\nA\n"
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn nested_string_owners_survive_normal_path_and_clean_up_on_early_return() {
    let run = emit_link_and_run(
        "nested-string-owners.ko",
        r#"value class Packet(val text: String)
        class Holder(val text: String)
        enum class Choice { Text(text: String), Empty }
        fun inspect(text: String): Unit { println(text) }
        fun exercise(early: Boolean): Unit {
            val packet = Packet("value")
            val holder = Holder("class")
            val choice: Choice = Choice.Text("enum")
            val boxed = Box(Packet("box"))
            val shared = Rc("rc")
            val retained = shared.share()
            if (early) return
            val normal = println("normal")
            val sharedRead = inspect(shared.value)
            val retainedRead = inspect(retained.value)
        }
        fun ownerEntry(): Unit {
            val early = exercise(true)
            val normal = exercise(false)
        }"#,
        "ownerEntry",
    );

    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"normal\nrc\nrc\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn string_containers_clean_up_on_normal_path_and_early_return() {
    let run = emit_link_and_run(
        "string-containers.ko",
        r#"fun holdContainers(early: Boolean): Unit {
            val array = arrayOf<String>("array", "数组")
            val list = listOf<String>("list", "\0")
            val mutable = mutableListOf<String>("mutable")
            if (early) return
            val reached = println("containers")
        }
        fun containerEntry(): Unit {
            val early = holdContainers(true)
            val normal = holdContainers(false)
        }"#,
        "containerEntry",
    );

    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"containers\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn int_container_construction_links_and_runs_with_size_t_header() {
    let run = emit_link_and_run(
        "int-container-header.ko",
        "fun containerEntry(): Unit {\n val values = arrayOf<Int>(1, 2)\n val reached = println(\"done\")\n }",
        "containerEntry",
    );

    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"done\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn named_construction_inputs_are_not_forwarded_after_value_delivery() {
    let run = emit_link_and_run(
        "named-construction-inputs.ko",
        r#"class Holder(val text: String)
        fun hold(early: Boolean): Unit {
            val field = "field"
            val holder = Holder(field)
            val element = "element"
            val array = arrayOf<String>(element)
            if (early) return
            val reached = println("done")
        }
        fun entry(): Unit {
            val early = hold(true)
            val normal = hold(false)
        }"#,
        "entry",
    );

    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"done\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn construction_inputs_survive_later_operand_branches() {
    let run = emit_link_and_run(
        "construction-input-branch.ko",
        r#"class Holder(val text: String, val number: Int)
        fun hold(flag: Boolean): Unit {
            val text = "field"
            val holder = Holder(text, if (flag) { 1 } else { 2 })
            val element = "element"
            val array = arrayOf<String>(element, if (flag) { "left" } else { "right" })
            if (holder.number == 0) { error("bad holder") }
        }
        fun entry(): Unit {
            val first = hold(true)
            val second = hold(false)
        }"#,
        "entry",
    );

    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn copyable_construction_result_keeps_its_source_binding() {
    let run = emit_link_and_run(
        "copyable-construction-source.ko",
        r#"class Holder(val number: Int)
        fun source(flag: Boolean): Int {
            val number = 7
            val holder = Holder(if (flag) { number } else { number })
            return number
        }
        fun entry(): Unit {
            if (source(true) != 7) { error("bad true branch") }
            if (source(false) != 7) { error("bad false branch") }
        }"#,
        "entry",
    );

    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn construction_inputs_drop_when_later_operand_returns() {
    let run = emit_link_and_run(
        "construction-input-return.ko",
        r#"class Holder(val text: String, val number: Int)
        fun hold(flag: Boolean): Unit {
            val text = "field"
            val holder = Holder(text, if (flag) { return } else { 1 })
            val element = "element"
            val array = arrayOf<String>(element, if (flag) { return } else { "last" })
        }
        fun entry(): Unit {
            val first = hold(true)
            val second = hold(false)
        }"#,
        "entry",
    );

    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn move_closure_drops_or_invokes_its_owned_string_capture_once() {
    let run = emit_link_and_run(
        "string-move-closure.ko",
        r#"fun holdCapture(early: Boolean): Unit {
            val text = "captured"
            val action = move { println(text) }
            if (early) return
            val invoked = action()
        }
        fun closureEntry(): Unit {
            val early = holdCapture(true)
            val normal = holdCapture(false)
        }"#,
        "closureEntry",
    );

    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"captured\n");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn move_only_rc_payload_borrow_call_links_runs_and_releases_once() {
    let analysis = UnitSource::analyze(
        "borrow-call.ko",
        "class Resource {}\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun borrowEntry(): Unit {\n\
             val owner = Rc(Resource())\n\
             val inspected = inspect(owner.value)\n\
             val retained = owner.share()\n\
         }",
    )
    .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let directory = TestDirectory::create();
    let object = directory.join("borrow-call.o");
    let executable = directory.join("borrow-call");
    analysis
        .emit(
            analysis.declaration("borrowEntry", SymbolKind::Function),
            &object,
        )
        .expect("MoveOnly Rc payload Borrow must emit an object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked Borrow executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn nullable_class_box_and_rc_sources_link_run_and_release() {
    let analysis = UnitSource::analyze(
        "nullable-owners.ko",
        "class Node {}\n\
         value class Token(val item: Int)\n\
         fun observeNode(node: Node): Unit {}\n\
         fun observeBox(boxed: Box<Token>): Unit {}\n\
         fun inspectNode(own node: Node?): Unit {\n\
             if (node != null) { observeNode(node) }\n\
         }\n\
         fun inspectBox(own boxed: Box<Token>?): Unit {\n\
             if (boxed != null) { observeBox(boxed) }\n\
         }\n\
         fun inspectRc(own owner: Rc<Int>?): Unit {\n\
             if (owner != null) {\n\
                 val retained = owner.share()\n\
                 val copied = owner.value\n\
                 if (copied != 41) { error(\"bad nullable Rc payload\") }\n\
             }\n\
         }\n\
         fun nullableEntry(): Unit {\n\
             val node: Node? = Node()\n\
             val observedNode = inspectNode(node)\n\
             val boxed: Box<Token>? = Box(Token(1))\n\
             val observedBox = inspectBox(boxed)\n\
             val owner: Rc<Int>? = Rc(41)\n\
             val observedRc = inspectRc(owner)\n\
             val absent: Rc<Int>? = null\n\
             val observedAbsent = inspectRc(absent)\n\
         }",
    )
    .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let directory = TestDirectory::create();
    let object = directory.join("nullable-owners.o");
    let executable = directory.join("nullable-owners");
    analysis
        .emit(
            analysis.declaration("nullableEntry", SymbolKind::Function),
            &object,
        )
        .expect("nullable class, Box, and Rc source must emit an object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("system clang must launch");
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable)
        .output()
        .expect("linked nullable owner executable must launch");
    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty(), "{run:?}");
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[test]
fn nullable_when_class_box_and_rc_sources_link_run() {
    for (inner, constructor) in [
        ("Node", "Node()"),
        ("Box<Token>", "Box(Token(1))"),
        ("Rc<Int>", "Rc(41)"),
    ] {
        let consume = if inner == "Rc<Int>" {
            r#"val retained = node.share()
val checked = if (retained.value != 41) { error("lost shared payload") } else { 0 }
val printed = println("consume")
return"#
        } else {
            r#"println("consume")"#
        };
        let source = format!(
            r#"
class Node()
value class Token(val item: Int)
fun observe(node: {inner}): Unit {{
val printed = println("view")
return }}
fun consume(own node: {inner}): Unit {{ {consume} }}
fun outer(node: {inner}, own result: Int): Unit {{ error("transfer called outer") }}
fun earlyReturn(own subject: {inner}?): Unit =
    outer({constructor}, when (subject) {{ null -> 0; else -> return }})
fun earlyBreak(own subject: {inner}?): Unit {{
    loop {{ outer({constructor}, when (subject) {{ null -> 0; else -> break }}) }}
}}
fun earlyContinue(own subject: {inner}?): Unit {{
    var count = 0
    while (count < 1) {{
        count = count + 1
        val transferred = outer({constructor}, when (subject) {{ null -> 0; else -> continue }})
    }}
}}
fun make(): {inner}? {{ val printed = println("make")
val result: {inner}? = {constructor}
return result }}
fun inspect(own node: {inner}?): Unit {{
    when (node) {{ null -> {{ println("null") }}; else -> {{ observe(node) }} }}
    var count = 0
    while (count < 2) {{
        when (node) {{ null -> {{}}; else -> {{ observe(node) }} }}
        count = count + 1
    }}
    when (node) {{ null -> {{}}; else -> {{ consume(node) }} }}
}}
fun nullableEntry(): Unit {{
    val input: {inner}? = {constructor}
    val present = inspect(input)
    val absent = inspect(null)
    val returnSubject: {inner}? = {constructor}
    val returned = earlyReturn(returnSubject)
    val breakSubject: {inner}? = {constructor}
    val broken = earlyBreak(breakSubject)
    val continueSubject: {inner}? = {constructor}
    val continued = earlyContinue(continueSubject)
    when (make()) {{ null -> {{}}; else -> {{ println("temporary") }} }}
}}
"#
        );
        let analysis = UnitSource::analyze("nullable-when.ko", &source)
            .expect("original source must pass strict Unit admission");
        analysis.assert_frontend_clean();
        let directory = TestDirectory::create();
        let object = directory.join("nullable-when.o");
        let executable = directory.join("nullable-when");
        analysis
            .emit(
                analysis.declaration("nullableEntry", SymbolKind::Function),
                &object,
            )
            .unwrap_or_else(|error| panic!("{inner}: {error:?}"));
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("system clang must launch");
        assert!(linked.status.success(), "{inner}: {linked:?}");
        let run = Command::new(&executable)
            .output()
            .expect("nullable when executable must launch");
        assert!(run.status.success(), "{inner}: {run:?}");
        assert_eq!(
            run.stdout, b"view\nview\nview\nconsume\nnull\nmake\ntemporary\n",
            "{inner}"
        );
        assert!(run.stderr.is_empty(), "{inner}: {run:?}");

        // 只改测试模块的 allocator 符号，记录 Koven 自身的分配与释放，隔离 libc I/O。
        let (program, entry) = analysis
            .lower_entry(analysis.declaration("nullableEntry", SymbolKind::Function))
            .expect("counted nullable program must verify");
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("counted nullable LLVM must verify")
            .replace("@malloc(", "@counted_malloc(")
            .replace("@free(", "@counted_free(");
        // Rc runtime 的引用计数加减各只有一个静态站点；计数调用验证动态执行次数。
        let mut instrumented = String::new();
        let mut retain_sites = 0;
        let mut release_sites = 0;
        for line in llvm.lines() {
            instrumented.push_str(line);
            instrumented.push('\n');
            if line.contains(".next = add i64") {
                retain_sites += 1;
                instrumented.push_str("  call void @counted_retain()\n");
            } else if line.contains("%strong.next = sub i64") {
                release_sites += 1;
                instrumented.push_str("  call void @counted_release()\n");
            }
        }
        let shared = usize::from(inner == "Rc<Int>");
        assert_eq!((retain_sites, release_sites), (shared, shared));
        instrumented.push_str("declare void @counted_retain()\ndeclare void @counted_release()\n");
        let ir = directory.join("counted.ll");
        let counter = directory.join("counter.c");
        let counted = directory.join("counted");
        fs::write(&ir, instrumented).expect("write instrumented LLVM");
        fs::write(
            &counter,
            r#"
#include <stdlib.h>
#include <assert.h>
static void *live[16];
static int allocations, releases;
static int retains, shared_releases;
void counted_retain(void) { ++retains; }
void counted_release(void) { ++shared_releases; }
void *counted_malloc(size_t size) {
    assert(allocations < 16);
    void *value = malloc(size);
    assert(value);
    live[allocations++] = value;
    return value;
}
void counted_free(void *value) {
    int index = 0;
    while (index < allocations && live[index] != value) ++index;
    assert(index < allocations && value);
    live[index] = 0;
    ++releases;
    free(value);
}
__attribute__((destructor)) static void verify_counts(void) {
    /* input, three subjects, three Borrow temporaries, and make(): once each. */
    assert(allocations == 8);
    assert(releases == allocations);
    assert(retains == EXPECT_SHARED);
    assert(shared_releases == 9 * EXPECT_SHARED);
}
"#,
        )
        .expect("write allocator counter");
        let linked = Command::new(crate::test_support::ir_clang())
            .arg(&ir)
            .arg(&counter)
            .arg(format!("-DEXPECT_SHARED={shared}"))
            .arg("-o")
            .arg(&counted)
            .output()
            .expect("clang must build counted program");
        assert!(linked.status.success(), "{inner}: {linked:?}");
        let run = Command::new(&counted)
            .output()
            .expect("run counted program");
        assert!(run.status.success(), "{inner}: {run:?}");
        assert_eq!(
            run.stdout,
            b"view\nview\nview\nconsume\nnull\nmake\ntemporary\n"
        );
        assert!(run.stderr.is_empty(), "{inner}: {run:?}");
    }
}

#[test]
fn declarative_roots_emit_with_scalar_entry_while_runtime_globals_stay_unsupported() {
    let directory = TestDirectory::create();
    let declarative = UnitSource::analyze(
        "declarative.ko",
        "object Config { const val UNUSED = 42 }\n\
         const val UNUSED = 7\n\
         value class Pair<A, B>(val first: A, val second: B)\n\
         class Holder(val item: Int)\n\
         interface Marker\n\
         enum class Outcome<T, E> { Ok(item: T), Err(failure: E) }\n\
         fun bootstrap(): Unit {}",
    )
    .expect("original source must pass strict Unit admission");
    declarative.assert_frontend_clean();
    let object = directory.join("declarative.o");
    declarative
        .emit(
            declarative.declaration("bootstrap", SymbolKind::Function),
            &object,
        )
        .expect("pure type declarations must not block the scalar entry object");
    assert!(object.is_file());

    let runtime_value =
        UnitSource::analyze("global-root.ko", "val state = 1\nfun bootstrap(): Unit {}")
            .expect("original source must pass strict Unit admission");
    runtime_value.assert_frontend_clean();
    let rejected = directory.join("global-root.o");
    let error = runtime_value
        .emit(
            runtime_value.declaration("bootstrap", SymbolKind::Function),
            &rejected,
        )
        .expect_err("runtime globals still require an explicit runtime contract");
    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
    assert!(!rejected.exists());
}

#[test]
fn target_layout_failure_bridges_to_l0145_with_use_and_declaration_spans() {
    let source = "value class Huge(val item: Int)\nfun bootstrap(): Unit { Huge(1) }";
    let analysis = UnitSource::analyze("layout-failure.ko", source)
        .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let source_id = analysis
        .declaration_span(analysis.declaration("bootstrap", SymbolKind::Function))
        .source_id();
    let declaration = analysis
        .sources()
        .span(source_id, 12, 16)
        .expect("declaration span must be valid");
    let use_start = source.rfind("Huge(1)").expect("constructor use must exist");
    let primary = analysis
        .sources()
        .span(source_id, use_start, use_start + 7)
        .expect("constructor span must be valid");
    let mut program = Program::default();
    let module_id = program.add_module("layout_bridge");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let aggregate = module
        .add_aggregate_type("Huge", vec![integer])
        .expect("aggregate must be valid");
    module.set_type_origin(
        aggregate,
        TypeOrigin {
            primary,
            declaration,
        },
    );

    let error = map_backend_error(
        analysis.sources(),
        &program,
        LlvmAdapterError::InvalidLayout(TargetLayoutError {
            ty: aggregate,
            quantity: LayoutQuantity::Size,
            failure: LayoutFailure::ExceedsTarget {
                value: u128::MAX,
                maximum: u64::MAX.into(),
            },
        }),
    );

    assert_eq!(error.kind(), NativeObjectErrorKind::TargetLayout);
    assert_eq!(error.span(), Some(primary));
    let diagnostic = error.diagnostic().expect("L0145 diagnostic must exist");
    assert_eq!(diagnostic.code().to_string(), "L0145");
    assert_eq!(diagnostic.primary_span(), primary);
    assert!(diagnostic.details().iter().any(|detail| matches!(
        detail,
        DiagnosticDetail::Label(label) if label.span() == declaration
    )));
}

#[test]
fn comma_when_match_edge_cleanup_links_and_runs() {
    for (flag, expected) in [("true", 7), ("false", 8)] {
        let text = format!(
            "class Node {{}}\nfun probe(own node: Node): Boolean = false\nfun choose(flag: Boolean, own node: Node): Int = when {{ flag, probe(node) -> 7; else -> 8 }}\nfun main(): Unit {{ val result = choose({flag}, Node()) if (result != {expected}) {{ error(\"wrong branch\") }} }}"
        );
        let output = emit_link_and_run("comma-match.ko", &text, "main");
        assert!(output.status.success(), "{output:?}");
    }
}

#[test]
fn nominal_enum_box_source_emits_links_and_runs() {
    let analysis = UnitSource::analyze(
        "nominal-run.ko",
        "value class Wrapped(val item: Int)\n\
         class Holder(val item: Int)\n\
         class Resource {}\n\
         enum class Maybe<T> { Some(item: T), None }\n\
         enum class Owned<T> { Some(item: T), None }\n\
         fun bootstrap(): Unit {\n\
             val wrapped = Wrapped(11)\n\
             val (item) = wrapped\n\
             if (item != 11) { error(\"bad value destructuring\") }\n\
             val holder = Holder(12)\n\
             if (holder.item != 12) { error(\"bad class projection\") }\n\
             val maybe: Maybe<Int> = Maybe.Some(13)\n\
             val selected: Int = when (maybe) {\n\
                 is Maybe.Some<Int> -> maybe.item\n\
                 is Maybe.None<Int> -> 0\n\
             }\n\
             if (selected != 13) { error(\"bad enum tag\") }\n\
             val boxed = Box(Wrapped(14))\n\
             val owned: Owned<Resource> = Owned.Some(Resource())\n\
         }",
    )
    .expect("original source must pass strict Unit admission");
    analysis.assert_frontend_clean();
    let directory = TestDirectory::create();
    let object = directory.join("nominal.o");
    let executable = directory.join("nominal");
    analysis
        .emit(
            analysis.declaration("bootstrap", SymbolKind::Function),
            &object,
        )
        .expect("nominal source must emit an object");
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
}

#[test]
fn associated_constants_reach_native_scalar_char_and_string_operations() {
    let run = emit_link_and_run(
        "associated-constants.ko",
        r#"const val BASE = 40
        const val TEXT = "中" + "文"
        object Labels { const val COUNT = BASE + 2 }
        class Tokens { companion object { const val LETTER: Char = '文' } }
        fun letter(): Char = Tokens.LETTER
        fun text(): String = TEXT
        fun output(): Unit {
            if (Labels.COUNT == 42 && letter() == Tokens.LETTER) { println(TEXT) }
            val joined = println(TEXT + TEXT)
            if (text() == TEXT) { println("equal") }
        }"#,
        "output",
    );
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, "中文\n中文中文\nequal\n".as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");
}

#[path = "native_bitwise_tests.rs"]
mod bitwise_tests;

#[path = "native_resource_deinit_tests.rs"]
mod resource_deinit_tests;

#[path = "native_resource_exchange_tests.rs"]
mod resource_exchange_tests;

#[path = "native_field_resource_replace_tests.rs"]
mod field_resource_replace_tests;
