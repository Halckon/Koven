//! SPEC-0320: basic and constant project paths retain direct enum Borrow.
use super::{Command, OsStr, TestProject, fs, project_build, run};

#[path = "../support/enum_case_borrow.rs"]
mod support;
use support::{BuildRunCase, SOURCE, assert_successes, remaining_temporaries};

#[test]
fn project_direct_case_borrow_builds_and_runs() {
    let mut cases = Vec::new();
    for (context, prefix) in [
        ("project basic", ""),
        (
            "project constant",
            "const val ENUM_BORROW_MARKER: Int = 1\n",
        ),
    ] {
        let project = TestProject::create("enum-case-borrow");
        let manifest = project.manifest(&["src"]);
        let text = format!("{prefix}{SOURCE}");
        project.write("src/main.ko", &text);
        let source = project.join("src/main.ko");
        let executable = project.join("enum-borrow");
        let built = project_build(&manifest, "main", &executable, None);
        let executed = run([
            OsStr::new("run"),
            OsStr::new("--project"),
            manifest.as_os_str(),
            OsStr::new("--entry"),
            OsStr::new("main"),
        ]);
        let executable_exists = executable.is_file();
        let artifact = (built.status.success() && executable_exists).then(|| {
            Command::new(&executable)
                .output()
                .expect("ordinary built artifact starts")
        });
        cases.push(BuildRunCase {
            context,
            built,
            executed,
            artifact,
            executable_exists,
            source_unchanged: fs::read(&source).expect("read project source") == text.as_bytes(),
            remaining_temporaries: remaining_temporaries(project.path()),
        });
    }
    assert_successes(&cases);
}
