//! Both public CLI modes must load the real trusted text source and execute its owned results.
use super::*;

#[path = "text_bytes/matrix.rs"]
mod matrix;

const PREFIX: &str = r#"import koven.text.byteLength
import koven.text.toBytes
import koven.text.decodeUtf8
import koven.text.sliceBytes
import koven.text.checkedPlus
import koven.text.TextError
fun invalidUtf8(bytes: Array<UByte>, expected: Int): Unit {
    val decoded = decodeUtf8(bytes)
    when (decoded) {
        is Ok -> error("invalid UTF-8 accepted")
        is Err -> {
            val problem = decoded.error
            when (problem) {
                is TextError.InvalidUtf8 -> {
                    if (problem.offset != expected) { error("wrong UTF-8 offset") }
                }
                else -> error("wrong UTF-8 classification")
            }
        }
    }
}
"#;

const PROGRAM: &str = r#"fun leftText(): String { println("left"); return "x".clone() }
fun rightText(): String { println("right"); return "y".clone() }
fun main(): Unit {
    val ordered = leftText().checkedPlus(rightText())
    if (ordered != null) { println(ordered) } else { error("small ordered concatenation failed") }
    val source = "A\0é".clone()
    if (source.byteLength() != 4) { error("byte length") }
    val bytes = source.toBytes()
    val decoded = decodeUtf8(bytes)
    when (decoded) {
        is Ok -> println(decoded.success)
        is Err -> error("decode failed")
    }
    val sliced = source.sliceBytes(2, 4)
    when (sliced) {
        is Ok -> println(sliced.success)
        is Err -> error("slice failed")
    }
    val invalid = source.sliceBytes(3, 3)
    when (invalid) {
        is Ok -> error("midpoint accepted")
        is Err -> {
            val problem = invalid.error
            when (problem) {
                is TextError.NotScalarBoundary -> println("boundary")
                else -> error("wrong error")
            }
        }
    }
    val range = source.sliceBytes(-1, 3)
    when (range) {
        is Ok -> error("negative range accepted")
        is Err -> {
            val problem = range.error
            when (problem) {
                is TextError.InvalidRange -> println("range")
                else -> error("range must precede boundary")
            }
        }
    }
    invalidUtf8(Array<UByte>(4, { index -> if (index == 0) 65u else if (index == 1) 226u else if (index == 2) 40u else 161u }), 1)
    invalidUtf8(Array<UByte>(3, { index -> if (index == 0) 65u else if (index == 1) 226u else 130u }), 1)
    val joined = source.checkedPlus("!")
    if (joined != null) { println(joined) } else { error("small concatenation failed") }
    println(source)
    if (bytes.size != 4) { error("byte array consumed") }
    val afterSourceDrop = decodeUtf8(bytes)
    when (afterSourceDrop) {
        is Ok -> println(afterSourceDrop.success)
        is Err -> error("byte copy did not survive source")
    }
}
"#;
const EXPECTED: &[u8] =
    b"left\nright\nxy\nA\0\xc3\xa9\n\xc3\xa9\nboundary\nrange\nA\0\xc3\xa9!\nA\0\xc3\xa9\nA\0\xc3\xa9\n";

#[test]
fn public_text_byte_apis_check_build_and_run_in_single_and_project_modes() {
    check_build_run_both_modes(PROGRAM, EXPECTED);
}

#[test]
fn concatenation_outside_a_lexical_string_loan_preserves_the_owner() {
    assert_both_native_entries(
        r#"fun view(borrow source: String): borrow String from source = source
fun left(): String { println("left"); return "λ" }
fun right(): String { println("right"); return "\0x" }
fun main(): Unit {
    val owner = (left() + right()) + "!"
    block {
        borrow val alias = view(owner)
        if (alias == "λ\0x!") { println(alias.clone()) }
        println(alias)
    }
    println(owner)
    val after = owner + "."
    println(after)
}
"#,
        b"left\nright\n\xce\xbb\0x!\n\xce\xbb\0x!\n\xce\xbb\0x!\n\xce\xbb\0x!.\n",
    );
}

#[test]
fn concatenation_reads_both_sides_of_an_active_string_loan_without_consuming_it() {
    assert_both_native_entries(
        r#"fun view(borrow source: String): borrow String from source = source
fun prefix(): String { println("left"); return "[" }
fun suffix(): String { println("right"); return "]" }
fun main(): Unit {
    val owner = "λ\0x"
    borrow val alias = view(owner)
    val ending = alias + suffix()
    val unrelated = "un" + "related"
    val beginning = prefix() + alias
    println(alias + "!")
    println(alias)
    println(beginning)
    println(ending)
    println(unrelated)
    println(owner)
}
"#,
        b"right\nleft\n\xce\xbb\0x!\n\xce\xbb\0x\n[\xce\xbb\0x\n\xce\xbb\0x]\nunrelated\n\xce\xbb\0x\n",
    );
}

#[test]
fn concatenation_ends_an_active_string_loan_on_an_early_operand_return() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
fun view(borrow source: String): borrow String from source = source
fun prefix(): String { println("left"); return "[" }
fun joined(stop: Boolean): String {
    val marker = Marker("drop")
    val owner = "lo" + "an"
    borrow val alias = view(owner)
    val result = (prefix() + alias) + (if (stop) { return "st" + "op" } else { "]" })
    println(alias)
    return result
}
fun main(): Unit {
    println(joined(true))
    println(joined(false))
}
"#,
        b"left\ndrop\nstop\nleft\nloan\ndrop\n[loan]\n",
    );
}

#[test]
fn concatenation_inside_a_shared_argument_cleans_up_on_return() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
fun view(borrow source: String): borrow String from source = source
fun prefix(): String { println("left"); return "[" }
fun observed(stop: Boolean): String {
    val marker = Marker("drop")
    val owner = "lo" + "an"
    borrow val alias = view(owner)
    println((prefix() + alias) + (if (stop) { return "st" + "op" } else { "]" }))
    println(alias)
    return "done"
}
fun main(): Unit {
    println(observed(true))
    println(observed(false))
}
"#,
        b"left\ndrop\nstop\nleft\n[loan]\nloan\ndrop\ndone\n",
    );
}

#[test]
fn concatenation_in_a_later_argument_releases_the_earlier_temporary_resource() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
fun view(borrow source: String): borrow String from source = source
fun first(): Marker { println("first"); return Marker("first-drop") }
fun prefix(): String { println("left"); return "[" }
fun consume(borrow first: Marker, borrow second: String): Unit {
    println("read:" + first.label)
    println(second)
}
fun observed(stop: Boolean): String {
    val marker = Marker("drop")
    val owner = "lo" + "an"
    borrow val alias = view(owner)
    consume(first(), (prefix() + alias) + (if (stop) { println("return-value"); return "st" + "op" } else { "]" }))
    println(alias)
    return "done"
}
fun main(): Unit {
    println(observed(true))
    println(observed(false))
}
"#,
        b"first\nleft\nreturn-value\nfirst-drop\ndrop\nstop\nfirst\nleft\nread:first-drop\n[loan]\nfirst-drop\nloan\ndrop\ndone\n",
    );
}

#[test]
fn concatenation_in_a_method_argument_releases_the_temporary_receiver_on_return() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
fun view(borrow source: String): borrow String from source = source
fun prefix(): String { println("left"); return "[" }
class Sink(val label: String) {
    deinit() { println(label) }
    borrow fun accept(borrow message: String): Unit {
        println("call")
        println(message)
    }
}
fun sink(): Sink { println("receiver"); return Sink("receiver-drop") }
fun observed(stop: Boolean): String {
    val marker = Marker("drop")
    val owner = "lo" + "an"
    borrow val alias = view(owner)
    ((sink())).accept((prefix() + alias) + (if (stop) { println("return-value"); return "st" + "op" } else { "]" }))
    println(alias)
    return "done"
}
fun main(): Unit {
    println(observed(true))
    println(observed(false))
}
"#,
        b"receiver\nleft\nreturn-value\nreceiver-drop\ndrop\nstop\nreceiver\nleft\ncall\n[loan]\nreceiver-drop\nloan\ndrop\ndone\n",
    );
}

#[test]
fn concatenation_closes_the_active_string_loan_on_each_operand_return() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
fun view(borrow source: String): borrow String from source = source
fun prefix(): String { println("left"); return "[" }
fun render(first: Boolean, second: Boolean): String {
    val marker = Marker("drop")
    val owner = "lo" + "an"
    borrow val alias = view(owner)
    val result = (prefix() + alias) + (if (first) {
        println("first-return")
        return "st" + "op"
    } else {
        if (second) {
            println("second-return")
            return "sk" + "ip"
        }
        "]"
    })
    println(alias)
    return result
}
fun main(): Unit {
    println(render(true, false))
    println(render(false, true))
    println(render(false, false))
}
"#,
        b"left\nfirst-return\ndrop\nstop\nleft\nsecond-return\ndrop\nskip\nleft\nloan\ndrop\n[loan]\n",
    );
}

#[test]
fn concatenation_checks_return_prefixes_of_both_string_value_arms() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
fun view(borrow source: String): borrow String from source = source
fun prefix(): String { println("left"); return "[" }
fun render(first: Boolean, stop: Boolean): String {
    val marker = Marker("drop")
    val owner = "lo" + "an"
    borrow val alias = view(owner)
    val result = (prefix() + alias) + (if (first) {
        if (stop) {
            println("first-return")
            return "st" + "op"
        }
        "}"
    } else {
        if (stop) {
            println("second-return")
            return "sk" + "ip"
        }
        "]"
    })
    println(alias)
    return result
}
fun main(): Unit {
    println(render(true, true))
    println(render(false, true))
    println(render(true, false))
    println(render(false, false))
}
"#,
        b"left\nfirst-return\ndrop\nstop\nleft\nsecond-return\ndrop\nskip\nleft\nloan\ndrop\n[loan}\nleft\nloan\ndrop\n[loan]\n",
    );
}

#[test]
fn concatenation_uses_the_reachable_string_when_arms_and_their_exit_facts() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
fun view(borrow source: String): borrow String from source = source
fun prefix(): String { println("left"); return "[" }
fun selector(first: Boolean): Boolean { println("selector"); return first }
fun render(first: Boolean, stop: Boolean): String {
    val marker = Marker("drop")
    val owner = "lo" + "an"
    borrow val alias = view(owner)
    val result = (prefix() + alias) + (when (selector(first)) {
        true -> {
            if (stop) { println("first-return"); return "st" + "op" }
            "}"
        }
        false -> {
            if (stop) { println("second-return"); return "sk" + "ip" }
            "]"
        }
        else -> { println("unreachable"); return "dead" }
    })
    println(alias)
    return result
}
fun main(): Unit {
    println(render(true, true))
    println(render(false, true))
    println(render(true, false))
    println(render(false, false))
}
"#,
        b"left\nselector\nfirst-return\ndrop\nstop\nleft\nselector\nsecond-return\ndrop\nskip\nleft\nselector\nloan\ndrop\n[loan}\nleft\nselector\nloan\ndrop\n[loan]\n",
    );
}

#[test]
fn concatenation_preserves_compound_integer_when_subject_and_selector_order() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
fun view(borrow source: String): borrow String from source = source
fun prefix(): String { println("left"); return "[" }
fun selector(): Int { println("selector"); return 1 }
fun render(first: Boolean, mode: Int): String {
    val marker = Marker("drop")
    val owner = "lo" + "an"
    borrow val alias = view(owner)
    val result = (prefix() + alias) + (when (2 + (if (first) {
        println("subject-first")
        if (mode == 0) { println("subject-return"); return "subject" }
        0 + 1
    } else {
        println("subject-second")
        0
    }) - 2) {
        selector() -> {
            if (mode == 1) { println("first-return"); return "st" + "op" }
            "}"
        }
        else -> {
            if (mode == 1) { println("second-return"); return "sk" + "ip" }
            "]"
        }
    })
    println(alias)
    return result
}
fun main(): Unit {
    println(render(true, 0))
    println(render(true, 1))
    println(render(false, 1))
    println(render(true, 2))
    println(render(false, 2))
}
"#,
        b"left\nsubject-first\nsubject-return\ndrop\nsubject\n\
left\nsubject-first\nselector\nfirst-return\ndrop\nstop\n\
left\nsubject-second\nselector\nsecond-return\ndrop\nskip\n\
left\nsubject-first\nselector\nloan\ndrop\n[loan}\n\
left\nsubject-second\nselector\nloan\ndrop\n[loan]\n",
    );
}

#[test]
fn concatenation_preserves_cleanup_for_compound_boolean_text_and_enum_when_subjects() {
    for (declaration, yes, no, selector) in [
        ("", "true", "false", "true"),
        ("", r#""y" + "es""#, r#""n" + "o""#, r#""missing", "yes""#),
        (
            "enum class Choice { Yes, No }",
            "Choice.Yes",
            "Choice.No",
            "Choice.Yes",
        ),
    ] {
        let source = r#"$declaration
class Marker(val label: String) { deinit() { println(label) } }
fun view(borrow source: String): borrow String from source = source
fun prefix(): String { println("left"); return "[" }
fun render(first: Boolean, mode: Int): String {
    val marker = Marker("drop")
    val owner = "lo" + "an"
    borrow val alias = view(owner)
    val result = (prefix() + alias) + (when (if (first) {
        println("subject-first")
        if (mode == 0) { println("subject-return"); return "subject" }
        $yes
    } else {
        println("subject-second")
        $no
    }) {
        $selector -> {
            if (mode == 1) { println("first-return"); return "st" + "op" }
            "}"
        }
        else -> {
            if (mode == 1) { println("second-return"); return "sk" + "ip" }
            "]"
        }
    })
    println(alias)
    return result
}
fun main(): Unit {
    println(render(true, 0))
    println(render(true, 1))
    println(render(false, 1))
    println(render(true, 2))
    println(render(false, 2))
}
"#
        .replace("$declaration", declaration)
        .replace("$yes", yes)
        .replace("$no", no)
        .replace("$selector", selector);
        assert_both_native_entries(
            &source,
            b"left\nsubject-first\nsubject-return\ndrop\nsubject\n\
left\nsubject-first\nfirst-return\ndrop\nstop\n\
left\nsubject-second\nsecond-return\ndrop\nskip\n\
left\nsubject-first\nloan\ndrop\n[loan}\n\
left\nsubject-second\nloan\ndrop\n[loan]\n",
        );
    }
}

#[test]
fn concatenation_cleans_up_a_temporary_move_only_enum_when_subject_on_each_exit() {
    assert_both_native_entries(
        r#"class Marker(val label: String) { deinit() { println(label) } }
enum class Choice { Yes(payload: Marker), Other(payload: Marker) }
fun direct(): String {
    return when (((Choice.Yes(Marker("direct-drop"))))) {
        is Choice.Other -> "unexpected"
        is Choice.Yes -> { println("matched"); "direct" }
    }
}
fun make(first: Boolean): Choice =
    if (first) { Choice.Yes(Marker("first-pending")) } else { Choice.Other(Marker("second-pending")) }
fun nested(first: Boolean, side: Boolean): String {
    return when (make(first)) {
        is Choice.Yes -> if (side) { return "a" } else { return "b" }
        else -> "c"
    }
}
fun nested_boolean(side: Boolean, inner: Boolean): String {
    return when (make(true)) {
        is Choice.Yes -> when (side) {
            true -> if (inner) { return "a" } else { return "b" }
            else -> "c"
        }
        else -> "unexpected"
    }
}
fun loop_arm(run: Boolean, side: Boolean): String {
    return when (Choice.Yes(Marker("loop-drop"))) {
        is Choice.Yes -> {
            var pending = run
            while (pending) {
                pending = false
                if (side) { println("break"); break } else { println("continue"); continue }
            }
            println("loop-arm")
            "done"
        }
        else -> "unexpected"
    }
}
fun for_arm(rounds: Int, side: Boolean): String {
    return when (Choice.Yes(Marker("for-drop"))) {
        is Choice.Yes -> {
            for (index in 0..<rounds) {
                if (side) { println("break"); break } else { println("continue"); continue }
            }
            println("for-arm")
            "done"
        }
        else -> "unexpected"
    }
}
fun view(borrow source: String): borrow String from source = source
fun render(first: Boolean, stop: Boolean): String {
    val outer = Marker("outer-drop")
    val owner = "lo" + "an"
    borrow val alias = view(owner)
    val result = ("[" + alias) + (when (if (first) {
        println("construct-first")
        Choice.Yes(Marker("first-drop"))
    } else {
        println("construct-second")
        Choice.Other(Marker("second-drop"))
    }) {
        is Choice.Yes -> {
            println("first-arm")
            if (stop) { println("return-value"); return "st" + "op" }
            "}"
        }
        is Choice.Other -> {
            println("second-arm")
            if (stop) { println("return-value"); return "sk" + "ip" }
            "]"
        }
    })
    println(alias)
    return result
}
fun main(): Unit {
    println(direct())
    println(nested(true, true))
    println(nested(true, false))
    println(nested(false, false))
    println(nested_boolean(true, true))
    println(nested_boolean(true, false))
    println(nested_boolean(false, false))
    println(loop_arm(true, true))
    println(loop_arm(true, false))
    println(loop_arm(false, false))
    println(for_arm(2, true))
    println(for_arm(2, false))
    println(for_arm(0, false))
    println(render(true, true))
    println(render(false, true))
    println(render(true, false))
    println(render(false, false))
}
"#,
        b"matched\ndirect-drop\ndirect\n\
first-pending\na\nfirst-pending\nb\nsecond-pending\nc\n\
first-pending\na\nfirst-pending\nb\nfirst-pending\nc\n\
break\nloop-arm\nloop-drop\ndone\ncontinue\nloop-arm\nloop-drop\ndone\nloop-arm\nloop-drop\ndone\n\
break\nfor-arm\nfor-drop\ndone\ncontinue\ncontinue\nfor-arm\nfor-drop\ndone\nfor-arm\nfor-drop\ndone\n\
construct-first\nfirst-arm\nreturn-value\nfirst-drop\nouter-drop\nstop\n\
construct-second\nsecond-arm\nreturn-value\nsecond-drop\nouter-drop\nskip\n\
construct-first\nfirst-arm\nfirst-drop\nloan\nouter-drop\n[loan}\n\
construct-second\nsecond-arm\nsecond-drop\nloan\nouter-drop\n[loan]\n",
    );
}

fn check_build_run_both_modes(body: &str, expected: &[u8]) {
    let program = format!("{PREFIX}{body}");
    let directory = TestDirectory::create();
    let source = directory.join("text.ko");
    fs::write(&source, &program).unwrap();
    let checked = run([OsStr::new("check"), source.as_os_str()]);
    assert!(checked.status.success(), "{checked:?}");
    let executable = directory.join("text");
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert!(built.status.success(), "{built:?}");
    let direct = Command::new(&executable).output().unwrap();
    assert!(direct.status.success(), "{direct:?}");
    assert_eq!(direct.stdout, expected);
    assert!(direct.stderr.is_empty(), "{direct:?}");
    let executed = run([OsStr::new("run"), source.as_os_str()]);
    assert!(executed.status.success(), "{executed:?}");
    assert_eq!(executed.stdout, expected);
    assert!(executed.stderr.is_empty(), "{executed:?}");

    let root = directory.join("project");
    fs::create_dir_all(root.join("src/app")).unwrap();
    fs::write(
        root.join("src/app/Main.ko"),
        format!("package app\n{program}"),
    )
    .unwrap();
    let manifest = root.join("project.toml");
    fs::write(&manifest, "schema = \"koven.project\"\nversion = 1\n[project]\nname = \"text-bytes\"\nsource-roots = [\"src\"]\n").unwrap();
    let checked = run([
        OsStr::new("check"),
        OsStr::new("--project"),
        manifest.as_os_str(),
    ]);
    assert!(checked.status.success(), "{checked:?}");
    let executable = directory.join("project-text");
    let built = run([
        OsStr::new("build"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.main"),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert!(built.status.success(), "{built:?}");
    let direct = Command::new(&executable).output().unwrap();
    assert!(direct.status.success(), "{direct:?}");
    assert_eq!(direct.stdout, expected);
    assert!(direct.stderr.is_empty(), "{direct:?}");
    let executed = run([
        OsStr::new("run"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.main"),
    ]);
    assert!(executed.status.success(), "{executed:?}");
    assert_eq!(executed.stdout, expected);
    assert!(executed.stderr.is_empty(), "{executed:?}");
}

#[test]
fn public_text_extension_requires_import_and_storage_intrinsics_require_host_authority() {
    for (name, text, code) in [
        (
            "unimported",
            "fun main(): Unit { val length = \"x\".byteLength() }",
            "L0080",
        ),
        (
            "intrinsic",
            "fun main(): Unit { val length = textByteLength(\"x\") }",
            "L0164",
        ),
    ] {
        let directory = TestDirectory::create();
        let source = directory.join(&format!("{name}.ko"));
        fs::write(&source, text).unwrap();
        let output = run([
            OsStr::new("--message-format=json"),
            OsStr::new("check"),
            source.as_os_str(),
        ]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let errors = String::from_utf8(output.stderr).unwrap();
        let records = errors
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert!(
            records.iter().any(|record| record["code"] == code),
            "{records:?}"
        );
    }
}

#[test]
fn public_text_field_receiver_allows_disjoint_sibling_replace() {
    check_build_run_both_modes(
        r#"class Holder(val text: String, var other: String)
fun main(): Unit {
    val holder = Holder("field".clone(), "old".clone())
    val joined = holder.text.checkedPlus(replace(&holder.other, "new".clone()))
    if (joined != null) { println(joined) }
    println(holder.text)
    println(holder.other)
}
"#,
        b"fieldold\nfield\nnew\n",
    );
}
