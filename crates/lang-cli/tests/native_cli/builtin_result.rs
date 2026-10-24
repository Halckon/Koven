//! SPEC-0349：内建 `Result`/`Ok`/`Err` 在单文件与 project 两种模式无需 import 即可构建运行。
use super::*;

// String payload enum 的 native lowering 仍未支持（conformance q08），此处用 Int 载荷。
const PROGRAM: &str = r#"fun f(): Result<Int, Int> = Ok(1)
fun g(): Result<Int, Int> = Result.Err(2)
fun main(): Unit {
    val first: Int = when (f()) {
        is Ok -> 40
        is Err -> 0
    }
    val r = f()
    val second: Int = when (r) {
        is Ok -> r.success
        is Err -> 0
    }
    val failed = g()
    val third: Int = when (failed) {
        is Ok<Int, Int> -> 0
        is Err<Int, Int> -> failed.error
    }
    if (first + second + third == 43) { println("builtin-result") }
}
"#;

#[test]
fn builtin_result_constructs_and_matches_without_declaration_or_import() {
    let directory = TestDirectory::create();
    let source = directory.join("result.ko");
    fs::write(&source, PROGRAM).expect("source write");
    let executed = run([OsStr::new("run"), source.as_os_str()]);
    assert_eq!(executed.status.code(), Some(0), "{executed:?}");
    assert_eq!(executed.stdout, b"builtin-result\n");
    assert!(executed.stderr.is_empty(), "{executed:?}");

    let root = directory.join("project");
    fs::create_dir_all(root.join("src/app")).expect("project source root");
    fs::write(
        root.join("src/app/Main.ko"),
        format!("package app\n{PROGRAM}"),
    )
    .expect("project source write");
    let manifest = root.join("project.toml");
    fs::write(
        &manifest,
        "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"builtin-result\"\nsource-roots = [\"src\"]\n",
    )
    .expect("project manifest write");
    let project = run([
        OsStr::new("run"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.main"),
    ]);
    assert_eq!(project.status.code(), Some(0), "{project:?}");
    assert_eq!(project.stdout, executed.stdout);
    assert!(project.stderr.is_empty(), "{project:?}");
}

#[test]
fn user_top_level_result_conflicts_with_the_builtin_environment() {
    let directory = TestDirectory::create();
    let source = directory.join("conflict.ko");
    fs::write(
        &source,
        "enum class Result<T, E> { Ok(success: T), Err(error: E) }\nfun main(): Unit {}\n",
    )
    .expect("source write");
    let executed = run([
        OsStr::new("--message-format=json"),
        OsStr::new("run"),
        source.as_os_str(),
    ]);
    assert_eq!(executed.status.code(), Some(1), "{executed:?}");
    assert!(executed.stdout.is_empty());
    let records = String::from_utf8(executed.stderr)
        .expect("JSON Lines")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON record"))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0]["code"], "L0150");
    assert_eq!(
        records[0]["message"],
        "top-level declaration `Result` conflicts with the built-in environment name"
    );
}
