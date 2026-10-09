//! View-source descriptors inherit the collection root, independently of parent metadata.
use super::*;

#[test]
fn range_view_source_take_single_and_unit_end_parent_before_child_use() {
    let consumer = "fun metadata(source:View<String>):borrow View<String> from source=source\nfun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun consume(own source:List<String>):Unit{}\nfun main():Unit{val source=listOf(\"first\".clone(),\"second\".clone());borrow val parent=take(source,2);borrow val child=take(parent,1);borrow val alias=metadata(child);read(alias);consume(source)}";
    for program in [
        single(consumer),
        unit(&format!(
            "package app\nimport koven.algorithms.take\n{consumer}"
        )),
    ] {
        let caller = program.modules[0]
            .functions
            .iter()
            .find(|function| {
                function
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        matches!(instruction.operation, Operation::RangeCall { .. })
                    })
                    .count()
                    == 2
            })
            .unwrap();
        let calls = caller
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::RangeCall { .. }))
            .collect::<Vec<_>>();
        let EntityId::Value(parent) = calls[0].results[0] else {
            panic!("parent descriptor");
        };
        let parent_end = caller.instructions.iter().find(|instruction| {
            matches!(instruction.operation, Operation::RangeEnd { view, .. } if view == parent)
        }).unwrap();
        let metadata_call = caller
            .instructions
            .iter()
            .find(|instruction| matches!(instruction.operation, Operation::BorrowCall { .. }))
            .unwrap();
        assert!(calls[1].id.index() < parent_end.id.index());
        assert!(parent_end.id.index() < metadata_call.id.index());
        crate::llvm::render_verified_program(&program).unwrap();
    }
}

#[test]
fn range_view_source_native_single_and_unit_preserve_collection_and_element_identities() {
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
            "{declaration}\nfun metadata(source:View<{element}>):borrow View<{element}> from source=source\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}\nfun consume(own source:List<{element}>):Unit{{println(\"consume\")}}\nfun main():Unit{{val source=listOf({values});borrow val parent=take(source,2);borrow val child=take(parent,2);borrow val grandchild=take(child,1);borrow val alias=metadata(grandchild);read(alias);println(\"after\");consume(source);println(\"done\")}}"
        );
        for (program, entry) in [
            single_with_entry(&consumer),
            unit_with_entry(&format!(
                "package app\nimport koven.algorithms.take\n{consumer}"
            )),
        ] {
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            // Only the two elements and List allocate; metadata creation and forwarding allocate nothing.
            let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
            assert_success(
                &run,
                format!("first\nafter\nconsume\n{drops}done\n").as_bytes(),
            );
        }
    }
}

#[test]
fn range_view_source_offsets_and_forwarded_return_preserve_absolute_root_coordinates() {
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    let trusted = "fun <T> window(source:List<T>):View<T> from source=rangeView(source,1,2)\nfun <T> forward(source:View<T>,count:Int):View<T> from source=take(source,count)";
    for count in [0, 1, 2147483647] {
        let consumer = format!(
            "class Item(val text:String){{deinit(){{println(this.text)}}}}\nfun read(view:View<Item>):Unit{{for(item in view){{println(item.text)}}}}\nfun main():Unit{{val source=listOf(Item(\"first\"),Item(\"second\"));borrow val parent=window(source);borrow val child=forward(parent,{count});read(child);println(\"after\")}}"
        );
        for (program, entry) in [
            single_with_entry(&format!("{trusted}\n{consumer}")),
            unit_with_provider_entry(
                &format!(
                    "package app\nimport koven.algorithms.window\nimport koven.algorithms.forward\n{consumer}"
                ),
                trusted,
            ),
        ] {
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
            let visits = if count == 0 { "" } else { "second\n" };
            assert_success(&run, format!("{visits}after\nsecond\nfirst\n").as_bytes());
        }
    }
}

#[test]
fn range_view_source_parent_child_and_elements_coexist_until_their_own_last_uses() {
    let consumer = "class Item(val text:String){deinit(){println(this.text)}}\nfun read(view:View<Item>):Unit{for(item in view){println(item.text)}}\nfun consume(own source:List<Item>):Unit{}\nfun main():Unit{val source=listOf(Item(\"first\"),Item(\"second\"));{borrow val parent=take(source,2);borrow val child=take(parent,1);for(item in parent){read(child);println(item.text)};read(child)};consume(source)}";
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
    for (program, entry) in [
        single_with_entry(consumer),
        unit_with_entry(&format!(
            "package app\nimport koven.algorithms.take\n{consumer}"
        )),
    ] {
        let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2]);
        assert_success(&run, b"first\nfirst\nfirst\nsecond\nfirst\nsecond\nfirst\n");
    }
}

#[test]
fn range_view_source_nested_temporary_chain_is_consumed_synchronously() {
    let consumer = "fun source():List<String> =listOf(\"a\".clone())\nfun read(view:View<String>):Unit{for(item in view){println(item)}}\nfun main():Unit{read(take(take(source(),1),1));for(item in take(take(source(),1),1)){println(item)}}";
    single(consumer);
    unit(&format!(
        "package app\nimport koven.algorithms.take\n{consumer}"
    ));
}

#[test]
fn range_view_source_verifier_rejects_premature_collection_release_after_parent_end() {
    let consumer = "fun read(view:View<String>):Unit{}\nfun main():Unit{val source=listOf(\"a\".clone());borrow val parent=take(source,1);borrow val child=take(parent,1);read(child)}";
    for mut program in [
        single(consumer),
        unit(&format!(
            "package app\nimport koven.algorithms.take\n{consumer}"
        )),
    ] {
        let caller = program.modules[0]
            .functions
            .iter_mut()
            .find(|function| {
                function
                    .instructions
                    .iter()
                    .filter(|instruction| {
                        matches!(instruction.operation, Operation::RangeCall { .. })
                    })
                    .count()
                    == 2
            })
            .unwrap();
        let (parent, source) = caller
            .instructions
            .iter()
            .find_map(|instruction| {
                match (&instruction.operation, instruction.results.as_slice()) {
                    (
                        Operation::RangeCall { source, .. },
                        [EntityId::Value(view), EntityId::Loan(_)],
                    ) => Some((*view, *source)),
                    _ => None,
                }
            })
            .unwrap();
        let Definition::InstructionResult { instruction, .. } =
            caller.entity(EntityId::Loan(source)).unwrap().definition
        else {
            panic!("source loan");
        };
        let Operation::BorrowBegin { place, .. } =
            caller.instruction(instruction).unwrap().operation
        else {
            panic!("source place");
        };
        let Definition::InstructionResult { instruction, .. } =
            caller.entity(EntityId::Place(place)).unwrap().definition
        else {
            panic!("root place");
        };
        let Operation::RootPlace { owner } = caller.instruction(instruction).unwrap().operation
        else {
            panic!("collection owner");
        };
        caller.instructions.iter_mut().find(|instruction| matches!(instruction.operation, Operation::RangeEnd { view, .. } if view == parent)).unwrap().operation = Operation::Drop { owner };
        let errors = verify_program(&program)
            .expect_err("the child's independent root loan must still protect the collection");
        assert!(errors.errors.iter().any(|error| matches!(error.kind, super::super::verify::VerifyErrorKind::OwnerLoanConflict { value } if value == owner)), "{errors:?}");
        // The rejected Program is never emitted or executed.
    }
}

#[test]
fn range_view_source_verifier_rejects_a_forwarded_sibling_root() {
    let trusted = "fun choose(source:View<String>,sibling:View<String>):View<String> from source=take(source,1)";
    let consumer = "fun read(view:View<String>):Unit{}\nfun main():Unit{val left=listOf(\"a\");val right=listOf(\"b\");borrow val parent=take(left,1);borrow val sibling=take(right,1);borrow val child=choose(parent,sibling);read(child)}";
    for mut program in [
        single(&format!("{trusted}\n{consumer}")),
        unit_with_provider_entry(&format!("package app\nimport koven.algorithms.take\nimport koven.algorithms.choose\n{consumer}"), trusted).0,
    ] {
        let function = program.modules[0].functions.iter_mut().find(|function| {
            function.carrier_return == Some(0) && function.blocks[0].parameters.len() == 2 && function.blocks[0].parameters.iter().all(|parameter| {
                matches!(function.entity(*parameter).unwrap().ty, EntityType::Loan { target, .. } if function.return_types == [target])
            })
        }).unwrap();
        let EntityId::Loan(sibling) = function.blocks[0].parameters[1] else { panic!("sibling input"); };
        let instruction = function.instructions.iter_mut().find(|instruction| matches!(instruction.operation, Operation::RangeCall { .. })).unwrap();
        let Operation::RangeCall { source, arguments, .. } = &mut instruction.operation else { unreachable!(); };
        *source = sibling;
        arguments[0] = EntityId::Loan(sibling);
        let errors = verify_program(&program).expect_err("the declared source must be the actual returned root");
        assert!(errors.errors.iter().any(|error| matches!(error.kind, super::super::verify::VerifyErrorKind::ReturnType { .. })), "{errors:?}");
        // Signature-valid sibling substitution stays inside the pure verifier.
    }
}

#[test]
fn range_view_source_nested_temporary_native_releases_each_original_collection_once() {
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
        for (count, exit) in [
            (0, ""),
            (1, ""),
            (2147483647, ""),
            (2, "break"),
            (2, "continue"),
            (2, "return"),
        ] {
            let consumer = format!(
                "{declaration}\nfun source():List<{element}>{{println(\"source\");return listOf({values})}}\nfun read(view:View<{element}>):Unit{{for(item in view){{{read}}};println(\"read-end\")}}\nfun main():Unit{{read(take(take(source(),2),{count}));for(item in take(take(source(),2),{count})){{{read};{exit}}};println(\"done\")}}"
            );
            for (program, entry) in [
                single_with_entry(&consumer),
                unit_with_entry(&format!(
                    "package app\nimport koven.algorithms.take\n{consumer}"
                )),
            ] {
                let llvm =
                    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
                let run = run_counted_allocations_in_order(&llvm, &[1, 0, 2, 4, 3, 5]);
                let visits = match count {
                    0 => "",
                    1 => "first\n",
                    _ => "first\nsecond\n",
                };
                let for_visits = if exit == "break" || exit == "return" {
                    "first\n"
                } else {
                    visits
                };
                let done = if exit == "return" { "" } else { "done\n" };
                assert_success(
                    &run,
                    format!("source\n{visits}read-end\n{drops}source\n{for_visits}{drops}{done}")
                        .as_bytes(),
                );
            }
        }
    }
}
