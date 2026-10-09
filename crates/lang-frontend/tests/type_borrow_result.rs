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
        BorrowReturnOrigin, TypeKind, check_compilation_unit_types, check_types,
        collect_compilation_unit_signatures, standard_environments,
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
fn callable_contract_keeps_parameter_identity_and_owned_default_in_generic_declarations() {
    let text = "fun <T> view(aux: Int, source: T): borrow T from source = source\nfun <T> owned(own source: T): T = source";
    let mut sources = SourceMap::new();
    let source = sources.add_source("signature.ko", text).unwrap();
    let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(file.diagnostics().is_empty());
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &file, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &file, &names, &types).unwrap();
    assert_eq!(typed.diagnostics().len(), 1);
    assert_eq!(typed.diagnostics()[0].code().to_string(), "L0164");
    let view = typed
        .callables()
        .iter()
        .find(|c| c.borrow_return().is_some())
        .unwrap();
    let contract = view.borrow_return().unwrap();
    assert_eq!(contract.origin(), BorrowReturnOrigin::Parameter(1));
    assert_eq!(contract.source_span().source_id(), source);
    assert_eq!(sources.slice(contract.source_span()).unwrap(), "source");
    let parameter = view.parameter_symbols()[1].unwrap();
    assert_eq!(
        sources
            .slice(names.symbols()[parameter.index()].span())
            .unwrap(),
        "source"
    );
    assert!(
        matches!(typed.types().get(view.return_type()), Some(TypeKind::TypeParameter(symbol)) if *symbol == view.type_parameters()[0])
    );
    let owned = typed
        .callables()
        .iter()
        .find(|c| {
            sources
                .slice(names.symbols()[c.symbol().index()].span())
                .unwrap()
                == "owned"
        })
        .unwrap();
    assert!(owned.borrow_return().is_none());
    assert_eq!(
        owned.parameters()[0].mode,
        lang_frontend::type_checking::ParameterMode::Value
    );
}

#[test]
fn unit_callable_contract_uses_qualified_symbols_and_canonical_declaration_spans() {
    use lang_frontend::type_checking::{ParameterMode, UnitTypeKind};
    let mut sources = SourceMap::new();
    let a = sources
        .add_source(
            "a.ko",
            "package alpha\nfun <T> view(aux: Int, source: T): borrow T from source = source",
        )
        .unwrap();
    let b = sources.add_source("b.ko", "package beta\nfun <T> view(source: T): borrow T from source = source\nfun owned(own source: String): String = source").unwrap();
    let files = [
        parse_file(&sources, &lex(&sources, a).unwrap()).unwrap(),
        parse_file(&sources, &lex(&sources, b).unwrap()).unwrap(),
    ];
    assert!(files.iter().all(|f| f.diagnostics().is_empty()));
    let forward = [
        SourceUnitInput::new("root", "alpha/a.ko", a, &files[0]),
        SourceUnitInput::new("root", "beta/b.ko", b, &files[1]),
    ];
    let reverse = [forward[1], forward[0]];
    let (environment, types) = standard_environments();
    let mut observed = Vec::new();
    for inputs in [&forward, &reverse] {
        let index = index_compilation_unit(&sources, inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let signatures =
            collect_compilation_unit_signatures(&sources, inputs, &names, &types).unwrap();
        assert_eq!(signatures.diagnostics().len(), 2);
        assert!(
            signatures
                .diagnostics()
                .iter()
                .all(|d| d.code().to_string() == "L0164")
        );
        let mut origins = Vec::new();
        for declaration in signatures.declarations() {
            let Some(callable) = declaration.callable() else {
                continue;
            };
            if let Some(contract) = callable.borrow_return() {
                let BorrowReturnOrigin::Parameter(index) = contract.origin() else {
                    panic!("parameter contract")
                };
                let symbol = callable.parameters()[index].symbol().unwrap();
                let source =
                    names.names().index().source_units()[symbol.source_unit().index()].source_id();
                assert_eq!(contract.source_span().source_id(), source);
                assert_eq!(contract.marker_span().source_id(), source);
                assert_eq!(sources.slice(contract.source_span()).unwrap(), "source");
                assert_eq!(callable.parameters()[index].mode(), ParameterMode::Borrow);
                assert!(
                    matches!(signatures.types().get(callable.return_type()), Some(UnitTypeKind::TypeParameter(symbol)) if *symbol == callable.type_parameters()[0])
                );
                origins.push((symbol, contract));
            } else {
                assert_eq!(callable.name(), "owned");
                assert_eq!(callable.parameters()[0].mode(), ParameterMode::Value);
            }
        }
        assert_eq!(origins.len(), 2);
        assert_ne!(origins[0].0.source_unit(), origins[1].0.source_unit());
        observed.push(origins);
        assert!(signatures.validate().is_err());
    }
    assert_eq!(observed[0], observed[1]);
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
