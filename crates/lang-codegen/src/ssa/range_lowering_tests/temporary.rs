//! Healthy trusted std take sources; allocator identities observe normal releases.
use super::*;

#[test]
fn range_temporary_native_single_and_unit_release_string_move_only_and_resource_identities() {
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
                single_with_entry(&consumer),
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
            single_with_entry(&consumer),
            unit_with_entry(&format!(
                "package app\nimport koven.algorithms.take\n{consumer}"
            )),
        ] {
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
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
        single(&consumer);
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
