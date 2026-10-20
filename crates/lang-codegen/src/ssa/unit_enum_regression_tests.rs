//! 两份完整原 enum 源码的普通 Unit 成功合同；全采后才统一断言。
//! 首证只运行 pipeline exact。native exact 须在双源 SSA/LLVM 真绿后独立准入。
mod queries;

use std::{
    fmt::Debug,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::{
        SourceUnitInput, SymbolKind, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{check_compilation_unit_ownership, owned_compilation_unit_view},
    parser::{Item, NameMarker, parse_file},
    source::{SourceMap, Span},
    type_checking::{
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use crate::emit_native_owned_unit_object;

struct NativeDirectory {
    path: PathBuf,
    cleaned: bool,
}

impl NativeDirectory {
    fn create() -> Result<Self, String> {
        static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "koven-unit-enum-native-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).map_err(|error| format!("native directory create: {error}"))?;
        Ok(Self {
            path,
            cleaned: false,
        })
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn cleanup(&mut self) -> Result<(), String> {
        let result = fs::remove_dir_all(&self.path)
            .map_err(|error| format!("native directory cleanup: {error}"));
        self.cleaned = result.is_ok();
        result
    }
}

impl Drop for NativeDirectory {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[derive(Clone, Copy)]
struct Case {
    id: &'static str,
    path: &'static str,
    package: &'static [&'static str],
    entry_span: (usize, usize),
    text: &'static str,
    stdout: &'static [u8],
    conditions: usize,
    expression_conditions: usize,
}

fn cases() -> [Case; 2] {
    [
        Case {
            id: "enum_payloadless",
            path: "enum-tag-identity.ko",
            package: &[],
            entry_span: (246, 251),
            text: include_str!(
                "../../tests/fixtures/unit-enum-regression-0321/enum_payloadless.ko"
            ),
            stdout: include_bytes!(
                "../../tests/fixtures/unit-enum-regression-0321/enum_payloadless.stdout"
            ),
            conditions: 4,
            expression_conditions: 4,
        },
        Case {
            id: "enum_payload_guide",
            path: "demo/enums/litmus4.ko",
            package: &["demo", "enums"],
            entry_span: (302, 307),
            text: include_str!(
                "../../tests/fixtures/unit-enum-regression-0321/enum_payload_guide.ko"
            ),
            stdout: include_bytes!(
                "../../tests/fixtures/unit-enum-regression-0321/enum_payload_guide.stdout"
            ),
            conditions: 3,
            expression_conditions: 1,
        },
    ]
}

fn require(ok: bool, stage: &str, detail: impl Debug) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(format!("{stage}: {detail:?}"))
    }
}

fn at(sources: &SourceMap, stage: &str, error: impl Debug, span: Option<Span>) -> String {
    let source = span.map(|span| {
        (
            span,
            sources.source_name(span.source_id()),
            sources.slice(span),
        )
    });
    format!("{stage}: {error:?}; original_source={source:?}")
}

fn clean(sources: &SourceMap, stage: &str, diagnostics: &[Diagnostic]) -> Result<(), String> {
    require(
        diagnostics.is_empty(),
        stage,
        diagnostics
            .iter()
            .map(|diagnostic| at(sources, stage, diagnostic, Some(diagnostic.primary_span())))
            .collect::<Vec<_>>(),
    )
}

/// 单一原 source；不补 package/wrapper、不重选 entry、不制造或替换任何事实。
/// 现有 Map helper 锁定默认包/provider.ko，旧 unit analyze 又没有 complete 准入；
/// 此处仅按已有公开 API 顺序做 Result 适配，保证首份失败不遮住第二份。
fn observe(case: Case, native: bool, stages: &mut Vec<&'static str>) -> Result<(), String> {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(case.path, case.text)
        .map_err(|error| format!("source: {error:?}"))?;
    let lexed = lex(&sources, source).map_err(|error| format!("lex: {error:?}"))?;
    clean(&sources, "lex diagnostics", lexed.diagnostics())?;
    stages.push("lex:ok");
    let parsed = parse_file(&sources, &lexed).map_err(|error| format!("parse: {error:?}"))?;
    clean(&sources, "parse diagnostics", parsed.diagnostics())?;
    require(
        parsed.source_id() == source && sources.source_text(source) == Ok(case.text),
        "original source bytes",
        case.path,
    )?;
    stages.push("parse:ok");
    let inputs = [SourceUnitInput::new("root", case.path, source, &parsed)];
    let (name_environment, environment) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs)
        .map_err(|error| format!("unit index: {error:?}"))?;
    clean(&sources, "index diagnostics", index.diagnostics())?;
    stages.push("index:ok");
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .map_err(|error| format!("unit names: {error:?}"))?;
    clean(&sources, "names diagnostics", names.diagnostics())?;
    let names = names
        .validate()
        .map_err(|error| format!("names validate: {error:?}"))?;
    stages.push("names validate:ok");
    let [source_unit] = names.names().index().source_units() else {
        return Err("expected exactly one original source unit".to_owned());
    };
    require(
        source_unit.source_id() == source
            && source_unit.key().root().expect("rooted source").as_str() == "root"
            && source_unit
                .key()
                .logical_path()
                .expect("rooted source")
                .as_str()
                == case.path,
        "source-unit identity",
        source_unit,
    )?;
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.name() == "entry" && declaration.kind() == SymbolKind::Function
            && declaration.source_unit() == source_unit.id()
            && declaration.package() == source_unit.package()
            && declaration.name_span().source_id() == source
            && (declaration.name_span().start(), declaration.name_span().end()) == case.entry_span
            && names.names().index().packages().get(declaration.package().index())
                .is_some_and(|package| package.name().expect("named package").segments().iter().map(String::as_str)
                    .eq(case.package.iter().copied()))
            && parsed.roots().contains(&declaration.root())
            && parsed.ast().items().get(declaration.root()).is_ok_and(|node| {
                matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                    if *name == declaration.name_span())
            })
        })
        .collect::<Vec<_>>();
    let [entry] = entries.as_slice() else {
        return Err(format!("original driver entry selection: {entries:?}"));
    };
    stages.push("original driver entry:ok");
    let raw = check_compilation_unit_types(&sources, &inputs, &names, &environment)
        .map_err(|error| format!("unit raw types: {error:?}"))?;
    clean(&sources, "raw type diagnostics", raw.diagnostics())?;
    {
        let complete =
            complete_compilation_unit_types(&sources, &inputs, &names, &environment, &raw)
                .map_err(|error| format!("complete type admission: {error:?}"))?;
        require(
            complete.is_same_analysis(&raw)
                && std::ptr::eq(complete.types(), &raw)
                && raw.is_compatible_with(&sources, &inputs, &names, &environment),
            "complete original analysis identity",
            case.id,
        )?;
    }
    stages.push("complete type admission:ok");
    let typed = raw
        .validate()
        .map_err(|error| format!("basic type validate: {error:?}"))?;
    stages.push("basic type validate:ok");
    let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &environment, &typed)
        .map_err(|error| format!("unit ownership: {error:?}"))?;
    clean(&sources, "ownership diagnostics", owned.diagnostics())?;
    require(
        owned.deferred().is_empty(),
        "ownership deferred",
        owned.deferred(),
    )?;
    require(
        owned.is_compatible_with(&typed),
        "ownership original analysis identity",
        case.id,
    )?;
    let owned = owned
        .validate()
        .map_err(|error| format!("ownership validate: {error:?}"))?;
    stages.push("ownership validate:ok");
    let view = owned_compilation_unit_view(&sources, &inputs, &names, &environment, &typed, &owned)
        .map_err(|error| format!("sealed owned view: {error:?}"))?;
    require(
        std::ptr::eq(view.sources(), &sources)
            && std::ptr::eq(view.inputs(), inputs.as_slice())
            && std::ptr::eq(view.names(), &names)
            && std::ptr::eq(view.types(), typed.types())
            && std::ptr::eq(view.ownership(), owned.ownership()),
        "sealed view retains original references",
        case.id,
    )?;
    stages.push("sealed owned view:ok");
    if native {
        // Same ordinary object/link/run flow as native_tests::emit_link_and_run;
        // this entry uses the original ordinary owned-unit view, never a const adapter.
        let mut directory = NativeDirectory::create()?;
        let result: Result<(), String> = (|| {
            let object = directory.join("program.o");
            let executable = directory.join("program");
            emit_native_owned_unit_object(&view, entry.id(), &object)
                .map_err(|error| at(&sources, "public native object", &error, error.span()))?;
            stages.push("public native object:ok");
            let linked = Command::new(crate::test_support::clang())
                .arg(&object)
                .arg("-o")
                .arg(&executable)
                .output()
                .map_err(|error| format!("link launch: {error:?}"))?;
            require(linked.status.success(), "link status", &linked)?;
            stages.push("link:ok");
            let run = Command::new(&executable)
                .output()
                .map_err(|error| format!("run launch: {error:?}"))?;
            require(
                run.status.code() == Some(0) && run.stdout == case.stdout && run.stderr.is_empty(),
                "exit=0, exact original stdout, empty stderr",
                run,
            )?;
            stages.push("native golden:ok");
            Ok(())
        })();
        let cleanup = directory.cleanup();
        match (result, cleanup) {
            (Ok(()), Ok(())) => stages.push("native cleanup:ok"),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => return Err(error),
            (Err(error), Err(cleanup)) => return Err(format!("{error}; {cleanup}")),
        }
    } else {
        stages.push("ordinary unit lower:entered");
        let (program, driver) =
            super::unit_lower::lower_owned_unit_with_entry(&view, entry.id())
                .map_err(|error| at(&sources, "ordinary unit lower", error, error.span))?;
        stages.push("ordinary unit lower:ok");
        super::verify::verify_program(&program)
            .map_err(|error| format!("SSA verifier: {error:?}"))?;
        stages.push("SSA verifier:ok");
        crate::llvm::render_verified_program_with_entry(&program, driver)
            .map_err(|error| format!("LLVM verifier: {error:?}"))?;
        stages.push("LLVM verifier:ok");
        queries::check(
            case,
            &sources,
            &parsed,
            &names,
            typed.types(),
            &program,
            driver,
        )?;
        stages.push("source-anchored enum identity:ok");
    }
    Ok(())
}

fn collect_all(native: bool) {
    let observations = cases()
        .into_iter()
        .map(|case| {
            let mut stages = Vec::new();
            let result = observe(case, native, &mut stages);
            (case.id, case.path, case.entry_span, stages, result)
        })
        .collect::<Vec<_>>();
    // Neither stop point becomes an oracle: both original sources must succeed.
    for observation in &observations {
        println!("ENUM ORIGINAL UNIT native={native} {observation:#?}");
    }
    assert_eq!(observations.len(), 2);
    assert!(
        observations.iter().all(|row| row.4.is_ok()),
        "both complete original enum sources must succeed: {observations:#?}"
    );
}

#[test]
fn original_enum_sources_unit_pipeline_exact() {
    collect_all(false);
}

#[test]
fn original_enum_sources_unit_native_exact() {
    collect_all(true);
}
