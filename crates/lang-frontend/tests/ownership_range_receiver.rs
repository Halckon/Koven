//! 可信 receiver 的实际来源事实；正常源码验证，后端交付仍由 L0164 阻止。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        LoanTarget, UnitLoanTarget, check_compilation_unit_ownership, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

#[test]
fn trusted_list_receiver_return_publishes_an_actual_source_qualified_root() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(
            "loaded.ko",
            "borrow fun <T> List<T>.prefix(count: Int): View<T> from this = rangeView(this, 0, 0)",
        )
        .unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    types
        .authorize_range_extension_source(&sources, source)
        .unwrap();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let owned = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    assert!(
        owned
            .diagnostics()
            .iter()
            .all(|d| d.code().to_string() == "L0164"),
        "{:?}",
        owned.diagnostics()
    );
    assert!(
        owned
            .diagnostics()
            .iter()
            .any(|d| d.code().to_string() == "L0164")
    );
    let [returned] = owned.borrow_results().range_return_origins() else {
        panic!("actual receiver return")
    };
    let LoanTarget::Place(root) = returned.origin() else {
        panic!("bound receiver place")
    };
    let receiver = &names.symbols()[root.root().index()];
    assert!(receiver.is_synthetic());
    assert_eq!(receiver.name(), "this");
    assert_eq!(receiver.span().source_id(), source);
    let inputs = [SourceUnitInput::new("std", "loaded.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let typed = typed.validate().unwrap();
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    assert!(
        owned
            .diagnostics()
            .iter()
            .all(|d| d.code().to_string() == "L0164"),
        "{:?}",
        owned.diagnostics()
    );
    let [returned] = owned.borrow_results().range_return_origins() else {
        panic!("unit actual receiver return")
    };
    let UnitLoanTarget::Place(root) = returned.origin() else {
        panic!("unit bound receiver place")
    };
    let symbol = root.root();
    let receiver = &names.names().source_units()[symbol.source_unit().index()]
        .resolution()
        .symbols()[symbol.symbol().index()];
    assert!(receiver.is_synthetic());
    assert_eq!(receiver.name(), "this");
    assert_eq!(receiver.span().source_id(), source);
    assert!(owned.validate().is_err());
}

fn checked(
    text: &str,
) -> (
    SourceMap,
    lang_frontend::ownership_checking::OwnershipCheckedFile,
    lang_frontend::ownership_checking::CompilationUnitOwnership,
) {
    let mut sources = SourceMap::new();
    let source = sources.add_source("receiver.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    types
        .authorize_range_extension_source(&sources, source)
        .unwrap();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty(), "{:?}", names.diagnostics());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::new("root", "receiver.ko", source, &parsed)];
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
        .unwrap()
        .validate()
        .unwrap();
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
        .unwrap()
        .validate()
        .unwrap();
    let unit = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
    (sources, single, unit)
}

const RECEIVERS: &str = "borrow fun <T> List<T>.prefix(count: Int): View<T> from this = rangeView(this, 0, count)\nborrow fun <T> View<T>.prefix(count: Int): View<T> from this = rangeView(this, 0, count)\n";

#[test]
fn named_receiver_continues_each_real_root_loan_until_dependent_bindings_end() {
    for (element, value) in [
        ("String", "\"kept\""),
        ("Item", "Item(1)"),
        ("Resource", "Resource()"),
    ] {
        let text = format!(
            "{RECEIVERS}class Item(val value: Int) {{}}\nclass Resource {{ deinit() {{}} }}\nfun alias(source: View<{element}>): borrow View<{element}> from source = source\nfun inspect(source: View<{element}>): Unit {{}}\nfun consume(own source: List<{element}>): Unit {{}}\nfun run(): Unit {{ val root = listOf({value}); borrow val parent = root.prefix(1); borrow val child = parent.prefix(1); borrow val metadata = alias(child); inspect(metadata); inspect(parent); consume(root) }}"
        );
        let (_, single, unit) = checked(&text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(
                diagnostics.iter().all(|d| d.code().to_string() == "L0164"),
                "{diagnostics:?}"
            );
            assert_eq!(diagnostics.len(), 2);
        }
        assert_eq!(single.borrow_results().bindings().len(), 3);
        assert_eq!(unit.borrow_results().bindings().len(), 3);
        let [parent, child, metadata] = single.borrow_results().bindings() else {
            unreachable!()
        };
        assert!(child.parent().is_none(), "new descriptor is root-flat");
        assert_eq!(metadata.parent(), Some(child.binding()));
        assert_eq!(
            single
                .borrow_results()
                .ends()
                .iter()
                .map(|end| end.binding())
                .collect::<Vec<_>>(),
            [metadata.binding(), child.binding(), parent.binding()]
        );
        let [parent, child, metadata] = unit.borrow_results().bindings() else {
            unreachable!()
        };
        assert!(child.parent().is_none(), "unit new descriptor is root-flat");
        assert_eq!(metadata.parent(), Some(child.binding()));
        assert_eq!(
            unit.borrow_results()
                .ends()
                .iter()
                .map(|end| end.binding())
                .collect::<Vec<_>>(),
            [metadata.binding(), child.binding(), parent.binding()]
        );
        for binding in single.borrow_results().bindings() {
            let source = binding
                .source_loan()
                .expect("real receiver/argument source loan");
            let loan = single
                .loans()
                .iter()
                .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
                .unwrap();
            assert_eq!(loan.target(), binding.origin());
            assert!(
                !single
                    .loan_ends()
                    .iter()
                    .any(|end| end.call() == source.call()
                        && end.argument() == source.argument()
                        && end.point()
                            == lang_frontend::ownership_checking::LoanEndPoint::CallReturn(
                                source.call()
                            )),
                "source loan survives producer return"
            );
            assert_eq!(
                binding.origin(),
                single.borrow_results().bindings()[0].origin()
            );
        }
        for binding in unit.borrow_results().bindings() {
            let source = binding
                .source_loan()
                .expect("unit receiver/argument source loan");
            let loan = unit
                .loans()
                .iter()
                .find(|loan| loan.call() == source.call() && loan.argument() == source.argument())
                .unwrap();
            assert_eq!(loan.target(), binding.origin());
            assert_eq!(
                binding.origin(),
                unit.borrow_results().bindings()[0].origin()
            );
        }
        assert_eq!(single.borrow_results().ends().len(), 3);
        assert_eq!(unit.borrow_results().ends().len(), 3);
    }
}

#[test]
fn temporary_receiver_chain_publishes_source_loans_for_the_actual_immediate_consumer() {
    use lang_frontend::ownership_checking::{
        DropPoint, DropTarget, RangeUseSite, UnitDropPoint, UnitDropTarget,
    };
    for (element, value) in [
        ("String", "\"kept\""),
        ("Item", "Item(1)"),
        ("Resource", "Resource()"),
    ] {
        let text = format!(
            "{RECEIVERS}class Item(val value: Int) {{}}\nclass Resource {{ deinit() {{}} }}\nfun inspect(source: View<{element}>): Unit {{}}\nfun run(): Unit {{ inspect(listOf({value}).prefix(1).prefix(1)) }}"
        );
        let (_, single, unit) = checked(&text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
            assert!(
                diagnostics.iter().all(|d| d.code().to_string() == "L0164"),
                "{diagnostics:?}"
            );
        }
        assert_eq!(single.borrow_results().range_uses().len(), 2);
        assert_eq!(unit.borrow_results().range_uses().len(), 2);
        let root = single.borrow_results().range_uses()[0].origin();
        let LoanTarget::Temporary(root_expression) = root else {
            panic!("actual temporary root")
        };
        assert_eq!(single.borrow_results().range_uses()[1].origin(), root);
        let RangeUseSite::Call(consumer) = single.borrow_results().range_uses()[1].site() else {
            panic!("actual consumer")
        };
        let cleanup: Vec<_> = single
            .drops()
            .iter()
            .filter(|drop| drop.target() == DropTarget::Temporary(*root_expression))
            .collect();
        assert_eq!(cleanup.len(), 1);
        assert_eq!(cleanup[0].point(), DropPoint::CallReturn(consumer));
        let root = unit.borrow_results().range_uses()[0].origin();
        let UnitLoanTarget::Temporary(root_expression) = root else {
            panic!("unit actual temporary root")
        };
        assert_eq!(unit.borrow_results().range_uses()[1].origin(), root);
        let RangeUseSite::Call(consumer) = unit.borrow_results().range_uses()[1].site() else {
            panic!("unit actual consumer")
        };
        let cleanup: Vec<_> = unit
            .drops()
            .iter()
            .filter(|drop| drop.target() == UnitDropTarget::Temporary(*root_expression))
            .collect();
        assert_eq!(cleanup.len(), 1);
        assert_eq!(cleanup[0].point(), UnitDropPoint::CallReturn(consumer));
    }
}

#[test]
fn receiver_producer_cannot_substitute_a_sibling_local_or_temporary_root() {
    for body in [
        "rangeView(other, 0, 0)",
        "{ val local = listOf(\"local\"); return rangeView(local, 0, 0) }",
        "rangeView(listOf(\"temporary\"), 0, 0)",
    ] {
        let text = format!(
            "borrow fun List<String>.wrong(other: List<String>): View<String> from this = {body}"
        );
        // Block bodies have no expression-body '=' marker.
        let text = text.replace("= {", "{");
        let (_, single, unit) = checked(&text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == "L0162"),
                "{diagnostics:?}"
            );
        }
        assert!(single.borrow_results().range_return_origins().is_empty());
        assert!(unit.borrow_results().range_return_origins().is_empty());
        assert!(single.borrow_results().forwarded_source_loans().is_empty());
        assert!(unit.borrow_results().forwarded_source_loans().is_empty());
    }
}

#[test]
fn temporary_receiver_cannot_escape_into_a_persistent_borrow_binding() {
    let text = format!(
        "{RECEIVERS}fun inspect(source: View<String>): Unit {{}}\nfun run(): Unit {{ borrow val part = listOf(\"temporary\").prefix(1); inspect(part) }}"
    );
    let (_, single, unit) = checked(&text);
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0162"),
            "{diagnostics:?}"
        );
    }
    assert!(single.borrow_results().bindings().is_empty());
    assert!(unit.borrow_results().bindings().is_empty());
}

#[test]
fn each_live_parent_child_sibling_and_metadata_dependency_blocks_root_move() {
    for tail in [
        "borrow val parent = root.prefix(1); consume(root); inspect(parent)",
        "borrow val parent = root.prefix(1); borrow val child = parent.prefix(1); inspect(parent); consume(root); inspect(child)",
        "borrow val parent = root.prefix(1); borrow val sibling = root.prefix(1); inspect(parent); consume(root); inspect(sibling)",
        "borrow val parent = root.prefix(1); borrow val metadata = alias(parent); consume(root); inspect(metadata)",
        "inspectBoth(root.prefix(1), consume(root))",
    ] {
        let text = format!(
            "{RECEIVERS}fun alias(source: View<String>): borrow View<String> from source = source\nfun inspect(source: View<String>): Unit {{}}\nfun inspectBoth(source: View<String>, next: Unit): Unit {{}}\nfun consume(own source: List<String>): Unit {{}}\nfun run(): Unit {{ val root = listOf(\"kept\"); {tail} }}"
        );
        let (_, single, unit) = checked(&text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
                "{tail}: {diagnostics:?}"
            );
        }
        assert!(single.borrow_results().bindings().is_empty());
        assert!(unit.borrow_results().bindings().is_empty());
    }
}

#[test]
fn extension_this_is_shared_and_cannot_be_moved_by_the_producer_body() {
    let (_, single, unit) = checked(
        "fun consume(own source: List<String>): Unit {}\nfun inspect(source: View<String>): Unit {}\nborrow fun List<String>.invalid(): View<String> from this { consume(this); return rangeView(this, 0, 0) }\nfun run(): Unit { val root = listOf(\"kept\"); borrow val part = root.invalid(); inspect(part) }",
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0133"),
            "{diagnostics:?}"
        );
    }
    assert!(single.borrow_results().range_return_origins().is_empty());
    assert!(unit.borrow_results().range_return_origins().is_empty());
    assert!(single.borrow_results().bindings().is_empty());
    assert!(unit.borrow_results().bindings().is_empty());
}

#[test]
fn receiver_element_loans_protect_root_and_end_before_scope_restores_permission() {
    for tail in ["consume(root)", "consumeItem(item)"] {
        let text = format!(
            "{RECEIVERS}class Item(val value: Int) {{}}\nfun consume(own source: List<Item>): Unit {{}}\nfun consumeItem(own source: Item): Unit {{}}\nfun run(): Unit {{ val root = listOf(Item(1)); borrow val part = root.prefix(1); for(item in part) {{ {tail} }} }}"
        );
        let (_, single, unit) = checked(&text);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            let code = if tail == "consume(root)" {
                "L0135"
            } else {
                "L0133"
            };
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == code),
                "{diagnostics:?}"
            );
        }
    }
    let text = format!(
        "{RECEIVERS}class Resource {{ deinit() {{}} }}\nfun inspect(source: Resource): Unit {{}}\nfun consume(own source: List<Resource>): Unit {{}}\nfun run(): Unit {{ val root = listOf(Resource()); {{ borrow val part = root.prefix(1); for(item in part) {{ inspect(item) }} }}; consume(root) }}"
    );
    let (_, single, unit) = checked(&text);
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(
            diagnostics.iter().all(|d| d.code().to_string() == "L0164"),
            "{diagnostics:?}"
        );
    }
    assert_eq!(single.iterations().len(), 1);
    assert_eq!(unit.iterations().len(), 1);
    let LoanTarget::Place(metadata) = single.iterations()[0].source() else {
        panic!("named provider metadata")
    };
    assert_eq!(
        metadata.root(),
        single.borrow_results().bindings()[0].binding()
    );
    let UnitLoanTarget::Place(metadata) = unit.iterations()[0].source() else {
        panic!("unit named provider metadata")
    };
    assert_eq!(
        metadata.root(),
        unit.borrow_results().bindings()[0].binding()
    );
    assert_eq!(single.borrow_results().ends().len(), 1);
    assert_eq!(unit.borrow_results().ends().len(), 1);
}

#[test]
fn imported_receiver_and_producer_forwarding_preserve_each_actual_source_identity() {
    let mut sources = SourceMap::new();
    let mut parsed = Vec::new();
    for (path, text) in [
        ("trusted/Ranges.ko", format!("package trusted\n{RECEIVERS}")),
        ("api/Wrappers.ko", "package api\nimport trusted.prefix as head\nfun forward(source: List<String>): View<String> from source = source.head(1)".to_owned()),
        ("app/Main.ko", "package app\nimport trusted.prefix as first\nimport api.forward\nfun inspect(source: View<String>): Unit {}\nfun consume(own source: List<String>): Unit {}\nfun run(): Unit { val root = listOf(\"kept\"); borrow val parent = forward(root); borrow val child = parent.first(1); inspect(child); consume(root) }".to_owned()),
    ] {
        let source = sources.add_source(path, text).unwrap();
        let file = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(file.diagnostics().is_empty(), "{:?}", file.diagnostics());
        parsed.push((path, source, file));
    }
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, parsed[0].1).unwrap();
    types
        .authorize_range_extension_source(&sources, parsed[0].1)
        .unwrap();
    types.authorize_range_source(&sources, parsed[1].1).unwrap();
    for reverse in [false, true] {
        let mut inputs: Vec<_> = parsed
            .iter()
            .map(|(path, source, file)| SourceUnitInput::new("root", path, *source, file))
            .collect();
        if reverse {
            inputs.reverse();
        }
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
            .unwrap()
            .validate()
            .unwrap();
        let unit =
            check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap();
        assert_eq!(unit.diagnostics().len(), 2, "{:?}", unit.diagnostics());
        assert!(
            unit.diagnostics()
                .iter()
                .all(|d| d.code().to_string() == "L0164")
        );
        assert_eq!(unit.borrow_results().range_return_origins().len(), 3);
        assert_eq!(unit.borrow_results().bindings().len(), 2);
        for binding in unit.borrow_results().bindings() {
            let UnitLoanTarget::Place(root) = binding.origin() else {
                panic!("actual caller root")
            };
            let root = root.root();
            let symbol = &names.names().source_units()[root.source_unit().index()]
                .resolution()
                .symbols()[root.symbol().index()];
            assert_eq!(symbol.span().source_id(), parsed[2].1);
            assert_eq!(symbol.name(), "root");
            let continuation = binding.source_loan().unwrap();
            let loan = unit
                .loans()
                .iter()
                .find(|loan| {
                    loan.call() == continuation.call() && loan.argument() == continuation.argument()
                })
                .unwrap();
            assert_eq!(loan.target(), binding.origin());
        }
        for returned in unit.borrow_results().range_return_origins() {
            let UnitLoanTarget::Place(root) = returned.origin() else {
                panic!("actual declaration root")
            };
            let root = root.root();
            let symbol = &names.names().source_units()[root.source_unit().index()]
                .resolution()
                .symbols()[root.symbol().index()];
            assert_eq!(
                symbol.span().source_id(),
                returned.declaration_span().source_id()
            );
            assert_ne!(symbol.span().source_id(), parsed[2].1);
        }
        assert!(unit.validate().is_err());
    }
}
