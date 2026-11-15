//! Normal no-carry sources, without native execution or manufactured evidence.
use super::*;
use crate::ssa::{refine::fresh_loop_receipt, unit_lower, unit_lower_test_support};
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

#[test]
fn fresh_body_lease_closes_one_loop_across_source_and_parameter_order() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            check(
                reverse_sources,
                reverse_parameters,
                "consume(item)",
                (0, 0),
                true,
            );
        }
    }
}
#[test]
fn fresh_source_segments_preserve_entry_and_exit_source_consumption() {
    for reverse_sources in [false, true] {
        for reverse_parameters in [false, true] {
            check(
                reverse_sources,
                reverse_parameters,
                "consume(item)",
                (1, 1),
                true,
            );
        }
    }
}
#[test]
fn original_multiple_consumers_and_unbalanced_segments_remain_outside_fresh_domain() {
    check(false, false, "consume(item);other(item)", (0, 0), false);
    for segments in [(1, 0), (0, 1), (2, 1), (1, 2)] {
        check(false, false, "consume(item)", segments, false);
    }
}
fn check(
    reverse_sources: bool,
    reverse_parameters: bool,
    uses: &str,
    segments: (usize, usize),
    certified: bool,
) {
    let mut sources = SourceMap::new();
    let (provider_id, provider) = unit_lower_test_support::parsed(
        &mut sources,
        "fresh-provider.ko",
        "fun view(source:String):borrow String from source=source\nfun consume(item:String):Unit {}\nfun other(item:String):Unit {}\nfun before(item:String):Unit {}\nfun after(item:String):Unit {}",
    );
    let parameters = if reverse_parameters {
        "flag:Boolean,source:String"
    } else {
        "source:String,flag:Boolean"
    };
    let before = "before(source);".repeat(segments.0);
    let after = "after(source);".repeat(segments.1);
    let text = format!(
        "fun run({parameters}):Unit {{{before}while(flag){{borrow val item=view(source);{uses}}};{after}}}"
    );
    let (caller_id, caller) =
        unit_lower_test_support::parsed(&mut sources, "fresh-caller.ko", &text);
    let mut inputs = [
        SourceUnitInput::new("root", "fresh-provider.ko", provider_id, &provider),
        SourceUnitInput::new("root", "fresh-caller.ko", caller_id, &caller),
    ];
    if reverse_sources {
        inputs.reverse();
    }
    let (environment, types) = standard_environments();
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    let declaration = |name| {
        names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.name() == name)
            .unwrap()
            .id()
    };
    let facts = owned.ownership().no_carry_loops();
    assert_eq!(facts.len(), 1);
    assert!(owned.ownership().owns_no_carry_loop(&facts[0]));
    let ((results, audits), receipts) = fresh_loop_receipt::observe(|| {
        observe(|| {
            (0..2)
                .map(|_| {
                    unit_lower::lower_scalar_unit_with_entry(
                        &sources,
                        &inputs,
                        &names,
                        &types,
                        &typed,
                        &owned,
                        declaration("run"),
                    )
                })
                .collect::<Vec<_>>()
        })
    });
    assert_eq!(audits.len(), 2);
    assert_eq!(
        receipts.len(),
        2,
        "original route must still publish its genuine receipt"
    );
    for ((result, audit), receipt) in results.into_iter().zip(audits).zip(receipts) {
        let (program, entry) = result.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        let function = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .find(|f| f.id == entry)
            .unwrap();
        receipt.validate(function).unwrap();
        assert_eq!(receipt.loops.len(), 1);
        assert!(receipt.loops[0].fact.same_identity(&facts[0]));
        assert_eq!(receipt.loops[0].fact, facts[0]);
        assert!(
            function.blocks.len() >= 4,
            "entry, condition, body and zero-iteration exit"
        );
        let edge = (declaration("run"), declaration("view"));
        if certified {
            assert!(audit.borrow_edges.contains(&edge));
            assert!(
                audit
                    .direct_edges
                    .contains(&(declaration("run"), declaration("consume")))
            );
            if segments == (1, 1) {
                assert!(
                    audit
                        .direct_edges
                        .contains(&(declaration("run"), declaration("before")))
                );
                assert!(
                    audit
                        .direct_edges
                        .contains(&(declaration("run"), declaration("after")))
                );
                assert_eq!(receipt.segments.len(), 2);
                assert_eq!(receipt.segments[0].instructions.len(), 1);
                assert_eq!(receipt.segments[1].instructions.len(), 1);
                let first = function
                    .instruction(receipt.segments[0].instructions[0])
                    .unwrap();
                let last = function
                    .instruction(receipt.segments[1].instructions[0])
                    .unwrap();
                assert_eq!(Some(first.block), function.entry_block());
                assert_ne!(first.block, last.block);
                let source_index = usize::from(reverse_parameters);
                let entry_source = function
                    .block(function.entry_block().unwrap())
                    .unwrap()
                    .parameters[source_index];
                for (instruction, name) in [(first, ".before."), (last, ".after.")] {
                    let Operation::DirectCall {
                        callee,
                        receiver: None,
                        arguments,
                    } = &instruction.operation
                    else {
                        panic!("actual segment call");
                    };
                    let target = program
                        .modules
                        .iter()
                        .flat_map(|m| &m.functions)
                        .find(|f| f.id == *callee)
                        .unwrap();
                    assert!(target.name.contains(name), "{}", target.name);
                    let [argument] = arguments.as_slice() else {
                        panic!("one source argument");
                    };
                    assert!(
                        function
                            .block(instruction.block)
                            .unwrap()
                            .parameters
                            .contains(argument)
                    );
                    assert_eq!(
                        function.entity(*argument).unwrap().ty,
                        function.entity(entry_source).unwrap().ty
                    );
                }
            }
            assert!(audit.unsupported.is_empty(), "{:?}", audit.unsupported);
        } else {
            assert!(!audit.borrow_edges.contains(&edge));
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.starts_with("fresh no-carry missing/duplicate")),
                "{:?}",
                audit.unsupported
            );
            assert!(
                audit
                    .unsupported
                    .iter()
                    .any(|g| g.contains("unclassified") && g.contains("BorrowCall")),
                "{:?}",
                audit.unsupported
            );
        }
    }
}
