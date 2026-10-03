//! SPEC-0253：迁移前 e6dfea4 生产 CLI 捕获的单文件 human/JSON 进程 oracle。
//!
//! golden 不是由新门面生成；每个 fixture 含有后续阶段错误，锁定首次诊断停点。
//! 所有 gate 同时覆盖 build/run、合法/非法/不存在/默认 entry 与失败原子性。

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    name: &'static str,
    source: &'static str,
    human: &'static str,
    json: &'static str,
    codes: &'static [&'static str],
}

macro_rules! fixture {
    ($name:literal, $codes:expr) => {
        Fixture {
            name: $name,
            source: include_str!(concat!("fixtures/single_file_analysis/", $name, ".ko")),
            human: include_str!(concat!(
                "fixtures/single_file_analysis/",
                $name,
                ".human.stderr"
            )),
            json: include_str!(concat!(
                "fixtures/single_file_analysis/",
                $name,
                ".json.stderr"
            )),
            codes: $codes,
        }
    };
}

#[test]
fn single_file_five_stops_preserve_complete_output_and_precede_entry_and_codegen() {
    for fixture in [
        fixture!("lexer", &["L0001", "L0001"]),
        fixture!("parser", &["L0018", "L0018"]),
        fixture!("names", &["L0080", "L0080"]),
        fixture!("typed", &["L0084", "L0084"]),
        fixture!("ownership", &["L0131", "L0131"]),
    ] {
        assert_stage_gate(fixture);
    }
}

#[test]
fn single_file_constants_preserve_type_stop_and_independent_move_errors() {
    assert_stage_gate(fixture!("const_typed", &["L0090"]));
    assert_stage_gate(fixture!("const_move", &["L0131"]));
}

#[test]
fn single_file_unicode_crlf_preserves_bytes_scalar_columns_and_json_fields() {
    assert_stage_gate(fixture!("unicode", &["L0080"]));
}

#[test]
fn single_file_constant_success_preserves_native_build_run_and_entry_errors() {
    let directory = Directory::new(
        "success",
        include_str!("fixtures/single_file_analysis/success.ko"),
    );
    for format in ["human", "json"] {
        for entry in [None, Some("main")] {
            let built = directory.invoke(format, "build", entry);
            assert_output(&built, 0, "", "");
            let launched = Command::new(directory.path().join("program"))
                .output()
                .expect("built constant program must launch");
            assert_output(&launched, 0, "界é😀界é😀\n", "");
            directory.assert_no_temporaries();
            fs::remove_file(directory.path().join("program")).expect("remove built program");
            assert_output(
                &directory.invoke(format, "run", entry),
                0,
                "界é😀界é😀\n",
                "",
            );
            directory.assert_no_temporaries();
        }
        for (entry, expected) in [
            (
                "absent",
                include_str!("fixtures/single_file_analysis/absent.human.stderr"),
            ),
            (
                "invalid",
                include_str!("fixtures/single_file_analysis/invalid.human.stderr"),
            ),
        ] {
            for operation in ["build", "run"] {
                assert_output(
                    &directory.invoke(format, operation, Some(entry)),
                    2,
                    "",
                    expected,
                );
                assert!(!directory.path().join("program").exists());
                directory.assert_no_temporaries();
            }
        }
    }
    // Operational entry/codegen failures stay human text even in JSON mode.
    assert_eq!(
        include_str!("fixtures/single_file_analysis/absent.human.stderr"),
        include_str!("fixtures/single_file_analysis/absent.json.stderr")
    );
    assert_eq!(
        include_str!("fixtures/single_file_analysis/invalid.human.stderr"),
        include_str!("fixtures/single_file_analysis/invalid.json.stderr")
    );
}

fn assert_stage_gate(fixture: Fixture) {
    let directory = Directory::new(fixture.name, fixture.source);
    let codes = fixture
        .json
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).expect("captured JSON line")["code"]
                .as_str()
                .expect("diagnostic code")
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        codes, fixture.codes,
        "{} golden stage coverage",
        fixture.name
    );
    for entry in [None, Some("main"), Some("invalid"), Some("absent")] {
        for (format, expected) in [("human", fixture.human), ("json", fixture.json)] {
            for operation in ["build", "run"] {
                let output = directory.invoke(format, operation, entry);
                assert_output(&output, 2, "", expected);
                assert!(
                    !directory.path().join("program").exists(),
                    "failed frontend published an executable"
                );
                directory.assert_no_temporaries();
            }
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
    fn new(label: &str, source: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "koven single file oracle {label}-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("oracle directory");
        fs::write(path.join("source.ko"), source).expect("oracle source");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn invoke(&self, format: &str, operation: &str, entry: Option<&str>) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_kovenc"));
        command.current_dir(self.path()).args([
            &format!("--message-format={format}"),
            operation,
            "source.ko",
        ]);
        if let Some(entry) = entry {
            command.args(["--entry", entry]);
        }
        if operation == "build" {
            command.args(["-o", "program"]);
        }
        command.output().expect("public single-file CLI")
    }

    fn assert_no_temporaries(&self) {
        for entry in fs::read_dir(self.path()).expect("oracle directory") {
            let name = entry.expect("directory entry").file_name();
            assert!(
                !name.to_string_lossy().contains(".kovenc-"),
                "temporary object leaked: {name:?}"
            );
        }
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(self.path()).expect("remove owned oracle directory");
    }
}
