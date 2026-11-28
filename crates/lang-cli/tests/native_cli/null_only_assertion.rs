//! Ordinary source execution for the always-null assertion.
use super::{OsStr, TestDirectory, assert_both_native_entries, fs, run};

#[test]
fn nullable_scalar_if_joins_before_a_temporary_container_loop() {
    assert_both_native_entries(
        r#"fun number(present: Boolean): Int? {
    val result: Int? = if (present) { 0 } else { null }
    for (_ in listOf("item")) { println("number-loop") }
    return result
}
fun flag(present: Boolean): Boolean? {
    val result: Boolean? = if (present) { false } else { null }
    for (_ in listOf("item")) { println("flag-loop") }
    return result
}
fun report(present: Boolean): Unit {
    val n = number(present)
    if (n != null && n == 0) { println("zero") }
    if (n == null) { println("number-null") }
    val b = flag(present)
    if (b != null && !b) { println("false") }
    if (b == null) { println("flag-null") }
}
fun main(): Unit { report(true); report(false) }
"#,
        b"number-loop\nzero\nflag-loop\nfalse\nnumber-loop\nnumber-null\nflag-loop\nflag-null\n",
    );
}

#[test]
fn run_null_only_assertion_preserves_operand_effects_and_control_exits() {
    let directory = TestDirectory::create();
    fs::create_dir(directory.join("src")).unwrap();
    let source = directory.join("src/main.ko");
    let text = r#"class Guard(val label: String) { deinit() { println(label) } }
fun missing(): Nothing? {
    val guard = Guard("drop:operand")
    println("operand")
    return null
}
fun abortRefined(): Unit {
    val guard = Guard("unexpected:unwind")
    ((missing()))!!
    println("unexpected:success")
}
fun abortCompatible(): Unit {
    for (label in arrayOf("loop")) {
        val guard = Guard("unexpected:unwind")
        println(label)
        missing()!!
        println("unexpected:success")
    }
}
fun guarded(): Guard? = Guard("drop:subject")
fun bareWhen(): Unit {
    when (guarded()) { null -> null; else -> null }!!
}
fun groupedWhen(): Unit {
    (when (guarded()) { null -> null; else -> null })!!
}
fun <T> unwrap(own source: T?): Unit { source!! }
fun abortGeneric(): Unit { unwrap<Nothing>(null) }
fun choose(flag: Boolean): String {
    val guard = Guard("drop:early")
    return (if (flag) {
        println("early")
        return "value"
    } else { null })!!
}
fun callWhenReady(own input: String): Nothing? { println("unexpected:called"); return null }
fun escapeBeforeCall(): String = callWhenReady((return "before:call"))!!
fun evaluateLoopExit(flag: Boolean): Unit {
    var running = true
    while (running) {
        (if (flag) { running = false; println("continue"); continue } else { null })!!
    }
    println("after:continue")
    while (true) {
        (if (flag) { println("break"); break } else { null })!!
    }
    println("after:break")
}
fun early(): Unit {
    println(choose(true))
    println(escapeBeforeCall())
    evaluateLoopExit(true)
    println("continued")
}
"#;
    fs::write(&source, text).unwrap();
    let manifest = directory.join("project.toml");
    fs::write(
        &manifest,
        "schema = \"koven.project\"\nversion = 1\n[project]\nname = \"null-assertions\"\nsource-roots = [\"src\"]\n",
    )
    .unwrap();

    for project in [false, true] {
        let run_entry = |entry: &str| {
            if project {
                run([
                    OsStr::new("run"),
                    OsStr::new("--project"),
                    manifest.as_os_str(),
                    OsStr::new("--entry"),
                    OsStr::new(entry),
                ])
            } else {
                run([
                    OsStr::new("run"),
                    source.as_os_str(),
                    OsStr::new("--entry"),
                    OsStr::new(entry),
                ])
            }
        };
        for (entry, spelling, stdout) in [
            (
                "abortRefined",
                "((missing()))!!",
                b"operand\ndrop:operand\n".as_slice(),
            ),
            (
                "abortCompatible",
                "missing()!!",
                b"loop\noperand\ndrop:operand\n".as_slice(),
            ),
            (
                "bareWhen",
                "when (guarded()) { null -> null; else -> null }!!",
                b"drop:subject\n".as_slice(),
            ),
            (
                "groupedWhen",
                "(when (guarded()) { null -> null; else -> null })!!",
                b"drop:subject\n".as_slice(),
            ),
            ("abortGeneric", "source!!", b"".as_slice()),
        ] {
            let output = run_entry(entry);
            assert_eq!(
                output.status.code(),
                Some(134),
                "{entry}, project={project}: {output:?}"
            );
            assert_eq!(output.stdout, stdout, "{entry}, project={project}");
            let (line, column) = text
                .lines()
                .enumerate()
                .find_map(|(line, text)| text.find(spelling).map(|column| (line + 1, column + 1)))
                .unwrap();
            let path = if project {
                "src/main.ko"
            } else {
                source.to_str().unwrap()
            };
            assert_eq!(
                String::from_utf8_lossy(&output.stderr),
                format!("abort: null assertion failed\n  at {path}:{line}:{column}\n"),
            );
        }
        let output = run_entry("early");
        assert_eq!(
            output.status.code(),
            Some(0),
            "project={project}: {output:?}"
        );
        assert_eq!(
            output.stdout,
            b"early\ndrop:early\nvalue\nbefore:call\ncontinue\nafter:continue\nbreak\nafter:break\ncontinued\n"
        );
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}
