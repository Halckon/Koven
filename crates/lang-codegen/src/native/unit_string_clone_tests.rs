use super::*;

#[test]
fn string_clone_cross_file_borrow_container_and_return_reach_native() {
    let analysis = analyze_sources(
        "package p\nfun duplicate(text: String): String = text.clone()\nfun take(own text: String): Unit { println(text) }",
        r#"package q
        fun entry(): Unit {
            val source = "界\0" + "é"
            val copy = p.duplicate(source)
            p.take(copy)
            println(source)
            val texts = listOf(source, "tail")
            val item = texts[0].clone()
            println(texts[0])
            p.take(item)
            println("".clone())
            println("static".clone())
        }"#,
    );
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("clone.o");
    let executable = directory.join("clone");
    emit_native_unit_object(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        NativeUnitEntry::NoArguments(analysis.declaration("q", "entry")),
        &object,
    )
    .expect("unit clone emits native object");
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        "界\0é\n界\0é\n界\0é\n界\0é\n\nstatic\n".as_bytes()
    );
}

#[test]
fn string_clone_unit_borrows_rc_payload() {
    let analysis = analyze_sources(
        "package p\nfun duplicate(text: String): String = text.clone()",
        r#"package q
        fun entry(): Unit { val shared = Rc("shared" + "text"); println(shared.value.clone()); println(shared.value) }
    "#,
    );
    let inputs = analysis.inputs();
    let (program, entry) = lower_scalar_unit_with_entry(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
    )
    .expect("Rc payload is a String place");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let directory = TestDirectory::create();
    let ir = directory.join("rc-clone.ll");
    let executable = directory.join("rc-clone");
    fs::write(&ir, llvm).unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"sharedtext\nsharedtext\n");
}

#[test]
fn string_clone_unit_borrows_named_fields_without_moving_them() {
    let analysis = analyze_sources(
        r#"package p
class Holder(val text: String)
fun duplicate(holder: Holder): String = holder.text.clone()"#,
        r#"package q
import p.Holder
        fun entry(): Unit { val holder = Holder("field" + "text"); println(p.duplicate(holder)); println(holder.text.clone()) }
    "#,
    );
    let inputs = analysis.inputs();
    let (program, entry) = lower_scalar_unit_with_entry(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
    )
    .expect("String fields are shared borrowed places");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let directory = TestDirectory::create();
    let ir = directory.join("field-clone.ll");
    let executable = directory.join("field-clone");
    fs::write(&ir, llvm).unwrap();
    let linked = Command::new(crate::test_support::ir_clang())
        .arg(&ir)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(run.stdout, b"fieldtext\nfieldtext\n");
}

#[test]
fn string_clone_unit_borrowed_rc_payload_is_rejected_before_native_emission() {
    let analysis = analyze_sources(
        "package p\nfun duplicate(shared: Rc<String>): String = shared.value.clone()",
        r#"package q
        fun entry(): Unit { val shared = Rc("text"); println(p.duplicate(shared)) }
    "#,
    );
    let inputs = analysis.inputs();
    let error = lower_scalar_unit_with_entry(
        &analysis.sources,
        &inputs,
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
    )
    .err()
    .expect("Borrow Rc payload must not reinterpret a handle-slot pointer as a control block");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
}

#[test]
fn integrated_numeric_clone_and_boxed_enum_cross_file_facts_reach_native() {
    let analysis = analyze_sources(
        r#"package p
        enum class Text { Value(text: String), Empty }
        fun boxed(text: String): Box<Text> {
            val value = Text.Value(text.clone())
            return Box(value)
        }
        fun consume(own value: Box<Text>): Unit { println("boxed") }
        "#,
        r#"package q
        fun entry(): Unit {
            val texts = listOf("unused", "界" + "é")
            p.consume(p.boxed(texts[0b0_1]))
            println(texts[0x0_1].clone())
            println(texts[1])
        }
        "#,
    );
    let inputs = analysis.inputs();
    let lower = |inputs: &[_]| {
        lower_scalar_unit_with_entry(
            &analysis.sources,
            inputs,
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .expect("cross-file numeric clone and boxed enum facts compose")
    };
    let (program, entry) = lower(&inputs);
    let (reversed, reversed_entry) = lower(&[inputs[1], inputs[0]]);
    assert_eq!(
        crate::ssa::render_program(&program),
        crate::ssa::render_program(&reversed)
    );
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert_eq!(
        llvm,
        crate::llvm::render_verified_program_with_entry(&reversed, reversed_entry).unwrap()
    );
    let output = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 5);
    crate::native_tests::boxed_enum_tests::assert_success(&output, "boxed\n界é\n界é\n".as_bytes());
}
