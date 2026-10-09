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
            "}",
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
        single(&source),
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
        for (named, view) in [(true, false), (false, false), (true, true), (false, true)] {
            let source = consumer(declaration, element, values, read, named, view);
            for (program, entry) in [
                single_with_entry(&source),
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
    use crate::native_tests::boxed_enum_tests::{assert_success, run_counted_allocations_in_order};
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
                single_with_entry(&source),
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
