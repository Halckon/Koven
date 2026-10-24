//! CI-only ordinary language Abort cases, separate from successful conversion tests.
use std::{
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(super) const CASES: [(&str, &str, &str, &str); 9] = [
    ("byte-upper", "Long", "Byte", "128L"),
    ("byte-lower", "Long", "Byte", "-129L"),
    ("negative-to-wider-unsigned", "Int", "ULong", "-1"),
    (
        "ulong-upper-half-to-long",
        "ULong",
        "Long",
        "9223372036854775808uL",
    ),
    ("short-upper", "UInt", "Short", "32768u"),
    ("ushort-upper", "UInt", "UShort", "65536u"),
    ("int-upper", "Long", "Int", "2147483648L"),
    ("uint-upper", "ULong", "UInt", "4294967296uL"),
    ("ubyte-upper", "Int", "UByte", "256"),
];

pub(super) fn source(source: &str, target: &str, argument: &str) -> String {
    format!(
        "fun convert(value: {source}): {target} = value.to{target}()\nfun main(): Unit {{ println(\"before\"); val result: {target} = convert({argument}); println(\"after\") }}\n"
    )
}

pub(super) fn assert_abort(executable: &Path, directory: &Path) {
    // Core dumps are disabled only in this child shell; executable is passed as an
    // argv operand, never interpolated into shell source. All files remain owned by the test.
    let mut child = Command::new("bash")
        .args([
            "-c",
            "ulimit -c 0 || exit 125; exec \"$@\"",
            "conversion-abort",
        ])
        .arg(executable)
        .current_dir(directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("ordinary conversion executable starts");
    let start = Instant::now();
    loop {
        if child.try_wait().expect("child status").is_some() {
            break;
        }
        if start.elapsed() >= Duration::from_secs(10) {
            child.kill().expect("stop timed out child");
            let output = child.wait_with_output().expect("reap timed out child");
            panic!("conversion exceeded 10 seconds: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().expect("reap conversion child");
    assert_eq!(output.status.signal(), Some(6), "{output:?}");
    assert_eq!(output.stdout, b"before\n", "{output:?}");
    // SPEC-0355: the Guide v0.46 abort report precedes SIGABRT.
    assert!(
        String::from_utf8_lossy(&output.stderr).starts_with("abort: value out of range for "),
        "{output:?}"
    );
}
