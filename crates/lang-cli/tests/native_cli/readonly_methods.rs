//! Public single-file source path: real object emission, host link, and run.
use super::*;

fn check(name: &str, text: &str, expected: &[u8]) {
    let directory = TestDirectory::create();
    let source = directory.join(&format!("{name}.ko"));
    let executable = directory.join("program");
    fs::write(&source, text).unwrap();
    let built = run([
        OsStr::new("build"),
        source.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("entry"),
        OsStr::new("-o"),
        executable.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{name}: {built:?}");
    assert!(
        built.stdout.is_empty() && built.stderr.is_empty(),
        "{name}: {built:?}"
    );
    let launched = Command::new(&executable).output().unwrap();
    assert_eq!(launched.status.code(), Some(0), "{name}: {launched:?}");
    assert_eq!(launched.stdout, expected, "{name}: {launched:?}");
    assert!(launched.stderr.is_empty(), "{name}: {launched:?}");
    let executed = run([
        OsStr::new("run"),
        source.as_os_str(),
        OsStr::new("--entry"),
        OsStr::new("entry"),
    ]);
    assert_eq!(executed.status.code(), Some(0), "{name}: {executed:?}");
    assert_eq!(executed.stdout, expected, "{name}: {executed:?}");
    assert!(executed.stderr.is_empty(), "{name}: {executed:?}");
}

#[test]
fn public_single_readonly_methods_literal_and_constant_use_real_fields() {
    for (name, fixture, expected) in [
        (
            "int",
            include_str!("../../../lang-codegen/tests/fixtures/readonly_methods/int.ko"),
            &b"seven\n"[..],
        ),
        (
            "bool",
            include_str!("../../../lang-codegen/tests/fixtures/readonly_methods/bool.ko"),
            &b"enabled\n"[..],
        ),
    ] {
        check(name, fixture, expected);
        let text = format!(
            "const val SEVEN:Int=7\nconst val ENABLED:Boolean=true\n{}",
            fixture
                .replace("Reading(7)", "Reading(SEVEN)")
                .replace("Switch(true)", "Switch(ENABLED)")
        );
        check(&format!("{name}-constant"), &text, expected);
    }
}

#[test]
fn public_single_readonly_methods_named_and_moveonly_sources() {
    check(
        "named",
        include_str!("../../../lang-codegen/tests/fixtures/readonly_methods/named.ko"),
        b"receiver\nsecond\nfirst\nfourteen\n",
    );
    check(
        "moveonly",
        include_str!("../../../lang-codegen/tests/fixtures/readonly_methods/moveonly.ko"),
        b"seven\nseven\nseven\n",
    );
    check(
        "moveonly-temporary",
        include_str!("../../../lang-codegen/tests/fixtures/readonly_methods/moveonly_temporary.ko"),
        b"seven\n",
    );
}
