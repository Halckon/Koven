//! SPEC-0054 公开无依赖 project build/run 进程验收。

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[test]
fn project_constants_build_run_and_preserve_outputs_on_invalid_constants() {
    let project = TestProject::create("constants");
    let manifest = project.manifest(&["src"]);
    project.write(
        "src/lib/Values.ko",
        "package lib\nconst val INDEX = 0\nobject Labels { const val TEXT = \"中文\" }",
    );
    project.write("src/app/Main.ko", "package app\nimport lib.Labels\nimport lib.INDEX\nfun start(): Unit { println(Labels.TEXT + lib.Labels.TEXT) }\nfun argv(args: Array<String>): Unit { val text = println(Labels.TEXT)\nval arg = println(args[INDEX]) }");
    let executable = project.join("constant-program");
    let built = run([
        OsStr::new("build"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.start"),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    let launched = Command::new(&executable).output().unwrap();
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(launched.stdout, "中文中文\n".as_bytes());
    let executed = run([
        OsStr::new("run"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.argv"),
        OsStr::new("--"),
        OsStr::new("实参"),
    ]);
    assert_eq!(executed.status.code(), Some(0), "{executed:?}");
    assert_eq!(executed.stdout, "中文\n实参\n".as_bytes());
    let original = fs::read(&executable).unwrap();
    project.write(
        "src/lib/Values.ko",
        "package lib\nconst val INDEX: Byte = 128\nobject Labels { const val TEXT = \"中文\" }",
    );
    let failed = run([
        OsStr::new("build"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.start"),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert!(!failed.status.success(), "{failed:?}");
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("output already exists"),
        "{failed:?}"
    );
    let invalid_output = project.join("invalid-program");
    let invalid = run([
        OsStr::new("build"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.start"),
        OsStr::new("-o"),
        invalid_output.as_os_str(),
    ]);
    assert!(!invalid.status.success(), "{invalid:?}");
    assert!(
        String::from_utf8_lossy(&invalid.stderr).contains("src/lib/Values.ko"),
        "{invalid:?}"
    );
    assert!(!invalid_output.exists());
    assert_eq!(fs::read(&executable).unwrap(), original);
    assert_no_build_temporaries(project.path());
}

struct TestProject(PathBuf);

impl TestProject {
    fn create(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven project cli {label}-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("project directory must be creatable");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("source parent must be creatable");
        }
        fs::write(path, text).expect("project fixture must be writable");
    }

    fn manifest(&self, roots: &[&str]) -> PathBuf {
        let roots = roots
            .iter()
            .map(|root| format!("{root:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.write(
            "project.toml",
            &format!(
                "schema = \"koven.project\"\nversion = 1\n\n[project]\nname = \"cli-project\"\nsource-roots = [{roots}]\n"
            ),
        );
        self.join("project.toml")
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("owned project directory must be removable");
    }
}

#[test]
fn project_build_and_run_link_multi_package_exact_alias_and_argv_entries() {
    let project = TestProject::create("success");
    let manifest = project.manifest(&["src"]);
    project.write(
        "src/lib/Values.ko",
        "package lib\n\
         fun answer(): Int = 40\n\
         fun offset(): Int = 2\n",
    );
    project.write(
        "src/app/Main.ko",
        "package app\n\
         import lib.answer\n\
         import lib.offset as extra\n\
         fun start(): Unit {\n\
             val result = answer() + extra()\n\
         }\n\
         fun argv(args: Array<String>): Unit {\n\
             val first = println(args[0])\n\
             val empty = println(args[1])\n\
             val unicode = println(args[2])\n\
             val last = println(args[3])\n\
         }\n\
         fun fail(): Unit {\n\
             val before = println(\"before abort\")\n\
             val failure = error(\"project failure\")\n\
         }\n",
    );
    let executable = project.join("program");

    let built = run([
        OsStr::new("build"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.start"),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(built.stdout.is_empty(), "{built:?}");
    assert!(built.stderr.is_empty(), "{built:?}");
    assert!(executable.is_file());
    assert_no_build_temporaries(project.path());
    let launched = Command::new(&executable)
        .output()
        .expect("built project executable must launch");
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");

    let executed = run([
        OsStr::new("run"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.start"),
    ]);
    assert_eq!(executed.status.code(), Some(0), "{executed:?}");
    assert!(executed.stdout.is_empty(), "{executed:?}");
    assert!(executed.stderr.is_empty(), "{executed:?}");

    let argv = run([
        OsStr::new("run"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.argv"),
        OsStr::new("--"),
        OsStr::new("first"),
        OsStr::new(""),
        OsStr::new("你好"),
        OsStr::new("last"),
    ]);
    assert_eq!(argv.status.code(), Some(0), "{argv:?}");
    assert_eq!(argv.stdout, "first\n\n你好\nlast\n".as_bytes(), "{argv:?}");
    assert!(argv.stderr.is_empty(), "{argv:?}");

    let failed = run([
        OsStr::new("run"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.fail"),
    ]);
    assert_eq!(failed.status.code(), Some(1), "{failed:?}");
    assert_eq!(failed.stdout, b"before abort\n", "{failed:?}");
    assert!(failed.stderr.is_empty(), "{failed:?}");
}

#[test]
fn project_build_and_run_execute_instance_receivers() {
    let project = TestProject::create("receiver");
    let manifest = project.manifest(&["src"]);
    project.write(
        "src/model/Counter.ko",
        "package model\n\
         class Counter(val item: Int) {\n\
             fun read(): Int = item\n\
         }\n",
    );
    project.write(
        "src/app/Main.ko",
        "package app\n\
         import model.Counter\n\
         fun start(): Unit {\n\
             val counter = Counter(42)\n\
             if (counter.read() == 42) { println(\"receiver-cli\") }\n\
             else { error(\"wrong receiver result\") }\n\
         }\n",
    );
    let executable = project.join("receiver-program");

    let built = project_build(&manifest, "app.start", &executable, None);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    assert!(built.stdout.is_empty(), "{built:?}");
    assert!(built.stderr.is_empty(), "{built:?}");
    let launched = Command::new(&executable)
        .output()
        .expect("receiver project executable must launch");
    assert_eq!(launched.status.code(), Some(0), "{launched:?}");
    assert_eq!(launched.stdout, b"receiver-cli\n");
    assert!(launched.stderr.is_empty(), "{launched:?}");

    let executed = run([
        OsStr::new("run"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.start"),
    ]);
    assert_eq!(executed.status.code(), Some(0), "{executed:?}");
    assert_eq!(executed.stdout, b"receiver-cli\n");
    assert!(executed.stderr.is_empty(), "{executed:?}");
}

#[test]
fn project_frontend_diagnostics_precede_entry_and_preserve_source_key_order() {
    let project = TestProject::create("diagnostics");
    let manifest = project.manifest(&["zroot", "aroot"]);
    project.write(
        "zroot/z/Bad.ko",
        "package z\nfun zed(): Unit { missingZ }\n",
    );
    project.write(
        "aroot/a/Bad.ko",
        "package a\nfun alpha(): Unit { missingA }\n",
    );

    let human_output = project.join("human");
    let human = project_build(&manifest, "absent.entry", &human_output, None);
    assert_eq!(human.status.code(), Some(1), "{human:?}");
    let human_text = String::from_utf8(human.stderr).expect("UTF-8 diagnostics");
    let alpha = human_text.find("aroot/a/Bad.ko").expect("alpha diagnostic");
    let zed = human_text.find("zroot/z/Bad.ko").expect("zed diagnostic");
    assert!(alpha < zed, "{human_text}");
    assert!(human_text.contains("error[L0080]"), "{human_text}");
    assert!(!human_text.contains("project entry `absent.entry`"));
    assert!(!human_output.exists());

    let json_output = project.join("json");
    let json = project_build(&manifest, "absent.entry", &json_output, Some("json"));
    assert_eq!(json.status.code(), Some(1), "{json:?}");
    let records = String::from_utf8(json.stderr)
        .expect("JSON Lines")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON diagnostic"))
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2, "{records:?}");
    assert_eq!(records[0]["primary"]["source"], "aroot/a/Bad.ko");
    assert_eq!(records[1]["primary"]["source"], "zroot/z/Bad.ko");
    assert!(!json_output.exists());
    assert_no_build_temporaries(project.path());
}

const TYPE_GATE_BODY: &str = "fun typed(): Unit { val item: String = 1 }\n";
const OWNERSHIP_GATE_BODY: &str = "class Resource()\n\
    fun moved(own resource: Resource): Unit {\n\
    val first = resource\n\
    val second = resource\n\
    }\n";

#[test]
fn project_names_gate_aggregates_lex_parse_and_names_without_later_diagnostics() {
    let project = TestProject::create("names-gate");
    let manifest = project.manifest(&["zroot", "aroot"]);
    project.write(
        "zroot/z/Names.ko",
        "package z\nfun names(): Unit { missingZ }\n",
    );
    project.write(
        "aroot/a/Prefix.ko",
        "package a\n#\nfun parsed(): Unit { ) val kept = 1 }\n\
         fun names(): Unit { missingA }\n",
    );
    project.write(
        "aroot/a/Later.ko",
        &format!("package a\n{TYPE_GATE_BODY}{OWNERSHIP_GATE_BODY}"),
    );

    // Lexer/Parser 诊断已由 names 聚合；重复收集或提前 fail-fast 都会改变精确结果。
    assert_project_gate(
        &project,
        &manifest,
        &[
            gate_diagnostic("L0001", "aroot/a/Prefix.ko", 10..11, 2, 1),
            gate_diagnostic("L0029", "aroot/a/Prefix.ko", 33..34, 3, 22),
            gate_diagnostic("L0080", "aroot/a/Prefix.ko", 70..78, 4, 21),
            gate_diagnostic("L0080", "zroot/z/Names.ko", 30..38, 2, 21),
        ],
    );
}

#[test]
fn project_type_gate_precedes_ownership_and_entry_in_source_key_order() {
    let project = TestProject::create("type-gate");
    let manifest = project.manifest(&["zroot", "aroot"]);
    for (source, package) in [("zroot/z/Later.ko", "z"), ("aroot/a/Later.ko", "a")] {
        project.write(
            source,
            &format!("package {package}\n{TYPE_GATE_BODY}{OWNERSHIP_GATE_BODY}"),
        );
    }

    // 与 names gate 中相同的后续错误，在无前缀错误时只发布类型诊断。
    assert_project_gate(
        &project,
        &manifest,
        &[
            gate_diagnostic("L0084", "aroot/a/Later.ko", 49..50, 2, 40),
            gate_diagnostic("L0084", "zroot/z/Later.ko", 49..50, 2, 40),
        ],
    );
}

#[test]
fn project_ownership_gate_precedes_entry_in_source_key_order() {
    let project = TestProject::create("ownership-gate");
    let manifest = project.manifest(&["zroot", "aroot"]);
    for (source, package) in [("zroot/z/Moves.ko", "z"), ("aroot/a/Moves.ko", "a")] {
        project.write(source, &format!("package {package}\n{OWNERSHIP_GATE_BODY}"));
    }

    // 同一 move-only fixture 去掉类型错误后，ownership 诊断仍先于缺失 entry。
    assert_project_gate(
        &project,
        &manifest,
        &[
            gate_diagnostic("L0131", "aroot/a/Moves.ko", 103..111, 5, 14),
            gate_diagnostic("L0131", "zroot/z/Moves.ko", 103..111, 5, 14),
        ],
    );
}

fn gate_diagnostic(
    code: &str,
    source: &str,
    bytes: std::ops::Range<usize>,
    line: usize,
    column: usize,
) -> serde_json::Value {
    // 这些 oracle 的 primary 均是单行 ASCII token；字节宽度等于展示列宽。
    serde_json::json!({
        "code": code,
        "primary": {
            "source": source,
            "byte_start": bytes.start,
            "byte_end": bytes.end,
            "start": { "line": line, "column": column },
            "end": { "line": line, "column": column + bytes.len() },
        },
    })
}

fn assert_project_gate(project: &TestProject, manifest: &Path, expected: &[serde_json::Value]) {
    let human_output = project.join("human");
    let human = project_build(manifest, "absent.entry", &human_output, None);
    assert_eq!(human.status.code(), Some(1), "{human:?}");
    assert!(human.stdout.is_empty(), "{human:?}");
    let human_text = String::from_utf8(human.stderr).expect("UTF-8 gate diagnostics");
    assert!(human_text.ends_with('\n'), "{human_text:?}");
    assert!(!human_text.contains("absent.entry"), "{human_text}");
    let headers = human_text
        .lines()
        .filter(|line| !line.starts_with("  "))
        .collect::<Vec<_>>();
    assert_eq!(headers.len(), expected.len(), "{human_text}");
    for (header, diagnostic) in headers.iter().zip(expected) {
        let primary = &diagnostic["primary"];
        let prefix = format!(
            "error[{}] {}:{}:{}-{}:{}: ",
            diagnostic["code"].as_str().expect("expected code"),
            primary["source"].as_str().expect("expected source"),
            primary["start"]["line"],
            primary["start"]["column"],
            primary["end"]["line"],
            primary["end"]["column"],
        );
        assert!(header.starts_with(&prefix), "{human_text}");
    }
    assert!(!human_output.exists());
    assert_no_build_temporaries(project.path());

    let json_output = project.join("json");
    let json = project_build(manifest, "absent.entry", &json_output, Some("json"));
    assert_eq!(json.status.code(), Some(1), "{json:?}");
    assert!(json.stdout.is_empty(), "{json:?}");
    let json_text = String::from_utf8(json.stderr).expect("JSON Lines gate diagnostics");
    assert!(json_text.ends_with('\n'), "{json_text:?}");
    let records = json_text
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("JSON gate diagnostic"))
        .collect::<Vec<_>>();
    let actual = records
        .iter()
        .map(|record| {
            assert_eq!(record["schema"], "koven.diagnostic");
            assert_eq!(record["version"], 1);
            assert_eq!(record["severity"], "error");
            serde_json::json!({ "code": record["code"], "primary": record["primary"] })
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "{records:?}");
    assert!(!json_output.exists());
    assert_no_build_temporaries(project.path());
}

#[test]
fn project_output_preflight_and_operational_errors_are_non_clobbering() {
    let project = TestProject::create("failures");
    let manifest = project.manifest(&["src"]);
    project.write("src/app/Main.ko", "package app\nfun start(): Unit {}\n");

    let manifest_before = fs::read(&manifest).expect("read manifest before overlap check");
    let manifest_overlap = project_build(&manifest, "app.start", &manifest, None);
    assert_eq!(
        manifest_overlap.status.code(),
        Some(1),
        "{manifest_overlap:?}"
    );
    assert!(String::from_utf8_lossy(&manifest_overlap.stderr).contains("paths overlap"));
    assert_eq!(
        fs::read(&manifest).expect("read preserved manifest"),
        manifest_before
    );
    let source = project.join("src/app/Main.ko");
    let source_before = fs::read(&source).expect("read source before overlap check");
    let source_overlap = project_build(&manifest, "app.start", &source, None);
    assert_eq!(source_overlap.status.code(), Some(1), "{source_overlap:?}");
    assert_eq!(
        fs::read(&source).expect("read preserved source"),
        source_before
    );

    let existing = project.join("existing");
    fs::write(&existing, b"caller owned").expect("seed existing output");
    let preflight = project_build(&project.join("missing.toml"), "app.start", &existing, None);
    assert_eq!(preflight.status.code(), Some(1), "{preflight:?}");
    assert!(String::from_utf8_lossy(&preflight.stderr).contains("output already exists"));
    assert_eq!(fs::read(&existing).expect("read existing"), b"caller owned");

    let source_shaped_output = project.join("src/app/generated.ko");
    let overlap = project_build(&manifest, "app.start", &source_shaped_output, None);
    assert_eq!(overlap.status.code(), Some(1), "{overlap:?}");
    assert!(
        String::from_utf8_lossy(&overlap.stderr).contains("next source discovery"),
        "{overlap:?}"
    );
    assert!(!source_shaped_output.exists());

    let missing_output = project.join("missing-entry");
    let missing = project_build(&manifest, "app.absent", &missing_output, Some("json"));
    assert_eq!(missing.status.code(), Some(1), "{missing:?}");
    let missing_text = String::from_utf8(missing.stderr).expect("UTF-8 operational error");
    assert!(missing_text.contains("project entry `app.absent` was not found"));
    assert!(!missing_text.contains("koven.diagnostic"));
    assert_eq!(missing_text.lines().count(), 1);
    assert!(!missing_output.exists());

    project.write(
        "src/app/Unsupported.ko",
        "package app\nfun unsupported(): Unit { val output = println(\"${1}\") }\n",
    );
    let unsupported_output = project.join("unsupported");
    let unsupported = project_build(&manifest, "app.unsupported", &unsupported_output, None);
    assert_eq!(unsupported.status.code(), Some(1), "{unsupported:?}");
    assert!(!unsupported_output.exists());
    assert_no_build_temporaries(project.path());
}

fn project_build(
    manifest: &Path,
    entry: &str,
    executable: &Path,
    message_format: Option<&str>,
) -> Output {
    let mut arguments = Vec::new();
    if let Some(format) = message_format {
        arguments.push(format!("--message-format={format}").into());
    }
    arguments.extend([
        "build".into(),
        "--project".into(),
        manifest.as_os_str().to_owned(),
        "--entry".into(),
        entry.into(),
        "-o".into(),
        executable.as_os_str().to_owned(),
    ]);
    run(arguments)
}

fn run(arguments: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kovenc"))
        .args(arguments)
        .output()
        .expect("kovenc must launch")
}

fn assert_no_build_temporaries(directory: &Path) {
    assert!(
        fs::read_dir(directory)
            .expect("project directory read")
            .filter_map(Result::ok)
            .all(|entry| !entry.file_name().to_string_lossy().contains(".kovenc-")),
        "project build must clean all sibling temporaries"
    );
}

#[test]
fn string_clone_project_preserves_borrowed_argv_and_cross_file_return() {
    let project = TestProject::create("string-clone");
    let manifest = project.manifest(&["src"]);
    project.write(
        "src/lib/Copy.ko",
        "package lib\nfun duplicate(text: String): String = text.clone()",
    );
    project.write(
        "src/app/Main.ko",
        r#"package app
fun start(args: Array<String>): Unit {
    val copy = lib.duplicate(args[0])
    println(args[0])
    println(copy)
    println(args[0].clone())
}"#,
    );
    let executed = run([
        OsStr::new("run"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.start"),
        OsStr::new("--"),
        OsStr::new("克隆"),
    ]);
    assert!(executed.status.success(), "{executed:?}");
    assert_eq!(executed.stdout, "克隆\n克隆\n克隆\n".as_bytes());
    assert!(executed.stderr.is_empty(), "{executed:?}");
}
