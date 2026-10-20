//! SPEC-0341：真实单文件 Unit source/root/import 的普通 CLI 契约。

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{Value, json};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
const MAIN: &str = "fun main(): Unit { println(\"root\") }\n";
const PACKAGE_MAIN: &str = "package a.b\nfun main(): Unit { println(\"root\") }\n";
const MISMATCH: &str = "package declaration does not match logical source path";

#[test]
fn single_source_roots_follow_logical_components_and_explicit_identity() {
    let directory = Directory::new();
    directory.write("Entry.ko", MAIN);
    directory.write("sub/Entry.ko", MAIN);
    directory.write("x/a/b/Entry.ko", PACKAGE_MAIN);
    directory.write("x/A/b/Entry.ko", PACKAGE_MAIN);
    directory.write("x/xa/b/Entry.ko", PACKAGE_MAIN);
    directory.write("x/b/Entry.ko", PACKAGE_MAIN);
    for (source, root) in [
        ("Entry.ko", None),
        ("Entry.ko", Some(".")),
        ("sub/Entry.ko", Some("sub")),
        ("x/a/b/Entry.ko", None),
        ("x/a/b/Entry.ko", Some("x/.")),
        ("x/a/../a/b/Entry.ko", Some("x")),
    ] {
        let arguments = source_arguments("run", source, root);
        assert_output(&directory.invoke("human", &arguments), 0, "root\n", "");
        directory.assert_no_temporaries();
    }
    for (source, root, text, end) in [
        ("x/A/b/Entry.ko", None, PACKAGE_MAIN, 11),
        ("x/xa/b/Entry.ko", None, PACKAGE_MAIN, 11),
        ("x/a/../b/Entry.ko", None, PACKAGE_MAIN, 11),
        ("x/a/b/Entry.ko", Some("."), PACKAGE_MAIN, 11),
        ("Entry.ko", Some("sub"), MAIN, 0),
    ] {
        let expected = [diagnostic(source, text, "L0146", MISMATCH, 0, end, vec![])];
        assert_diagnostics(&directory, source, root, &expected);
    }
    assert_output(
        &directory.invoke(
            "json",
            &[
                "run",
                "x/a/b/Entry.ko",
                "--source-root",
                "x",
                "--entry",
                "main",
            ],
        ),
        0,
        "root\n",
        "",
    );
    let root_only = directory.invoke(
        "human",
        &[
            "build",
            "x/a/b/Entry.ko",
            "--source-root",
            "x",
            "-o",
            "root-only",
        ],
    );
    assert_output(&root_only, 0, "", "");
    assert_output(
        &Command::new(directory.path().join("root-only"))
            .output()
            .unwrap(),
        0,
        "root\n",
        "",
    );
    let built = directory.invoke(
        "json",
        &[
            "build",
            "x/a/b/Entry.ko",
            "--source-root",
            "x",
            "--entry",
            "main",
            "-o",
            "program",
        ],
    );
    assert_output(&built, 0, "", "");
    assert_output(
        &Command::new(directory.path().join("program"))
            .output()
            .unwrap(),
        0,
        "root\n",
        "",
    );
    directory.assert_no_temporaries();
}

#[cfg(unix)]
#[test]
fn single_source_symlinks_keep_logical_package_and_physical_root_identity() {
    use std::os::unix::fs::symlink;

    let directory = Directory::new();
    directory.write("physical/different/File.ko", PACKAGE_MAIN);
    fs::create_dir_all(directory.path().join("logical/a")).unwrap();
    symlink(
        directory.path().join("physical/different"),
        directory.path().join("logical/a/b"),
    )
    .unwrap();
    symlink(
        directory.path().join("logical"),
        directory.path().join("root-alias"),
    )
    .unwrap();
    directory.write("logical/a/b/Unused.ko", "#\n");
    for root in ["logical", "root-alias"] {
        assert_output(
            &directory.invoke(
                "human",
                &["run", "logical/a/b/File.ko", "--source-root", root],
            ),
            0,
            "root\n",
            "",
        );
    }
    fs::create_dir_all(directory.path().join("file-alias/a/b")).unwrap();
    symlink(
        directory.path().join("physical/different/File.ko"),
        directory.path().join("file-alias/a/b/Entry.ko"),
    )
    .unwrap();
    assert_output(
        &directory.invoke(
            "human",
            &[
                "run",
                "file-alias/a/b/Entry.ko",
                "--source-root",
                "file-alias",
            ],
        ),
        0,
        "root\n",
        "",
    );
    let source = "logical/a/b/File.ko";
    let expected = [diagnostic(
        source,
        PACKAGE_MAIN,
        "L0146",
        MISMATCH,
        0,
        11,
        vec![],
    )];
    assert_diagnostics(&directory, source, Some("physical"), &expected);
    // The OS resolves the symlink before `..`; the logical root must not be collapsed.
    directory.write("physical/a/b/File.ko", PACKAGE_MAIN);
    fs::create_dir(directory.path().join("physical/deep")).unwrap();
    symlink(
        directory.path().join("physical/deep"),
        directory.path().join("jump"),
    )
    .unwrap();
    let source = "jump/../a/b/File.ko";
    assert_output(
        &directory.invoke("human", &["run", source, "--source-root", "physical"]),
        0,
        "root\n",
        "",
    );
    let expected = [diagnostic(
        source,
        PACKAGE_MAIN,
        "L0146",
        MISMATCH,
        0,
        11,
        vec![],
    )];
    assert_diagnostics(&directory, source, Some("."), &expected);
    directory.assert_no_temporaries();
}

#[test]
fn single_header_failures_preserve_complete_diagnostics_before_root_setup() {
    let directory = Directory::new();
    for (source, text, expected) in [
        (
            "poison.ko",
            "package $ junk\nimport q.value\nval x=1",
            vec![("L0001", "unexpected character", 8, 9)],
        ),
        (
            "separator.ko",
            "package a.b val x=1",
            vec![("L0053", "expected file header separator", 12, 15)],
        ),
        (
            "late.ko",
            "package a.b\npackage\nfun main(){missing}",
            vec![
                ("L0146", MISMATCH, 0, 11),
                ("L0051", "misplaced package directive", 12, 19),
                ("L0048", "expected package name", 20, 20),
            ],
        ),
    ] {
        directory.write(source, text);
        let expected = expected
            .into_iter()
            .map(|(code, message, start, end)| {
                diagnostic(source, text, code, message, start, end, vec![])
            })
            .collect::<Vec<_>>();
        // Incomplete header and logical mismatch both precede inaccessible root inspection.
        assert_diagnostics(&directory, source, Some("missing-root"), &expected);
    }
}

#[test]
fn single_missing_explicit_root_is_an_operational_error_after_logical_validation() {
    let directory = Directory::new();
    directory.write("Entry.ko", MAIN);
    let expected = format!(
        "error: native build failed: cannot inspect single-file source root missing-root: {}\n",
        std::io::ErrorKind::NotFound,
    );
    for format in ["human", "json"] {
        for operation in ["build", "run"] {
            let mut arguments = source_arguments(operation, "Entry.ko", Some("missing-root"));
            if operation == "build" {
                arguments.extend(["-o", "must-not-publish"]);
            }
            assert_output(&directory.invoke(format, &arguments), 2, "", &expected);
            assert!(!directory.path().join("must-not-publish").exists());
            directory.assert_no_temporaries();
        }
    }
}

#[test]
fn single_source_root_options_keep_usage_and_program_argument_boundaries() {
    let directory = Directory::new();
    directory.write(
        "Entry.ko",
        "fun main(args: Array<String>): Unit { println(args[0]) }\n",
    );
    for (operation, arguments) in [
        ("run", vec!["run", "Entry.ko", "--source-root"]),
        (
            "run",
            vec![
                "run",
                "Entry.ko",
                "--source-root",
                ".",
                "--source-root",
                ".",
            ],
        ),
        (
            "run",
            vec!["run", "Entry.ko", "--entry", "main", "--source-root", "."],
        ),
        ("run", vec!["run", "Entry.ko", "--other", "."]),
        ("build", vec!["build", "Entry.ko"]),
        (
            "build",
            vec!["build", "Entry.ko", "--source-root", "-o", "program"],
        ),
        (
            "build",
            vec![
                "build",
                "Entry.ko",
                "--source-root",
                ".",
                "--source-root",
                ".",
                "-o",
                "program",
            ],
        ),
    ] {
        let detail = match (operation, arguments.contains(&"--source-root")) {
            ("build", true) => {
                "expected source, optional `--source-root <dir>`, optional `--entry <name>` and `-o <executable>`"
            }
            ("build", false) => "expected source, optional `--entry <name>` and `-o <executable>`",
            ("run", true) => {
                "expected source, optional `--source-root <dir>` and optional `--entry <name>`"
            }
            ("run", false) => "expected source and optional `--entry <name>`",
            _ => unreachable!("fixed build/run cases"),
        };
        let tail = if operation == "build" {
            " -o <executable>"
        } else {
            " [-- <program-arg>...]"
        };
        let expected = format!(
            "error: {detail}\nusage: kovenc {operation} <source.ko> [--source-root <dir>] [--entry <name>]{tail}\n"
        );
        assert_output(&directory.invoke("human", &arguments), 2, "", &expected);
        assert!(!directory.path().join("program").exists());
    }
    for (arguments, expected) in [
        (
            vec![
                "build",
                "--project",
                "project.toml",
                "--source-root",
                ".",
                "--entry",
                "main",
                "-o",
                "program",
            ],
            "error: expected fixed `--project`, `--entry` and `-o` arguments\nusage: kovenc build --project <project.toml> --entry <qualified-name> -o <executable>\n",
        ),
        (
            vec![
                "run",
                "--project",
                "project.toml",
                "--source-root",
                ".",
                "--entry",
                "main",
            ],
            "error: expected fixed `--project` and `--entry` arguments\nusage: kovenc run --project <project.toml> --entry <qualified-name> [-- <program-arg>...]\n",
        ),
    ] {
        for format in ["human", "json"] {
            assert_output(&directory.invoke(format, &arguments), 2, "", expected);
            assert!(!directory.path().join("program").exists());
        }
    }
    assert_output(
        &directory.invoke(
            "json",
            &[
                "run",
                "Entry.ko",
                "--source-root",
                ".",
                "--",
                "--source-root",
                "unchanged",
            ],
        ),
        0,
        "--source-root\n",
        "",
    );
    directory.assert_no_temporaries();
}

#[test]
fn single_imports_use_only_the_selected_file_and_authorized_standard_sources() {
    let directory = Directory::new();
    directory.write("own/Entry.ko", "package own\nimport own.helper as selected\nprivate fun helper(): Unit { println(\"own\") }\nfun main(): Unit { selected() }\n");
    directory.write("own/Unused.ko", "#\n");
    assert_output(
        &directory.invoke("human", &["run", "own/Entry.ko"]),
        0,
        "own\n",
        "",
    );
    for (index, import, call) in [
        (0, "import koven.algorithms.take", "take"),
        (1, "import koven.algorithms.take as prefix", "prefix"),
        (2, "import koven.algorithms.*", "take"),
        (3, "", "koven.algorithms.take"),
    ] {
        let source = format!("Standard{index}.ko");
        directory.write(&source, &format!("{import}\nfun main(): Unit {{ val values = listOf(\"standard\"); borrow val result = {call}(values, 1); for (value in result) {{ println(value) }} }}\n"));
        assert_output(
            &directory.invoke("human", &["run", &source]),
            0,
            "standard\n",
            "",
        );
    }
    let collision = "koven-std/koven/algorithms/ranges.ko";
    directory.write(
        collision,
        "package koven.algorithms\nfun main(): Unit { println(\"collision\") }\n",
    );
    assert_output(
        &directory.invoke("human", &["run", collision]),
        0,
        "collision\n",
        "",
    );

    let source = "Unloaded.ko";
    let text = "import q.value\nfun main(): Unit {}\n";
    directory.write(source, text);
    directory.write("q/Value.ko", "package q\nval value = 1\n");
    let expected = [diagnostic(
        source,
        text,
        "L0148",
        "unresolved exact import target",
        9,
        14,
        vec![
            json!({"kind":"note", "message":"package 'q' is not loaded in this compilation unit"}),
            json!({"kind":"help", "message":"include the required sources in the explicit project source set; single-file commands can use --project"}),
        ],
    )];
    assert_diagnostics(&directory, source, None, &expected);

    let source = "MissingStandard.ko";
    let text = "import koven.algorithms.absent\nfun main(): Unit {}\n";
    directory.write(source, text);
    let expected = [diagnostic(
        source,
        text,
        "L0148",
        "unresolved exact import target",
        24,
        30,
        vec![],
    )];
    assert_diagnostics(&directory, source, None, &expected);

    let source = "MemberTail.ko";
    let text = "import koven.algorithms.take.tail\nfun main(): Unit {}\n";
    directory.write(source, text);
    let start = text.find("tail").unwrap();
    let expected = [diagnostic(
        source,
        text,
        "L0148",
        "unresolved exact import target",
        start,
        start + 4,
        vec![],
    )];
    assert_diagnostics(&directory, source, None, &expected);

    let source = "own/Exact.ko";
    let text = "package own\nimport own.helper\nfun helper(): Unit {}\nfun main(): Unit {}\n";
    directory.write(source, text);
    let imported = text.find("helper").unwrap();
    let declared = text.rfind("helper").unwrap();
    let expected = [diagnostic(
        source,
        text,
        "L0150",
        "exact import binding conflicts with an existing file binding",
        imported,
        imported + 6,
        vec![
            json!({"kind":"label", "message":"existing binding is declared here", "location":location(source, text, declared, declared + 6)}),
        ],
    )];
    assert_diagnostics(&directory, source, None, &expected);

    let source = "Bare.ko";
    let text = "fun main(): Unit { Pair\nResult\ntake }\n";
    directory.write(source, text);
    let expected = ["Pair", "Result", "take"].map(|name| {
        let start = text.find(name).unwrap();
        diagnostic(
            source,
            text,
            "L0080",
            "unresolved name",
            start,
            start + name.len(),
            vec![],
        )
    });
    assert_diagnostics(&directory, source, None, &expected);
    directory.assert_no_temporaries();
}

fn source_arguments<'a>(
    operation: &'a str,
    source: &'a str,
    root: Option<&'a str>,
) -> Vec<&'a str> {
    let mut arguments = vec![operation, source];
    if let Some(root) = root {
        arguments.extend(["--source-root", root]);
    }
    arguments
}

fn diagnostic(
    source: &str,
    text: &str,
    code: &str,
    message: &str,
    start: usize,
    end: usize,
    details: Vec<Value>,
) -> Value {
    json!({"schema":"koven.diagnostic", "version":1, "severity":"error", "code":code,
           "message":message, "details":details, "primary":location(source, text, start, end)})
}

fn location(source: &str, text: &str, start: usize, end: usize) -> Value {
    let position = |offset| {
        let prefix = &text[..offset];
        json!({"line": prefix.bytes().filter(|byte| *byte == b'\n').count() + 1,
               "column": prefix.rsplit('\n').next().unwrap().chars().count() + 1})
    };
    json!({"source":source,"byte_start":start,"byte_end":end,
           "start":position(start),"end":position(end)})
}

fn assert_diagnostics(directory: &Directory, source: &str, root: Option<&str>, expected: &[Value]) {
    let mut human = String::new();
    let mut json_lines = String::new();
    for record in expected {
        let location = &record["primary"];
        human.push_str(&format!(
            "error[{}] {}:{}:{}-{}:{}: {}\n",
            record["code"].as_str().unwrap(),
            source,
            location["start"]["line"],
            location["start"]["column"],
            location["end"]["line"],
            location["end"]["column"],
            record["message"].as_str().unwrap()
        ));
        for detail in record["details"].as_array().unwrap() {
            if detail["kind"] == "label" {
                let location = &detail["location"];
                human.push_str(&format!(
                    "  label {}:{}:{}-{}:{}: {}\n",
                    location["source"].as_str().unwrap(),
                    location["start"]["line"],
                    location["start"]["column"],
                    location["end"]["line"],
                    location["end"]["column"],
                    detail["message"].as_str().unwrap()
                ));
            } else {
                human.push_str(&format!(
                    "  {}: {}\n",
                    detail["kind"].as_str().unwrap(),
                    detail["message"].as_str().unwrap()
                ));
            }
        }
        json_lines.push_str(&serde_json::to_string(record).unwrap());
        json_lines.push('\n');
    }
    for (format, stderr) in [("human", human.as_str()), ("json", json_lines.as_str())] {
        for operation in ["build", "run"] {
            let mut arguments = source_arguments(operation, source, root);
            if operation == "build" {
                arguments.extend(["-o", "must-not-publish"]);
            }
            assert_output(&directory.invoke(format, &arguments), 2, "", stderr);
            assert!(!directory.path().join("must-not-publish").exists());
            directory.assert_no_temporaries();
        }
    }
}

fn assert_output(output: &Output, status: i32, stdout: &str, stderr: &str) {
    assert_eq!(output.status.code(), Some(status), "{output:?}");
    assert_eq!(output.stdout, stdout.as_bytes(), "{output:?}");
    assert_eq!(output.stderr, stderr.as_bytes(), "{output:?}");
}

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven single unit cli-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir(path.join("temporary")).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn invoke(&self, format: &str, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kovenc"))
            .current_dir(self.path())
            .env("TMPDIR", self.path().join("temporary"))
            .arg(format!("--message-format={format}"))
            .args(arguments)
            .output()
            .expect("public single Unit CLI must launch")
    }

    fn assert_no_temporaries(&self) {
        assert!(
            fs::read_dir(self.path().join("temporary"))
                .unwrap()
                .next()
                .is_none()
        );
        for entry in fs::read_dir(self.path()).unwrap() {
            assert!(
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".kovenc-")
            );
        }
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(self.path()).expect("owned ordinary fixture directory");
    }
}
