//! SPEC-0269: export bounded generated owner cases without launching native tools.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use inkwell::{context::Context, memory_buffer::MemoryBuffer};
use lang_frontend::{
    analysis::{SingleFileAnalysisError, SingleFileStage, analyze_single_file},
    diagnostic::DiagnosticDetail,
    source::SourceMap,
    type_checking::standard_environments,
};

use crate::native_tests::boxed_enum_tests::{lower_to_llvm, write_counted_allocations};

const MINIMAL: &str = r#"
class Leaf(val name: String) { deinit() { println(this.name) } }
fun inspect(item: Leaf): Unit { println(item.name) }
fun entry(): Unit {
    val leaf = Leaf("leaf-0")
    inspect(leaf)
    println("done")
}
"#;

#[test]
fn export_generated_owner_case() {
    if let Some(path) = std::env::var_os("KOVEN_GENERATED_OWNER_CASE") {
        assert!(!path.is_empty(), "generated case path must not be empty");
        export_case(Path::new(&path));
    } else {
        let directory = CaseDirectory::new();
        directory.input(MINIMAL, 1);
        export_case(&directory.0);
        for name in [
            "case.raw.ll",
            "case.asan.ll",
            "case.counter.ll",
            "counter.c",
            "stages.tsv",
            "diagnostics.tsv",
        ] {
            assert!(directory.0.join(name).is_file(), "missing {name}");
        }
        assert_eq!(
            fs::read_to_string(directory.0.join("stages.tsv")).unwrap(),
            "parse\t0\nnames\t0\ntypes\t0\nownership\t0\n"
        );
        assert!(
            fs::read(directory.0.join("diagnostics.tsv"))
                .unwrap()
                .is_empty()
        );
        assert!(!directory.0.join("mutants.tsv").exists());
        assert!(!directory.0.join("case").exists(), "export never executes");
    }
}

/// Observe only real frontend stages, including an accepted input with no backend inputs.
#[test]
fn export_generated_owner_frontend_case() {
    if let Some(path) = std::env::var_os("KOVEN_GENERATED_OWNER_CASE") {
        assert!(!path.is_empty(), "generated case path must not be empty");
        let _ = export_frontend(Path::new(&path));
    } else {
        let directory = CaseDirectory::new();
        fs::write(directory.0.join("case.ko"), MINIMAL).unwrap();
        assert!(export_frontend(&directory.0).is_some());
        assert_eq!(
            fs::read_to_string(directory.0.join("stages.tsv")).unwrap(),
            "parse\t0\nnames\t0\ntypes\t0\nownership\t0\n"
        );
        assert!(
            fs::read(directory.0.join("diagnostics.tsv"))
                .unwrap()
                .is_empty()
        );
        assert_frontend_only(&directory.0);
    }
}

#[test]
fn generated_owner_frontend_export_records_borrow_rejections() {
    let prefix = "class Leaf(val name: String) { deinit() {} }\n\
        fun inspect(item: Leaf): Unit { println(item.name) }\n\
        fun consume(own item: Leaf): Unit {}\n";
    for (source, code) in [
        (
            format!(
                "{prefix}fun entry(): Unit {{ val source = Leaf(\"枝\"); \
                 val moved = source; inspect(source); inspect(moved) }}"
            ),
            "L0131",
        ),
        (
            format!(
                "{prefix}fun invalid(item: Leaf): Unit {{ consume(item) }}\n\
                 fun entry(): Unit {{}}"
            ),
            "L0133",
        ),
    ] {
        let directory = CaseDirectory::new();
        fs::write(directory.0.join("case.ko"), source).unwrap();
        assert!(export_frontend(&directory.0).is_none());
        assert_eq!(
            fs::read_to_string(directory.0.join("stages.tsv")).unwrap(),
            "parse\t0\nnames\t0\ntypes\t0\nownership\t1\n"
        );
        let diagnostics = fs::read_to_string(directory.0.join("diagnostics.tsv")).unwrap();
        assert_eq!(diagnostics.lines().count(), 1, "{diagnostics}");
        assert!(diagnostics.starts_with(&format!("ownership\t{code}\t")));
        assert_frontend_only(&directory.0);
    }
}

fn assert_frontend_only(directory: &Path) {
    let mut names = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["case.ko", "diagnostics.tsv", "stages.tsv"]);
}

/// A rejected input records the first failed phase, never LLVM or a native file.
#[test]
fn generated_owner_export_records_rejection_without_executing() {
    let prefix = "class Leaf { deinit() {} }\nfun consume(own item: Leaf): Unit {}\n";
    for (source, stage, code) in [
        ("fun entry(: Unit {}".to_owned(), "parse", None),
        ("fun entry(): Unit { missing() }".to_owned(), "names", None),
        (
            "fun entry(): Unit { val n: Int = true }".to_owned(),
            "types",
            None,
        ),
        (
            format!(
                "{prefix}fun entry(): Unit {{ val source = Leaf(); consume(source); consume(source) }}"
            ),
            "ownership",
            Some("L0131"),
        ),
        (
            format!(
                "{prefix}fun invalid(item: Leaf): Unit {{ consume(item) }}\nfun entry(): Unit {{}}"
            ),
            "ownership",
            Some("L0133"),
        ),
    ] {
        let directory = CaseDirectory::new();
        directory.input(&source, 0);
        export_case(&directory.0);
        let stages = fs::read_to_string(directory.0.join("stages.tsv")).unwrap();
        let last = stages.lines().last().expect("rejection must reach a phase");
        assert!(last.starts_with(&format!("{stage}\t")), "{stages}");
        assert!(!last.ends_with("\t0"), "{stages}");
        let diagnostics = fs::read_to_string(directory.0.join("diagnostics.tsv")).unwrap();
        assert!(!diagnostics.is_empty());
        if let Some(code) = code {
            assert_eq!(diagnostics.lines().count(), 1, "{diagnostics}");
            assert!(diagnostics.starts_with(&format!("ownership\t{code}\t")));
        }
        for artifact in [
            "case.raw.ll",
            "case.asan.ll",
            "case.counter.ll",
            "counter.c",
        ] {
            assert!(!directory.0.join(artifact).exists());
        }
    }
}

/// Invalid source is a frontend observation, never an expected exporter panic.
fn export_frontend(directory: &Path) -> Option<String> {
    assert!(directory.is_dir(), "generated case directory must exist");
    for name in ["stages.tsv", "diagnostics.tsv", "case.raw.ll", "counter.c"] {
        assert!(
            !directory.join(name).exists(),
            "generated output must be new: {name}"
        );
    }
    let source = fs::read_to_string(directory.join("case.ko")).expect("read generated case.ko");
    assert!(source.len() <= 8192, "generated source exceeds 8 KiB");
    let mut sources = SourceMap::new();
    let id = sources.add_source("case.ko", &source).unwrap();
    let (names, types) = standard_environments();
    let mut stages = String::new();
    let mut diagnostics = String::new();
    let analysis = analyze_single_file(
        &sources,
        id,
        &names,
        &types,
        |stage, found| {
            let stage = match stage {
                // Parser diagnostics include lexical diagnostics with their identity.
                SingleFileStage::Lexer => return Ok(()),
                SingleFileStage::Parser => "parse",
                SingleFileStage::NameResolution => "names",
                SingleFileStage::TypeChecking => "types",
                SingleFileStage::OwnershipChecking => "ownership",
            };
            stages.push_str(&format!("{stage}\t{}\n", found.len()));
            for diagnostic in found {
                let span = diagnostic.primary_span();
                let labels = diagnostic
                    .details()
                    .iter()
                    .filter_map(|detail| match detail {
                        DiagnosticDetail::Label(label) => {
                            Some(format!("{}:{}", label.span().start(), label.span().end()))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                diagnostics.push_str(&format!(
                    "{stage}\t{}\t{}\t{}\t{labels}\n",
                    diagnostic.code(),
                    span.start(),
                    span.end(),
                ));
            }
            if found.is_empty() { Ok(()) } else { Err(()) }
        },
        |_| Ok(()),
    );
    fs::write(directory.join("stages.tsv"), stages).unwrap();
    fs::write(directory.join("diagnostics.tsv"), diagnostics).unwrap();
    match analysis {
        Ok(_) => Some(source),
        Err(SingleFileAnalysisError::Host(())) => None,
        Err(error) => panic!("internal frontend failure, not an invalid-source verdict: {error:?}"),
    }
}

fn export_case(directory: &Path) {
    let Some(source) = export_frontend(directory) else {
        return;
    };
    let expected: usize = fs::read_to_string(directory.join("expected-allocations.txt"))
        .expect("read expected allocation count")
        .trim()
        .parse()
        .expect("expected allocation count must be an integer");
    let order = if directory.join("expected-order.txt").exists() {
        Some(
            fs::read_to_string(directory.join("expected-order.txt"))
                .expect("read expected release order")
                .trim()
                .split(',')
                .map(|value| value.trim().parse::<usize>().expect("allocation identity"))
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    assert!(
        (1..=9).contains(&expected),
        "generated valid fixture must allocate between one and nine owners"
    );
    let llvm = lower_to_llvm("case.ko", &source);
    fs::write(directory.join("case.raw.ll"), &llvm).unwrap();
    let context = Context::create();
    let buffer = MemoryBuffer::create_from_memory_range_copy(llvm.as_bytes(), "generated-case");
    let module = context.create_module_from_ir(buffer).unwrap();
    crate::native_sanitizer_tests::mark_address_sanitizer(&context, &module);
    module
        .verify()
        .expect("ASan attributes preserve valid LLVM");
    fs::write(
        directory.join("case.asan.ll"),
        module.print_to_string().to_bytes(),
    )
    .unwrap();
    write_counted_allocations(
        &llvm,
        expected,
        order.as_deref(),
        &directory.join("case.counter.ll"),
        &directory.join("counter.c"),
    );
}

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct CaseDirectory(PathBuf);

impl CaseDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven-generated-owner-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("generated test directory must be new");
        Self(path)
    }

    fn input(&self, source: &str, allocations: usize) {
        fs::write(self.0.join("case.ko"), source).unwrap();
        fs::write(
            self.0.join("expected-allocations.txt"),
            format!("{allocations}\n"),
        )
        .unwrap();
    }
}

impl Drop for CaseDirectory {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("generated owner artifacts retained: {}", self.0.display());
        } else {
            fs::remove_dir_all(&self.0).expect("remove only this test's own artifacts");
        }
    }
}
