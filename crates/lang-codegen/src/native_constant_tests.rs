//! SPEC-0209: source-to-native constant acceptance, including actual owner lifetimes.
use super::*;

#[test]
fn constant_types_run_in_every_supported_namespace() {
    let values = [
        ("Boolean", "true"),
        ("Byte", "127"),
        ("Short", "32767"),
        ("Int", "2147483647"),
        ("Long", "9223372036854775807L"),
        ("UByte", "255u"),
        ("UShort", "65535u"),
        ("UInt", "4294967295u"),
        ("ULong", "18446744073709551615uL"),
    ];
    let mut declarations = values
        .iter()
        .enumerate()
        .map(|(index, (ty, literal))| format!("const val V{index}: {ty} = {literal}\n"))
        .collect::<String>();
    declarations.push_str("const val LETTER: Char = '文'\nconst val OTHER: Char = '中'\n");
    for (prefix, root) in [
        ("", declarations.clone()),
        ("Config.", format!("object Config {{ {declarations} }}")),
        (
            "Config.",
            format!("class Config {{ companion object {{ {declarations} }} }}"),
        ),
        (
            "Config.",
            format!(
                "value class Config(val item: Int) {{ companion object {{ {declarations} }} }}"
            ),
        ),
        (
            "Config.",
            format!("interface Config {{ companion object {{ {declarations} }} }}"),
        ),
        (
            "Config.",
            format!("enum class Config {{ One; companion object {{ {declarations} }} }}"),
        ),
    ] {
        let checks = values
            .iter()
            .enumerate()
            .map(|(index, (ty, literal))| {
                format!("val expected{index}: {ty} = {literal}\nif ({prefix}V{index} == expected{index}) {{ println(\"{index}\") }}\n")
            })
            .collect::<String>();
        let source = format!(
            "{root}\nfun letter(): Char = {prefix}LETTER\nfun entry(): Unit {{\n{checks}if (letter() == {prefix}LETTER && letter() != {prefix}OTHER) {{ println(\"char\") }}\n}}"
        );
        let run = emit_link_and_run("constant-matrix.ko", &source, "entry");
        assert!(run.status.success(), "{root}: {run:?}");
        assert_eq!(run.stdout, b"0\n1\n2\n3\n4\n5\n6\n7\n8\nchar\n", "{root}");
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

#[test]
fn constants_coexist_with_argument_entry_and_repeatable_objects() {
    let analysis = analyze(
        "constant-argv.ko",
        r#"
const val INDEX: Int = 0
object Config { const val TEXT = "中" + "文" }
fun main(args: Array<String>): Unit {
    val text = println(Config.TEXT)
    val argument = println(args[INDEX])
}
"#,
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
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
    let mut objects = Vec::new();
    for index in 0..2 {
        let object = directory.join(&format!("entry-{index}.o"));
        emit_native_object(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
            NativeEntry::BorrowedArguments(symbol(&analysis, "main", SymbolKind::Function)),
            &object,
        )
        .expect("constant declarations must coexist with borrowed argv");
        objects.push(fs::read(&object).unwrap());
        let executable = directory.join(&format!("entry-{index}"));
        let linked = Command::new("/usr/bin/clang")
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).arg("参数").output().unwrap();
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, "中文\n参数\n".as_bytes());
        assert!(run.stderr.is_empty(), "{run:?}");
    }
    assert_eq!(
        objects[0], objects[1],
        "the same analysis must emit identical object bytes"
    );
}

#[test]
fn constant_analysis_mismatch_and_invalid_values_never_write_objects() {
    let first = analyze(
        "constant.ko",
        "const val VALUE = 1\nfun entry(): Unit { val result = VALUE }",
    );
    let second = analyze(
        "constant.ko",
        "const val VALUE = 2\nfun entry(): Unit { val result = VALUE }",
    );
    let directory = TestDirectory::create();
    for (typed, owned) in [
        (&first.typed, &second.owned),
        (&second.typed, &second.owned),
    ] {
        let output = directory.join("foreign.o");
        let error = emit_native_object(
            &first.sources,
            &first.parsed,
            &first.names,
            typed,
            owned,
            symbol(&first, "entry", SymbolKind::Function),
            &output,
        )
        .expect_err("same IDs cannot authorize values from another analysis");
        assert_eq!(error.kind(), NativeObjectErrorKind::MismatchedAnalysis);
        assert!(!output.exists());
    }
    let invalid = analyze(
        "overflow.ko",
        "const val VALUE: Byte = 128\nfun entry(): Unit {}",
    );
    assert!(invalid.typed.constants().is_none());
    assert!(invalid.owned.constant_materializations().is_none());
    let output = directory.join("invalid.o");
    let error = emit_native_object(
        &invalid.sources,
        &invalid.parsed,
        &invalid.names,
        &invalid.typed,
        &invalid.owned,
        symbol(&invalid, "entry", SymbolKind::Function),
        &output,
    )
    .expect_err("invalid constant facts cannot reach object emission");
    assert_eq!(error.kind(), NativeObjectErrorKind::FrontendDiagnostics);
    assert!(!output.exists());
}

#[test]
fn string_constant_owners_drop_once_without_allocating_literal_buffers() {
    let analysis = analyze(
        "constant-owners.ko",
        r#"
const val TEXT = "中" + "文"
fun make(): String = TEXT
fun take(own text: String): String = text
fun view(text: String): Unit { println(text) }
fun entry(): Unit {
    val first = view((TEXT))
    val joined = TEXT + TEXT
    val second = println(joined)
    if (make() == TEXT) { println(TEXT) }
    val transferred = take(TEXT)
    val third = view(transferred)
    val dynamic = take(TEXT + TEXT)
    val fourth = view(dynamic)
}
"#,
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
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
    let (program, entry) = crate::ssa::lower_scalar_file_with_entry(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
        symbol(&analysis, "entry", SymbolKind::Function),
    )
    .expect("constant owner source must verify");
    let string = program.modules[0]
        .types
        .iter()
        .position(|ty| *ty == SsaTypeKind::StringOwner)
        .unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let llvm = llvm
        .replace("@malloc(", "@counted_malloc(")
        .replace("@free(", "@counted_free(");
    let mut instrumented = String::new();
    let mut sites = 0;
    for line in llvm.lines() {
        instrumented.push_str(line);
        instrumented.push('\n');
        if line.contains(&format!("call void @koven.drop.t{string}(")) {
            sites += 1;
            instrumented.push_str("  call void @counted_drop()\n");
        }
    }
    assert!(sites > 0, "instrumentation must reach String drop calls");
    instrumented.push_str("declare void @counted_drop()\n");
    let directory = TestDirectory::create();
    let ir = directory.join("owners.ll");
    let counter = directory.join("counter.c");
    let executable = directory.join("owners");
    fs::write(&ir, instrumented).unwrap();
    fs::write(
        &counter,
        r#"
#include <stdlib.h>
#include <assert.h>
static void *live[2];
static int allocations, releases, drops;
void counted_drop(void) { ++drops; }
void *counted_malloc(size_t size) {
    assert(allocations < 2);
    void *value = malloc(size);
    assert(value);
    live[allocations++] = value;
    return value;
}
void counted_free(void *value) {
    for (int i = 0; i < 2; ++i) {
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
    /* Nine literal owners + two concat owners; only concat buffers allocate. */
    assert(drops == 11);
    assert(allocations == 2 && releases == 2 && !live[0] && !live[1]);
}
"#,
    )
    .unwrap();
    let linked = Command::new("/usr/bin/clang")
        .arg(&ir)
        .arg(&counter)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    let run = Command::new(&executable).output().unwrap();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        run.stdout,
        "中文\n中文中文\n中文\n中文\n中文中文\n".as_bytes()
    );
    assert!(run.stderr.is_empty(), "{run:?}");
}
