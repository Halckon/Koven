use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::{
    diagnostic::DiagnosticDetail,
    lexer::lex,
    name_resolution::{NameResolution, SymbolId, SymbolKind, resolve_names},
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{TypedFile, check_types, standard_environments},
};

use super::{NativeObjectErrorKind, emit_native_object};
use crate::{
    llvm::{
        LlvmAdapterError,
        layout::{LayoutFailure, LayoutQuantity, TargetLayoutError},
    },
    native::map_backend_error,
    ssa::model::{Program, SsaTypeKind, TypeOrigin},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Analysis {
    sources: SourceMap,
    parsed: ParsedFile,
    names: NameResolution,
    typed: TypedFile,
    owned: OwnershipCheckedFile,
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-native-facade-test-{}-{}",
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

fn analyze(source_name: &str, text: &str) -> Analysis {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(source_name, text)
        .expect("source must be unique");
    let lexed = lex(&sources, source).expect("lexing must succeed internally");
    let parsed = parse_file(&sources, &lexed).expect("parsing must succeed internally");
    let (name_environment, type_environment) = standard_environments();
    let names = resolve_names(&sources, &parsed, &name_environment)
        .expect("name resolution must succeed internally");
    let typed = check_types(&sources, &parsed, &names, &type_environment)
        .expect("type checking must succeed internally");
    let owned = check_ownership(&sources, &parsed, &names, &typed)
        .expect("ownership checking must succeed internally");
    Analysis {
        sources,
        parsed,
        names,
        typed,
        owned,
    }
}

fn symbol(analysis: &Analysis, name: &str, kind: SymbolKind) -> SymbolId {
    analysis
        .names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name && symbol.kind() == kind)
        .map(|symbol| symbol.id())
        .unwrap_or_else(|| {
            panic!(
                "missing fixture symbol {name:?} {kind:?}; symbols: {:?}",
                analysis.names.symbols()
            )
        })
}

#[test]
fn resolved_unit_entry_emits_the_existing_native_wrapper_object() {
    let analysis = analyze("facade.ko", "fun bootstrap(): Unit {}\n");
    let directory = TestDirectory::create();
    let object = directory.join("bootstrap.o");

    emit_native_object(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "bootstrap", SymbolKind::Function),
        &object,
    )
    .expect("resolved entry must emit object");

    assert_eq!(
        &fs::read(&object).expect("object bytes")[..4],
        b"\xcf\xfa\xed\xfe"
    );
    let symbols = Command::new("/usr/bin/nm")
        .arg("-gj")
        .arg(&object)
        .output()
        .expect("nm must launch");
    assert!(symbols.status.success(), "{symbols:?}");
    let names = String::from_utf8(symbols.stdout).expect("nm output must be UTF-8");
    assert_eq!(names.lines().filter(|name| *name == "_main").count(), 1);
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
        let analysis = analyze("invalid-entry.ko", source);
        let output = directory.join(&format!("invalid-{index}.o"));
        let error = emit_native_object(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
            symbol(&analysis, entry_name, kind),
            &output,
        )
        .expect_err("invalid entry must fail");
        assert_eq!(error.kind(), NativeObjectErrorKind::InvalidEntry);
        assert!(!output.exists());
    }

    let one_symbol = analyze("one.ko", "fun only(): Unit {}");
    let two_symbols = analyze("two.ko", "fun first(): Unit {}\nfun foreign(): Unit {}");
    let unknown_output = directory.join("unknown.o");
    let error = emit_native_object(
        &one_symbol.sources,
        &one_symbol.parsed,
        &one_symbol.names,
        &one_symbol.typed,
        &one_symbol.owned,
        symbol(&two_symbols, "foreign", SymbolKind::Function),
        &unknown_output,
    )
    .expect_err("unknown symbol id must fail");
    assert_eq!(error.kind(), NativeObjectErrorKind::InvalidEntry);
    assert!(!unknown_output.exists());

    let first = analyze("first.ko", "fun bootstrap(): Unit {}");
    let second = analyze("second.ko", "fun bootstrap(): Unit {}");
    let output = directory.join("foreign.o");
    let error = emit_native_object(
        &first.sources,
        &first.parsed,
        &first.names,
        &second.typed,
        &second.owned,
        symbol(&first, "bootstrap", SymbolKind::Function),
        &output,
    )
    .expect_err("mixed analysis must fail");
    assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
    assert!(!output.exists());
}

#[test]
fn frontend_diagnostics_and_unsupported_source_do_not_write_objects() {
    let directory = TestDirectory::create();
    let diagnostic = analyze("diagnostic.ko", "fun broken(): Unit { missing }");
    assert!(!diagnostic.names.diagnostics().is_empty());
    let diagnostic_output = directory.join("diagnostic.o");
    let error = emit_native_object(
        &diagnostic.sources,
        &diagnostic.parsed,
        &diagnostic.names,
        &diagnostic.typed,
        &diagnostic.owned,
        symbol(&diagnostic, "broken", SymbolKind::Function),
        &diagnostic_output,
    )
    .expect_err("diagnostics must block lowering");
    assert_eq!(error.kind(), NativeObjectErrorKind::FrontendDiagnostics);
    assert!(!diagnostic_output.exists());

    let unsupported = analyze("unsupported.ko", "fun closure(): Unit { val f = { -> } }");
    assert!(unsupported.parsed.diagnostics().is_empty());
    assert!(unsupported.names.diagnostics().is_empty());
    assert!(unsupported.typed.diagnostics().is_empty());
    assert!(unsupported.owned.diagnostics().is_empty());
    let unsupported_output = directory.join("unsupported.o");
    let error = emit_native_object(
        &unsupported.sources,
        &unsupported.parsed,
        &unsupported.names,
        &unsupported.typed,
        &unsupported.owned,
        symbol(&unsupported, "closure", SymbolKind::Function),
        &unsupported_output,
    )
    .expect_err("unsupported source must fail");
    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
    assert!(!unsupported_output.exists());
}

#[test]
fn standard_error_emits_object_while_interpolated_message_fails_before_writing() {
    let directory = TestDirectory::create();
    let canonical = analyze(
        "error-entry.ko",
        "fun abortEntry(): Unit { error(\"fatal\") }",
    );
    assert!(canonical.names.diagnostics().is_empty());
    assert!(canonical.typed.diagnostics().is_empty());
    assert!(canonical.owned.diagnostics().is_empty());
    let object = directory.join("error.o");
    emit_native_object(
        &canonical.sources,
        &canonical.parsed,
        &canonical.names,
        &canonical.typed,
        &canonical.owned,
        symbol(&canonical, "abortEntry", SymbolKind::Function),
        &object,
    )
    .expect("canonical standard error must emit an object");
    assert!(object.is_file());

    let unsupported = analyze(
        "interpolated-error.ko",
        "fun abortEntry(): Unit { error(\"${1}\") }",
    );
    assert!(unsupported.names.diagnostics().is_empty());
    assert!(unsupported.typed.diagnostics().is_empty());
    assert!(unsupported.owned.diagnostics().is_empty());
    let rejected = directory.join("interpolated.o");
    let error = emit_native_object(
        &unsupported.sources,
        &unsupported.parsed,
        &unsupported.names,
        &unsupported.typed,
        &unsupported.owned,
        symbol(&unsupported, "abortEntry", SymbolKind::Function),
        &rejected,
    )
    .expect_err("interpolated String remains unsupported");
    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
    assert!(!rejected.exists());
}

#[test]
fn standard_println_links_and_writes_exact_utf8_stdout() {
    let analysis = analyze(
        "println-entry.ko",
        "fun output(): Unit { if (true) { println(\"Hello, World!\") }\nif (true) { println(\"你好\") } }",
    );
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let directory = TestDirectory::create();
    let object = directory.join("println.o");
    let executable = directory.join("println");
    emit_native_object(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "output", SymbolKind::Function),
        &object,
    )
    .expect("standard println must emit an object");
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
    assert_eq!(run.stdout, "Hello, World!\n你好\n".as_bytes());
    assert!(run.stderr.is_empty(), "{run:?}");

    let unsupported = analyze(
        "interpolated-println.ko",
        "fun output(): Unit { println(\"${1}\") }",
    );
    let rejected = directory.join("interpolated-println.o");
    let error = emit_native_object(
        &unsupported.sources,
        &unsupported.parsed,
        &unsupported.names,
        &unsupported.typed,
        &unsupported.owned,
        symbol(&unsupported, "output", SymbolKind::Function),
        &rejected,
    )
    .expect_err("interpolated println must fail before writing an object");
    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
    assert!(!rejected.exists());
}

#[test]
fn move_only_rc_payload_borrow_call_links_runs_and_releases_once() {
    let analysis = analyze(
        "borrow-call.ko",
        "class Resource {}\n\
         fun inspect(resource: Resource): Unit {}\n\
         fun borrowEntry(): Unit {\n\
             val owner = Rc(Resource())\n\
             val inspected = inspect(owner.value)\n\
             val retained = owner.share()\n\
         }",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let directory = TestDirectory::create();
    let object = directory.join("borrow-call.o");
    let executable = directory.join("borrow-call");
    emit_native_object(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "borrowEntry", SymbolKind::Function),
        &object,
    )
    .expect("MoveOnly Rc payload Borrow must emit an object");
    let linked = Command::new("/usr/bin/clang")
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
fn declarative_type_roots_emit_with_a_scalar_entry_while_object_root_stays_unsupported() {
    let directory = TestDirectory::create();
    let declarative = analyze(
        "declarative.ko",
        "value class Pair<A, B>(val first: A, val second: B)\n\
         class Holder(val item: Int)\n\
         interface Marker\n\
         enum class Outcome<T, E> { Ok(item: T), Err(failure: E) }\n\
         fun bootstrap(): Unit {}",
    );
    assert!(
        declarative.parsed.diagnostics().is_empty(),
        "{:?}",
        declarative.parsed.diagnostics()
    );
    assert!(declarative.names.diagnostics().is_empty());
    assert!(declarative.typed.diagnostics().is_empty());
    assert!(declarative.owned.diagnostics().is_empty());
    let object = directory.join("declarative.o");
    emit_native_object(
        &declarative.sources,
        &declarative.parsed,
        &declarative.names,
        &declarative.typed,
        &declarative.owned,
        symbol(&declarative, "bootstrap", SymbolKind::Function),
        &object,
    )
    .expect("pure type declarations must not block the scalar entry object");
    assert!(object.is_file());

    let runtime_value = analyze(
        "object-root.ko",
        "object Config {}\nfun bootstrap(): Unit {}",
    );
    assert!(runtime_value.names.diagnostics().is_empty());
    assert!(runtime_value.typed.diagnostics().is_empty());
    assert!(runtime_value.owned.diagnostics().is_empty());
    let rejected = directory.join("object-root.o");
    let error = emit_native_object(
        &runtime_value.sources,
        &runtime_value.parsed,
        &runtime_value.names,
        &runtime_value.typed,
        &runtime_value.owned,
        symbol(&runtime_value, "bootstrap", SymbolKind::Function),
        &rejected,
    )
    .expect_err("object identity still requires an explicit runtime contract");
    assert_eq!(error.kind(), NativeObjectErrorKind::UnsupportedSource);
    assert!(!rejected.exists());
}

#[test]
fn target_layout_failure_bridges_to_l0145_with_use_and_declaration_spans() {
    let source = "value class Huge(val item: Int)\nfun bootstrap(): Unit { Huge(1) }";
    let analysis = analyze("layout-failure.ko", source);
    let declaration = analysis
        .sources
        .span(analysis.parsed.source_id(), 12, 16)
        .expect("declaration span must be valid");
    let use_start = source.rfind("Huge(1)").expect("constructor use must exist");
    let primary = analysis
        .sources
        .span(analysis.parsed.source_id(), use_start, use_start + 7)
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
        &analysis.sources,
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
fn nominal_enum_box_source_emits_links_and_runs() {
    let analysis = analyze(
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
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let directory = TestDirectory::create();
    let object = directory.join("nominal.o");
    let executable = directory.join("nominal");
    emit_native_object(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "bootstrap", SymbolKind::Function),
        &object,
    )
    .expect("nominal source must emit an object");
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
