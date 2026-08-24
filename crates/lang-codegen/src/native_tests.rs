use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::{
    lexer::lex,
    name_resolution::{NameResolution, SymbolId, SymbolKind, resolve_names},
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{TypedFile, check_types, standard_environments},
};

use super::{NativeObjectErrorKind, emit_native_object};

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
