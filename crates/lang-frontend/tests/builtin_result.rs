//! SPEC-0349 Guide 02 内建环境：宿主授权来源提供 `Result`/`Ok`/`Err`，以及无实参
//! `is Ok`/`is Err` 从 subject 推断 root 实参。

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        CompilationUnitNames, NameEnvironment, Namespace, SourceUnitInput, UnitReferenceTarget,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        TypeEnvironment, UnitTypeKind, UnitTypeRefId, check_compilation_unit_types,
        standard_environments,
    },
};

/// 与 `crates/lang-std/koven/core/result.ko` 相同的声明形状；frontend 不依赖 lang-std 路径。
const RESULT_SOURCE: &str =
    "package koven.core\nenum class Result<T, E> {\n    Ok(success: T),\n    Err(error: E)\n}\n";

struct Fixture {
    sources: SourceMap,
    files: Vec<(String, SourceId, ParsedFile)>,
    names: NameEnvironment,
    types: TypeEnvironment,
    builtin: SourceId,
}

impl Fixture {
    /// 用户源按给定逻辑路径登记，随后追加并（可选）授权内建来源。
    fn with_authority(users: &[(&str, &str)], authorize: bool) -> Self {
        let mut sources = SourceMap::new();
        let mut files = Vec::new();
        for (path, text) in users {
            let id = sources
                .add_source(*path, *text)
                .expect("unique user source");
            files.push(((*path).to_owned(), id, parse(&sources, id)));
        }
        let builtin = sources
            .add_source("koven-std/koven/core/result.ko", RESULT_SOURCE)
            .expect("unique builtin source");
        files.push((
            "koven/core/result.ko".to_owned(),
            builtin,
            parse(&sources, builtin),
        ));
        let (mut names, types) = standard_environments();
        if authorize {
            names
                .authorize_builtin_source(&sources, builtin)
                .expect("builtin source belongs to the map");
        }
        Self {
            sources,
            files,
            names,
            types,
            builtin,
        }
    }

    fn new(users: &[(&str, &str)]) -> Self {
        Self::with_authority(users, true)
    }

    fn inputs(&self) -> Vec<SourceUnitInput<'_>> {
        self.files
            .iter()
            .map(|(path, id, parsed)| {
                let root = if *id == self.builtin {
                    "koven-std"
                } else {
                    "user"
                };
                SourceUnitInput::new(root, path, *id, parsed)
            })
            .collect()
    }

    fn resolve(&self) -> CompilationUnitNames {
        let inputs = self.inputs();
        let index = index_compilation_unit(&self.sources, &inputs).expect("valid unit input");
        resolve_compilation_unit_names(&self.sources, &inputs, &index, &self.names)
            .expect("unit names resolve internally")
    }

    fn type_diagnostics(&self) -> Vec<String> {
        let names = self.resolve();
        assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
        let names = names.validate().expect("validated names");
        let inputs = self.inputs();
        let typed = check_compilation_unit_types(&self.sources, &inputs, &names, &self.types)
            .expect("unit types check internally");
        codes(typed.diagnostics())
    }

    fn reference(
        &self,
        names: &CompilationUnitNames,
        text: &str,
        namespace: Namespace,
    ) -> UnitReferenceTarget {
        names
            .references()
            .iter()
            .find(|reference| {
                reference.namespace() == Some(namespace)
                    && self.sources.slice(reference.span()) == Ok(text)
                    && reference.span().source_id() != self.builtin
            })
            .unwrap_or_else(|| panic!("reference {text} in {namespace:?}"))
            .target()
            .clone()
    }
}

fn parse(sources: &SourceMap, id: SourceId) -> ParsedFile {
    let lexed = lex(sources, id).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    parsed
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[test]
fn authorized_source_binds_result_ok_and_err_without_import() {
    let fixture = Fixture::new(&[(
        "app/Main.ko",
        "package app\nfun f(): Result<Int, String> = Ok(1)\nfun g(): Result<Int, String> = Err(\"bad\")\n",
    )]);
    let names = fixture.resolve();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let declaration = names
        .builtin_result_declaration()
        .expect("published builtin Result");
    let builtin = &names.index().declarations()[declaration.index()];
    assert_eq!(builtin.name(), "Result");
    assert_eq!(builtin.name_span().source_id(), fixture.builtin);
    assert_eq!(
        fixture.reference(&names, "Result", Namespace::Type),
        UnitReferenceTarget::Declaration(declaration)
    );
    for case in ["Ok", "Err"] {
        let UnitReferenceTarget::Symbol(symbol) = fixture.reference(&names, case, Namespace::Value)
        else {
            panic!("{case} value is a builtin case symbol");
        };
        assert_eq!(symbol.source_unit(), builtin.source_unit());
    }
    let validated = names.validate().expect("validated names");
    assert_eq!(validated.builtin_result_declaration(), Some(declaration));
    assert!(fixture.type_diagnostics().is_empty());
}

#[test]
fn qualified_result_case_path_uses_the_builtin_root() {
    let fixture = Fixture::new(&[(
        "Main.ko",
        "fun f(): Result<Int, Int> = Result.Ok(1)\nfun g(): Result<Int, Int> = Result.Err(2)\n",
    )]);
    let names = fixture.resolve();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    assert!(matches!(
        fixture.reference(&names, "Result", Namespace::Type),
        UnitReferenceTarget::Declaration(_)
    ));
    assert!(fixture.type_diagnostics().is_empty());
}

#[test]
fn locals_and_type_parameters_shadow_builtin_names_silently() {
    let fixture = Fixture::new(&[(
        "Main.ko",
        "fun local(): Int { val Ok = 5\nreturn Ok }\nfun <Result> keep(own value: Result): Result = value\n",
    )]);
    let names = fixture.resolve();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    assert!(matches!(
        fixture.reference(&names, "Ok", Namespace::Value),
        UnitReferenceTarget::Symbol(symbol) if symbol.source_unit() != names
            .index()
            .declarations()[names.builtin_result_declaration().unwrap().index()]
            .source_unit()
    ));
    assert!(fixture.type_diagnostics().is_empty());
}

#[test]
fn user_top_level_builtin_names_conflict_in_any_package() {
    for (path, text, name) in [
        (
            "app/File.ko",
            "package app\nenum class Result<T, E> { A(t: T), B(e: E) }\n",
            "Result",
        ),
        (
            "other/deep/File.ko",
            "package other.deep\nfun Ok(): Unit {}\n",
            "Ok",
        ),
        ("File.ko", "val Err = 1\n", "Err"),
        ("app/File.ko", "package app\nclass Ok\n", "Ok"),
    ] {
        let fixture = Fixture::new(&[(path, text)]);
        let names = fixture.resolve();
        assert_eq!(codes(names.diagnostics()), ["L0150"], "{text}");
        let diagnostic = &names.diagnostics()[0];
        assert_eq!(fixture.sources.slice(diagnostic.primary_span()), Ok(name));
        assert_eq!(
            diagnostic.message(),
            format!("top-level declaration `{name}` conflicts with the built-in environment name")
        );
        assert_eq!(diagnostic.details().len(), 1, "{text}");
    }
    // 值命名空间的 `Result` 不占用内建类型名，enum case 也不是顶层声明。
    let fixture = Fixture::new(&[(
        "Main.ko",
        "fun Result(): Int = 1\nenum class Res { Ok(v: Int), Err(code: Int) }\n",
    )]);
    let names = fixture.resolve();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
}

#[test]
fn without_host_authority_builtin_names_stay_unresolved() {
    let fixture = Fixture::with_authority(&[("Main.ko", "fun f(): Unit { Ok(1) }\n")], false);
    let names = fixture.resolve();
    assert_eq!(codes(names.diagnostics()), ["L0080"]);
    assert_eq!(names.builtin_result_declaration(), None);
    // 用户声明 `package koven.core` 不取得内建身份：只有宿主授权的 SourceId 生效。
    let fixture = Fixture::with_authority(
        &[(
            "koven/core/Forged.ko",
            "package koven.core\nfun forged(): Unit { Err(1) }\n",
        )],
        false,
    );
    assert_eq!(codes(fixture.resolve().diagnostics()), ["L0080"]);
}

#[test]
fn bare_case_type_tests_infer_root_arguments_from_the_subject() {
    let fixture = Fixture::new(&[(
        "Main.ko",
        "fun score(result: Result<Int, String>): Int = when (result) {\n    is Ok -> result.success\n    is Err -> 0\n}\n\
         fun failed(result: Result<Int, String>?): Boolean = result is Err\n\
         fun flag(result: Result<Int, String>): Int { if (result is Ok) { return result.success }\nreturn 0 }\n\
         fun explicit(result: Result<Int, String>): Int = when (result) {\n    is Ok<Int, String> -> 1\n    is Err<Int, String> -> 0\n}\n",
    )]);
    assert!(fixture.type_diagnostics().is_empty());

    let names = fixture.resolve().validate().expect("validated names");
    let inputs = fixture.inputs();
    let typed = check_compilation_unit_types(&fixture.sources, &inputs, &names, &fixture.types)
        .expect("unit types check internally");
    let user = names
        .names()
        .index()
        .source_units()
        .iter()
        .find(|unit| unit.source_id() != fixture.builtin)
        .expect("user unit")
        .id();
    let parsed = &fixture.files[0].2;
    let mut inferred = 0;
    for (id, node) in parsed.ast().type_refs().iter() {
        let text = fixture.sources.slice(node.span()).expect("type ref text");
        if text != "Ok" && text != "Err" {
            continue;
        }
        let ty = typed
            .type_ref_type(UnitTypeRefId::new(user, id))
            .expect("type-test target is typed");
        let Some(UnitTypeKind::EnumCase { root, .. }) = typed.types().get(ty) else {
            panic!("{text} is a case type");
        };
        let Some(UnitTypeKind::Nominal { arguments, .. }) = typed.types().get(*root) else {
            panic!("{text} root is nominal");
        };
        assert!(
            arguments.iter().all(|argument| !matches!(
                typed.types().get(*argument),
                Some(UnitTypeKind::TypeParameter(_))
            )),
            "{text} uses the subject's arguments, not the enum's own parameters"
        );
        inferred += 1;
    }
    assert_eq!(inferred, 4);
}

#[test]
fn bare_case_type_test_without_an_enum_subject_is_an_arity_error() {
    let fixture = Fixture::new(&[("Main.ko", "fun probe(value: Int): Boolean = value is Ok\n")]);
    let diagnostics = fixture.type_diagnostics();
    assert!(!diagnostics.is_empty());
    assert!(
        diagnostics.iter().all(|code| code != "L0080"),
        "{diagnostics:?}"
    );
}
