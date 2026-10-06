//! Public project build/run specializes a cross-file helper for each concrete environment.
use super::*;

#[test]
fn runtime_constructor_public_project_build_and_run_forward_callbacks() {
    for container in ["Array", "List"] {
        for (literal, last, first) in [
            ("{ index -> index }", 2, 0),
            ("{ index -> index + scale }", 9, 7),
            ("move { index -> index + scale }", 9, 7),
        ] {
            let project = TestProject::create("runtime-constructor");
            let manifest = project.manifest(&["src"]);
            project.write("src/lib/Generate.ko", &format!("package lib\nfun <T> generate(size: Int, initializer: (Int)->T): {container}<T> = {container}<T>(size, initializer)"));
            project.write("src/app/Main.ko", &format!("package app\nfun entry(): Unit {{ val scale = 7\nval callback: (Int)->Int = {literal}\nval items = lib.generate<Int>(3, callback)\nif (items[2] != {last}) {{ error(\"generated value\") }}\nval reused = lib.generate<Int>(1, callback)\nif (reused[0] != {first}) {{ error(\"callback reuse\") }}\nprintln(\"runtime-ok\") }}"));
            let executable = project.join("runtime");
            let built = project_build(&manifest, "app.entry", &executable, None);
            assert!(built.status.success(), "{built:?}");
            assert!(
                built.stdout.is_empty() && built.stderr.is_empty(),
                "{built:?}"
            );
            let artifact = Command::new(&executable).output().expect("artifact runs");
            let executed = run([
                OsStr::new("run"),
                OsStr::new("--project"),
                manifest.as_os_str(),
                OsStr::new("--entry"),
                OsStr::new("app.entry"),
            ]);
            for output in [artifact, executed] {
                assert!(output.status.success(), "{output:?}");
                assert_eq!(output.stdout, b"runtime-ok\n");
                assert!(output.stderr.is_empty(), "{output:?}");
            }
            assert_no_build_temporaries(project.path());
        }
    }
}
