//! 普通 borrow 结果签名的解析与尚未接通来源事实时的闭合门。

use lang_frontend::{
    analysis::{SingleFileAnalysisError, SingleFileStage, analyze_single_file},
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    parser::{BorrowReturnSource, FunctionForm, Item, NameMarker, TypeRef, parse_file},
    source::SourceMap,
    type_checking::{
        check_compilation_unit_types, check_types, collect_compilation_unit_signatures,
        standard_environments,
    },
};

fn assert_frontier(text: &str, code: &str, marker: &str) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("signature.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
    let (names, types) = standard_environments();
    let single_names = resolve_names(&sources, &file, &names).unwrap();
    assert!(single_names.diagnostics().is_empty());
    let typed = check_types(&sources, &file, &single_names, &types).unwrap();
    let inputs = [SourceUnitInput::new("root", "signature.ko", source, &file)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let unit_names = resolve_compilation_unit_names(&sources, &inputs, &index, &names)
        .unwrap()
        .validate()
        .unwrap();
    let signatures =
        collect_compilation_unit_signatures(&sources, &inputs, &unit_names, &types).unwrap();
    let unit = check_compilation_unit_types(&sources, &inputs, &unit_names, &types).unwrap();
    for diagnostics in [
        typed.diagnostics(),
        signatures.diagnostics(),
        unit.diagnostics(),
    ] {
        assert_eq!(diagnostics.len(), 1, "{text}: {diagnostics:?}");
        assert_eq!(diagnostics[0].code().to_string(), code);
        assert_eq!(
            sources.slice(diagnostics[0].primary_span()).unwrap(),
            marker
        );
    }
    assert!(signatures.validate().is_err());
    assert!(unit.validate().is_err());
    let mut stages = Vec::new();
    let analysis = analyze_single_file::<(), _>(
        &sources,
        source,
        &names,
        &types,
        |stage, diagnostics| {
            stages.push(stage);
            if diagnostics.is_empty() {
                Ok(())
            } else {
                Err(stage)
            }
        },
        |_| panic!("rejected signature must not reach the typed observer"),
    );
    assert!(matches!(
        analysis,
        Err(SingleFileAnalysisError::Host(SingleFileStage::TypeChecking))
    ));
    assert_eq!(stages.last(), Some(&SingleFileStage::TypeChecking));
}

#[test]
fn result_markers_preserve_real_spans_and_keep_borrow_out_of_type_refs() {
    let text = "fun view(source: String?): borrow String? from source = source";
    let mut sources = SourceMap::new();
    let source = sources.add_source("signature.ko", text).unwrap();
    let lexed = lex(&sources, source).unwrap();
    let file = parse_file(&sources, &lexed).unwrap();
    let repeated = parse_file(&sources, &lexed).unwrap();
    assert_eq!(format!("{:?}", file.ast()), format!("{:?}", repeated.ast()));
    assert_eq!(file.diagnostics(), repeated.diagnostics());
    assert!(file.diagnostics().is_empty());
    let Item::Function {
        form:
            FunctionForm::Explicit {
                type_ref,
                borrow_return: Some(syntax),
                ..
            },
        ..
    } = file.ast().items().get(file.roots()[0]).unwrap().payload()
    else {
        panic!("ordinary borrow result AST");
    };
    assert_eq!(sources.slice(syntax.borrow_span).unwrap(), "borrow");
    assert_eq!(sources.slice(syntax.from_span).unwrap(), "from");
    let BorrowReturnSource::Parameter(NameMarker::Present(origin)) = syntax.source else {
        panic!("named parameter source");
    };
    assert_eq!((origin.start(), origin.end()), (47, 53));
    assert_eq!(sources.slice(origin).unwrap(), "source");
    let target = file.ast().type_refs().get(*type_ref).unwrap();
    assert_eq!(sources.slice(target.span()).unwrap(), "String?");
    assert!(matches!(
        target.payload(),
        TypeRef::Qualified {
            nullable_span: Some(_),
            ..
        }
    ));
}

#[test]
fn missing_from_recovers_with_empty_marker_and_preserves_next_function() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "signature.ko",
            "fun view(source: String): borrow String = source\nfun after() {}",
        )
        .unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert_eq!(file.diagnostics().len(), 1);
    assert_eq!(file.diagnostics()[0].code().to_string(), "L0162");
    assert!(file.diagnostics()[0].primary_span().is_empty());
    let Item::Function {
        form:
            FunctionForm::Explicit {
                borrow_return: Some(syntax),
                ..
            },
        ..
    } = file.ast().items().get(file.roots()[0]).unwrap().payload()
    else {
        panic!("recovered result");
    };
    assert!(syntax.from_span.is_empty());
    assert!(
        matches!(syntax.source, BorrowReturnSource::Parameter(NameMarker::Missing(span)) if span.is_empty())
    );
    assert_eq!(file.roots().len(), 2);
}

#[test]
fn unsupported_conditional_nested_and_standalone_borrow_types_stay_rejected() {
    for text in [
        "fun view(source: String): borrow? String from source = source",
        "fun view(source: String): List<borrow String> = [source]",
        "val item: borrow String = \"x\"",
    ] {
        let mut sources = SourceMap::new();
        let source = sources.add_source("signature.ko", text).unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(!file.diagnostics().is_empty(), "{text}");
    }
}

#[test]
fn contextual_names_and_default_owned_signatures_keep_their_existing_behavior() {
    let text = "fun view(source: String): String = source\nfun run() { val borrow = \"x\"; val from = borrow; println(from) }";
    let mut sources = SourceMap::new();
    let source = sources.add_source("signature.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty());
    let (names, types) = standard_environments();
    let resolved = resolve_names(&sources, &file, &names).unwrap();
    assert!(resolved.diagnostics().is_empty());
    assert!(
        check_types(&sources, &file, &resolved, &types)
            .unwrap()
            .diagnostics()
            .is_empty()
    );
    for (_, node) in file.ast().items().iter() {
        if let Item::Function {
            form: FunctionForm::Explicit { borrow_return, .. },
            ..
        } = node.payload()
        {
            assert!(borrow_return.is_none());
        }
    }
}

#[test]
fn valid_non_owning_signature_stays_closed_without_origin_and_continuation() {
    for text in [
        "fun view(source: String): borrow String from source = source",
        "fun view(source: String?): borrow String? from source = source",
        "fun view(inout source: String): borrow String from source = source",
        "fun view(aux: Int, borrow source: String): borrow String from source = source",
        "class Record(val text: String) { fun view(): borrow String from this = this.text }",
    ] {
        assert_frontier(text, "L0164", "borrow");
    }
}

#[test]
fn invalid_signature_source_is_rejected_before_the_unsupported_path_gate() {
    for (text, marker) in [
        (
            "fun view(own source: String): borrow String from source = source",
            "source",
        ),
        (
            "fun view(source: String): borrow String from missing = source",
            "missing",
        ),
        (
            "fun view(source: String): borrow String from this = source",
            "this",
        ),
        (
            "class Record(val text: String) { own fun view(): borrow String from this = this.text }",
            "this",
        ),
    ] {
        assert_frontier(text, "L0162", marker);
    }
}
