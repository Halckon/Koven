use super::{Command, TestDirectory, assert_no_sibling_temporary, fs};
use crate::{NativeObjectErrorKind, NativeUnitEntry, emit_native_constant_unit_object};
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[test]
fn constant_object_runs_deterministically_and_preserves_output_on_failure() {
    use super::parsed;
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\nconst val TEXT = \"界\"\nconst val FLAG = true",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\nimport p.TEXT\nfun entry(): Unit { println(TEXT + p.TEXT) }\nfun argvEntry(args: Array<String>): Unit { println(TEXT) }\nfun <T> generic(): Unit {}\nfun invalid(): String = TEXT\nfun interpolated(): Unit { println(\"${p.TEXT}\") }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .unwrap()
        .validate()
        .unwrap();
    let declaration = |name: &str| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| declaration.name() == name)
            .unwrap()
            .id()
    };
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .unwrap()
            .validate()
            .unwrap();
    let directory = TestDirectory::create();
    let object = directory.join("program.o");
    fs::write(&object, b"previous object").unwrap();
    for (entry, expected) in [
        (NativeUnitEntry::NoArguments(declaration("entry")), "界界\n"),
        (
            NativeUnitEntry::BorrowedArguments(declaration("argvEntry")),
            "界\n",
        ),
    ] {
        let mut first = None;
        for ordered in [inputs, [inputs[1], inputs[0]], inputs] {
            emit_native_constant_unit_object(
                &sources,
                &ordered,
                &names,
                &environment,
                &typed,
                &owned,
                entry,
                &object,
            )
            .unwrap();
            let bytes = fs::read(&object).unwrap();
            if let Some(first) = &first {
                assert_eq!(&bytes, first);
            } else {
                first = Some(bytes);
            }
            assert_no_sibling_temporary(&directory.0);
        }
        let executable = directory.join("program");
        let linked = Command::new("/usr/bin/clang")
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(linked.status.success(), "{linked:?}");
        let run = Command::new(&executable).arg("参数").output().unwrap();
        assert!(run.status.success(), "{run:?}");
        assert_eq!(run.stdout, expected.as_bytes());
    }
    let original = fs::read(&object).unwrap();
    for (name, expected) in [
        ("generic", NativeObjectErrorKind::InvalidEntry),
        ("invalid", NativeObjectErrorKind::InvalidEntry),
        ("interpolated", NativeObjectErrorKind::UnsupportedSource),
    ] {
        assert_eq!(
            emit_native_constant_unit_object(
                &sources,
                &inputs,
                &names,
                &environment,
                &typed,
                &owned,
                declaration(name),
                &object
            )
            .unwrap_err()
            .kind(),
            expected
        );
        assert_eq!(fs::read(&object).unwrap(), original);
        assert_no_sibling_temporary(&directory.0);
    }
    let foreign = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    assert_eq!(
        emit_native_constant_unit_object(
            &sources,
            &inputs,
            &names,
            &environment,
            &foreign,
            &owned,
            declaration("entry"),
            &object
        )
        .unwrap_err()
        .kind(),
        NativeObjectErrorKind::MismatchedAnalysis
    );
    assert_eq!(fs::read(&object).unwrap(), original);
    assert_no_sibling_temporary(&directory.0);
}
