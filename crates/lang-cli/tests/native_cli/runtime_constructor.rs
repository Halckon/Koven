//! Public single-file build/run preserves all three concrete initializer environments.
use super::*;

#[test]
fn runtime_constructor_public_single_build_and_run_reuse_named_callbacks() {
    for container in ["Array", "List"] {
        for (literal, last, first) in [
            ("{ index -> index }", 2, 0),
            ("{ index -> index + scale }", 9, 7),
            ("move { index -> index + scale }", 9, 7),
        ] {
            let directory = TestDirectory::create();
            let source = directory.join("runtime.ko");
            let executable = directory.join("runtime");
            fs::write(&source, format!("fun entry(): Unit {{ val scale = 7\nval callback: (Int)->Int = {literal}\nval items = {container}<Int>(3, callback)\nif (items[2] != {last}) {{ error(\"generated value\") }}\nval reused = {container}<Int>(1, callback)\nif (reused[0] != {first}) {{ error(\"callback reuse\") }}\nprintln(\"runtime-ok\") }}")).expect("fixture write");
            let built = build(source.as_os_str(), "entry", executable.as_os_str());
            assert!(built.status.success(), "{built:?}");
            assert!(
                built.stdout.is_empty() && built.stderr.is_empty(),
                "{built:?}"
            );
            let artifact = Command::new(&executable).output().expect("artifact runs");
            let executed = run([
                OsStr::new("run"),
                source.as_os_str(),
                OsStr::new("--entry"),
                OsStr::new("entry"),
            ]);
            for output in [artifact, executed] {
                assert!(output.status.success(), "{output:?}");
                assert_eq!(output.stdout, b"runtime-ok\n");
                assert!(output.stderr.is_empty(), "{output:?}");
            }
            assert!(
                fs::read_dir(&directory.0)
                    .expect("directory read")
                    .filter_map(Result::ok)
                    .all(|entry| !entry.file_name().to_string_lossy().contains(".kovenc-")),
                "public build must clean sibling temporaries"
            );
        }
    }
}
