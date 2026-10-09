//! 新范围 descriptor 对实际 owning List 根的持有、root-flat 派生与终止。
use lang_frontend::{
    lexer::lex,
    name_resolution::{
        SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names, resolve_names,
    },
    ownership_checking::{
        BorrowBindingStorage, CompilationUnitOwnership, LoanTarget, OwnershipCheckedFile,
        UnitLoanTarget, check_compilation_unit_ownership, check_ownership,
    },
    parser::parse_file,
    source::SourceMap,
    type_checking::{check_compilation_unit_types, check_types, standard_environments},
};

fn checked(body: &str) -> (SourceMap, OwnershipCheckedFile, CompilationUnitOwnership) {
    let mut sources = SourceMap::new();
    let text = format!(
        "fun observe(source: View<String>): Unit {{}}\nfun alias(source: View<String>): borrow View<String> from source = source\nfun consume(own source: List<String>): Unit {{}}\n{body}"
    );
    let source = sources.add_source("range-construct.ko", text).unwrap();
    let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, source).unwrap();
    let names = resolve_names(&sources, &parsed, &environment).unwrap();
    assert!(names.diagnostics().is_empty());
    let typed = check_types(&sources, &parsed, &names, &types).unwrap();
    assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    let single = check_ownership(&sources, &parsed, &names, &typed).unwrap();
    let inputs = [SourceUnitInput::new(
        "std",
        "range-construct.ko",
        source,
        &parsed,
    )];
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

#[test]
fn constructing_a_range_continues_the_real_root_loan_and_ends_before_move() {
    let (_, single, unit) = checked(
        "fun run(): Unit { val source = listOf(\"kept\"); borrow val part = rangeView(source, 0, 1); observe(part); consume(source) }",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let [binding] = single.borrow_results().bindings() else {
        panic!("new descriptor binding")
    };
    assert_eq!(binding.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert!(binding.parent().is_none());
    let LoanTarget::Place(root) = binding.origin() else {
        panic!("owning root")
    };
    assert_ne!(root.root(), binding.binding());
    let continuation = binding.source_loan().unwrap();
    let loan = single
        .loans()
        .iter()
        .find(|loan| {
            loan.call() == continuation.call() && loan.argument() == continuation.argument()
        })
        .unwrap();
    assert_eq!(loan.target(), binding.origin());
    assert_eq!(
        single.borrow_results().ends()[0].binding(),
        binding.binding()
    );
    let [binding] = unit.borrow_results().bindings() else {
        panic!("unit new descriptor binding")
    };
    assert_eq!(binding.storage(), BorrowBindingStorage::NewRangeDescriptor);
    let UnitLoanTarget::Place(root) = binding.origin() else {
        panic!("unit owning root")
    };
    assert_ne!(root.root(), binding.binding());
    let continuation = binding.source_loan().unwrap();
    let loan = unit
        .loans()
        .iter()
        .find(|loan| {
            loan.call() == continuation.call() && loan.argument() == continuation.argument()
        })
        .unwrap();
    assert_eq!(loan.target(), binding.origin());
    assert_eq!(unit.borrow_results().ends()[0].binding(), binding.binding());
}

#[test]
fn range_iteration_keeps_root_and_noncopyable_element_read_only_in_single_and_unit() {
    for (body, code) in [
        (
            "fun run():Unit{val source=listOf(\"kept\");borrow val part=rangeView(source,0,1);for(item in part){consume(source)}}",
            "L0135",
        ),
        (
            "class Item(val n:Int){}\nfun consumeItem(own item:Item):Unit{}\nfun run(view:View<Item>):Unit{for(item in view){consumeItem(item)}}",
            "L0133",
        ),
    ] {
        let (_, single, unit) = checked(body);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(
                !diagnostics.is_empty(),
                "illegal move through a View provider was accepted"
            );
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == code),
                "{diagnostics:?}"
            );
        }
    }
}

#[test]
fn range_descriptor_iteration_never_publishes_an_owned_metadata_drop() {
    use lang_frontend::ownership_checking::{DropTarget, UnitDropTarget};
    let (_, single, unit) = checked(
        "fun run():Unit{val source=listOf(\"kept\");{borrow val part=rangeView(source,0,1);for(item in part){println(item)}};consume(source)}",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let binding = single.borrow_results().bindings()[0].binding();
    assert!(
        !single
            .drops()
            .iter()
            .any(|fact| fact.target() == DropTarget::Named(binding))
    );
    let binding = unit.borrow_results().bindings()[0].binding();
    assert!(
        !unit
            .drops()
            .iter()
            .any(|fact| fact.target() == UnitDropTarget::Named(binding))
    );
    for provider in single
        .iterations()
        .iter()
        .map(|plan| plan.descriptor().provider())
        .chain(
            unit.iterations()
                .iter()
                .map(|plan| plan.descriptor().provider()),
        )
    {
        assert_eq!(
            provider,
            lang_frontend::type_checking::IterationProvider::RangeView
        );
    }
}

#[test]
fn temporary_range_provider_continues_the_actual_root_in_single_and_unit() {
    let (sources, single, unit) =
        checked("fun run():Unit{for(item in rangeView(listOf(\"temporary\"),0,1)){println(item)}}");
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let [fact] = single.borrow_results().range_uses() else {
        panic!("single continuation");
    };
    let LoanTarget::Temporary(root) = *fact.origin() else {
        panic!("actual temporary root");
    };
    assert_ne!(root, fact.expression());
    assert_eq!(single.iterations()[0].source(), fact.origin());
    assert_eq!(
        sources
            .slice(
                single
                    .loans()
                    .iter()
                    .find(|loan| loan.call() == fact.source_loan().call()
                        && loan.argument() == fact.source_loan().argument())
                    .unwrap()
                    .begin_span()
            )
            .unwrap(),
        "listOf(\"temporary\")"
    );
    let [fact] = unit.borrow_results().range_uses() else {
        panic!("unit continuation");
    };
    let UnitLoanTarget::Temporary(root) = *fact.origin() else {
        panic!("unit actual temporary root");
    };
    assert_ne!(root, fact.expression());
    assert_eq!(unit.iterations()[0].source(), fact.origin());
}

#[test]
fn nested_view_construction_continues_the_original_temporary_collection_root() {
    let (sources, single, unit) = checked(
        "fun run():Unit{for(item in rangeView(rangeView(listOf(\"a\".clone()),0,1),0,1)){println(item)}}",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    assert_eq!(single.borrow_results().range_uses().len(), 2);
    for fact in single.borrow_results().range_uses() {
        let LoanTarget::Temporary(root) = *fact.origin() else {
            panic!("actual collection root");
        };
        assert_eq!(
            sources
                .slice(
                    single
                        .loans()
                        .iter()
                        .find(|loan| loan.argument() == root)
                        .unwrap()
                        .begin_span()
                )
                .unwrap(),
            "listOf(\"a\".clone())"
        );
    }
    assert_eq!(unit.borrow_results().range_uses().len(), 2);
    for fact in unit.borrow_results().range_uses() {
        let UnitLoanTarget::Temporary(root) = *fact.origin() else {
            panic!("unit collection root");
        };
        assert_eq!(
            sources
                .slice(
                    unit.loans()
                        .iter()
                        .find(|loan| loan.argument() == root)
                        .unwrap()
                        .begin_span()
                )
                .unwrap(),
            "listOf(\"a\".clone())"
        );
    }
    assert_eq!(
        single.iterations()[0].source(),
        single.borrow_results().range_uses()[0].origin()
    );
    assert_eq!(
        unit.iterations()[0].source(),
        unit.borrow_results().range_uses()[0].origin()
    );
}

#[test]
fn immediate_range_borrow_keeps_named_root_protected_through_later_arguments() {
    let (_, single, unit) = checked(
        "fun read(view:View<String>,own removed:List<String>):Unit{}\nfun run():Unit{val source=listOf(\"kept\");read(rangeView(source,0,1),source)}",
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn range_call_prefix_return_publishes_only_established_outer_argument_ends() {
    use lang_frontend::ownership_checking::{LoanEndPoint, RangeUseSite};
    for body in [
        "val root=listOf(\"kept\");read(rangeView(root,0,1),if(flag){return}else{0});consume(root)",
        "read(rangeView(listOf(\"kept\"),0,1),if(flag){return}else{0})",
        "val root=listOf(\"kept\");{borrow val parent=rangeView(root,0,1);read(rangeView(parent,0,1),if(flag){return}else{0})};consume(root)",
        "read(rangeView(rangeView(listOf(\"kept\"),0,1),0,1),if(flag){return}else{0})",
    ] {
        let (_, single, unit) = checked(&format!(
            "fun read(view:View<String>,number:Int):Unit{{}}\nfun run(flag:Boolean):Unit{{{body}}}"
        ));
        assert!(
            single.diagnostics().is_empty(),
            "{body}: {:?}",
            single.diagnostics()
        );
        assert!(
            unit.diagnostics().is_empty(),
            "{body}: {:?}",
            unit.diagnostics()
        );
        let range = single.borrow_results().range_uses().iter().find(|fact| {
            matches!(fact.site(), RangeUseSite::Call(call) if single.loan_ends().iter().any(|end| {
                end.call() == call && end.argument() == fact.expression()
                    && matches!(end.point(), LoanEndPoint::ControlTransfer(_))
            }))
        }).expect("the established outer range operand has a transfer end");
        let RangeUseSite::Call(call) = range.site() else {
            unreachable!()
        };
        assert!(single.loan_ends().iter().any(|end| end.call() == call
            && end.argument() == range.expression()
            && end.point() == LoanEndPoint::CallReturn(call)));
        assert!(unit.borrow_results().range_uses().iter().any(|fact| {
            matches!(fact.site(), RangeUseSite::Call(call) if unit.loans().iter().any(|loan| {
                loan.call() == call && loan.argument() == fact.expression()
                    && loan.target() == fact.origin()
            }))
        }));
    }
}

#[test]
fn conditional_call_does_not_enable_general_borrow_binding_last_use() {
    let (_, single, unit) = checked(
        "fun read(view:View<String>,number:Int):Unit{}\nfun run(flag:Boolean):Unit{val root=listOf(\"kept\");borrow val parent=rangeView(root,0,1);read(rangeView(parent,0,1),if(flag){return}else{0});consume(root)}",
    );
    for diagnostics in [single.diagnostics(), unit.diagnostics()] {
        assert!(
            diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn immediate_range_use_and_loop_restore_named_root_permissions() {
    for body in [
        "fun run():Unit{val source=listOf(\"kept\");observe(rangeView(source,0,1));consume(source)}",
        "fun run():Unit{val source=listOf(\"kept\");for(item in rangeView(source,0,1)){println(item)};consume(source)}",
    ] {
        let (_, single, unit) = checked(body);
        assert!(
            single.diagnostics().is_empty(),
            "{:?}",
            single.diagnostics()
        );
        assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    }
}

#[test]
fn temporary_range_cannot_escape_into_binding_or_return() {
    for body in [
        "fun run():Unit{borrow val saved=rangeView(listOf(\"temporary\"),0,1);observe(saved)}",
        "fun run():Unit{borrow val saved=alias(rangeView(listOf(\"temporary\"),0,1));observe(saved)}",
        "fun escape(source:List<String>):View<String> from source=rangeView(listOf(\"temporary\"),0,1)",
    ] {
        let (_, single, unit) = checked(body);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == "L0162"),
                "{diagnostics:?}"
            );
        }
        assert!(single.borrow_results().range_uses().is_empty());
        assert!(unit.borrow_results().range_uses().is_empty());
    }
}

#[test]
fn live_range_or_sibling_keeps_the_root_protected() {
    for body in [
        "fun run(): Unit { val source = listOf(\"kept\"); borrow val part = rangeView(source, 0, 1); consume(source); observe(part) }",
        "fun run():Unit{val source=listOf(\"kept\");borrow val parent=rangeView(source,0,1);borrow val child=rangeView(parent,0,1);consume(source);observe(child)}",
        "fun run(): Unit { val source = listOf(\"kept\"); borrow val left = rangeView(source, 0, 1); borrow val right = rangeView(source, 0, 0); observe(left); consume(source); observe(right) }",
    ] {
        let (_, single, unit) = checked(body);
        for diagnostics in [single.diagnostics(), unit.diagnostics()] {
            assert!(
                diagnostics.iter().any(|d| d.code().to_string() == "L0135"),
                "{diagnostics:?}"
            );
        }
    }
}

#[test]
fn derived_descriptor_inherits_root_without_parent_metadata_and_alias_keeps_metadata() {
    let (_, single, unit) = checked(
        "fun run(): Unit { val source = listOf(\"kept\"); borrow val parent = rangeView(source, 0, 1); borrow val child = rangeView(parent, 0, 1); borrow val metadata = alias(child); observe(metadata); consume(source) }",
    );
    assert!(
        single.diagnostics().is_empty(),
        "{:?}",
        single.diagnostics()
    );
    assert!(unit.diagnostics().is_empty(), "{:?}", unit.diagnostics());
    let [parent, child, metadata] = single.borrow_results().bindings() else {
        panic!("three bindings")
    };
    assert_eq!(parent.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert_eq!(child.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert!(child.parent().is_none());
    assert_eq!(
        metadata.storage(),
        BorrowBindingStorage::BorrowedCarrierMetadata
    );
    assert_eq!(metadata.parent(), Some(child.binding()));
    assert_eq!(parent.origin(), child.origin());
    assert_eq!(child.origin(), metadata.origin());
    assert_eq!(
        single
            .borrow_results()
            .ends()
            .iter()
            .map(|e| e.binding())
            .collect::<Vec<_>>(),
        [parent.binding(), metadata.binding(), child.binding()]
    );
    let [parent, child, metadata] = unit.borrow_results().bindings() else {
        panic!("three unit bindings")
    };
    assert_eq!(parent.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert_eq!(child.storage(), BorrowBindingStorage::NewRangeDescriptor);
    assert!(child.parent().is_none());
    assert_eq!(
        metadata.storage(),
        BorrowBindingStorage::BorrowedCarrierMetadata
    );
    assert_eq!(metadata.parent(), Some(child.binding()));
    assert_eq!(parent.origin(), child.origin());
    assert_eq!(child.origin(), metadata.origin());
    assert_eq!(
        unit.borrow_results()
            .ends()
            .iter()
            .map(|e| e.binding())
            .collect::<Vec<_>>(),
        [parent.binding(), metadata.binding(), child.binding()]
    );
}
