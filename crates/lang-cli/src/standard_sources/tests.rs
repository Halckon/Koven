//! 宿主真实加载的标准源与普通同名来源使用同一 frontend，权限互不继承。
use super::*;
use lang_frontend::{
    analysis::analyze_unit_names,
    ownership_checking::{BorrowBindingStorage, check_compilation_unit_ownership},
    type_checking::{CallableResultSource, check_compilation_unit_types, standard_environments},
};

#[test]
fn loaded_std_ranges_prove_generic_return_caller_root_continuation_and_end() {
    for (algorithm, (element, value, receiver)) in ["take", "drop", "dropLast"]
        .into_iter()
        .flat_map(|algorithm| {
            [
                ("String", "\"kept\"", false),
                ("String", "\"kept\"", true),
                ("Item", "Item(7)", false),
                ("Item", "Item(7)", true),
            ]
            .map(|case| (algorithm, case))
        })
    {
        let mut sources = SourceMap::new();
        let text = format!(
            "package app\nimport koven.algorithms.{algorithm} as prefix\nclass Item(val number: Int) {{}}\nfun inspect(source: View<{element}>): Unit {{}}\nfun consume(own source: List<{element}>): Unit {{}}\nfun run(): Unit {{ val source = listOf({value}); borrow val part = prefix(source, 2147483647); borrow val child = prefix(part, 1); inspect(child); consume(source) }}"
        );
        let text = if receiver {
            text.replace("prefix(source, 2147483647)", "source.prefix(2147483647)")
                .replace("prefix(part, 1)", "part.prefix(1)")
        } else {
            text
        };
        let application = sources.add_source("src/app/Main.ko", text).unwrap();
        let mut descriptors = vec![UnitSourceDescriptor::new("src", "app/Main.ko", application)];
        let (environment, mut types) = standard_environments();
        append_standard_sources(&mut sources, &mut descriptors, &mut types).unwrap();
        let standard = descriptors[1].source_id();
        assert!(!types.is_authorized_range_source(application));
        assert!(types.is_authorized_range_source(standard));
        assert!(!types.is_authorized_range_extension_source(application));
        assert!(types.is_authorized_range_extension_source(standard));
        let snapshot = analyze_unit_names(sources, descriptors, environment).unwrap();
        assert!(
            snapshot.names().diagnostics().is_empty(),
            "{:?}",
            snapshot.names().diagnostics()
        );
        let inputs = snapshot.inputs();
        let names = snapshot.validated_names().unwrap();
        let typed =
            check_compilation_unit_types(snapshot.sources(), &inputs, names, &types).unwrap();
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        let typed = typed.validate().unwrap();
        let owned =
            check_compilation_unit_ownership(snapshot.sources(), &inputs, names, &types, &typed)
                .unwrap();
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let [binding, child] = owned.borrow_results().bindings() else {
            panic!("List and View caller bindings")
        };
        assert_eq!(child.storage(), BorrowBindingStorage::NewRangeDescriptor);
        assert!(child.parent().is_none());
        assert_eq!(binding.origin(), child.origin());
        let child_source = child.source_loan().unwrap();
        assert_eq!(
            owned
                .loans()
                .iter()
                .find(|loan| loan.call() == child_source.call()
                    && loan.argument() == child_source.argument())
                .unwrap()
                .target(),
            child.origin()
        );
        assert_eq!(
            owned
                .borrow_results()
                .ends()
                .iter()
                .map(|end| end.binding())
                .collect::<Vec<_>>(),
            [binding.binding(), child.binding()]
        );
        assert_eq!(binding.storage(), BorrowBindingStorage::NewRangeDescriptor);
        assert!(binding.parent().is_none());
        let source = binding.source_loan().unwrap();
        assert_eq!(
            owned
                .loans()
                .iter()
                .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
                .unwrap()
                .target(),
            binding.origin()
        );
        assert_eq!(
            owned.borrow_results().ends()[0].binding(),
            binding.binding()
        );
        let returned = owned.borrow_results().range_return_origins();
        assert_eq!(returned.len(), 12);
        let mut kinds = Vec::new();
        let mut forwarded = 0;
        for returned in returned {
            assert_eq!(returned.declaration_span().source_id(), standard);
            assert_eq!(
                snapshot
                    .sources()
                    .slice(returned.declaration_span())
                    .unwrap(),
                "from"
            );
            let call = typed
                .types()
                .calls()
                .iter()
                .find(|call| call.expression() == returned.expression())
                .unwrap();
            if let Some(construction) = call.range_construction() {
                kinds.push(construction.source_kind());
            } else {
                assert!(call.receiver().is_none());
                let CallableResultSource::Carrier(contract) = call.result_source() else {
                    panic!("receiver wrapper must forward the existing top-level carrier")
                };
                assert_eq!(
                    contract.origin(),
                    lang_frontend::type_checking::BorrowReturnOrigin::Parameter(0)
                );
                forwarded += 1;
            }
        }
        assert_eq!(forwarded, 6);
        assert_eq!(
            kinds,
            [
                lang_frontend::type_checking::RangeSourceKind::List,
                lang_frontend::type_checking::RangeSourceKind::View,
                lang_frontend::type_checking::RangeSourceKind::List,
                lang_frontend::type_checking::RangeSourceKind::View,
                lang_frontend::type_checking::RangeSourceKind::List,
                lang_frontend::type_checking::RangeSourceKind::View
            ]
        );
        assert!(owned.borrow_return_origins().is_empty());
        let call = typed
            .types()
            .calls()
            .iter()
            .find(|call| call.expression() == binding.initializer())
            .unwrap();
        let CallableResultSource::Carrier(contract) = call.result_source() else {
            panic!("carrier call")
        };
        assert_eq!(contract.from_span().unwrap().source_id(), standard);
        let child_call = typed
            .types()
            .calls()
            .iter()
            .find(|call| call.expression() == child.initializer())
            .unwrap();
        assert_ne!(
            call.target(),
            child_call.target(),
            "each overload has a canonical declaration identity"
        );
        assert_eq!(call.receiver().is_some(), receiver);
        assert_eq!(child_call.receiver().is_some(), receiver);
        let CallableResultSource::Carrier(child_contract) = child_call.result_source() else {
            panic!("View carrier source");
        };
        assert_eq!(child_contract.from_span().unwrap().source_id(), standard);
    }
}

#[test]
fn a_user_std_path_and_package_do_not_receive_loader_authority() {
    let mut sources = SourceMap::new();
    let forged = sources.add_source("koven-std/koven/algorithms/forged.ko", "package koven.algorithms\nfun forged(source: List<String>): View<String> from source = rangeView(source, 0, 0)").unwrap();
    let mut descriptors = vec![UnitSourceDescriptor::new(
        "koven-std",
        "koven/algorithms/forged.ko",
        forged,
    )];
    let (environment, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut types).unwrap();
    assert!(!types.is_authorized_range_source(forged));
    assert!(!types.is_authorized_range_extension_source(forged));
    assert!(types.is_authorized_range_extension_source(descriptors[1].source_id()));
    let snapshot = analyze_unit_names(sources, descriptors, environment).unwrap();
    assert!(snapshot.names().diagnostics().is_empty());
    let inputs = snapshot.inputs();
    let typed = check_compilation_unit_types(
        snapshot.sources(),
        &inputs,
        snapshot.validated_names().unwrap(),
        &types,
    )
    .unwrap();
    assert!(
        typed
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0164"),
        "{:?}",
        typed.diagnostics()
    );
    assert!(
        typed
            .calls()
            .iter()
            .all(|call| call.range_construction().is_none())
    );
}

#[test]
fn a_forged_standard_receiver_cannot_inherit_loader_extension_authority() {
    let mut sources = SourceMap::new();
    let forged = sources.add_source("koven-std/koven/algorithms/forged.ko", "package koven.algorithms\nborrow fun <T> List<T>.imposter(count:Int):View<T> from this").unwrap();
    let mut descriptors = vec![UnitSourceDescriptor::new(
        "koven-std",
        "koven/algorithms/forged.ko",
        forged,
    )];
    let (environment, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut types).unwrap();
    assert!(!types.is_authorized_range_source(forged));
    assert!(!types.is_authorized_range_extension_source(forged));
    let standard = descriptors[1].source_id();
    assert!(types.is_authorized_range_source(standard));
    assert!(types.is_authorized_range_extension_source(standard));
    let snapshot = analyze_unit_names(sources, descriptors, environment).unwrap();
    let inputs = snapshot.inputs();
    let typed = check_compilation_unit_types(
        snapshot.sources(),
        &inputs,
        snapshot.validated_names().unwrap(),
        &types,
    )
    .unwrap();
    assert!(
        typed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().to_string() == "L0164"
                && diagnostic.primary_span().source_id() == forged),
        "{:?}",
        typed.diagnostics()
    );
    let bindings = typed
        .signatures()
        .declarations()
        .iter()
        .filter_map(|declaration| declaration.callable())
        .filter_map(|callable| {
            callable
                .range_extension()
                .map(|binding| (callable.name(), binding))
        })
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 6);
    let mut actual = Vec::new();
    let mut identities = Vec::new();
    for (name, binding) in bindings {
        assert_eq!(binding.source(), standard);
        assert_eq!(
            snapshot.sources().slice(binding.from_span()).unwrap(),
            "from"
        );
        assert_eq!(
            snapshot.sources().slice(binding.source_span()).unwrap(),
            "this"
        );
        assert!(!identities.contains(&binding.callable()));
        identities.push(binding.callable());
        actual.push((name, binding.source_kind()));
    }
    use lang_frontend::type_checking::RangeSourceKind::{List, View};
    assert_eq!(
        actual,
        [
            ("take", List),
            ("take", View),
            ("drop", List),
            ("drop", View),
            ("dropLast", List),
            ("dropLast", View)
        ]
    );
}
