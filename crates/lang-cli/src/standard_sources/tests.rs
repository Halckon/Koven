//! 宿主真实加载的标准源与普通同名来源使用同一 frontend，权限互不继承。
use super::*;
use lang_frontend::{
    analysis::{UnitNameSnapshot, analyze_unit_names},
    diagnostic::Severity,
    name_resolution::ordered_unit_diagnostics,
    ownership_checking::{BorrowBindingStorage, check_compilation_unit_ownership},
    type_checking::{
        CallableResultSource, IntrinsicTypeConstructor, UnitCallTarget, UnitTypeKind,
        check_compilation_unit_types, standard_environments,
    },
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
        let (mut environment, mut types) = standard_environments();
        append_standard_sources(&mut sources, &mut descriptors, &mut environment, &mut types)
            .unwrap();
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
    let (mut environment, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut environment, &mut types).unwrap();
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
    let (mut environment, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut environment, &mut types).unwrap();
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

// 仅核既有标准来源；展示名的独立expected由每个exact以固定字面量提供。
fn assert_standard_registration(
    sources: &SourceMap,
    descriptor: &UnitSourceDescriptor,
    types: &TypeEnvironment,
    expected_display: &str,
) {
    let standard = descriptor.source_id();
    assert_eq!(
        descriptor.root_identity().expect("rooted source"),
        "koven-std"
    );
    assert_eq!(
        descriptor.logical_path().expect("rooted source"),
        "koven/algorithms/ranges.ko"
    );
    assert_eq!(sources.source_name(standard).unwrap(), expected_display);
    assert_eq!(sources.source_text(standard).unwrap(), RANGE_TEXT);
    assert!(types.is_authorized_range_source(standard));
    assert!(types.is_authorized_range_extension_source(standard));
}

// 仅续接调用者已创建的真实snapshot；不注册来源、不创建fixture、不生成oracle。
fn assert_unit_names_and_types(snapshot: &UnitNameSnapshot, types: &TypeEnvironment) {
    assert!(snapshot.names().diagnostics().is_empty());
    let inputs = snapshot.inputs();
    let typed = check_compilation_unit_types(
        snapshot.sources(),
        &inputs,
        snapshot.validated_names().unwrap(),
        types,
    )
    .unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    assert!(typed.validate().is_ok());
}

#[test]
fn standard_display_without_collision_keeps_the_canonical_name() {
    let mut sources = SourceMap::new();
    let text = "package koven.algorithms\nfun main(): Unit {}\n";
    let display = "user/koven/algorithms/ranges.ko";
    let user = sources.add_source(display, text).unwrap();
    let original = UnitSourceDescriptor::new("single", "koven/algorithms/ranges.ko", user);
    let mut descriptors = vec![original.clone()];
    let (mut environment, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut environment, &mut types).unwrap();
    assert_eq!(descriptors.len(), 3);
    assert_eq!(descriptors[0], original);
    let standard = descriptors[1].source_id();
    assert_ne!(standard, user);
    assert_eq!(sources.source_name(user).unwrap(), display);
    assert_eq!(sources.source_text(user).unwrap(), text);
    assert_standard_registration(
        &sources,
        &descriptors[1],
        &types,
        "koven-std/koven/algorithms/ranges.ko",
    );
    assert!(!types.is_authorized_range_source(user));
    assert!(!types.is_authorized_range_extension_source(user));
    let snapshot = analyze_unit_names(sources, descriptors, environment).unwrap();
    assert_unit_names_and_types(&snapshot, &types);
}

#[test]
fn standard_display_collision_preserves_the_user_source_and_standard_key() {
    let mut sources = SourceMap::new();
    let text = "package koven.algorithms\nfun main(): Unit {}\n";
    let display = "koven-std/koven/algorithms/ranges.ko";
    let user = sources.add_source(display, text).unwrap();
    let original = UnitSourceDescriptor::new("single", "koven/algorithms/ranges.ko", user);
    let mut descriptors = vec![original.clone()];
    let (mut environment, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut environment, &mut types).unwrap();
    assert_eq!(descriptors.len(), 3);
    assert_eq!(descriptors[0], original);
    let standard = descriptors[1].source_id();
    assert_ne!(standard, user);
    assert_eq!(sources.source_name(user).unwrap(), display);
    assert_eq!(sources.source_text(user).unwrap(), text);
    assert_standard_registration(
        &sources,
        &descriptors[1],
        &types,
        "koven-std/koven/algorithms/ranges.ko [standard source 1]",
    );
    assert!(!types.is_authorized_range_source(user));
    assert!(!types.is_authorized_range_extension_source(user));
    let snapshot = analyze_unit_names(sources, descriptors, environment).unwrap();
    assert_unit_names_and_types(&snapshot, &types);
}

#[test]
fn standard_display_collision_reaches_the_last_bounded_candidate() {
    let users = [
        (
            "koven-std/koven/algorithms/ranges.ko",
            "package koven.algorithms\nfun main(): Unit {}\n",
            "koven/algorithms/ranges.ko",
        ),
        (
            "koven-std/koven/algorithms/ranges.ko [standard source 1]",
            "package helpers\nfun first(): Unit {}\n",
            "helpers/First.ko",
        ),
        (
            "koven-std/koven/algorithms/ranges.ko [standard source 2]",
            "package helpers\nfun second(): Unit {}\n",
            "helpers/Second.ko",
        ),
    ];
    for order in [[0, 1, 2], [2, 0, 1]] {
        let mut sources = SourceMap::new();
        let mut descriptors = Vec::new();
        for index in order {
            let (display, text, path) = users[index];
            let source = sources.add_source(display, text).unwrap();
            descriptors.push(UnitSourceDescriptor::new("user", path, source));
        }
        let originals = descriptors.clone();
        let (mut environment, mut types) = standard_environments();
        append_standard_sources(&mut sources, &mut descriptors, &mut environment, &mut types)
            .unwrap();
        assert_eq!(descriptors.len(), 5);
        assert_eq!(&descriptors[..3], originals.as_slice());
        let standard = descriptors[3].source_id();
        for (original, index) in originals.iter().zip(order) {
            let user = original.source_id();
            assert_ne!(user, standard);
            assert_eq!(sources.source_name(user).unwrap(), users[index].0);
            assert_eq!(sources.source_text(user).unwrap(), users[index].1);
            assert!(!types.is_authorized_range_source(user));
            assert!(!types.is_authorized_range_extension_source(user));
        }
        assert_standard_registration(
            &sources,
            &descriptors[3],
            &types,
            "koven-std/koven/algorithms/ranges.ko [standard source 3]",
        );
        let snapshot = analyze_unit_names(sources, descriptors, environment).unwrap();
        assert_unit_names_and_types(&snapshot, &types);
    }
}

#[test]
fn standard_display_collision_uses_the_first_free_candidate() {
    let users = [
        (
            "koven-std/koven/algorithms/ranges.ko",
            "package koven.algorithms\nfun main(): Unit {}\n",
            "koven/algorithms/ranges.ko",
        ),
        (
            "koven-std/koven/algorithms/ranges.ko [standard source 2]",
            "package helpers\nfun first(): Unit {}\n",
            "helpers/First.ko",
        ),
        (
            "unrelated/helpers/Second.ko",
            "package helpers\nfun second(): Unit {}\n",
            "helpers/Second.ko",
        ),
    ];
    let mut sources = SourceMap::new();
    let mut descriptors = Vec::new();
    for (display, text, path) in users {
        let source = sources.add_source(display, text).unwrap();
        descriptors.push(UnitSourceDescriptor::new("user", path, source));
    }
    let originals = descriptors.clone();
    let (mut environment, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut environment, &mut types).unwrap();
    assert_eq!(descriptors.len(), 5);
    assert_eq!(&descriptors[..3], originals.as_slice());
    let standard = descriptors[3].source_id();
    for (original, (display, text, _)) in originals.iter().zip(users) {
        let user = original.source_id();
        assert_ne!(user, standard);
        assert_eq!(sources.source_name(user).unwrap(), display);
        assert_eq!(sources.source_text(user).unwrap(), text);
        assert!(!types.is_authorized_range_source(user));
        assert!(!types.is_authorized_range_extension_source(user));
    }
    assert_standard_registration(
        &sources,
        &descriptors[3],
        &types,
        "koven-std/koven/algorithms/ranges.ko [standard source 1]",
    );
    let snapshot = analyze_unit_names(sources, descriptors, environment).unwrap();
    assert_unit_names_and_types(&snapshot, &types);
}

#[test]
fn standard_display_collision_keeps_complete_user_diagnostic_locations() {
    let mut sources = SourceMap::new();
    let text = "package koven.algorithms\nfun main(): Unit { missing }\n";
    let user = sources
        .add_source("koven-std/koven/algorithms/ranges.ko", text)
        .unwrap();
    let original = UnitSourceDescriptor::new("single", "koven/algorithms/ranges.ko", user);
    let mut descriptors = vec![original.clone()];
    let (mut environment, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut environment, &mut types).unwrap();
    assert_eq!(descriptors.len(), 3);
    assert_eq!(descriptors[0], original);
    let standard = descriptors[1].source_id();
    assert_ne!(user, standard);
    assert_eq!(sources.source_text(user).unwrap(), text);
    assert_eq!(
        sources.source_name(standard).unwrap(),
        "koven-std/koven/algorithms/ranges.ko [standard source 1]"
    );
    assert!(!types.is_authorized_range_source(user));
    assert!(!types.is_authorized_range_extension_source(user));
    assert!(types.is_authorized_range_source(standard));
    assert!(types.is_authorized_range_extension_source(standard));
    let snapshot = analyze_unit_names(sources, descriptors, environment).unwrap();
    assert!(snapshot.validated_names().is_none());
    let [diagnostic] = snapshot.names().diagnostics() else {
        panic!("one real unresolved user reference");
    };
    assert_eq!(diagnostic.severity(), Severity::Error);
    assert_eq!(diagnostic.code().to_string(), "L0080");
    assert_eq!(diagnostic.message(), "unresolved name");
    assert_eq!(
        diagnostic.primary_span(),
        snapshot.sources().span(user, 44, 51).unwrap()
    );
    assert_eq!(
        snapshot.sources().slice(diagnostic.primary_span()).unwrap(),
        "missing"
    );
    assert!(diagnostic.details().is_empty());
    let ordered = ordered_unit_diagnostics(
        snapshot.sources(),
        snapshot.names().index().source_units(),
        snapshot.names().diagnostics(),
    )
    .unwrap();
    assert_eq!(ordered, vec![diagnostic]);
    assert_eq!(
        crate::diagnostic_renderer::render_diagnostics_in_order(snapshot.sources(), &ordered)
            .unwrap(),
        "error[L0080] koven-std/koven/algorithms/ranges.ko:2:20-2:27: unresolved name\n"
    );
    assert_eq!(
        crate::machine_diagnostic_renderer::render_machine_diagnostics_in_order(
            snapshot.sources(),
            &ordered,
        )
        .unwrap(),
        concat!(
            "{\"code\":\"L0080\",\"details\":[],\"message\":\"unresolved name\",",
            "\"primary\":{\"byte_end\":51,\"byte_start\":44,",
            "\"end\":{\"column\":27,\"line\":2},",
            "\"source\":\"koven-std/koven/algorithms/ranges.ko\",",
            "\"start\":{\"column\":20,\"line\":2}},",
            "\"schema\":\"koven.diagnostic\",\"severity\":\"error\",\"version\":1}\n",
        )
    );
}

#[test]
fn standard_display_collision_preserves_range_call_source_identities() {
    let mut sources = SourceMap::new();
    let text = concat!(
        "package app\nimport koven.algorithms.take as prefix\n",
        "fun inspect(source: View<String>): Unit {}\n",
        "fun consume(own source: List<String>): Unit {}\n",
        "fun run(): Unit { val source = listOf(\"kept\"); ",
        "borrow val part = prefix(source, 1); borrow val child = prefix(part, 1); ",
        "inspect(child); consume(source) }\n",
    );
    let user = sources
        .add_source("koven-std/koven/algorithms/ranges.ko", text)
        .unwrap();
    let original = UnitSourceDescriptor::new("app", "app/Main.ko", user);
    let mut descriptors = vec![original.clone()];
    let (mut environment, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut environment, &mut types).unwrap();
    assert_eq!(descriptors.len(), 3);
    assert_eq!(descriptors[0], original);
    let standard = descriptors[1].source_id();
    assert_ne!(user, standard);
    assert_eq!(sources.source_text(user).unwrap(), text);
    assert_eq!(
        sources.source_name(user).unwrap(),
        "koven-std/koven/algorithms/ranges.ko"
    );
    assert_standard_registration(
        &sources,
        &descriptors[1],
        &types,
        "koven-std/koven/algorithms/ranges.ko [standard source 1]",
    );
    assert!(!types.is_authorized_range_source(user));
    assert!(!types.is_authorized_range_extension_source(user));
    let snapshot = analyze_unit_names(sources, descriptors, environment).unwrap();
    assert!(snapshot.names().diagnostics().is_empty());
    let inputs = snapshot.inputs();
    let names = snapshot.validated_names().unwrap();
    let typed = check_compilation_unit_types(snapshot.sources(), &inputs, names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(snapshot.sources(), &inputs, names, &types, &typed)
            .unwrap();
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let [binding, child] = owned.borrow_results().bindings() else {
        panic!("List and View caller bindings");
    };
    assert_eq!(binding.origin(), child.origin());
    assert_eq!(binding.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert_eq!(child.storage(), BorrowBindingStorage::NewRangeDescriptor);
    let index = snapshot.names().index();
    let input = inputs
        .iter()
        .find(|input| input.source_id() == user)
        .unwrap();
    let mut targets = Vec::new();
    for (binding, constructor, call_text) in [
        (binding, IntrinsicTypeConstructor::List, "prefix(source, 1)"),
        (child, IntrinsicTypeConstructor::View, "prefix(part, 1)"),
    ] {
        let call = typed
            .types()
            .calls()
            .iter()
            .find(|call| call.expression() == binding.initializer())
            .unwrap();
        assert!(call.receiver().is_none());
        let caller = index
            .source_units()
            .iter()
            .find(|unit| unit.id() == call.expression().source_unit())
            .unwrap();
        assert_eq!(caller.source_id(), user);
        let node = input
            .parsed()
            .ast()
            .expressions()
            .get(call.expression().expression())
            .unwrap();
        assert_eq!(node.span().source_id(), user);
        assert_eq!(snapshot.sources().slice(node.span()).unwrap(), call_text);
        let UnitCallTarget::Declaration(target) = call.target() else {
            panic!("a real standard top-level target");
        };
        let declaration = index
            .declarations()
            .iter()
            .find(|declaration| declaration.id() == target)
            .unwrap();
        assert_eq!(declaration.name(), "take");
        assert_eq!(declaration.name_span().source_id(), standard);
        let source_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.id() == declaration.source_unit())
            .unwrap();
        assert_eq!(source_unit.source_id(), standard);
        assert_eq!(
            source_unit.key().root().expect("rooted source").as_str(),
            "koven-std"
        );
        assert_eq!(
            source_unit
                .key()
                .logical_path()
                .expect("rooted source")
                .as_str(),
            "koven/algorithms/ranges.ko"
        );
        let signature = typed
            .types()
            .signatures()
            .declaration(target)
            .unwrap()
            .callable()
            .unwrap();
        let parameter = signature.parameters().first().unwrap();
        assert!(matches!(
            typed.types().types().get(parameter.ty()),
            Some(UnitTypeKind::Intrinsic { constructor: actual, .. }) if *actual == constructor
        ));
        let CallableResultSource::Carrier(contract) = call.result_source() else {
            panic!("standard carrier result");
        };
        assert_eq!(call.result_source(), signature.result_source());
        let from = contract.from_span().unwrap();
        assert_eq!(from.source_id(), standard);
        assert_eq!(snapshot.sources().slice(from).unwrap(), "from");
        assert_eq!(contract.source_span().source_id(), standard);
        assert_eq!(
            snapshot.sources().slice(contract.source_span()).unwrap(),
            "source"
        );
        targets.push(target);
    }
    assert_ne!(targets[0], targets[1]);
    assert_eq!(
        owned
            .borrow_results()
            .ends()
            .iter()
            .map(|end| end.binding())
            .collect::<Vec<_>>(),
        [binding.binding(), child.binding()]
    );
}

#[test]
fn loaded_result_source_alone_receives_builtin_name_authority() {
    let mut sources = SourceMap::new();
    let forged = sources
        .add_source(
            "koven-std/koven/core/result.ko",
            "package koven.core\nfun forged(): Result<Int, String> = Ok(1)\n",
        )
        .unwrap();
    let original = UnitSourceDescriptor::new("user", "koven/core/forged.ko", forged);
    let mut descriptors = vec![original.clone()];
    let (mut environment, mut types) = standard_environments();
    append_standard_sources(&mut sources, &mut descriptors, &mut environment, &mut types).unwrap();
    assert_eq!(descriptors.len(), 3);
    assert_eq!(descriptors[0], original);
    assert_eq!(
        descriptors[1].logical_path().expect("rooted source"),
        "koven/algorithms/ranges.ko"
    );
    let result = descriptors[2].source_id();
    assert_eq!(
        descriptors[2].logical_path().expect("rooted source"),
        "koven/core/result.ko"
    );
    assert_eq!(
        sources.source_name(result).unwrap(),
        "koven-std/koven/core/result.ko [standard source 1]"
    );
    assert_eq!(sources.source_text(result).unwrap(), RESULT_TEXT);
    assert_eq!(environment.builtin_source(), Some(result));
    assert!(!types.is_authorized_range_source(result));
    let snapshot = analyze_unit_names(sources, descriptors, environment).unwrap();
    assert_unit_names_and_types(&snapshot, &types);
    let names = snapshot.validated_names().unwrap();
    let declaration = names.builtin_result_declaration().expect("builtin Result");
    let declaration = &names.names().index().declarations()[declaration.index()];
    assert_eq!(declaration.name(), "Result");
    assert_eq!(declaration.name_span().source_id(), result);
}
