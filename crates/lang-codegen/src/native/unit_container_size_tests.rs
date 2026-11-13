//! SPEC-0274: native source length and temporary resource cleanup.

use crate::test_support::diagnostics::assert_diagnostic;
mod runtime_constructor_tests {
    //! Observable ordinary source behavior through the compilation-unit object facade.
    use super::*;
    use crate::native_tests::runtime_constructor_tests::{assert_output, cases, evaluation};

    /// Keep object publication, native linker and process execution identical for the intent cases.
    fn execute(analysis: &UnitAnalysis, entry: &str, label: &str) -> std::process::Output {
        let directory = TestDirectory::create();
        let object = directory.join("runtime.o");
        let executable = directory.join("runtime");
        emit_native_unit_object(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", entry),
            &object,
        )
        .unwrap_or_else(|error| panic!("{label}: {error:?}"));
        assert_no_sibling_temporary(&directory.0);
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("native link runs");
        assert!(linked.status.success(), "{label}: {linked:?}");
        Command::new(&executable)
            .output()
            .expect("native program runs")
    }

    #[test]
    fn runtime_constructor_native_unit_matrix_executes_logical_callbacks() {
        let cases = cases();
        assert_eq!(cases.len(), 144);
        for case in &cases {
            let analysis = analyze_sources(
                "package p\nfun unused(): Unit {}",
                &format!("package q\n{}", case.text),
            );
            assert_output(case, &execute(&analysis, "nativeEntry", &case.label));
        }
    }

    #[test]
    fn runtime_constructor_native_unit_orders_callbacks_elements_and_environment_cleanup() {
        for (text, expected) in
            crate::native_tests::runtime_constructor_tests::ordered_resource_cases()
        {
            let analysis = analyze_sources(
                "package p\nfun unused(): Unit {}",
                &format!("package q\n{text}"),
            );
            run(&analysis, expected.as_bytes());
        }
    }

    #[test]
    fn runtime_constructor_native_unit_cross_file_helper_preserves_element_and_environment() {
        let cases = crate::native_tests::runtime_constructor_tests::helper_cases();
        assert_eq!(cases.len(), 48);
        for (case, api) in cases {
            let analysis = analyze_sources(
                &format!("package p\n{api}"),
                &format!("package q\nimport p.generate\n{}", case.text),
            );
            assert_output(&case, &execute(&analysis, "nativeEntry", &case.label));
        }
    }

    #[test]
    fn runtime_constructor_native_unit_storable_helper_elements() {
        let cases = crate::native_tests::runtime_constructor_tests::storable_helpers::cases();
        assert_eq!(cases.len(), 48);
        for (case, api) in cases {
            let analysis = analyze_sources(
                &format!("package p\n{api}"),
                &format!("package q\nimport p.generate\n{}", case.text),
            );
            assert_output(&case, &execute(&analysis, "nativeEntry", &case.label));
        }
    }

    #[test]
    fn runtime_constructor_native_unit_evaluates_operands_once_in_order() {
        let cases = evaluation::evaluation_cases();
        assert_eq!(cases.len(), 6);
        for case in cases {
            let analysis = analyze_sources(
                &format!("package p\n{}", case.api),
                &format!("package q\nimport p.size\nimport p.factory\n{}", case.entry),
            );
            evaluation::assert_evaluation_output(&case, &execute(&analysis, "entry", &case.label));
        }
    }

    #[test]
    fn runtime_constructor_native_unit_nothing_operands_do_not_generate() {
        let cases = evaluation::nothing_operand_cases();
        assert_eq!(cases.len(), 4);
        for case in cases {
            let analysis = analyze_sources(
                &format!("package p\n{}", case.api),
                &format!("package q\n{}", case.entry),
            );
            evaluation::assert_evaluation_output(&case, &execute(&analysis, "entry", &case.label));
        }
    }
}
use super::*;

#[path = "unit_generic_container_tests.rs"]
mod generic_containers;

#[test]
fn unit_container_size_native_covers_empty_nonempty_and_reuses_owners() {
    let analysis = analyze_sources(
        r#"package p
        fun a(items: Array<Int>): Int = ((items)).size
        fun b(items: List<Int>): Int = ((items)).size
        fun c(items: MutableList<Int>): Int = ((items)).size
        fun take(own items: Array<Int>): Int = ((items)).size
        fun takeList(own items: List<Int>): Int = ((items)).size
        fun takeMutable(own items: MutableList<Int>): Int = ((items)).size
        fun emptyArray(): Array<Int> = arrayOf()
        fun emptyList(): List<Int> = listOf()
        fun emptyMutable(): MutableList<Int> = mutableListOf()"#,
        r#"package q
        fun entry(): Unit {
            val a0: Array<Int> = arrayOf()
            val a2 = arrayOf(1, 2)
            val b0: List<Int> = listOf()
            val b2 = listOf(1, 2)
            val c0: MutableList<Int> = mutableListOf()
            val c2 = mutableListOf(1, 2)
            if (((a0)).size != 0) { error("array empty") }
            if (((a2)).size != 2) { error("array nonempty") }
            if (((b0)).size != 0) { error("list empty") }
            if (((b2)).size != 2) { error("list nonempty") }
            if (((c0)).size != 0) { error("mutable empty") }
            if (((c2)).size != 2) { error("mutable nonempty") }
            if (p.a(a0) != 0) { error("borrow array empty") }
            if (p.a(a2) != 2) { error("borrow array nonempty") }
            if (p.b(b0) != 0) { error("borrow list empty") }
            if (p.b(b2) != 2) { error("borrow list nonempty") }
            if (p.c(c0) != 0) { error("borrow mutable empty") }
            if (p.c(c2) != 2) { error("borrow mutable nonempty") }
            if (p.take(a0) != 0) { error("empty array Value") }
            if (p.takeList(b0) != 0) { error("empty list Value") }
            if (p.takeMutable(c0) != 0) { error("empty mutable Value") }
            if (p.a(a2) != 2) { error("repeat borrow") }
            if (a2[0] != 1) { error("source reuse") }
            if (p.take(a2) != 2) { error("owner move after reads") }
            if (p.b(b2) != 2) { error("repeat list borrow") }
            if (b2[0] != 1) { error("list source reuse") }
            if (p.takeList(b2) != 2) { error("list owner move after reads") }
            if (p.c(c2) != 2) { error("repeat mutable borrow") }
            if (c2[0] != 1) { error("mutable source reuse") }
            if (p.takeMutable(c2) != 2) { error("mutable owner move after reads") }
            if (((p.emptyArray())).size != 0) { error("empty array temporary") }
            if (((p.emptyList())).size != 0) { error("empty list temporary") }
            if (((p.emptyMutable())).size != 0) { error("empty mutable temporary") }
            println("lengths-ok")
        }"#,
    );
    run(&analysis, b"lengths-ok\n");
}

#[test]
fn unit_container_size_native_temporary_resources_drop_once_after_header_read() {
    let analysis = analyze_sources(
        r#"package p
        class Resource(val name: String) { deinit() { println(this.name) } }
        fun sourceArray(): Array<Resource> {
            println("make-array")
            return arrayOf(Resource("a-one"), Resource("a-two"))
        }
        fun sourceList(): List<Resource> {
            println("make-list")
            return listOf(Resource("l-one"), Resource("l-two"))
        }
        fun sourceMutable(): MutableList<Resource> {
            println("make-mutable")
            return mutableListOf(Resource("m-one"), Resource("m-two"))
        }"#,
        r#"package q
        fun entry(): Unit {
            if (((p.sourceArray())).size != 2) { error("array temporary length") }
            println("after-array")
            if (((p.sourceList())).size != 2) { error("list temporary length") }
            println("after-list")
            if (((p.sourceMutable())).size != 2) { error("mutable temporary length") }
            println("after-mutable")
        }"#,
    );
    let expected = b"make-array\na-two\na-one\nafter-array\nmake-list\nl-two\nl-one\nafter-list\nmake-mutable\nm-two\nm-one\nafter-mutable\n";
    run(&analysis, expected);
    let (program, entry) = lower_scalar_unit_with_entry(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
    )
    .expect("temporary resource containers lower with exact cleanup");
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    // Each temporary owns two Resource allocations and one container buffer.
    let counted = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 9);
    crate::native_tests::boxed_enum_tests::assert_success(&counted, expected);
}

#[test]
fn unit_container_size_native_borrow_survives_cfg_and_reuse() {
    let analysis = analyze_sources(
        r#"package p
        fun arraySize(items: Array<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }
        fun listSize(items: List<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }
        fun mutableSize(items: MutableList<Int>, own flag: Boolean): Int {
            if (flag) { println("true") } else { println("false") }
            return ((items)).size
        }"#,
        r#"package q
        fun entry(): Unit {
            val a = arrayOf(1, 2)
            val b = listOf(1, 2)
            val c = mutableListOf(1, 2)
            if (p.arraySize(a, true) != 2) { error("CFG array true") }
            if (p.arraySize(a, false) != 2) { error("CFG array false") }
            if (p.listSize(b, true) != 2) { error("CFG list true") }
            if (p.listSize(b, false) != 2) { error("CFG list false") }
            if (p.mutableSize(c, true) != 2) { error("CFG mutable true") }
            if (p.mutableSize(c, false) != 2) { error("CFG mutable false") }
            if (a[0] != 1) { error("array after CFG") }
            if (b[0] != 1) { error("list after CFG") }
            if (c[0] != 1) { error("mutable after CFG") }
            println("cfg-lengths-ok")
        }"#,
    );
    run(
        &analysis,
        b"true\nfalse\ntrue\nfalse\ntrue\nfalse\ncfg-lengths-ok\n",
    );
}

#[test]
fn unit_container_size_native_assignment_rhs_keeps_old_owner_until_replacement() {
    let analysis = analyze_sources(
        r#"package p
        class Resource(val name: String) { deinit() { println(this.name) } }
        fun make(own length: Int): Resource {
            if (length != 1) { error("RHS reads the old container") }
            println("rhs")
            return Resource("new")
        }
        fun finish(items: List<Resource>): Unit { println("finish") }"#,
        r#"package q
        import p.Resource
        fun entry(): Unit {
            var values = listOf(Resource("old"))
            values = listOf(p.make(((values)).size))
            println("after")
            p.finish(values)
        }"#,
    );
    run(&analysis, b"rhs\nold\nafter\nfinish\nnew\n");
}

fn run(analysis: &UnitAnalysis, expected: &[u8]) {
    let directory = TestDirectory::create();
    let object = directory.join("length.o");
    let executable = directory.join("length");
    emit_native_unit_object(
        &analysis.sources,
        &analysis.inputs(),
        &analysis.names,
        &analysis.environment,
        &analysis.typed,
        &analysis.owned,
        analysis.declaration("q", "entry"),
        &object,
    )
    .expect("source size emits a verified native object");
    crate::test_support::assert_native_object(&fs::read(&object).unwrap());
    let linked = Command::new(crate::test_support::clang())
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("native link runs");
    assert_diagnostic!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let output = Command::new(&executable)
        .output()
        .expect("native program runs");
    assert_diagnostic!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected);
    assert!(output.stderr.is_empty());
}

pub(crate) mod companion_boundary {
    //! Ordinary source controls for the explicitly unsupported companion callable backend.

    use lang_frontend::type_checking::{UnitCallTarget, complete_compilation_unit_types};

    use super::super::*;

    struct Fixture {
        sources: SourceMap,
        source: SourceId,
        parsed: ParsedFile,
        names: ValidatedCompilationUnitNames,
        environment: TypeEnvironment,
        typed: ValidatedCompilationUnitTypes,
        owned: ValidatedCompilationUnitOwnership,
    }

    impl Fixture {
        fn new(text: &str) -> Self {
            let mut sources = SourceMap::new();
            let (source, parsed) = parsed(&mut sources, "main.ko", text);
            let inputs = [SourceUnitInput::new("root", "main.ko", source, &parsed)];
            let (name_environment, environment) = standard_environments();
            let index = index_compilation_unit(&sources, &inputs).expect("real compilation index");
            let names =
                resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
                    .expect("ordinary names")
                    .validate()
                    .expect("valid names");
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
                .expect("ordinary types")
                .validate()
                .expect("valid types");
            assert!(typed.types().diagnostics().is_empty());
            complete_compilation_unit_types(&sources, &inputs, &names, &environment, typed.types())
                .expect("companion source remains fully admitted by the Unit frontend");
            let owned =
                check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
                    .expect("ordinary ownership")
                    .validate()
                    .expect("valid ownership");
            Self {
                sources,
                source,
                parsed,
                names,
                environment,
                typed,
                owned,
            }
        }

        fn inputs(&self) -> [SourceUnitInput<'_>; 1] {
            [SourceUnitInput::new(
                "root",
                "main.ko",
                self.source,
                &self.parsed,
            )]
        }

        fn entry(&self) -> DeclarationId {
            self.names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|declaration| declaration.name() == "main")
                .expect("real entry declaration")
                .id()
        }
    }

    #[test]
    fn selected_companion_calls_report_precise_native_capability() {
        let mut observations = Vec::new();
        const INDEPENDENT: &str = "class Factory { companion { fun identity(value: Int): Int = value } }\nfun main(): Unit { val result = Factory.identity(7); if (result == 7) { println(\"factory\") } }";
        for (text, expected_start, expected_end, expected_call) in [
            (
                include_str!(
                    "../../../lang-frontend/tests/fixtures/final-admission-static-recovery-0323/p28_companion.ko"
                ),
                88,
                98,
                "P.origin()",
            ),
            (
                include_str!(
                    "../../../lang-frontend/tests/fixtures/final-admission-static-recovery-0323/companion_minimal.ko"
                ),
                88,
                98,
                "P.origin()",
            ),
            (INDEPENDENT, 102, 121, "Factory.identity(7)"),
        ] {
            let fixture = Fixture::new(text);
            let inputs = fixture.inputs();
            let call = fixture
                .typed
                .types()
                .calls()
                .iter()
                .find(|call| {
                    fixture
                        .parsed
                        .ast()
                        .expressions()
                        .get(call.expression().expression())
                        .is_ok_and(|node| fixture.sources.slice(node.span()) == Ok(expected_call))
                })
                .expect("real selected companion call");
            let UnitCallTarget::Symbol(symbol) = call.target() else {
                panic!("companion call has an actual selected Symbol");
            };
            assert!(call.receiver().is_none());
            let callable = fixture
                .typed
                .types()
                .signatures()
                .declarations()
                .iter()
                .filter_map(|signature| signature.nominal())
                .flat_map(|nominal| nominal.companion_members())
                .find(|callable| {
                    callable.target()
                        == lang_frontend::type_checking::UnitCallableTarget::Symbol(symbol)
                })
                .expect("selected identity belongs to an actual companion signature");
            assert!(callable.has_body());
            assert!(callable.receiver().is_none());
            assert_eq!(
                callable.type_parameters().len(),
                call.instance().type_arguments().len()
            );
            for previous in [None, Some(b"previous object".as_slice())] {
                let directory = TestDirectory::create();
                let object = directory.join("program.o");
                if let Some(bytes) = previous {
                    fs::write(&object, bytes).expect("ordinary existing output");
                }
                fs::write(directory.join("neighbor.txt"), b"ordinary neighbor").unwrap();
                let before = directory_bytes(&directory.0);
                let error = emit_native_unit_object(
                    &fixture.sources,
                    &inputs,
                    &fixture.names,
                    &fixture.environment,
                    &fixture.typed,
                    &fixture.owned,
                    fixture.entry(),
                    &object,
                )
                .expect_err("selected companion callable remains an explicit capability boundary");
                eprintln!("companion native boundary: {error:?}");
                let slice = error
                    .span()
                    .and_then(|span| fixture.sources.slice(span).ok())
                    .map(str::to_owned);
                observations.push((
                    error,
                    fixture.source,
                    slice,
                    expected_start,
                    expected_end,
                    expected_call,
                ));
                assert_eq!(
                    directory_bytes(&directory.0),
                    before,
                    "complete output directory bytes"
                );
                match previous {
                    Some(bytes) => assert_eq!(fs::read(&object).unwrap(), bytes),
                    None => assert!(!object.exists()),
                }
                assert_no_sibling_temporary(&directory.0);
            }
        }
        assert_eq!(
            observations.len(),
            6,
            "two unchanged sources plus an independent source, each with two output states"
        );
        for (error, source, slice, expected_start, expected_end, expected_call) in observations {
            assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
            assert_eq!(
                error.to_string(),
                "native object UnsupportedSource: companion function calls are not supported by native code generation"
            );
            assert!(
                error.diagnostic().is_none(),
                "not a fabricated language diagnostic"
            );
            let span = error.span().expect("original call source span");
            assert_eq!(span.source_id(), source);
            assert_eq!((span.start(), span.end()), (expected_start, expected_end));
            assert_eq!(slice.as_deref(), Some(expected_call));
        }
    }

    #[test]
    fn unused_companion_preserves_top_level_and_instance_native_calls() {
        let fixture = Fixture::new(
            "class Holder(val item: Int) {\n\
             companion { fun create(): Holder = Holder(1) }\n\
             fun value(): Int = item\n\
         }\n\
         fun top(): Int = 7\n\
         fun main(): Unit { val holder = Holder(1); if (holder.value() + top() == 8) { println(\"neighbors\") } }",
        );
        let inputs = fixture.inputs();
        let directory = TestDirectory::create();
        let object = directory.join("program.o");
        emit_native_unit_object(
            &fixture.sources,
            &inputs,
            &fixture.names,
            &fixture.environment,
            &fixture.typed,
            &fixture.owned,
            fixture.entry(),
            &object,
        )
        .expect("unused companion must not reject reachable ordinary functions");
        crate::test_support::assert_native_object(&fs::read(&object).expect("real native object"));
        assert_no_sibling_temporary(&directory.0);
        // This control emits an object only; it never links or launches a program.
    }

    fn directory_bytes(directory: &Path) -> Vec<(std::ffi::OsString, Vec<u8>)> {
        let mut entries = fs::read_dir(directory)
            .expect("owned directory")
            .map(|entry| {
                let entry = entry.expect("owned directory entry");
                assert!(
                    entry.file_type().unwrap().is_file(),
                    "only ordinary fixture files"
                );
                (
                    entry.file_name(),
                    fs::read(entry.path()).expect("whole file bytes"),
                )
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
    }
}
