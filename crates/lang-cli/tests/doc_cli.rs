//! API extraction is syntax-only, stable, non-mutating and never runs input.
use std::{fs, process::Command};

#[test]
fn doc_preserves_declaration_modes_ranges_and_explicit_omissions() {
    let dir = std::env::temp_dir().join(format!("koven-doc-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("input.ko");
    let source = "// 中文\nclass Cell(val text: String)\nfun inspect(cell: Cell, own n: Int, inout x: Int): Unit { missing() }\n";
    fs::write(&path, source).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_kovenc"))
            .arg("doc")
            .arg(&path)
            .output()
            .unwrap()
    };
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(output.stdout, run().stdout);
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(doc["validation"], "syntax-only");
    assert_eq!(doc["guideVersion"], "0.45");
    assert_eq!(doc["functions"][0]["name"], "inspect");
    assert_eq!(
        doc["functions"][0]["location"]["startByte"],
        source.find("fun inspect").unwrap()
    );
    assert_eq!(
        doc["functions"][0]["signature"],
        "fun inspect(cell: Cell, own n: Int, inout x: Int): Unit"
    );
    assert_eq!(doc["functions"][0]["parameters"][0]["type"], "Cell");
    assert_eq!(doc["functions"][0]["location"]["line"], 3);
    assert_eq!(doc["functions"][0]["parameters"][0]["mode"], "default");
    assert_eq!(doc["functions"][0]["parameters"][1]["mode"], "own");
    assert_eq!(doc["functions"][0]["parameters"][2]["mode"], "inout");
    assert_eq!(doc["omitted"].as_array().unwrap().len(), 1);
    assert!(!String::from_utf8_lossy(&output.stdout).contains(dir.to_str().unwrap()));
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    fs::write(
        &path,
        "fun echo(borrow text: String): String = text.clone()\nfun empty() {}\n",
    )
    .unwrap();
    let explicit: serde_json::Value = serde_json::from_slice(&run().stdout).unwrap();
    assert_eq!(explicit["functions"][0]["parameters"][0]["mode"], "borrow");
    assert_eq!(
        explicit["functions"][0]["signature"],
        "fun echo(borrow text: String): String"
    );
    assert_eq!(explicit["functions"][1]["signature"], "fun empty()");
    fs::write(
        &path,
        "private fun <T> keep(x: T): T = x\nfun String.extra(): Unit {}\n",
    )
    .unwrap();
    let generic: serde_json::Value = serde_json::from_slice(&run().stdout).unwrap();
    assert_eq!(
        generic["functions"][0]["signature"],
        "private fun <T> keep(x: T): T"
    );
    assert_eq!(generic["omitted"].as_array().unwrap().len(), 1);
    fs::write(&path, "fun broken(").unwrap();
    let bad = Command::new(env!("CARGO_BIN_EXE_kovenc"))
        .args(["--message-format=json", "doc"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(bad.status.code(), Some(2));
    assert!(bad.stdout.is_empty());
    assert!(!bad.stderr.is_empty());
    for line in String::from_utf8(bad.stderr).unwrap().lines() {
        let _: serde_json::Value = serde_json::from_str(line).unwrap();
    }
    fs::remove_dir_all(dir).unwrap();
}
