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

use super::{NativeObjectErrorKind, emit_native_unit_object};

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
}

#[test]
fn unit_object_failures_preserve_targets_and_cleanup_sibling_temporary() {
    let analysis = analyze_unit();
    let foreign = analyze_unit();
    let inputs = analysis.inputs();
    let directory = TestDirectory::create();
    let object = directory.join("preserved.o");
    fs::write(&object, b"preserve me").expect("seed output");

    for (entry, expected) in [
        (
            analysis.declaration("q", "invalidEntry"),
            NativeObjectErrorKind::InvalidEntry,
        ),
        (
            analysis.declaration("q", "unsupportedBorrow"),
            NativeObjectErrorKind::UnsupportedSource,
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
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
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
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         import p.make as build\n\
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
             val seen = p.inspect(message)\n\
             val ownedAction: move (own String) -> Unit = move { owned -> p.inspect(owned) }\n\
             val ownedInvoked = ownedAction(\"native-owned\")\n\
             val early = exercise(true)\n\
             val normal = exercise(false)\n\
         }\n\
         fun invalidEntry(number: Int): Unit {}\n\
         fun unsupportedBorrow(): Unit {\n\
             val action: move (borrow String) -> Unit = move { message -> p.inspect(message) }\n\
             val invoked = action(\"unsupported-borrow\")\n\
         }",
    );
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
