//! Normal source only: original object/link/run; no IR editing or allocation instrumentation.
use super::resource_exchange_tests::run_unit;
use crate::native_tests::{boxed_enum_tests::assert_success, emit_link_and_run};

const SOURCE: &str = include_str!("../../tests/fixtures/map-return-last-use-0318/cardinality.ko");
const EXPECTED: &[u8] =
    include_bytes!("../../tests/fixtures/map-return-last-use-0318/cardinality.stdout");

#[test]
fn single_map_operand_last_use_native_cardinality() {
    let run = emit_link_and_run("map-return-last-use.ko", SOURCE, "main");
    assert_success(&run, EXPECTED);
}

fn unit(constants: bool) {
    let provider = format!("package p\n{SOURCE}");
    let (run, _) = run_unit(
        &provider,
        "package q\nfun entry(): Unit { p.main() }",
        constants,
    );
    assert_success(&run, EXPECTED);
}

#[test]
fn unit_map_operand_last_use_native_basic_cardinality() {
    unit(false);
}

#[test]
fn unit_map_operand_last_use_native_constant_cardinality() {
    unit(true);
}

pub(crate) mod analysis_support {
    //! Same analyzed fixture owner shared by public unit native contracts.
    use super::super::*;
    pub(crate) struct UnitAnalysis {
        pub(crate) sources: SourceMap,
        pub(crate) provider_source: SourceId,
        pub(crate) provider: ParsedFile,
        pub(crate) consumer_source: SourceId,
        pub(crate) consumer: ParsedFile,
        pub(crate) names: ValidatedCompilationUnitNames,
        pub(crate) environment: TypeEnvironment,
        pub(crate) typed: ValidatedCompilationUnitTypes,
        pub(crate) owned: ValidatedCompilationUnitOwnership,
    }

    impl UnitAnalysis {
        pub(crate) fn inputs(&self) -> [SourceUnitInput<'_>; 2] {
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

        pub(crate) fn declaration(&self, package: &str, name: &str) -> DeclarationId {
            self.names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|declaration| {
                    declaration.name() == name
                        && self.names.names().index().packages()[declaration.package().index()]
                            .name()
                            .expect("named package")
                            .segments()
                            .iter()
                            .map(String::as_str)
                            .eq(package.split('.'))
                })
                .expect("fixture declaration exists")
                .id()
        }
    }

    pub(crate) fn analyze_unit() -> UnitAnalysis {
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
         fun supportedBorrow(): Unit {\n\
             val action: move (borrow p.Bundle) -> Unit = move { message -> }\n\
             val invoked = action(p.makeBundle())\n\
         }\n\
         fun unsupportedBorrow(): Unit {\n\
             val action: move (borrow Unit) -> Unit = move { message -> }\n\
             val invoked = action(inspect(\"unsupported\"))\n\
         }",
        )
    }

    pub(crate) fn analyze_sources(provider_text: &str, consumer_text: &str) -> UnitAnalysis {
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
        let owned =
            check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
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

    pub(crate) fn parsed(
        sources: &mut SourceMap,
        name: &str,
        text: &str,
    ) -> (SourceId, ParsedFile) {
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
}

pub(crate) mod borrow_string_tests {
    //! SPEC-0274: cross-file Borrow String reads and independent concat ownership.

    use super::super::*;
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};

    #[test]
    fn unit_borrow_string_binary_native_preserves_complete_bytes_and_source_reuse() {
        let analysis = analyze_sources(
            r#"package p
        fun same(a: String, b: String): Boolean = ((a)) == (b)
        fun different(a: String, b: String): Boolean = (a) != ((b))
        fun decorated(text: String): String = (text) + "!"
        fun prefix(text: String): String = ">" + (text)
        fun mixed(text: String, own suffix: String): String = text + suffix
        fun choose(text: String, own flag: Boolean): String {
            val joined = (text) + (if (flag) "右\0" else "")
            println(text)
            return joined
        }"#,
            r#"package q
        fun entry(): Unit {
            val source = "界\0" + "é"
            val empty = "" + ""
            if (!p.same(source, "界\0é")) { error("dynamic/static equality") }
            if (!p.same(empty, "")) { error("empty equality") }
            if (!p.different(source, "界\0ê")) { error("bytes after NUL matter") }
            if (p.different(source, source)) { error("repeated loan must preserve source") }
            println(p.decorated(source))
            println(p.prefix(source))
            println(p.mixed(source, "?" + "尾"))
            println(p.decorated(empty))
            println(p.choose(source, true))
            println(p.choose(source, false))
            println(source)
            println(empty)
        }"#,
        );
        let directory = TestDirectory::create();
        let object = directory.join("borrow-string.o");
        let executable = directory.join("borrow-string");
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
        .expect("Borrow String operations emit a native object");
        crate::test_support::assert_native_object(&fs::read(&object).unwrap());
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).output().unwrap();
        assert_success(
            &run,
            "界\0é!\n>界\0é\n界\0é?尾\n!\n界\0é\n界\0é右\0\n界\0é\n界\0é\n界\0é\n\n".as_bytes(),
        );
    }

    #[test]
    fn unit_borrow_string_comparisons_native_preserve_loans_after_both_rhs_branches() {
        let analysis = analyze_sources(
            r#"package p
        fun sameAfter(text: String, own flag: Boolean): Boolean {
            val result = (text) == (if (flag) "界\0é" else "界\0ê")
            println(text)
            return result
        }
        fun differentAfter(text: String, own flag: Boolean): Boolean {
            val result = (text) != (if (flag) "界\0é" else "界\0ê")
            println(text)
            return result
        }"#,
            r#"package q
        fun entry(): Unit {
            val source = "界\0" + "é"
            if (!p.sameAfter(source, true)) { error("equal true branch") }
            if (p.sameAfter(source, false)) { error("equal false branch") }
            if (p.differentAfter(source, true)) { error("not-equal true branch") }
            if (!p.differentAfter(source, false)) { error("not-equal false branch") }
            println(source)
        }"#,
        );
        let directory = TestDirectory::create();
        let object = directory.join("borrow-string-comparisons.o");
        let executable = directory.join("borrow-string-comparisons");
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
        .expect("RHS CFG String comparisons emit a native object");
        crate::test_support::assert_native_object(&fs::read(&object).unwrap());
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).output().unwrap();
        assert_success(&run, "界\0é\n界\0é\n界\0é\n界\0é\n界\0é\n".as_bytes());
    }

    #[test]
    fn unit_borrow_string_concat_result_and_source_drop_independently() {
        for source_first in [false, true] {
            let tail = if source_first {
                "p.consume(source)\nprintln(joined)\np.consume(joined)"
            } else {
                "p.consume(joined)\nprintln(source)\np.consume(source)"
            };
            let analysis = analyze_sources(
                r#"package p
            fun same(a: String, b: String): Boolean = a == b
            fun decorated(text: String): String = text + "!"
            fun consume(own text: String): Unit { println(text) }"#,
                &format!(
                    r#"package q
            fun entry(): Unit {{
                val source = "界\0" + "é"
                if (!p.same(source, "界\0é")) {{ error("first read") }}
                val joined = p.decorated(source)
                if (!p.same(source, "界\0é")) {{ error("source after concat") }}
                {tail}
            }}"#
                ),
            );
            let (program, entry) = lower_scalar_unit_with_entry(
                &analysis.sources,
                &analysis.inputs(),
                &analysis.names,
                &analysis.environment,
                &analysis.typed,
                &analysis.owned,
                analysis.declaration("q", "entry"),
            )
            .expect("Borrow String concat uses source without transferring its owner");
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            // Allocation 0 is the dynamic source; allocation 1 is the concat result.
            let order = if source_first { [0, 1] } else { [1, 0] };
            let run = run_counted_allocations_in_order(&llvm, &order);
            let stdout = if source_first {
                "界\0é\n界\0é!\n界\0é!\n"
            } else {
                "界\0é!\n界\0é\n界\0é\n"
            };
            assert_success(&run, stdout.as_bytes());
        }
    }
}

pub(crate) mod bitwise_tests {
    use super::super::{Command, TestDirectory, analyze_sources, emit_native_unit_object};

    #[test]
    fn bitwise_unit_native_width_boundaries_masked_counts_and_eager_operands() {
        let (functions, checks, expected) = crate::bitwise_test_support::binary_fixture("p.");
        run_fixture(functions, checks, expected);
    }

    #[test]
    fn bitwise_inv_unit_native_boundaries_and_single_evaluation() {
        let (functions, checks, expected) = crate::bitwise_test_support::inv_fixture("p.");
        run_fixture(functions, checks, expected);
    }

    fn run_fixture(functions: String, checks: String, expected: String) {
        let analysis = analyze_sources(
            &format!("package p\n{functions}"),
            &format!("package q\nfun entry(): Unit {{\n{checks}\n}}\n"),
        );
        let directory = TestDirectory::create();
        let object = directory.join("bitwise.o");
        let executable = directory.join("bitwise");
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
        .expect("bitwise unit native object");
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("linker");
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).output().expect("executable");
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes());
        assert!(run.stderr.is_empty(), "{run:?}");
    }

    #[test]
    fn bitwise_inv_unit_native_literals_fields_elements_and_source_member_identity() {
        run_fixture(
            format!(
                "{}\nfun element(values: List<Int>): Int = values[0].inv()\nclass Sample {{ fun inv(): Int = 7 }}\nfun exerciseElements(): Unit {{ if (Sample().inv() == 7) {{ println(\"source-member\") }}; val values = listOf(3); if (element(values) == -4) {{ println(\"borrow-element\") }}; if (values[0].inv() == -4) {{ println(\"named-element\") }} }}",
                crate::bitwise_test_support::inv_receiver_fixture()
            ),
            "p.exercise()\np.exerciseElements()".to_owned(),
            format!(
                "{}source-member\nborrow-element\nnamed-element\n",
                crate::bitwise_test_support::INV_RECEIVER_STDOUT
            ),
        );
    }

    #[test]
    fn bitwise_unit_native_guide_litmus_12_runs_the_normative_source() {
        let run = run_constant_fixture(
            "demo/interop/litmus.ko",
            crate::bitwise_test_support::guide_litmus_12(),
            "package q\nfun entry(): Unit { demo.interop.main() }",
        );
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, b"Mask initialized\n");
        assert!(run.stderr.is_empty(), "{run:?}");
    }

    #[test]
    fn bitwise_unit_native_operands_preserve_control_exit_and_order() {
        let (functions, checks, expected) = crate::bitwise_test_support::control_fixture();
        run_fixture(
            functions.to_owned(),
            format!("p.{checks}"),
            expected.to_owned(),
        );
    }

    fn run_constant_fixture(
        provider_path: &str,
        provider_text: &str,
        consumer_text: &str,
    ) -> std::process::Output {
        use lang_frontend::{
            name_resolution::{
                SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
            },
            ownership_checking::check_compilation_unit_constant_ownership,
            source::SourceMap,
            type_checking::{check_compilation_unit_types, standard_environments},
        };
        let mut sources = SourceMap::new();
        let (provider_source, provider) =
            super::super::parsed(&mut sources, provider_path, provider_text);
        let (consumer_source, consumer) =
            super::super::parsed(&mut sources, "q/consumer.ko", consumer_text);
        let inputs = [
            SourceUnitInput::new("root", provider_path, provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
            .unwrap()
            .validate_constants()
            .unwrap();
        let owned = check_compilation_unit_constant_ownership(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
        )
        .unwrap()
        .validate()
        .unwrap();
        let entry = names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == "entry")
            .unwrap()
            .id();
        let directory = TestDirectory::create();
        let object = directory.join("constant-bitwise.o");
        crate::emit_native_constant_unit_object(
            &sources,
            &inputs,
            &names,
            &environment,
            &typed,
            &owned,
            entry,
            &object,
        )
        .expect("constant bitwise object");
        let executable = directory.join("constant-bitwise");
        let linked = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("linker");
        assert!(linked.status.success(), "{linked:?}");
        Command::new(&executable).output().expect("executable")
    }

    #[test]
    fn bitwise_unit_native_const_and_runtime_agree_with_independent_oracles() {
        let (functions, checks, expected) =
            crate::bitwise_test_support::constant_runtime_fixture("p.");
        let run = run_constant_fixture(
            "p/provider.ko",
            &format!("package p\n{functions}"),
            &format!("package q\nfun entry(): Unit {{\n{checks}\n}}"),
        );
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes());
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

pub(crate) mod runtime_global_tests {
    //! Guide v0.46 (SPEC-0343): top-level runtime values are rejected in Phase 2 with L0167,
    //! so they never reach ownership or native code. Declarative roots keep compiling, and the
    //! backend `UnsupportedSource` runtime-global gate stays only as a defensive assertion.

    use super::super::parsed;
    use crate::{emit_native_constant_unit_object, native_tests::TestDirectory};
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, ValidatedCompilationUnitNames, index_compilation_unit,
            resolve_compilation_unit_names,
        },
        ownership_checking::check_compilation_unit_constant_ownership,
        source::SourceMap,
        type_checking::{
            CompilationUnitTypes, TypeEnvironment, check_compilation_unit_types,
            standard_environments,
        },
    };
    use std::fs;

    fn raw_frontend(
        sources: &SourceMap,
        inputs: &[SourceUnitInput<'_>],
    ) -> (
        ValidatedCompilationUnitNames,
        TypeEnvironment,
        CompilationUnitTypes,
    ) {
        let (name_environment, environment) = standard_environments();
        let index = index_compilation_unit(sources, inputs).expect("real source index");
        let names = resolve_compilation_unit_names(sources, inputs, &index, &name_environment)
            .expect("real source names")
            .validate()
            .expect("valid names");
        let typed = check_compilation_unit_types(sources, inputs, &names, &environment)
            .expect("real source types");
        (names, environment, typed)
    }

    /// Every runtime top-level value reports exactly one L0167 on its keyword, independent of
    /// input order, and the unit cannot qualify for basic or constant native lowering.
    fn reject_runtime_globals(provider_text: &str, expected: &[(&str, usize, &str)]) {
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", provider_text);
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            "package q\nval consumerState = 3\nfun entry(): Unit {}",
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        let mut expected = expected
            .iter()
            .map(|&(file, start, keyword)| (file.to_owned(), start, start + keyword.len()))
            .chain([("q/consumer.ko".to_owned(), 10, 13)])
            .collect::<Vec<_>>();
        expected.sort();
        for ordered in [inputs, [inputs[1], inputs[0]]] {
            let (_, _, typed) = raw_frontend(&sources, &ordered);
            let mut actual = typed
                .diagnostics()
                .iter()
                .map(|diagnostic| {
                    assert_eq!(diagnostic.code().to_string(), "L0167");
                    let span = diagnostic.primary_span();
                    let keyword = sources.slice(span).unwrap();
                    assert!(keyword == "val" || keyword == "var", "{keyword}");
                    (
                        sources.source_name(span.source_id()).unwrap().to_owned(),
                        span.start(),
                        span.end(),
                    )
                })
                .collect::<Vec<_>>();
            actual.sort();
            assert_eq!(actual, expected);
            assert!(typed.clone().validate().is_err());
            assert!(typed.validate_constants().is_err());
        }
    }

    #[test]
    fn runtime_global_roots_reject_in_canonical_source_order() {
        reject_runtime_globals(
            "package p\nval first = 1\nvar later = 2",
            &[("p/provider.ko", 10, "val"), ("p/provider.ko", 24, "var")],
        );
        reject_runtime_globals(
            "package p\nvar first = 1\nval later = 2",
            &[("p/provider.ko", 10, "var"), ("p/provider.ko", 24, "val")],
        );
    }

    #[test]
    fn modified_global_initializer_calls_remain_unsupported() {
        reject_runtime_globals(
            "package p\nprivate val state = initialize()\nfun initialize(): Int { println(\"initializer\"); return 7 }",
            &[("p/provider.ko", 18, "val")],
        );
    }

    #[test]
    fn constant_capability_rejects_runtime_globals_and_preserves_declarations() {
        const DECLARATIONS: &str = "package p\nconst val VALUE = 7\nobject Config { const val EXTRA = 1 }\nclass Holder(val item: Int)\ninterface Marker\nenum class Choice { One }";
        let mut sources = SourceMap::new();
        let (provider_source, provider) = parsed(&mut sources, "p/provider.ko", DECLARATIONS);
        let (consumer_source, consumer) = parsed(
            &mut sources,
            "q/consumer.ko",
            "package q\nfun entry(): Unit { val number = p.VALUE + p.Config.EXTRA }",
        );
        let inputs = [
            SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
            SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
        ];
        for ordered in [inputs, [inputs[1], inputs[0]]] {
            let (names, environment, typed) = raw_frontend(&sources, &ordered);
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let typed = typed
                .validate_constants()
                .expect("actual constant capability");
            let owned = check_compilation_unit_constant_ownership(
                &sources,
                &ordered,
                &names,
                &environment,
                &typed,
            )
            .expect("real constant ownership")
            .validate()
            .expect("valid constant ownership");
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|declaration| declaration.name() == "entry")
                .expect("real entry")
                .id();
            let directory = TestDirectory::create();
            let object = directory.join("program.o");
            emit_native_constant_unit_object(
                &sources,
                &ordered,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
                &object,
            )
            .expect("constant and declarative roots remain supported");
            crate::test_support::assert_native_object(&fs::read(&object).unwrap());
            assert_eq!(
                directory.snapshot().len(),
                1,
                "no sibling temporary remains"
            );
        }

        let runtime_global = format!(
            "{DECLARATIONS}\nprivate var state = initialize()\nfun initialize(): Int {{ println(\"initializer\"); return VALUE }}"
        );
        let start = runtime_global.find("var state").unwrap();
        reject_runtime_globals(&runtime_global, &[("p/provider.ko", start, "var")]);
    }
}

pub(crate) mod non_null_assertion_tests {
    use super::super::{Command, TestDirectory, analyze_sources, emit_native_unit_object};
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn unit_non_null_assertion_cross_file_results_and_shadowed_error_run_natively() {
        for (inner, constructor, payload) in [
            ("Node", "Node(37)", "node.item"),
            ("Box<Token>", "Box(Token(37))", ""),
            ("Rc<Int>", "Rc(37)", "node.value"),
        ] {
            let observe = if payload.is_empty() {
                // Box payload projection is deferred; its Value parameter verifies owner delivery.
                "println(\"ok\")".to_owned()
            } else {
                format!("if ({payload} == 37) {{ println(\"ok\") }} else {{ println(\"bad\") }}")
            };
            let provider = format!(
                r#"package p
class Node(val item: Int)
value class Token(val item: Int)
fun make(present: Boolean): {inner}? {{ val printed = println("make")
if (present) {{ val result: {inner}? = {constructor}
return result }}
val absent: {inner}? = null
return absent }}
fun observe(own node: {inner}): Unit {{ {observe} }}
"#
            );
            for present in [true, false] {
                let consumer = format!(
                    r#"package q
fun error(message: String): Int {{ val printed = println("shadow")
return 7 }}
fun entry(): Unit {{ val explicit = error("ordinary")
val checked = if (explicit == 7) {{ p.observe(p.make({present})!!) }} else {{ println("wrong target") }} }}
"#
                );
                let analysis = analyze_sources(&provider, &consumer);
                let directory = TestDirectory::create();
                let object = directory.join("assertion.o");
                let executable = directory.join("assertion");
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
                .expect("cross-file assertion must emit a verified object");
                let linked = Command::new(crate::test_support::clang())
                    .arg(&object)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .expect("clang must launch");
                assert!(linked.status.success(), "{inner}/{present}: {linked:?}");
                let run = Command::new(&executable)
                    .output()
                    .expect("assertion executable must launch");
                if present {
                    assert!(run.status.success(), "{inner}: {run:?}");
                    assert_eq!(run.stdout, b"shadow\nmake\nok\n", "{inner}");
                    assert!(run.stderr.is_empty(), "{inner}: {run:?}");
                } else {
                    assert_eq!(run.status.signal(), Some(6), "{inner}: {run:?}");
                }
            }
        }
    }

    #[test]
    fn unit_non_null_assertion_pending_temporary_is_freed_after_control_flow_call() {
        let mut consumer = String::from("package q\n");
        let operands = [
            "if (flag) { p.make() } else { p.make() }",
            "when (flag) { true -> p.make(); false -> p.make() }",
            "when { flag -> p.make(); else -> p.make() }",
            "when (flag) { true, false -> p.make() }",
            "when { flag, flag -> p.make(); else -> p.make() }",
            "if (1 + 2 == 3) { p.make() } else { p.make() }",
            "if (flag && flag) { p.make() } else { p.make() }",
        ];
        for (index, operand) in operands.iter().enumerate() {
            consumer.push_str(&format!(
                "fun inspect{index}(flag: Boolean): Int = p.take(Rc(11), ({operand})!!)\n"
            ));
        }
        consumer.push_str("fun entry(): Unit {\n");
        for index in 0..operands.len() {
            for flag in [true, false] {
                consumer.push_str(&format!(
                "val checked{index}{flag} = if (inspect{index}({flag}) == 32) {{ println(\"ok\") }} else {{ println(\"bad\") }}\n"
            ));
            }
        }
        consumer.push_str("}\n");
        let analysis = analyze_sources(
            "package p\nfun make(): Rc<Int>? = Rc(32)\nfun take(first: Rc<Int>, own second: Rc<Int>): Int = second.value",
            &consumer,
        );
        let (program, entry) = super::super::lower_scalar_unit_with_entry(
            &analysis.sources,
            &analysis.inputs(),
            &analysis.names,
            &analysis.environment,
            &analysis.typed,
            &analysis.owned,
            analysis.declaration("q", "entry"),
        )
        .expect("pending temporary assertions must verify");
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
            .expect("pending temporary LLVM must verify")
            .replace("@malloc(", "@counted_malloc(")
            .replace("@free(", "@counted_free(");
        let directory = TestDirectory::create();
        let ir = directory.join("assertion.ll");
        let counter = directory.join("counter.c");
        let executable = directory.join("assertion-counted");
        std::fs::write(&ir, llvm).expect("write counted LLVM");
        // Track live identities too: a duplicate free cannot compensate for a leaked owner.
        std::fs::write(
            &counter,
            r#"
#include <stdlib.h>
#include <assert.h>
static void *live[28];
static int allocations, releases;
void *counted_malloc(size_t size) {
    assert(allocations < 28);
    void *value = malloc(size);
    assert(value);
    live[allocations++] = value;
    return value;
}
void counted_free(void *value) {
    for (int i = 0; i < allocations; ++i) {
        if (live[i] == value) {
            live[i] = 0;
            ++releases;
            free(value);
            return;
        }
    }
    abort();
}
__attribute__((destructor)) static void verify_counts(void) {
    assert(allocations == 28 && releases == 28);
    for (int i = 0; i < allocations; ++i) assert(!live[i]);
}
"#,
        )
        .expect("write ownership counter");
        let linked = Command::new(crate::test_support::ir_clang())
            .arg(&ir)
            .arg(&counter)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("clang must launch");
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable)
            .output()
            .expect("counted executable must launch");
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, "ok\n".repeat(14).as_bytes());
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}
