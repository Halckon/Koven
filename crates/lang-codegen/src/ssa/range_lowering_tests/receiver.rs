//! Trusted source receivers use the same checked descriptor ABI as free producers.
use super::*;

#[path = "receiver/count_break.rs"]
mod count_break;

const EXTENSIONS: &str = "borrow fun <T> List<T>.prefix(count:Int):View<T> from this=rangeView(this,0,count)\nborrow fun <T> View<T>.prefix(count:Int):View<T> from this=rangeView(this,0,count)";

#[test]
fn range_receiver_source_single_and_unit_keep_root_until_last_child() {
    let consumer = "fun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun consume(own source:List<String>):Unit{}\nfun main():Unit{val source=listOf(\"first\",\"second\");borrow val parent=source.prefix(2);borrow val child=parent.prefix(1);read(child);consume(source)}";
    for program in [
        memory_unit_llvm(&format!("{EXTENSIONS}\n{consumer}")),
        unit_with_provider_entry(
            &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
            EXTENSIONS,
        )
        .0,
    ] {
        let module = &program.modules[0];
        assert!(
            module
                .functions
                .iter()
                .filter(|f| f.carrier_return.is_some())
                .all(|f| f.receiver.is_none() && f.carrier_return == Some(0))
        );
        assert_eq!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.instructions)
                .filter(|i| matches!(i.operation, Operation::RangeCall { .. }))
                .count(),
            2
        );
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
fn range_receiver_source_native_single_and_reversed_unit_clean_exactly_once() {
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    for (declaration, element, values, read, drops) in [
        (
            "",
            "String",
            "\"first\".clone(),\"second\".clone()",
            "println(item)",
            "",
        ),
        (
            "class Item(val text:String){}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "println(item.text)",
            "",
        ),
        (
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "println(item.text)",
            "second\nfirst\n",
        ),
    ] {
        let consumer = format!(
            "{declaration}\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}\nfun consume(own source:List<{element}>):Unit{{println(\"consume\")}}\nfun main():Unit{{val source=listOf({values});borrow val parent=(source).prefix(2);borrow val child=(parent).prefix(1);read(child);println(\"after\");consume(source);println(\"done\")}}"
        );
        for (program, entry) in [
            memory_unit_with_entry(&format!("{EXTENSIONS}\n{consumer}")),
            unit_with_provider_order(
                &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
                EXTENSIONS,
                false,
            ),
            unit_with_provider_order(
                &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
                EXTENSIONS,
                true,
            ),
        ] {
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
            assert_success(
                &run,
                format!("first\nafter\nconsume\n{drops}done\n").as_bytes(),
            );
        }
    }
}

#[test]
fn range_receiver_source_count_control_flow_ends_receiver_before_early_return() {
    let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun scan(stop:Boolean):Unit{val source=listOf(Item(\"first\"),Item(\"second\"));read(source.prefix(if(stop){return}else{1}))}\nfun main():Unit{scan(false);scan(true)}";
    for (program, entry) in [
        memory_unit_with_entry(&format!("{EXTENSIONS}\n{consumer}")),
        unit_with_provider_order(
            &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
            EXTENSIONS,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
            &llvm,
            &[1, 0, 2, 4, 3, 5],
        );
        crate::native_tests::boxed_enum_tests::assert_success(
            &run,
            b"first\nsecond\nfirst\nsecond\nfirst\n",
        );
    }
}

#[test]
fn range_receiver_source_this_size_and_group_size_preserve_actual_view_extent() {
    let trusted = "borrow fun <T> List<T>.all():View<T> from this=rangeView(this,0,this.size)\nborrow fun <T> View<T>.all():View<T> from this=rangeView(this,0,(this).size)";
    let consumer = "fun read(view:View<String>):Unit{if(view.size!=2){error(\"size\")}}\nfun main():Unit{val source=listOf(\"first\",\"second\");borrow val parent=source.all();borrow val child=parent.all();read(child)}";
    for (program, entry) in [
        memory_unit_with_entry(&format!("{trusted}\n{consumer}")),
        unit_with_provider_order(
            &format!("package app\nimport koven.algorithms.all\n{consumer}"),
            trusted,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run =
            crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"");
    }
}

#[test]
fn range_receiver_source_temporary_chain_cleans_the_root_once() {
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun source():List<Item>{println(\"source\");return listOf(Item(\"first\"),Item(\"second\"))}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun main():Unit{read(source().prefix(2).prefix(1));println(\"done\")}";
    for (program, entry) in [
        memory_unit_with_entry(&format!("{EXTENSIONS}\n{consumer}")),
        unit_with_provider_order(
            &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
            EXTENSIONS,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
        assert_success(&run, b"source\nfirst\nsecond\nfirst\ndone\n");
    }
}

#[test]
fn range_receiver_source_forwarded_nonzero_view_preserves_absolute_offsets() {
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    let trusted = format!(
        "{EXTENSIONS}\nborrow fun <T> List<T>.window():View<T> from this=rangeView(this,1,2)\nborrow fun <T> View<T>.forward(count:Int):View<T> from this=this.prefix(count)"
    );
    let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun main():Unit{val root=listOf(Item(\"first\"),Item(\"second\"));borrow val parent=root.window();borrow val child=parent.forward(1);read(child);println(\"done\")}";
    for (program, entry) in [
        memory_unit_with_entry(&format!("{trusted}\n{consumer}")),
        unit_with_provider_order(
            &format!(
                "package app\nimport koven.algorithms.window\nimport koven.algorithms.forward\n{consumer}"
            ),
            &trusted,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
        assert_success(&run, b"second\ndone\nsecond\nfirst\n");
    }
}

#[test]
fn range_receiver_source_count_break_retains_existing_loop_cfg_frontier() {
    let body = "for(item in source){read(source.prefix(if(stop){break}else{1}))}";
    assert_cfg_frontier(body, "source", UnitCfgFrontier::CountBreak);
}

#[test]
fn range_receiver_source_borrow_parameter_rebinds_across_nonreturn_count_branch() {
    let trusted = EXTENSIONS;
    let consumer = "fun read(view:View<String>):Unit{println(\"read\")}\nfun scan(source:List<String>,flag:Boolean):Unit{read((source).prefix(if(flag){1}else{0}))}\nfun scan_view(source:View<String>,flag:Boolean):Unit{read((source).prefix(if(flag){1}else{0}))}\nfun main():Unit{val root=listOf(\"first\");scan(root,true);scan(root,false);borrow val parent=root.prefix(1);scan_view(parent,true);scan_view(parent,false)}";
    for (program, entry) in [
        memory_unit_with_entry(&format!("{trusted}\n{consumer}")),
        unit_with_provider_order(
            &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
            trusted,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run =
            crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(&llvm, &[0]);
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"read\nread\nread\nread\n");
    }
}

#[test]
fn range_receiver_source_retains_existing_binding_cfg_frontier() {
    assert_cfg_frontier(
        "borrow val part=source.prefix(if(stop){return}else{1});read(part)",
        "return",
        UnitCfgFrontier::Reject("return"),
    );
}

enum UnitCfgFrontier<'a> {
    Reject(&'a str),
    CountBreak,
}

fn assert_cfg_frontier(body: &str, fragment: &str, unit_frontier: UnitCfgFrontier<'_>) {
    use lang_frontend::{
        name_resolution::resolve_names, ownership_checking::check_ownership,
        type_checking::check_types,
    };
    let mut sources = SourceMap::new();
    let text = format!(
        "package app\n{EXTENSIONS}\nfun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}\nfun scan(stop:Boolean):Unit{{val source=listOf(\"first\");{body}}}\nfun main():Unit{{scan(false);scan(true)}}"
    );
    let (source, parsed) =
        crate::ssa::unit_lower_test_support::parsed(&mut sources, "frontier.ko", &text);
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
    assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
    let entry = names
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == "main")
        .unwrap()
        .id();
    let error = crate::ssa::lower_frontend::orchestrate::lower_scalar_file_with_entry(
        &sources, &parsed, &names, &typed, &owned, entry,
    )
    .err()
    .expect("the existing CFG frontier must reject source before emission");
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
    assert_eq!(sources.slice(error.span.unwrap()).unwrap(), fragment);
    {
        let inputs = [SourceUnitInput::new(
            "app",
            "app/frontier.ko",
            source,
            &parsed,
        )];
        let (names, typed, owned) =
            crate::ssa::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
        let lowered = crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            crate::ssa::unit_lower_test_support::declaration(&names, "app", "main"),
        );
        match unit_frontier {
            UnitCfgFrontier::Reject(unit_fragment) => {
                let error = lowered
                    .err()
                    .expect("the existing CFG frontier must reject source before emission");
                assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
                assert_eq!(sources.slice(error.span.unwrap()).unwrap(), unit_fragment);
            }
            UnitCfgFrontier::CountBreak => {
                let (program, entry) =
                    lowered.expect("real loop-target cleanup admits this Unit source");
                let [source_unit] = names.names().index().source_units() else {
                    panic!("one original source unit")
                };
                assert_eq!(source_unit.source_id(), source);
                let receiver_begin_span = assert_count_break_products(
                    &sources,
                    &parsed,
                    source_unit.id(),
                    owned.ownership(),
                );
                count_break::assert_count_break_structure(&sources, &program, receiver_begin_span);
                verify_program(&program).unwrap();
                crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            }
        }
    }
}

// Observe the original public cleanup products, not a reconstructed liveness set.
fn assert_count_break_products(
    sources: &SourceMap,
    parsed: &lang_frontend::parser::ParsedFile,
    source_unit: lang_frontend::name_resolution::SourceUnitId,
    owned: &lang_frontend::ownership_checking::CompilationUnitOwnership,
) -> lang_frontend::source::Span {
    use lang_frontend::ownership_checking::{
        UnitDropTarget, UnitIterationCleanupAction as Action, UnitIterationExitKind as Exit,
        UnitLoanTarget, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    };
    use lang_frontend::type_checking::UnitCallReceiverOrigin;

    let text = |expression: lang_frontend::type_checking::UnitExpressionId| {
        assert_eq!(expression.source_unit(), source_unit);
        sources
            .slice(
                parsed
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .unwrap()
                    .span(),
            )
            .unwrap()
    };
    let plans = owned
        .iterations()
        .iter()
        .filter(|plan| text(plan.descriptor().source()) == "source")
        .collect::<Vec<_>>();
    let [plan] = plans.as_slice() else {
        panic!("one original scan provider")
    };
    let Some(UnitLoanTarget::Place(place)) = plan.source() else {
        panic!("original named List source")
    };
    assert!(place.is_root());
    let statement = plan.descriptor().statement();
    let exits = plan
        .exits()
        .iter()
        .filter(|exit| matches!(exit.kind(), Exit::Break(_) | Exit::Exhaustion))
        .collect::<Vec<_>>();
    assert_eq!(exits.len(), 2);
    let mut receiver_begin_span = None;
    for exit in exits {
        let actions = exit.actions();
        assert_eq!(owned.iteration_cleanup_at(exit.point()), Some(actions));
        let position = |action| actions.iter().position(|actual| *actual == action).unwrap();
        let finish = position(Action::FinishProvider(statement));
        let end_source = position(Action::EndSource(statement));
        let drops = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                Action::Drop(fact) if fact.target() == UnitDropTarget::Named(place.root()) => {
                    Some((index, fact))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(drop_index, fact)] = drops.as_slice() else {
            panic!("one source drop on each real exit, including break")
        };
        assert!(finish < end_source && end_source < *drop_index);
        assert_eq!(fact.point(), exit.point());
        assert_eq!(
            owned.drops().iter().filter(|other| *other == *fact).count(),
            1
        );
        let receivers = actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                Action::EndReceiverLoan(fact) => Some((index, fact)),
                _ => None,
            })
            .collect::<Vec<_>>();
        match exit.kind() {
            Exit::Break(jump) => {
                assert_eq!(text(jump), "break");
                let [(receiver_index, fact)] = receivers.as_slice() else {
                    panic!("cancel the exact evaluated receiver before its provider")
                };
                assert_eq!(text(fact.call()), "source.prefix(if(stop){break}else{1})");
                let UnitCallReceiverOrigin::Expression(receiver) = fact.source() else {
                    panic!("source receiver expression")
                };
                assert_eq!(text(receiver), "source");
                let span = parsed
                    .ast()
                    .expressions()
                    .get(receiver.expression())
                    .unwrap()
                    .span();
                assert_eq!(fact.begin_span(), span);
                assert!(receiver_begin_span.replace(span).is_none());
                assert_eq!(fact.kind(), UnitReceiverOwnershipKind::SharedLoan);
                assert!(matches!(
                    fact.target(),
                    UnitReceiverOwnershipTarget::Place(actual) if actual == place
                ));
                let end_element = position(Action::EndElement(statement));
                assert!(*receiver_index < end_element && end_element < finish);
            }
            Exit::Exhaustion => assert!(receivers.is_empty()),
            _ => unreachable!(),
        }
    }
    receiver_begin_span.expect("one original receiver loan begin")
}

#[test]
fn range_receiver_source_temporary_for_cleans_the_root_after_elements() {
    let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun source():List<Item>{println(\"source\");return listOf(Item(\"first\"),Item(\"second\"))}\nfun main():Unit{for(item in source().prefix(2).prefix(1)){println(item.text)};println(\"done\")}";
    for (program, entry) in [
        memory_unit_with_entry(&format!("{EXTENSIONS}\n{consumer}")),
        unit_with_provider_order(
            &format!("package app\nimport koven.algorithms.prefix\n{consumer}"),
            EXTENSIONS,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
            &llvm,
            &[1, 0, 2],
        );
        crate::native_tests::boxed_enum_tests::assert_success(
            &run,
            b"source\nfirst\nsecond\nfirst\ndone\n",
        );
    }
}

#[test]
fn real_std_receiver_take_preserves_nonzero_view_offsets_and_top_level_compatibility() {
    let trusted = "fun window(source:List<String>):View<String> from source=rangeView(source,1,2)";
    let consumer = "fun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun main():Unit{val root=listOf(\"first\".clone(),\"second\".clone());borrow val parent=window(root);borrow val child=parent.take(2147483647);read(child);read(take(child,1));read(child.take(0))}";
    for (program, entry) in [
        memory_unit_with_entry(&format!("{trusted}\n{consumer}")),
        unit_with_provider_order(
            &format!(
                "package app\nimport koven.algorithms.take\nimport koven.algorithms.window\n{consumer}"
            ),
            trusted,
            true,
        ),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = crate::native_tests::boxed_enum_tests::run_counted_allocations_in_order(
            &llvm,
            &[1, 0, 2],
        );
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"second\nsecond\n");
    }
}

#[test]
fn range_receiver_named_root_short_borrow_cleans_after_consumer_and_return() {
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    for (declaration, element, values, read, drops) in [
        (
            "",
            "String",
            "\"first\".clone(),\"second\".clone()",
            "println(item)",
            "",
        ),
        (
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "println(item.text)",
            "second\nfirst\n",
        ),
    ] {
        for operation in ["cut(root,1)", "root.prefix(1)"] {
            for reader in [
                "println(\"read\")".to_owned(),
                format!("for(item in view){{{read}}};println(\"read\")"),
            ] {
                let provider = format!(
                    "{EXTENSIONS}\nfun <T> cut(source:List<T>,count:Int):View<T> from source=rangeView(source,0,count)"
                );
                let consumer = format!(
                    "{declaration}\nfun read(view:View<{element}>,number:Int):Unit{{{reader}}}\nfun scan(stop:Boolean):Unit{{val root=listOf({values});read({operation},if(stop){{return}}else{{0}});println(\"after\")}}\nfun main():Unit{{scan(false);scan(true);println(\"done\")}}"
                );
                for (program, entry) in [
                    memory_unit_with_entry(&format!("{provider}\n{consumer}")),
                    unit_with_provider_order(
                        &format!(
                            "package app\nimport koven.algorithms.prefix\nimport koven.algorithms.cut\n{consumer}"
                        ),
                        &provider,
                        true,
                    ),
                ] {
                    let llvm =
                        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                    let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2, 4, 3, 5]);
                    let prefix = if reader.starts_with("for") {
                        "first\n"
                    } else {
                        ""
                    };
                    assert_success(
                        &run,
                        format!("{prefix}read\nafter\n{drops}{drops}done\n").as_bytes(),
                    );
                }
            }
        }
    }
}
