use super::{Command, TestDirectory, assert_no_sibling_temporary, fs};
use crate::{NativeObjectErrorKind, NativeUnitEntry, emit_native_constant_unit_object};
use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    ownership_checking::check_compilation_unit_constant_ownership,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, standard_environments},
};

#[path = "unit_constant_owner_tests.rs"]
mod owners;

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

#[test]
fn all_constant_types_run_across_files_and_namespaces() {
    let values = [
        ("Boolean", "true"),
        ("Byte", "127"),
        ("Short", "32767"),
        ("Int", "2147483647"),
        ("Long", "9223372036854775807L"),
        ("UByte", "255u"),
        ("UShort", "65535u"),
        ("UInt", "4294967295u"),
        ("ULong", "18446744073709551615uL"),
        ("Char", "'文'"),
        ("String", "\"中\\0文\""),
    ];
    let declarations = values
        .iter()
        .enumerate()
        .map(|(index, (ty, literal))| format!("const val V{index}: {ty} = {literal}\n"))
        .collect::<String>();
    for (prefix, root) in [
        ("", declarations.clone()),
        ("Config.", format!("object Config {{ {declarations} }}")),
        (
            "Config.",
            format!("class Config {{ companion object {{ {declarations} }} }}"),
        ),
        (
            "Config.",
            format!(
                "value class Config(val item: Int) {{ companion object {{ {declarations} }} }}"
            ),
        ),
        (
            "Config.",
            format!("interface Config {{ companion object {{ {declarations} }} }}"),
        ),
        (
            "Config.",
            format!("enum class Config {{ One; companion object {{ {declarations} }} }}"),
        ),
    ] {
        let imports = if prefix.is_empty() {
            (0..values.len())
                .map(|index| format!("import p.V{index}\n"))
                .collect()
        } else {
            "import p.Config\n".to_string()
        };
        let checks = values.iter().enumerate().map(|(index, (ty, literal))| {
            let literal = if *ty == "Char" { "EXPECTED_CHAR" } else { literal };
            format!(
            "val expected{index}: {ty} = {literal}\nif ({prefix}V{index} == expected{index} && p.{prefix}V{index} == expected{index}) {{ println(\"{index}\") }}\n"
        )}).collect::<String>();
        let consumer = format!(
            "package q\n{imports}\nconst val EXPECTED_CHAR: Char = '文'\nconst val OTHER_CHAR: Char = '中'\nconst val CHAIN: Int = p.{prefix}V3 - 2147483605\nfun entry(): Unit {{ {checks}if ({prefix}V9 != OTHER_CHAR && CHAIN == 42) {{ println(\"chain\") }}\nprintln({prefix}V10) }}"
        );
        let run = run_constant_sources(&format!("package p\n{root}"), &consumer);
        assert!(run.status.success(), "{root}: {run:?}");
        assert_eq!(
            run.stdout,
            "0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\nchain\n中\0文\n".as_bytes(),
            "{root}"
        );
        assert!(run.stderr.is_empty(), "{run:?}");
    }
}

fn run_constant_sources(provider_text: &str, consumer_text: &str) -> std::process::Output {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = super::parsed(&mut sources, "p/provider.ko", provider_text);
    let (consumer_source, consumer) = super::parsed(&mut sources, "q/consumer.ko", consumer_text);
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
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .unwrap()
        .validate_constants()
        .unwrap();
    let owned =
        check_compilation_unit_constant_ownership(&sources, &inputs, &names, &environment, &typed)
            .unwrap()
            .validate()
            .unwrap();
    let entry = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == "entry")
        .unwrap()
        .id();
    let directory = TestDirectory::create();
    let object = directory.join("matrix.o");
    emit_native_constant_unit_object(
        &sources,
        &inputs,
        &names,
        &environment,
        &typed,
        &owned,
        entry,
        &object,
    )
    .unwrap();
    let executable = directory.join("matrix");
    let linked = Command::new("/usr/bin/clang")
        .arg(&object)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{linked:?}");
    Command::new(&executable).output().unwrap()
}
