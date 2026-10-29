//! 最终产物必须在临时对象已清理后，仍能从其他 cwd 按实际源码位置调试。

use super::{TestDirectory, run};
use std::{fs, path::Path, process::Command};

#[test]
fn single_final_artifact_retains_source_breakpoint() {
    verify_source_breakpoint(false);
}

#[test]
fn project_final_artifact_retains_source_breakpoint() {
    verify_source_breakpoint(true);
}

fn verify_source_breakpoint(project: bool) {
    let directory = TestDirectory::create();
    fs::create_dir(directory.join("src")).expect("source root");
    let source = directory.join("src/main.ko");
    let executable = directory.join("program");
    fs::write(
        &source,
        "public fun main(): Unit {\n    println(\"debug witness\")\n}\n",
    )
    .expect("source write");
    let manifest = directory.join("project.toml");
    fs::write(
        &manifest,
        "schema=\"koven.project\"\nversion=1\n[project]\nname=\"debug\"\nsource-roots=[\"src\"]\n",
    )
    .expect("manifest write");
    let built = if project {
        run([
            "build".as_ref(),
            "--project".as_ref(),
            manifest.as_os_str(),
            "--entry".as_ref(),
            "main".as_ref(),
            "-o".as_ref(),
            executable.as_os_str(),
        ])
    } else {
        Command::new(env!("CARGO_BIN_EXE_kovenc"))
            .current_dir(&directory.0)
            .args(["build", "src/main.ko", "--entry", "main", "-o", "program"])
            .output()
            .expect("relative single build starts")
    };
    assert!(built.status.success(), "{built:?}");
    assert_source_breakpoint(&executable, &source, 2, "main");
}

#[test]
fn project_multiple_roots_retain_unicode_source_breakpoint() {
    let directory = TestDirectory::create();
    let entry_root = directory.join("应用 源/app");
    let helper_root = directory.join("共享 源/app");
    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(&helper_root).unwrap();
    fs::write(
        entry_root.join("入口.ko"),
        "package app\npublic fun main(): Unit { show() }\n",
    )
    .unwrap();
    let source = helper_root.join("工具.ko");
    fs::write(
        &source,
        "package app\nfun show(): Unit {\n    println(\"unicode debug witness\")\n}\n",
    )
    .unwrap();
    let manifest = directory.join("project.toml");
    fs::write(&manifest,
        "schema=\"koven.project\"\nversion=1\n[project]\nname=\"debug\"\nsource-roots=[\"应用 源\",\"共享 源\"]\n").unwrap();
    let executable = directory.join("程序 调试");
    let built = run([
        "build".as_ref(),
        "--project".as_ref(),
        manifest.as_os_str(),
        "--entry".as_ref(),
        "app.main".as_ref(),
        "-o".as_ref(),
        executable.as_os_str(),
    ]);
    assert!(built.status.success(), "{built:?}");
    assert_source_breakpoint(&executable, &source, 3, "app.show");
}

fn assert_source_breakpoint(executable: &Path, source: &Path, line: u32, function: &str) {
    let source = fs::canonicalize(source).expect("canonical source");
    assert_final_line_table(executable, line);
    let debugger = Command::new(if cfg!(target_os = "macos") { "/usr/bin/lldb" } else { "lldb" })
        .current_dir(std::env::temp_dir())
        .arg("--batch")
        .args(["-o", &format!("target create '{}'", executable.display())])
        .args([
            "-o",
            &format!("breakpoint set --file '{}' --line {line}", source.display()),
        ])
        .args(["-o", "run", "-o", "frame info"])
        .args(["-o", concat!(
            "script import json; f = lldb.debugger.GetSelectedTarget().GetProcess().GetSelectedThread().GetFrameAtIndex(0); ",
            "print('KOVEN_STOP_FRAME=' + json.dumps({'file': f.GetLineEntry().GetFileSpec().fullpath, ",
            "'line': f.GetLineEntry().GetLine(), 'function': f.GetFunctionName()}))"
        )])
        .output()
        .expect("LLDB is required for final debug artifact acceptance");
    let stdout = String::from_utf8_lossy(&debugger.stdout);
    let stderr = String::from_utf8_lossy(&debugger.stderr);
    assert!(
        !stdout.contains("no locations"),
        "source breakpoint must resolve: {stdout}\n{stderr}"
    );
    assert!(debugger.status.success(), "{stdout}\n{stderr}");
    assert!(
        stdout.contains("stop reason = breakpoint"),
        "must hit source breakpoint: {stdout}\n{stderr}"
    );
    let frame = stdout
        .lines()
        .find_map(|line| line.strip_prefix("KOVEN_STOP_FRAME="))
        .expect("LLDB must report the selected stopped frame");
    let frame: serde_json::Value = serde_json::from_str(frame).expect("stopped frame JSON");
    assert_eq!(
        frame["file"].as_str(),
        source.to_str(),
        "{stdout}\n{stderr}"
    );
    assert_eq!(
        frame["line"].as_u64(),
        Some(u64::from(line)),
        "{stdout}\n{stderr}"
    );
    let stopped_function = frame["function"].as_str().expect("stopped Koven function");
    assert!(
        is_koven_frame_function(stopped_function, function),
        "must stop in Koven function {function}: {frame:?}\n{stdout}\n{stderr}"
    );
}

// LLDB may expose the DI source name or the adapter's f<id> linkage name.
// Keep both forms exact so a different function cannot satisfy the frame check.
fn is_koven_frame_function(name: &str, function: &str) -> bool {
    let numeric = |id: &str| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit());
    let name = match name.split_once('.') {
        Some((prefix, rest)) if prefix.strip_prefix('f').is_some_and(numeric) => rest,
        _ => name,
    };
    name.strip_prefix(&format!("koven.{function}.d"))
        .is_some_and(numeric)
}

#[test]
fn frame_function_identity_rejects_other_or_malformed_declarations() {
    for (name, expected) in [
        ("koven.app.show.d13", true),
        ("f0.koven.app.show.d13", true),
        ("f21.koven.app.show.d987", true),
        ("f.koven.app.show.d13", false),
        ("f-1.koven.app.show.d13", false),
        ("f0.f1.koven.app.show.d13", false),
        ("f0.koven.other.show.d13", false),
        ("f0.koven.app.showcase.d13", false),
        ("f0.koven.app.show.d", false),
        ("f0.koven.app.show.d13suffix", false),
    ] {
        assert_eq!(
            is_koven_frame_function(name, "app.show"),
            expected,
            "{name}"
        );
    }
}

// Inspect the final published artifact, after the build has deleted its .o staging.
fn assert_final_line_table(executable: &Path, line: u32) {
    let tool;
    let artifact;
    #[cfg(target_os = "macos")]
    {
        tool = std::path::PathBuf::from("/usr/bin/dwarfdump");
        let mut bundle = executable.as_os_str().to_os_string();
        bundle.push(".dSYM");
        artifact = std::path::PathBuf::from(bundle);
    }
    #[cfg(not(target_os = "macos"))]
    {
        tool = std::env::var_os("LLVM_SYS_211_PREFIX")
            .map(std::path::PathBuf::from)
            .map(|prefix| prefix.join("bin/llvm-dwarfdump"))
            .unwrap_or_else(|| "llvm-dwarfdump".into());
        artifact = executable.to_owned();
        assert_eq!(&fs::read(executable).unwrap()[..4], b"\x7fELF");
    }
    let dwarf = Command::new(tool)
        .arg("--debug-line")
        .arg(artifact)
        .output()
        .expect("final artifact DWARF tool must launch");
    assert!(dwarf.status.success(), "{dwarf:?}");
    let text = String::from_utf8_lossy(&dwarf.stdout);
    // dsymutil can split directory/name and octal-escape Unicode in this dump.
    // Exact source identity is checked from the actual LLDB frame above.
    assert!(
        text.lines().any(|row| {
            let mut fields = row.split_whitespace();
            fields
                .next()
                .is_some_and(|address| address.starts_with("0x"))
                && fields.next().and_then(|value| value.parse::<u32>().ok()) == Some(line)
        }),
        "final artifact must contain the expected source line: {text}"
    );
}
