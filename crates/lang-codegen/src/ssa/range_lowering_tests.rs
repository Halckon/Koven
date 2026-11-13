//! Healthy source pipeline; negative verifier checks never compile defective IR.
use super::{model::*, verify::verify_program};
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

mod temporary {
    //! Healthy trusted std take sources; allocator identities observe normal releases.
    use super::*;

    #[test]
    fn range_temporary_native_single_and_unit_release_string_move_only_and_resource_identities() {
        use crate::native_tests::boxed_enum_tests::{
            assert_success, run_counted_allocations_in_order,
        };
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
            for (count, body, visits) in [
                (0, "", ""),
                (1, "", "first\n"),
                (2147483647, "", "first\nsecond\n"),
                (2, "break", "first\n"),
                (2, "continue", "first\nsecond\n"),
                (2, "return", "first\n"),
            ] {
                let consumer = format!(
                    "{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf({values})}}\nfun scan():Unit{{for(item in take(source(),{count})){{{read};{body}}}}}\nfun main():Unit{{scan();println(\"done\")}}"
                );
                for (program, entry) in [
                    memory_unit_with_entry(&consumer),
                    unit_with_entry(&format!(
                        "package app\nimport koven.algorithms.take\n{consumer}"
                    )),
                ] {
                    let llvm =
                        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                    // Factories create elements 0/1 then List 2; all roots release in reverse element order.
                    let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
                    assert_success(&run, format!("source\n{visits}{drops}done\n").as_bytes());
                }
            }
            let consumer = format!(
                "{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf({values})}}\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}};println(\"read-end\")}}\nfun main():Unit{{read(take(source(),1));println(\"done\")}}"
            );
            for (program, entry) in [
                memory_unit_with_entry(&consumer),
                unit_with_entry(&format!(
                    "package app\nimport koven.algorithms.take\n{consumer}"
                )),
            ] {
                let llvm =
                    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
                assert_success(
                    &run,
                    format!("source\nfirst\nread-end\n{drops}done\n").as_bytes(),
                );
            }
        }
    }

    #[test]
    fn range_temporary_sequential_uses_and_loop_exits_keep_correlated_ssa_facts() {
        for body in ["", "break", "continue", "return"] {
            let consumer = format!(
                "class Item(val n:Int){{}}\nfun source():List<Item> =listOf(Item(1))\nfun read(view:View<Item>):Unit{{for(item in view){{println(\"visit\")}}}}\nfun scan():Unit{{read(take(source(),1));read(take(source(),1));for(item in take(source(),1)){{println(\"visit\");{body}}};for(item in take(source(),1)){{println(\"visit\")}}}}\nfun main():Unit{{scan()}}"
            );
            memory_unit(&consumer);
            unit(&format!(
                "package app\nimport koven.algorithms.take\n{consumer}"
            ));
        }
    }

    #[test]
    fn range_temporary_verifier_rejects_premature_root_or_descriptor_end() {
        // The broken forms below stay within the pure verifier; no native emission follows.
        for end_root in [true, false] {
            let mut program = unit(
                "package app\nimport koven.algorithms.take\nfun main():Unit{for(item in take(listOf(\"a\".clone()),1)){println(item)}}",
            );
            let f = program.modules[0]
                .functions
                .iter_mut()
                .find(|f| {
                    f.instructions
                        .iter()
                        .any(|i| matches!(i.operation, Operation::RangeCall { .. }))
                })
                .unwrap();
            let (view, parent) = f
                .instructions
                .iter()
                .find_map(|i| match (&i.operation, i.results.as_slice()) {
                    (
                        Operation::RangeCall { source, .. },
                        [EntityId::Value(view), EntityId::Loan(_)],
                    ) => Some((*view, *source)),
                    _ => None,
                })
                .unwrap();
            let end = f
                .instructions
                .iter_mut()
                .find(|i| matches!(i.operation, Operation::RangeEnd { .. }))
                .unwrap();
            end.operation = if end_root {
                Operation::BorrowEnd { loan: parent }
            } else {
                Operation::Drop { owner: view }
            };
            assert!(verify_program(&program).is_err());
        }
    }
}

mod call_prefix {
    //! Ordinary legal argument return cleans only the already evaluated call prefix.
    use super::*;

    fn source_case(
        named: bool,
        view: bool,
    ) -> (&'static str, &'static str, &'static str, &'static str) {
        match (named, view) {
            (true, false) => ("val root=source()", "take(root,1)", "", "consume(root)"),
            (false, false) => ("", "take(source(),1)", "", ""),
            (true, true) => (
                "val root=source();{borrow val parent=take(root,2)",
                "take(parent,1)",
                "};",
                "consume(root)",
            ),
            (false, true) => ("", "take(take(source(),2),1)", "", ""),
        }
    }

    fn consumer(
        declaration: &str,
        element: &str,
        values: &str,
        read: &str,
        named: bool,
        view: bool,
    ) -> String {
        let (prefix, operand, close, after) = source_case(named, view);
        format!(
            "{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf({values})}}\nfun consume(own source:List<{element}>):Unit{{}}\nfun read(view:View<{element}>,number:Int):Unit{{for(item in view){{{read}}};println(\"called\")}}\nfun scan(stop:Boolean):Unit{{{prefix};read({operand},if(stop){{return}}else{{0}});{close}println(\"after\");{after}}}\nfun main():Unit{{scan(false);scan(true);println(\"done\")}}"
        )
    }

    fn check_both(named: bool, view: bool) {
        let source = consumer(
            "class Item(val text:String){deinit(){println(this.text)}}",
            "Item",
            "Item(\"first\"),Item(\"second\")",
            "println(item.text)",
            named,
            view,
        );
        for program in [
            memory_unit(&source),
            unit(&format!(
                "package app\nimport koven.algorithms.take\n{source}"
            )),
        ] {
            let function = program.modules[0]
                .functions
                .iter()
                .find(|f| f.name.contains("scan"))
                .unwrap();
            let returns = function
                .blocks
                .iter()
                .filter(|block| {
                    matches!(
                        block.terminator.as_ref().map(|t| &t.kind),
                        Some(TerminatorKind::Return { .. })
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(returns.len(), 2);
            let cancelled = returns
                .iter()
                .find(|block| {
                    block.instructions.iter().all(|id| {
                        !matches!(
                            function.instruction(*id).unwrap().operation,
                            Operation::DirectCall { .. }
                        )
                    })
                })
                .expect("the early path never calls the consumer");
            let operations = cancelled
                .instructions
                .iter()
                .map(|id| &function.instruction(*id).unwrap().operation)
                .collect::<Vec<_>>();
            let range_end = operations
                .iter()
                .position(|operation| matches!(operation, Operation::RangeEnd { .. }))
                .unwrap();
            let drop = operations
                .iter()
                .position(|operation| matches!(operation, Operation::Drop { .. }))
                .unwrap();
            assert!(range_end < drop);
        }
    }

    #[test]
    fn range_call_prefix_named_list_later_argument_return_is_legal() {
        check_both(true, false);
    }
    #[test]
    fn range_call_prefix_temporary_list_later_argument_return_is_legal() {
        check_both(false, false);
    }
    #[test]
    fn range_call_prefix_named_view_later_argument_return_is_legal() {
        check_both(true, true);
    }
    #[test]
    fn range_call_prefix_temporary_view_later_argument_return_is_legal() {
        check_both(false, true);
    }

    #[test]
    fn range_call_prefix_native_restores_roots_and_releases_each_identity_on_both_branches() {
        use crate::native_tests::boxed_enum_tests::{
            assert_success, run_counted_allocations_in_order,
        };
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
            for (named, view) in [(true, false), (false, false), (true, true), (false, true)] {
                let source = consumer(declaration, element, values, read, named, view);
                for (program, entry) in [
                    memory_unit_with_entry(&source),
                    unit_with_entry(&format!(
                        "package app\nimport koven.algorithms.take\n{source}"
                    )),
                ] {
                    let llvm =
                        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                    let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2, 4, 3, 5]);
                    let normal = if named {
                        format!("first\ncalled\nafter\n{drops}")
                    } else {
                        format!("first\ncalled\n{drops}after\n")
                    };
                    assert_success(
                        &run,
                        format!("source\n{normal}source\n{drops}done\n").as_bytes(),
                    );
                }
            }
        }
    }

    #[test]
    fn range_call_prefix_owned_arguments_transfer_only_after_the_later_argument_completes() {
        use crate::native_tests::boxed_enum_tests::{
            assert_success, run_counted_allocations_in_order,
        };
        for (declaration, element, values, sent, later, read, root_drops, sent_drop, later_drop) in [
            (
                "",
                "String",
                "\"first\".clone(),\"second\".clone()",
                "\"sent\".clone()",
                "\"later\".clone()",
                "println(item)",
                "",
                "",
                "",
            ),
            (
                "class Item(val text:String){}",
                "Item",
                "Item(\"first\"),Item(\"second\")",
                "Item(\"sent\")",
                "Item(\"later\")",
                "println(item.text)",
                "",
                "",
                "",
            ),
            (
                "class Item(val text:String){deinit(){println(this.text)}}",
                "Item",
                "Item(\"first\"),Item(\"second\")",
                "Item(\"sent\")",
                "Item(\"later\")",
                "println(item.text)",
                "second\nfirst\n",
                "sent\n",
                "later\n",
            ),
        ] {
            for (named, view) in [(true, false), (false, false), (true, true), (false, true)] {
                let (prefix, operand, close, after) = source_case(named, view);
                let source = format!(
                    "{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf({values})}}\nfun sent():List<{element}>{{println(\"sent-source\");return listOf({sent})}}\nfun later():List<{element}>{{println(\"late-source\");return listOf({later})}}\nfun consume(own source:List<{element}>):Unit{{}}\nfun read(view:View<{element}>,own sent:List<{element}>,number:Int,own later:List<{element}>):Unit{{for(item in view){{{read}}};println(\"called\");consume(sent);consume(later)}}\nfun scan(stop:Boolean):Unit{{{prefix};read({operand},sent(),if(stop){{return}}else{{0}},later());{close}println(\"after\");{after}}}\nfun main():Unit{{scan(false);scan(true);println(\"done\")}}"
                );
                for (program, entry) in [
                    memory_unit_with_entry(&source),
                    unit_with_entry(&format!(
                        "package app\nimport koven.algorithms.take\n{source}"
                    )),
                ] {
                    let llvm =
                        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                    let run = run_counted_allocations_in_order(
                        &llvm,
                        &[3, 4, 5, 6, 1, 0, 2, 10, 11, 8, 7, 9],
                    );
                    let normal = if named {
                        format!("{sent_drop}{later_drop}after\n{root_drops}")
                    } else {
                        format!("{sent_drop}{later_drop}{root_drops}after\n")
                    };
                    assert_success(&run, format!("source\nsent-source\nlate-source\nfirst\ncalled\n{normal}source\nsent-source\n{sent_drop}{root_drops}done\n").as_bytes());
                }
            }
        }
    }
}

#[path = "range_lowering_tests/scalar_counts.rs"]
pub(in crate::ssa) mod scalar_counts;

#[path = "range_lowering_tests/local_control.rs"]
pub(in crate::ssa) mod local_control;

mod drop_algorithms {
    //! Healthy source checks for std range algorithms; no IR mutation.
    use super::*;

    #[test]
    fn range_drop_algorithms_single_and_unit_share_the_descriptor_abi() {
        let consumer = "fun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun consume(own root:List<String>):Unit{}\nfun main():Unit{val root=listOf(\"a\",\"b\",\"c\");borrow val parent=drop(root,1);borrow val child=parent.dropLast(1);borrow val sibling=dropLast(parent,0);borrow val empty=parent.drop(2147483647);read(child);read(sibling);read(empty);consume(root)}";
        for program in [
            memory_unit_llvm(consumer),
            unit(&format!(
                "package app\nimport koven.algorithms.drop\nimport koven.algorithms.dropLast\n{consumer}"
            )),
        ] {
            assert!(
                program.modules[0]
                    .functions
                    .iter()
                    .flat_map(|f| &f.instructions)
                    .any(|i| matches!(i.operation, Operation::RangeCall { .. }))
            );
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }

    #[test]
    fn range_drop_algorithms_reuse_arbitrary_named_dynamic_boundary_producers() {
        let trusted = "fun <T> window(source:List<T>,begin:Int,end:Int):View<T> from source=rangeView(source,begin,end)\nfun <T> window(source:View<T>,begin:Int,end:Int):View<T> from source=rangeView(source,begin,end)";
        let consumer = "fun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun main():Unit{val root=listOf(\"a\",\"b\",\"c\",\"d\");borrow val parent=window(root,1,4);borrow val child=window(parent,1,2);read(dropLast(child,0));read(drop(parent,1))}";
        for program in [memory_unit_llvm(&format!("{trusted}\n{consumer}")), unit_with_provider_entry(&format!("package app\nimport koven.algorithms.window\nimport koven.algorithms.drop\nimport koven.algorithms.dropLast\n{consumer}"), trusted).0] {
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }

    #[test]
    fn range_drop_algorithms_single_and_reversed_unit_run_unmodified_llvm_natively() {
        use std::{fs, process::Command};
        let trusted = "fun <T> window(source:List<T>,begin:Int,end:Int):View<T> from source=rangeView(source,begin,end)";
        let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun consume(own root:List<Item>):Unit{println(\"consume\")}\nfun main():Unit{val root=listOf(Item(\"a\"),Item(\"b\"),Item(\"c\"),Item(\"d\"));borrow val parent=window(root,1,4);borrow val child=parent.dropLast(1);borrow val last=drop(child,1);read(last);read(child);consume(root);println(\"done\")}";
        for (index, (program, entry)) in [
            memory_unit_with_entry(&format!("{trusted}\n{consumer}")),
            unit_with_provider_order(&format!("package app\nimport koven.algorithms.window\nimport koven.algorithms.drop\nimport koven.algorithms.dropLast\n{consumer}"), trusted, true),
        ].into_iter().enumerate() {
            let directory = std::env::temp_dir().join(format!("koven-drop-native-{}-{index}", std::process::id()));
            fs::create_dir_all(&directory).unwrap();
            let ir = directory.join("program.ll");
            let executable = directory.join("program");
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            fs::write(&ir, llvm).unwrap();
            let linked = Command::new(crate::test_support::ir_clang()).arg(&ir).arg("-o").arg(&executable).output().unwrap();
            assert!(linked.status.success(), "{linked:?}");
            let run = Command::new(&executable).output().unwrap();
            assert_eq!(run.status.code(), Some(0), "{run:?}");
            assert_eq!(run.stdout, b"c\nb\nc\nconsume\nd\nc\nb\na\ndone\n");
            assert!(run.stderr.is_empty(), "{run:?}");
            fs::remove_dir_all(directory).unwrap();
        }
    }
}

pub(in crate::ssa) fn unit(consumer: &str) -> Program {
    unit_with_entry(consumer).0
}

pub(in crate::ssa) fn unit_with_entry(consumer: &str) -> (Program, FunctionId) {
    unit_with_provider_entry(consumer, "")
}

pub(in crate::ssa) fn unit_with_provider_entry(
    consumer: &str,
    trusted: &str,
) -> (Program, FunctionId) {
    unit_with_provider_order(consumer, trusted, false)
}

pub(in crate::ssa) fn unit_with_provider_order(
    consumer: &str,
    trusted: &str,
    reverse: bool,
) -> (Program, FunctionId) {
    let mut sources = SourceMap::new();
    let (p, provider) = super::unit_lower_test_support::parsed(
        &mut sources,
        "ranges.ko",
        &format!(
            "{}\n{trusted}",
            include_str!("../../../lang-std/koven/algorithms/ranges.ko")
        ),
    );
    let (q, consumer) = super::unit_lower_test_support::parsed(&mut sources, "main.ko", consumer);
    let mut inputs = [
        SourceUnitInput::new("std", "koven/algorithms/ranges.ko", p, &provider),
        SourceUnitInput::new("app", "app/main.ko", q, &consumer),
    ];
    if reverse {
        inputs.reverse();
    }
    let (environment, mut types) = standard_environments();
    types.authorize_range_source(&sources, p).unwrap();
    types.authorize_range_extension_source(&sources, p).unwrap();
    let (names, typed, owned) =
        super::unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let result = super::unit_lower::lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        super::unit_lower_test_support::declaration(&names, "app", "main"),
    );
    let (program, entry) = result.unwrap_or_else(|error| {
        panic!(
            "{error:?}: {:?}",
            error.span.and_then(|span| sources.slice(span).ok())
        )
    });
    verify_program(&program).unwrap();
    (program, entry)
}

#[test]
pub(in crate::ssa) fn range_temporary_for_single_and_unit_preserve_root_until_metadata_ends() {
    let consumer = "class Token(val text:String){deinit(){println(this.text)}}\nfun source():List<Token>{println(\"source\");return listOf(Token(\"first\"),Token(\"second\"))}\nfun main():Unit{for(item in take(source(),1)){println(item.text)}}";
    memory_unit(consumer);
    unit(&format!(
        "package app\nimport koven.algorithms.take\n{consumer}"
    ));
}

#[test]
pub(in crate::ssa) fn range_temporary_call_single_and_unit_preserve_root_until_metadata_ends() {
    let consumer = "class Token(val text:String){deinit(){println(this.text)}}\nfun source():List<Token>{println(\"source\");return listOf(Token(\"first\"),Token(\"second\"))}\nfun read(view:View<Token>):Unit{for(item in view){println(item.text)}}\nfun main():Unit{read(take(source(),1))}";
    memory_unit(consumer);
    unit(&format!(
        "package app\nimport koven.algorithms.take\n{consumer}"
    ));
}

#[test]
pub(in crate::ssa) fn range_list_producer_size_uses_metadata_and_source_loan() {
    let program = unit(
        "package app\nimport koven.algorithms.take\nfun check(source:View<String>,expected:Int):Unit { if(source.size != expected){error(\"wrong\")} }\nfun main():Unit {val source=listOf(\"kept\"); borrow val part=take(source,2147483647);check(part,1);println(\"done\")}",
    );
    let module = &program.modules[0];
    assert!(module.functions.iter().any(|f| f.carrier_return.is_some()));
    assert!(module.functions.iter().any(|f| {
        f.instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::RangeEnd { .. }))
    }));
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
pub(in crate::ssa) fn range_single_and_unit_for_deliver_only_shared_elements() {
    let consumer = "class Item(val text:String){}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun main():Unit{val source=listOf(Item(\"first\"),Item(\"excluded\"));borrow val part=take(source,1);read(part)}";
    for program in [
        memory_unit_llvm(consumer),
        unit(&format!(
            "package app\nimport koven.algorithms.take\n{consumer}"
        )),
    ] {
        let reader = program.modules[0]
            .functions
            .iter()
            .find(|f| {
                f.instructions
                    .iter()
                    .any(|i| matches!(i.operation, Operation::RangeElementPlace { .. }))
            })
            .unwrap();
        assert!(reader.instructions.iter().any(|i| matches!(
            i.operation,
            Operation::RangeLength {
                view: EntityId::Loan(_)
            }
        )));
        assert!(!reader.instructions.iter().any(|i| matches!(
            i.operation,
            Operation::Copy { .. }
                | Operation::HeapAllocate { .. }
                | Operation::SharedRetain { .. }
        )));
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
pub(in crate::ssa) fn range_single_and_unit_named_for_transport_descriptor_and_ancestor_loans() {
    for (case, exit) in ["", "break", "continue"].into_iter().enumerate() {
        let consumer = format!(
            "class Item(val text:String){{}}\nfun consume(own source:List<Item>):Unit{{}}\nfun main():Unit{{val source=listOf(Item(\"first\"),Item(\"excluded\"));{{borrow val part=take(source,1);for(item in part){{println(item.text);{exit}}}}};consume(source)}}"
        );
        for (entry, program) in [
            ("single", memory_unit_llvm(&consumer)),
            (
                "unit",
                unit(&format!(
                    "package app\nimport koven.algorithms.take\n{consumer}"
                )),
            ),
        ] {
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            for (kind, text) in [
                ("ssa", super::render_program(&program)),
                ("model", format!("{:#?}", program.modules)),
                ("llvm", llvm),
            ] {
                diagnostic!(
                    "\nBEGIN result-source case={case} entry={entry} kind={kind} bytes={}",
                    text.len()
                );
                print!("{text}");
                diagnostic!("\nEND result-source case={case} entry={entry} kind={kind}");
            }
        }
    }
}

#[test]
pub(in crate::ssa) fn range_verifier_rejects_metadata_end_while_an_element_loan_is_active() {
    let mut program = unit(
        "package app\nimport koven.algorithms.take\nfun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun main():Unit{val source=listOf(\"first\");borrow val part=take(source,1);read(part)}",
    );
    let function = program.modules[0]
        .functions
        .iter_mut()
        .find(|f| {
            f.instructions
                .iter()
                .any(|i| matches!(i.operation, Operation::RangeElementPlace { .. }))
        })
        .unwrap();
    let (place, source) = function
        .instructions
        .iter()
        .find_map(|i| match (&i.operation, i.results.as_slice()) {
            (Operation::RangeElementPlace { view, .. }, [EntityId::Place(place)]) => {
                Some((*place, *view))
            }
            _ => None,
        })
        .unwrap();
    let begin = function
        .instructions
        .iter()
        .find(|i| matches!(i.operation,Operation::BorrowBegin {place:actual,..} if actual==place))
        .unwrap()
        .id;
    let block = function
        .blocks
        .iter()
        .find(|block| block.instructions.contains(&begin))
        .unwrap()
        .id;
    let terminator = function.blocks[block.index()].terminator.take();
    let (end, _) = function
        .append_instruction(
            block,
            Operation::BorrowEnd { loan: source },
            Vec::new(),
            function.origin.clone(),
        )
        .unwrap();
    function.blocks[block.index()].terminator = terminator;
    let instructions = &mut function.blocks[block.index()].instructions;
    assert_eq!(instructions.pop(), Some(end));
    let position = instructions.iter().position(|id| *id == begin).unwrap() + 1;
    instructions.insert(position, end);
    let errors = verify_program(&program).expect_err("metadata must outlive its shared element");
    assert_diagnostic!(
        errors.errors.iter().any(|e| matches!(
            e.kind,
            super::verify::VerifyErrorKind::LoanDependencyActive { .. }
        )),
        "{errors:?}"
    );
    // Invalid SSA is checked only by the verifier, never emitted or executed.
}

#[test]
pub(in crate::ssa) fn range_verifier_preserves_descriptor_root_pairing_through_loop_edges() {
    let mut program = unit(
        "package app\nimport koven.algorithms.take\nfun main():Unit{val a=listOf(\"a\");val b=listOf(\"b\");{borrow val left=take(a,1);borrow val right=take(b,1);for(item in left){println(item)};for(item in right){println(item)}}}",
    );
    let caller = program.modules[0]
        .functions
        .iter_mut()
        .find(|f| {
            f.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::RangeCall { .. }))
                .count()
                == 2
        })
        .unwrap();
    let ends = caller
        .instructions
        .iter()
        .filter_map(|i| match i.operation {
            Operation::RangeEnd { view, source } => Some((i.id, view, source)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 2);
    assert_eq!(
        caller.entity(EntityId::Loan(ends[0].2)).unwrap().ty,
        caller.entity(EntityId::Loan(ends[1].2)).unwrap().ty
    );
    caller.instructions[ends[0].0.index()].operation = Operation::RangeEnd {
        view: ends[0].1,
        source: ends[1].2,
    };
    caller.instructions[ends[1].0.index()].operation = Operation::RangeEnd {
        view: ends[1].1,
        source: ends[0].2,
    };
    let errors = verify_program(&program)
        .expect_err("same List type cannot substitute a different descriptor's root");
    assert_diagnostic!(
        errors
            .errors
            .iter()
            .any(|e| matches!(e.kind, super::verify::VerifyErrorKind::ReturnType { .. })),
        "{errors:?}"
    );
    // Crossed pairs stay within this pure verifier test; no LLVM/native call follows.
}

// Historical `single` test names and labels identify this original source layout,
// not a legacy checker. Never replace it with the separate provider/consumer arm.
pub(in crate::ssa) fn memory_unit(consumer: &str) -> Program {
    memory_unit_roots(consumer, false).0
}

pub(in crate::ssa) fn memory_unit_llvm(consumer: &str) -> Program {
    memory_unit_with_entry(consumer).0
}

pub(in crate::ssa) fn memory_unit_with_entry(consumer: &str) -> (Program, FunctionId) {
    memory_unit_roots(consumer, true)
}

pub(in crate::ssa) fn memory_unit_roots(consumer: &str, check_llvm: bool) -> (Program, FunctionId) {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::{
        name_resolution::{SourceUnitKey, SymbolKind},
        parser::{Item, NameMarker},
    };

    let text = format!(
        "{}\n{consumer}",
        include_str!("../../../lang-std/koven/algorithms/ranges.ko")
            .strip_prefix("package koven.algorithms\n")
            .unwrap()
    );
    let analysis =
        UnitSource::analyze_with_type_environment("trusted.ko", &text, |sources, source, types| {
            types.authorize_range_source(sources, source).unwrap();
            types
                .authorize_range_extension_source(sources, source)
                .unwrap();
        })
        .unwrap_or_else(|error| panic!("original trusted Range admission: {error:?}"));
    analysis.assert_frontend_clean();
    let parsed = analysis.parsed();
    let index = analysis.names().names().index();
    let [source] = index.source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(source.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(source.source_id(), parsed.source_id());
    assert_eq!(
        analysis.sources().source_name(source.source_id()).unwrap(),
        "trusted.ko"
    );
    assert_eq!(
        analysis.sources().source_text(source.source_id()).unwrap(),
        text
    );
    let main = analysis.declaration("main", SymbolKind::Function);
    let mut main_program = None;
    let mut function_roots = 0;

    // The old whole-file lowerer admitted every non-generic top-level function,
    // including uncalled consume/read helpers. Each root keeps that obligation
    // in an independent Program; these Programs are not one cross-function graph.
    for root in parsed.roots() {
        let node = parsed.ast().items().get(*root).unwrap();
        let mut item = node.payload();
        while let Item::Modified { declaration, .. } = item {
            item = parsed.ast().items().get(*declaration).unwrap().payload();
        }
        let Item::Function {
            name: NameMarker::Present(name),
            type_parameters,
            ..
        } = item
        else {
            continue;
        };
        function_roots += 1;
        let declarations = index
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.root() == *root
                    && declaration.source_unit() == source.id()
                    && declaration.kind() == SymbolKind::Function
            })
            .collect::<Vec<_>>();
        let [declaration] = declarations.as_slice() else {
            panic!("one actual declaration for original function root {root:?}");
        };
        assert_eq!(declaration.package(), source.package());
        assert_eq!(declaration.name_span(), *name);
        assert_eq!(name.source_id(), source.source_id());
        let span = analysis.declaration_span(declaration.id());
        assert_eq!(span, node.span());
        let signature = analysis
            .types()
            .signatures()
            .declaration(declaration.id())
            .and_then(|signature| signature.callable())
            .expect("original function has a same-analysis callable signature");
        assert_eq!(signature.name_span(), *name);
        assert_eq!(signature.type_parameters().len(), type_parameters.len());
        // Generic library templates stay in complete FE/ownership admission.
        // Their codegen is witnessed only by real reachable concrete instances.
        if !signature.type_parameters().is_empty() {
            assert_ne!(declaration.id(), main);
            continue;
        }
        let (program, entry) = analysis
            .lower_entry(declaration.id())
            .unwrap_or_else(|error| {
                panic!(
                    "original Range root {}: {error:?}: {:?}",
                    declaration.name(),
                    error
                        .span
                        .and_then(|span| analysis.sources().slice(span).ok())
                )
            });
        let function = program
            .module(entry.module())
            .unwrap()
            .function(entry)
            .unwrap();
        assert_eq!(function.origin, Origin::Source(span));
        verify_program(&program).unwrap();
        // Preserve original whole-file LLVM obligations, including uncalled roots,
        // without widening SSA-only or verifier-negative admission.
        if check_llvm {
            crate::llvm::render_verified_program(&program).unwrap();
        }
        if declaration.id() == main {
            assert!(main_program.replace((program, entry)).is_none());
        }
    }
    assert_eq!(
        function_roots,
        index
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.source_unit() == source.id()
                    && declaration.kind() == SymbolKind::Function
            })
            .count(),
        "every original top-level function is accounted for"
    );
    main_program.expect("the unique original non-generic main was lowered")
}

#[test]
pub(in crate::ssa) fn range_single_and_unit_keep_the_same_new_descriptor_abi() {
    let consumer = "class Item(val n:Int){}\nfun check(view:View<Item>):Unit{if(view.size!=1){error(\"wrong\")}}\nfun main():Unit{val source=listOf(Item(7));borrow val part=take(source,1);check(part);println(\"done\")}";
    for program in [
        memory_unit_llvm(consumer),
        unit(&format!(
            "package app\nimport koven.algorithms.take\n{consumer}"
        )),
    ] {
        let module = &program.modules[0];
        assert!(module.functions.iter().any(|f| f.carrier_return.is_some()));
        assert!(module.functions.iter().all(|f| f.borrow_return.is_none()));
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
pub(in crate::ssa) fn range_metadata_borrow_is_separate_from_constructing_a_new_descriptor() {
    let consumer = "fun metadata(source:View<String>):borrow View<String> from source=source\nfun check(source:View<String>):Unit{if(source.size!=1){error(\"size\")}}\nfun consume(own source:List<String>):Unit{}\nfun main():Unit{val source=listOf(\"kept\");borrow val part=take(source,1);borrow val alias=part;borrow val returned=metadata(alias);check(returned);consume(source)}";
    for program in [
        memory_unit_llvm(consumer),
        unit(&format!(
            "package app\nimport koven.algorithms.take\n{consumer}"
        )),
    ] {
        let module = &program.modules[0];
        assert!(module.functions.iter().any(|f| f.borrow_return.is_some()));
        assert_eq!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.instructions)
                .filter(|i| matches!(i.operation, Operation::RangeConstruct { .. }))
                .count(),
            1
        );
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
pub(in crate::ssa) fn range_verifier_rejects_erasure_and_premature_source_end_without_emitting_ir()
{
    use super::verify::VerifyErrorKind;
    let consumer = "package app\nimport koven.algorithms.take\nfun check(source:View<String>):Unit{}\nfun main():Unit{val source=listOf(\"kept\");borrow val part=take(source,1);check(part)}";
    for variant in 0..5 {
        let mut program = unit(consumer);
        let module = &mut program.modules[0];
        if variant == 3 {
            module
                .functions
                .iter_mut()
                .find(|f| f.carrier_return.is_some())
                .unwrap()
                .carrier_return = Some(1);
        } else {
            let caller = module
                .functions
                .iter_mut()
                .find(|f| {
                    f.instructions
                        .iter()
                        .any(|i| matches!(i.operation, Operation::RangeCall { .. }))
                })
                .unwrap();
            let (view, child, parent) = caller
                .instructions
                .iter()
                .find_map(|i| {
                    let Operation::RangeCall { source, .. } = i.operation else {
                        return None;
                    };
                    let [EntityId::Value(view), EntityId::Loan(child)] = i.results.as_slice()
                    else {
                        panic!("pair");
                    };
                    Some((*view, *child, source))
                })
                .unwrap();
            let end = caller
                .instructions
                .iter_mut()
                .find(|i| matches!(i.operation, Operation::RangeEnd { .. }))
                .unwrap();
            end.operation = match variant {
                0 => Operation::BorrowEnd { loan: parent },
                1 => Operation::BorrowEnd { loan: child },
                2 => Operation::Drop { owner: view },
                4 => Operation::Consume { owner: view },
                _ => unreachable!(),
            };
        }
        let errors =
            verify_program(&program).expect_err("invalid descriptor lifetime must be rejected");
        assert_diagnostic!(
            errors.errors.iter().any(|e| matches!(
                e.kind,
                VerifyErrorKind::LoanDependencyActive { .. }
                    | VerifyErrorKind::OwnerLoanConflict { .. }
                    | VerifyErrorKind::OperationContract { .. }
                    | VerifyErrorKind::ReturnType { .. }
            )),
            "variant {variant}: {errors:?}"
        );
        if variant == 2 || variant == 4 {
            assert_diagnostic!(
                errors
                    .errors
                    .iter()
                    .any(|e| matches!(e.kind, VerifyErrorKind::OperationContract { .. })),
                "erased descriptor was not rejected by its operation contract: {errors:?}"
            );
        }
        // No defective Program is ever given to an LLVM or native API.
    }
}

#[test]
pub(in crate::ssa) fn abort_predecessor_range_end_source_loan_then_both_abort() {
    let program = unit(
        r#"package app
        import koven.algorithms.take
        fun test_case(source: List<String>, flag: Boolean): Unit {
            var marker = 10
            replace(&marker, 20)
            {
                borrow val part = take(source, 1)
                val len = part.size
            }
            if (flag) {
                error("abort-range-a")
            } else {
                error("abort-range-b")
            }
        }
        fun main(): Unit {
            test_case(listOf("item-a", "item-b"), true)
        }"#,
    );
    let module = &program.modules[0];
    assert!(module.functions.iter().any(|f| {
        f.instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::RangeEnd { .. }))
    }));
    crate::llvm::render_verified_program(&program).unwrap();
}
