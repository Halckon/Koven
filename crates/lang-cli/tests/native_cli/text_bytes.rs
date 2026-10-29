//! Both public CLI modes must load the real trusted text source and execute its owned results.
use super::*;

const PROGRAM: &str = r#"import koven.text.byteLength
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
fun leftText(): String { println("left"); return "x".clone() }
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
    let directory = TestDirectory::create();
    let source = directory.join("text.ko");
    fs::write(&source, PROGRAM).unwrap();
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
    assert_eq!(direct.stdout, EXPECTED);
    assert!(direct.stderr.is_empty(), "{direct:?}");
    let executed = run([OsStr::new("run"), source.as_os_str()]);
    assert!(executed.status.success(), "{executed:?}");
    assert_eq!(executed.stdout, EXPECTED);
    assert!(executed.stderr.is_empty(), "{executed:?}");

    let root = directory.join("project");
    fs::create_dir_all(root.join("src/app")).unwrap();
    fs::write(
        root.join("src/app/Main.ko"),
        format!("package app\n{PROGRAM}"),
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
    assert_eq!(direct.stdout, EXPECTED);
    assert!(direct.stderr.is_empty(), "{direct:?}");
    let executed = run([
        OsStr::new("run"),
        OsStr::new("--project"),
        manifest.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("app.main"),
    ]);
    assert!(executed.status.success(), "{executed:?}");
    assert_eq!(executed.stdout, EXPECTED);
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
