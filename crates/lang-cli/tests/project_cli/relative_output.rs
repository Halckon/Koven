//! Relative output spelling must not change project source-root protection.
//! Invalid source stops both builds before object emission, linking or native execution.
use super::{Command, Output, Path, TestProject, fs, gate_diagnostic};

const SOURCE: &str = "fun main(): Unit { missing }\n";

fn build(project: &TestProject, directory: &Path, output: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kovenc"))
        .current_dir(directory)
        .arg("--message-format=json")
        .arg("build")
        .arg("--project")
        .arg(project.join("project.toml"))
        .args(["--entry", "main", "-o", output])
        .output()
        .expect("kovenc must launch")
}

fn entries(directory: &Path) -> Vec<std::ffi::OsString> {
    let mut entries = fs::read_dir(directory)
        .expect("fixture directory")
        .map(|entry| entry.expect("fixture entry").file_name())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn check(source_directory: bool) {
    let project = TestProject::create("relative-output");
    let manifest = project.manifest(&["src"]);
    project.write("src/Main.ko", SOURCE);
    let source = project.join("src/Main.ko");
    let manifest_before = fs::read(&manifest).unwrap();
    let root_entries = entries(project.path());
    let source_entries = entries(&project.join("src"));
    let directory = if source_directory {
        project.join("src")
    } else {
        project.path().to_owned()
    };
    let mut diagnostics = Vec::new();
    for output in ["artifact.ko", "./artifact.ko"] {
        let result = build(&project, &directory, output);
        assert_eq!(result.status.code(), Some(1), "{result:?}");
        assert!(result.stdout.is_empty(), "{result:?}");
        let stderr = String::from_utf8(result.stderr).expect("UTF-8 diagnostics");
        if source_directory {
            assert!(stderr.contains("next source discovery"), "{stderr}");
            assert!(!stderr.contains("koven.diagnostic"), "{stderr}");
        } else {
            let records = stderr
                .lines()
                .map(|line| {
                    serde_json::from_str::<serde_json::Value>(line)
                        .expect("language diagnostic, not path IO error")
                })
                .collect::<Vec<_>>();
            let [record] = records.as_slice() else {
                panic!("one unresolved source name: {records:?}");
            };
            assert_eq!(record["schema"], "koven.diagnostic");
            assert_eq!(record["version"], 1);
            assert_eq!(record["severity"], "error");
            assert_eq!(
                serde_json::json!({"code": record["code"], "primary": record["primary"]}),
                gate_diagnostic("L0080", "src/Main.ko", 19..26, 1, 20)
            );
            diagnostics.push(records);
        }
        // Build reserves staging before checking the source; every failed attempt must clean it.
        assert_eq!(entries(project.path()), root_entries);
        assert_eq!(entries(&project.join("src")), source_entries);
        assert_eq!(fs::read(&manifest).unwrap(), manifest_before);
        assert_eq!(fs::read(&source).unwrap(), SOURCE.as_bytes());
        assert!(!directory.join(output).exists());
        assert!(!directory.join(format!("{output}.dSYM")).exists());
    }
    if !source_directory {
        assert_eq!(
            diagnostics[0], diagnostics[1],
            "complete diagnostics are spelling-independent"
        );
    }
}

#[test]
fn relative_source_shaped_output_outside_roots_reaches_original_diagnostic() {
    check(false);
}

#[test]
fn relative_source_shaped_output_inside_root_remains_rejected() {
    check(true);
}
