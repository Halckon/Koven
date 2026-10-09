//! 可信 receiver 类型选择；Phase 3 证明之后的后端仍受 L0164 门保护。
use lang_frontend::{
    lexer::lex,
    name_resolution::resolve_names,
    parser::{Item, parse_file},
    source::SourceMap,
    type_checking::{IntrinsicTypeConstructor, TypeKind, check_types, standard_environments},
};

#[test]
fn extension_receiver_type_uses_the_callable_type_parameter_scope() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "receiver.ko",
            "borrow fun <T> List<T>.prefix(count: Int): View<T> from this",
        )
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let receiver = parsed
        .ast()
        .items()
        .iter()
        .find_map(|(_, node)| match node.payload() {
            Item::Function {
                extension_receiver: Some(receiver),
                ..
            } => Some(*receiver),
            _ => None,
        })
        .unwrap();
    let (names_env, types_env) = standard_environments();
    let names = resolve_names(&sources, &parsed, &names_env).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types_env).unwrap();
    assert!(
        typed
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0164")
    );
    let ty = typed.type_ref_type(receiver.type_ref).unwrap();
    assert!(matches!(typed.types().get(ty), Some(TypeKind::Intrinsic {
        constructor: IntrinsicTypeConstructor::List,
        arguments,
    }) if arguments.len() == 1 && matches!(typed.types().get(arguments[0]), Some(TypeKind::TypeParameter(_)))));
}

use lang_frontend::{
    name_resolution::{SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names},
    type_checking::{
        BorrowReturnOrigin, CallableTarget, CompilationUnitTypes, ParameterMode, RangeSourceKind,
        TypedFile, UnitCallTarget, UnitCallableTarget, UnitTypeKind, check_compilation_unit_types,
    },
};

fn checked(
    text: &str,
    producer: bool,
    extension: bool,
) -> (SourceMap, TypedFile, CompilationUnitTypes) {
    let mut sources = SourceMap::new();
    let logical = if text.starts_with("package koven\n") {
        "koven/Main.ko"
    } else {
        "koven/algorithms/ranges.ko"
    };
    let text = if text.starts_with("package ") {
        text.to_owned()
    } else {
        format!("package koven.algorithms\n{text}")
    };
    let source = sources
        .add_source("koven-std/koven/algorithms/ranges.ko", text)
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, mut types) = standard_environments();
    if producer {
        types.authorize_range_source(&sources, source).unwrap();
    }
    if extension {
        types
            .authorize_range_extension_source(&sources, source)
            .unwrap();
    }
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let single = check_types(&sources, &parsed, &names, &types).unwrap();
    let inputs = [SourceUnitInput::new("koven-std", logical, source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    (sources, single, unit)
}

fn extension_count(single: &TypedFile, unit: &CompilationUnitTypes) -> (usize, usize) {
    (
        single
            .callables()
            .iter()
            .filter(|c| c.range_extension().is_some())
            .count(),
        unit.signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .filter(|c| c.range_extension().is_some())
            .count(),
    )
}

#[test]
fn receiver_authority_is_independent_owned_and_deduplicated() {
    let mut sources = SourceMap::new();
    let trusted = sources.add_source("trusted.ko", "").unwrap();
    let forged = sources
        .add_source("koven-std/koven/algorithms/ranges.ko", "")
        .unwrap();
    let mut foreign = SourceMap::new();
    let other_owner = foreign.add_source("trusted.ko", "").unwrap();
    let (_, mut environment) = standard_environments();
    assert!(
        environment
            .authorize_range_extension_source(&sources, other_owner)
            .is_err()
    );
    environment
        .authorize_range_extension_source(&sources, trusted)
        .unwrap();
    environment
        .authorize_range_extension_source(&sources, trusted)
        .unwrap();
    assert!(environment.is_authorized_range_extension_source(trusted));
    assert!(!environment.is_authorized_range_source(trusted));
    assert!(!environment.is_authorized_range_extension_source(forged));
    assert!(!environment.is_authorized_range_extension_source(other_owner));
    environment
        .authorize_range_source(&sources, forged)
        .unwrap();
    assert!(!environment.is_authorized_range_extension_source(forged));
}

#[test]
fn trusted_binding_keeps_canonical_callable_receiver_and_contract_spans() {
    let (sources, single, unit) = checked(
        "borrow fun <T> List<T>.renamed(count: Int): View<T> from this",
        false,
        true,
    );
    assert_eq!(extension_count(&single, &unit), (1, 1));
    let callable = &single.callables()[0];
    let binding = callable.range_extension().unwrap();
    assert_eq!(
        binding.callable(),
        CallableTarget::Source(callable.symbol())
    );
    assert_eq!(binding.source(), single.source_id());
    assert_eq!(binding.receiver_type(), callable.receiver().unwrap().ty());
    assert_eq!(binding.source_kind(), RangeSourceKind::List);
    assert_eq!(binding.receiver_mode(), ParameterMode::Borrow);
    assert_eq!(binding.origin(), BorrowReturnOrigin::Receiver);
    assert_eq!(sources.slice(binding.receiver_span()).unwrap(), "List<T>");
    assert_eq!(sources.slice(binding.from_span()).unwrap(), "from");
    assert_eq!(sources.slice(binding.source_span()).unwrap(), "this");
    let callable = unit
        .signatures()
        .declarations()
        .iter()
        .find_map(|d| d.callable())
        .unwrap();
    let binding = callable.range_extension().unwrap();
    assert_eq!(binding.callable(), callable.target());
    assert!(matches!(
        binding.callable(),
        UnitCallableTarget::Declaration(_)
    ));
    assert_eq!(binding.source(), single.source_id());
    assert_eq!(binding.receiver_type(), callable.receiver().unwrap().ty());
    assert_eq!(sources.slice(binding.receiver_span()).unwrap(), "List<T>");
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert!(unit.validate().is_ok());
}

#[test]
fn list_and_view_select_distinct_canonical_bindings_and_infer_string_and_move_only() {
    for element in ["String", "MoveOnly"] {
        let text = format!(
            "class MoveOnly(val value: Int) {{}}\nborrow fun <T> List<T>.prefix(count: Int): View<T> from this\nborrow fun <U> View<U>.prefix(count: Int): View<U> from this\nfun useList(xs: List<{element}>): Unit {{ borrow val part = xs.prefix(1) }}\nfun useView(xs: View<{element}>): Unit {{ borrow val part = xs.prefix(1) }}"
        );
        let (_, single, unit) = checked(&text, false, true);
        assert_eq!(extension_count(&single, &unit), (2, 2));
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(diagnostics.is_empty(), "{element}: {diagnostics:?}");
        }
        let calls: Vec<_> = single
            .calls()
            .iter()
            .filter(|c| c.receiver().is_some())
            .collect();
        assert_eq!(calls.len(), 2, "{element}: {:?}", single.diagnostics());
        assert_ne!(calls[0].target(), calls[1].target());
        for call in calls {
            let args = call.instance().type_arguments();
            assert_eq!(args.len(), 1);
            assert!(
                matches!(single.types().get(call.return_type()), Some(TypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::View, arguments }) if arguments == args)
            );
            assert!(call.range_construction().is_none());
        }
        let calls: Vec<_> = unit
            .calls()
            .iter()
            .filter(|c| c.receiver().is_some())
            .collect();
        assert_eq!(calls.len(), 2, "{element}: {:?}", unit.diagnostics());
        assert_ne!(calls[0].target(), calls[1].target());
        for call in calls {
            assert!(matches!(call.target(), UnitCallTarget::Declaration(_)));
            let args = call.instance().type_arguments();
            assert_eq!(args.len(), 1);
            assert!(
                matches!(unit.types().get(call.return_type()), Some(UnitTypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::View, arguments }) if arguments == args)
            );
            assert!(call.range_construction().is_none());
        }
        assert!(unit.validate().is_ok());
    }
}

#[test]
fn producer_authority_and_std_spelling_do_not_upgrade_to_receiver_authority() {
    for producer in [false, true] {
        let text = "package koven\nborrow fun <T> List<T>.prefix(count: Int): View<T> from this\nfun use(xs: List<String>): Unit { borrow val part = xs.prefix(1) }";
        let (_, single, unit) = checked(text, producer, false);
        assert_eq!(extension_count(&single, &unit), (0, 0));
        assert!(single.calls().iter().all(|c| c.receiver().is_none()));
        assert!(unit.calls().iter().all(|c| c.receiver().is_none()));
    }
}

#[test]
fn ordinary_invalid_receiver_declarations_never_publish_bindings() {
    for text in [
        "own fun <T> List<T>.prefix(count: Int): View<T> from this",
        "borrow fun <T> Array<T>.prefix(count: Int): View<T> from this",
        "borrow fun <T> List<T>.prefix(source: List<T>): View<T> from source",
        "borrow fun List<String>.prefix(count: Int): View<Int> from this",
        "class List<T>() {}\nborrow fun <T> List<T>.prefix(count: Int): View<T> from this",
        "class View<T>() {}\nborrow fun <T> List<T>.prefix(count: Int): View<T> from this",
    ] {
        let (_, single, unit) = checked(text, true, true);
        assert_eq!(extension_count(&single, &unit), (0, 0), "{text}");
        assert!(
            single
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164")
        );
        assert!(
            unit.diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0164")
        );
    }
}

#[test]
fn explicit_type_arguments_must_agree_with_the_actual_receiver_element() {
    let (_, single, unit) = checked(
        "borrow fun <T> List<T>.prefix(count: Int): View<T> from this\nfun use(xs: List<String>): Unit { borrow val part = xs.prefix<Int>(1) }",
        false,
        true,
    );
    assert!(
        single.calls().iter().all(|c| c.receiver().is_none()),
        "{:?}",
        single.diagnostics()
    );
    assert!(
        unit.calls().iter().all(|c| c.receiver().is_none()),
        "{:?}",
        unit.diagnostics()
    );
    assert!(
        single
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() != "L0164")
    );
    assert!(
        unit.diagnostics()
            .iter()
            .any(|d| d.code().to_string() != "L0164")
    );
}

fn checked_unit_sources(
    standard_text: &str,
    application_text: &str,
    authorize_standard: bool,
) -> CompilationUnitTypes {
    let mut sources = SourceMap::new();
    let standard = sources.add_source("loaded.ko", standard_text).unwrap();
    let application = sources.add_source("app.ko", application_text).unwrap();
    let parsed = [standard, application]
        .map(|id| parse_file(&sources, &lex(&sources, id).unwrap()).unwrap());
    assert!(parsed.iter().all(|p| p.diagnostics().is_empty()));
    let (environment, mut types) = standard_environments();
    if authorize_standard {
        types
            .authorize_range_extension_source(&sources, standard)
            .unwrap();
    }
    let package = standard_text
        .lines()
        .next()
        .unwrap()
        .strip_prefix("package ")
        .unwrap()
        .replace('.', "/");
    let standard_path = format!("{package}/Loaded.ko");
    let inputs = [
        SourceUnitInput::new("std", &standard_path, standard, &parsed[0]),
        SourceUnitInput::new("app", "app/Main.ko", application, &parsed[1]),
    ];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    if let Ok(validated) = typed.clone().validate() {
        let owned = lang_frontend::ownership_checking::check_compilation_unit_ownership(
            &sources, &inputs, &names, &types, &validated,
        )
        .unwrap();
        // These signature-only fixtures have no producer body or caller source proof.
        assert!(owned.borrow_results().bindings().is_empty());
        assert!(owned.borrow_results().range_return_origins().is_empty());
        assert!(
            owned.validate().is_err(),
            "signature selection cannot deliver owned capability"
        );
    }
    typed
}

#[test]
fn unit_import_alias_wildcard_and_same_package_reuse_existing_visibility() {
    for (package, imports, spelling) in [
        ("trusted", "import trusted.prefix", "prefix"),
        ("trusted", "import trusted.prefix as first", "first"),
        ("trusted", "import trusted.*", "prefix"),
        ("app", "", "prefix"),
    ] {
        let standard = format!(
            "package {package}\nborrow fun <T> List<T>.prefix(count: Int): View<T> from this"
        );
        let application = format!(
            "package app\n{imports}\nfun use(xs: List<String>): Unit {{ borrow val part = xs.{spelling}(1) }}"
        );
        let unit = checked_unit_sources(&standard, &application, true);
        assert!(
            unit.diagnostics().is_empty(),
            "{imports}: {:?}",
            unit.diagnostics()
        );
        let call = unit
            .calls()
            .iter()
            .find(|c| c.receiver().is_some())
            .unwrap();
        let UnitCallTarget::Declaration(id) = call.target() else {
            panic!("canonical declaration")
        };
        let callable = unit
            .signatures()
            .declaration(id)
            .unwrap()
            .callable()
            .unwrap();
        assert_eq!(callable.name(), "prefix");
        assert_eq!(
            callable.range_extension().unwrap().callable(),
            UnitCallableTarget::Declaration(id)
        );
        assert_eq!(call.instance().type_arguments().len(), 1);
        assert!(unit.validate().is_ok());
    }
}

#[test]
fn invisible_or_locally_shadowed_extensions_do_not_become_candidates() {
    let standard = "package trusted\nborrow fun <T> List<T>.prefix(count: Int): View<T> from this";
    for application in [
        "package app\nfun use(xs: List<String>): Unit { borrow val part = xs.prefix(1) }",
        "package app\nimport trusted.prefix\nfun use(xs: List<String>): Unit { val prefix = 0; borrow val part = xs.prefix(1) }",
    ] {
        let unit = checked_unit_sources(standard, application, true);
        assert!(unit.calls().iter().all(|c| c.receiver().is_none()));
        // Unselected ordinary member lookup remains Deferred in the type product.
        // The helper also verifies that ownership cannot publish backend capability.
        assert!(unit.validate().is_ok());
    }
    let (_, single, unit) = checked(
        "borrow fun <T> List<T>.prefix(count: Int): View<T> from this\nfun use(xs: List<String>): Unit { val prefix = 0; borrow val part = xs.prefix(1) }",
        false,
        true,
    );
    assert!(single.calls().iter().all(|c| c.receiver().is_none()));
    assert!(unit.calls().iter().all(|c| c.receiver().is_none()));
}

#[test]
fn trusted_authority_does_not_spread_to_a_foreign_declaration_in_the_same_unit() {
    let standard = "package trusted\nborrow fun <T> List<T>.prefix(count: Int): View<T> from this";
    let application = "package app\nborrow fun <T> List<T>.prefix(count: Int): View<T> from this\nfun use(xs: List<String>): Unit { borrow val part = xs.prefix(1) }";
    let unit = checked_unit_sources(standard, application, true);
    assert_eq!(
        unit.signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .filter(|c| c.range_extension().is_some())
            .count(),
        1
    );
    assert!(unit.calls().iter().all(|c| c.receiver().is_none()));
}

#[test]
fn a_receiver_extension_cannot_be_called_as_an_unbound_top_level_function() {
    let (_, single, unit) = checked(
        "borrow fun <T> List<T>.prefix(count: Int): View<T> from this\nfun use(): Unit { borrow val part = prefix<String>(1) }",
        false,
        true,
    );
    assert!(
        single.calls().iter().all(|c| !matches!(
            c.result_source(),
            lang_frontend::type_checking::CallableResultSource::Carrier(_)
        )),
        "{:?}",
        single.diagnostics()
    );
    assert!(
        unit.calls().iter().all(|c| !matches!(
            c.result_source(),
            lang_frontend::type_checking::CallableResultSource::Carrier(_)
        )),
        "{:?}",
        unit.diagnostics()
    );
}

#[test]
fn duplicate_receiver_shapes_still_reject_alpha_equivalent_declarations() {
    let (_, single, unit) = checked(
        "borrow fun <T> List<T>.prefix(count: Int): View<T> from this\nborrow fun <U> List<U>.prefix(other: Int): View<U> from this",
        false,
        true,
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0097"),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn trusted_receiver_analysis_observes_types_but_stops_before_backend_delivery() {
    use lang_frontend::analysis::{SingleFileAnalysisError, SingleFileStage, analyze_single_file};
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "loaded.ko",
            "borrow fun <T> List<T>.prefix(count: Int): View<T> from this = rangeView(this, 0, 0)",
        )
        .unwrap();
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    types
        .authorize_range_extension_source(&sources, source)
        .unwrap();
    let mut typed_observed = false;
    let result = analyze_single_file(
        &sources,
        source,
        &environment,
        &types,
        |stage, diagnostics| {
            if diagnostics.is_empty() {
                Ok(())
            } else {
                Err(stage)
            }
        },
        |_| {
            typed_observed = true;
            Ok(())
        },
    );
    assert!(matches!(
        result,
        Err(SingleFileAnalysisError::Host(
            SingleFileStage::OwnershipChecking
        ))
    ));
    assert!(typed_observed);
}
