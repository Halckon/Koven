//! 独立内存文档输入：真实来源、文件头恢复与共同 Unit 名称事实。

use lang_frontend::{
    analysis::{UnitSourceDescriptor, analyze_parsed_unit_names},
    diagnostic::{Diagnostic, Severity, codes},
    lexer::lex,
    name_resolution::{
        CompilationUnitInputError, PackageIdentity, SourceUnitInput, SourceUnitInputKey,
        SourceUnitKey, UnitReferenceTarget, index_compilation_unit, resolve_compilation_unit_names,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::standard_environments,
};

fn parsed(sources: &mut SourceMap, locator: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(locator, text).expect("ordinary source");
    let lexed = lex(sources, source).expect("real lexer");
    let parsed = parse_file(sources, &lexed).expect("real parser");
    (source, parsed)
}

fn diagnostic(
    sources: &SourceMap,
    source: SourceId,
    code: &str,
    message: &str,
    start: usize,
    end: usize,
) -> Diagnostic {
    Diagnostic::new(
        sources,
        Severity::Error,
        codes::catalog().expect("catalog").resolve(code).expect("known code"),
        message,
        sources.span(source, start, end).expect("real source range"),
    ).expect("complete expected diagnostic")
}

#[test]
fn memory_snapshot_preserves_original_source_and_key_without_locator_paths() {
    let text = "package p\nimport p.answer as localAnswer\nfun answer(): Int = 1\n\
                fun use(): Int = localAnswer()";
    for locator in ["untitled:Buffer-7", "file:///wrong/directory/F.ko", "virtual://editor/unrelated"] {
        let mut sources = SourceMap::new();
        let unselected = sources.add_source("unselected", "fun other(): Int = 0")
            .expect("real unselected source");
        let (source, parsed) = parsed(&mut sources, locator, text);
        assert_ne!(source, unselected);
        let expected_roots = parsed.roots().to_vec();
        let descriptor = UnitSourceDescriptor::memory_document(source);
        assert_eq!(descriptor.input_key(), SourceUnitInputKey::MemoryDocument);
        assert_eq!(descriptor.root_identity(), None);
        assert_eq!(descriptor.logical_path(), None);
        let (environment, _) = standard_environments();
        let snapshot = analyze_parsed_unit_names(sources, vec![(descriptor, parsed)], environment)
            .expect("memory Unit names");
        assert_eq!(snapshot.sources().source_text(source).expect("original source"), text);
        assert_eq!(snapshot.sources().source_name(source).expect("locator"), locator);
        assert_eq!(snapshot.descriptors().len(), 1);
        assert_eq!(snapshot.descriptors()[0].source_id(), source);
        assert_eq!(snapshot.parsed_files()[0].source_id(), source);
        assert_eq!(snapshot.parsed_files()[0].roots(), expected_roots);
        let inputs = snapshot.inputs();
        assert_eq!(inputs[0].key(), SourceUnitInputKey::MemoryDocument);
        assert_eq!(inputs[0].source_id(), source);
        assert!(std::ptr::eq(inputs[0].parsed(), &snapshot.parsed_files()[0]));
        let rebuilt = index_compilation_unit(snapshot.sources(), &inputs).expect("same memory key");
        assert_eq!(&rebuilt, snapshot.names().index());
        let unit = rebuilt.source_units().iter().find(|unit| unit.source_id() == source)
            .expect("selected original source");
        assert_eq!(unit.key(), &SourceUnitKey::MemoryDocument);
        assert_eq!(unit.key().root(), None);
        assert_eq!(unit.key().logical_path(), None);
        assert_eq!(rebuilt.packages()[unit.package().index()].name()
            .expect("complete declared package").segments(), ["p"]);
        assert!(snapshot.names().diagnostics().is_empty());
        assert!(snapshot.validated_names().is_some());
        let answer = rebuilt.declarations().iter().find(|item| item.name() == "answer")
            .expect("real answer declaration");
        let alias_use = text.rfind("localAnswer").expect("ordinary alias use");
        let target = snapshot.names().references().iter()
            .find(|reference| reference.span().start() == alias_use)
            .expect("published alias use").target();
        assert_eq!(target, &UnitReferenceTarget::Declaration(answer.id()));
    }
}

#[test]
fn incomplete_memory_headers_are_not_default_or_partial_packages() {
    let tail = "fun answer(): Int = 1\nfun use(): Int = answer()";
    for (header, code, message, start, end) in [
        ("package\n", "L0048", "expected package name", 8, 8),
        ("package a.\n", "L0048", "expected package name", 11, 11),
        ("package $ junk\n", "L0001", "unexpected character", 8, 9),
        ("package a.$ junk\n", "L0001", "unexpected character", 10, 11),
        ("package a.b ", "L0053", "expected file header separator", 12, 15),
    ] {
        let text = format!("{header}{tail}");
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(&mut sources, "untitled:Broken", &text);
        assert!(!parsed.initial_package_header_is_complete());
        let expected = [diagnostic(&sources, source, code, message, start, end)];
        assert_eq!(parsed.diagnostics(), expected);
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
        let index = index_compilation_unit(&sources, &inputs).expect("real memory input");
        assert_eq!(index.diagnostics(), expected);
        assert!(index.package_path_mismatches().is_empty());
        assert_eq!(index.packages().len(), 1);
        assert_eq!(index.packages()[0].name(), None);
        assert_eq!(index.packages()[0].identity(),
            &PackageIdentity::UnresolvedHeader(SourceUnitKey::MemoryDocument));
        assert_eq!(index.declarations().len(), 2);
        let (environment, _) = standard_environments();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("recover same-document names");
        assert_eq!(names.diagnostics(), expected);
        let answer = index.declarations().iter().find(|item| item.name() == "answer")
            .expect("retained sibling declaration");
        let use_offset = text.rfind("answer").expect("ordinary use");
        let reference = names.references().iter()
            .find(|reference| reference.span().start() == use_offset)
            .expect("retained ordinary reference");
        assert_eq!(reference.target(), &UnitReferenceTarget::Declaration(answer.id()));
        assert!(index.validate().is_err());
        assert!(names.validate().is_err());
    }
}

#[test]
fn absent_header_and_later_errors_keep_their_real_named_package() {
    for (text, expected_package, expected_error) in [
        ("fun answer(): Int = 1", Vec::<&str>::new(), None),
        ("package p\n#\nfun answer(): Int = 1", vec!["p"],
            Some(("L0001", "unexpected character", 10, 11))),
    ] {
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(&mut sources, "file:///does/not/match.ko", text);
        assert!(parsed.initial_package_header_is_complete());
        let expected = expected_error.map(|(code, message, start, end)| {
            diagnostic(&sources, source, code, message, start, end)
        }).into_iter().collect::<Vec<_>>();
        assert_eq!(parsed.diagnostics(), expected);
        let input = [SourceUnitInput::memory_document(source, &parsed)];
        let index = index_compilation_unit(&sources, &input).expect("memory index");
        assert_eq!(index.diagnostics(), expected);
        assert!(index.package_path_mismatches().is_empty());
        assert_eq!(index.packages()[0].name().expect("known package").segments(), expected_package);
        assert_eq!(index.validate().is_ok(), expected.is_empty());
    }
}

#[test]
fn memory_document_does_not_create_a_mixed_or_multi_document_unit() {
    let mut sources = SourceMap::new();
    let (first, a) = parsed(&mut sources, "untitled:One", "fun one(): Int = 1");
    let (second, b) = parsed(&mut sources, "untitled:Two", "fun two(): Int = 2");
    let memory = SourceUnitInput::memory_document(first, &a);
    let rooted = SourceUnitInput::new("root", "second.ko", second, &b);
    let other_memory = SourceUnitInput::memory_document(second, &b);
    for inputs in [[memory, rooted], [rooted, memory], [memory, other_memory]] {
        let error = index_compilation_unit(&sources, &inputs).expect_err("not a single document");
        assert!(matches!(&error, CompilationUnitInputError::MemoryDocumentRequiresSingleSource));
        assert_eq!(error.to_string(), "memory document requires exactly one selected source");
    }
}

#[test]
fn unresolved_memory_headers_do_not_publish_default_or_partial_import_targets() {
    for (header, imported, missing_package) in [
        ("package\n", "answer", None),
        ("package a.\n", "a.answer", Some("a")),
    ] {
        let text = format!("{header}import {imported} as imported\nfun answer(): Int = 1");
        let mut sources = SourceMap::new();
        let (source, parsed) = parsed(&mut sources, "virtual://editor/BadHeader", &text);
        let header_error = diagnostic(
            &sources, source, "L0048", "expected package name", header.len(), header.len(),
        );
        assert_eq!(parsed.diagnostics(), [header_error.clone()]);
        let inputs = [SourceUnitInput::memory_document(source, &parsed)];
        let index = index_compilation_unit(&sources, &inputs).expect("memory index");
        let (environment, _) = standard_environments();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .expect("ordinary unresolved import");
        let terminal = text.find("answer").expect("literal import terminal");
        let mut import_error = diagnostic(
            &sources, source, "L0148", "unresolved exact import target", terminal, terminal + 6,
        );
        if let Some(package) = missing_package {
            import_error.add_note(format!(
                "package '{package}' is not loaded in this compilation unit",
            )).expect("fixed import note");
            import_error.add_help(
                "include the required sources in the explicit project source set; single-file commands can use --project",
            ).expect("fixed import help");
        }
        assert_eq!(names.diagnostics(), [header_error, import_error]);
        let alias = text.find("as imported").expect("literal import alias") + 3;
        assert!(!names.references().iter().any(|reference| {
            reference.span().start() == terminal || reference.span().start() == alias
        }));
        assert!(names.validate().is_err());
    }
}
