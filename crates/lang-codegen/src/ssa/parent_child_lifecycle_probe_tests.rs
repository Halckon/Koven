//! One ordinary original-source probe; no fact mutation or forced routing.
use super::*;

#[test]
fn original_parent_child_lifecycle_records_real_dependency_and_ssa() {
    let text = "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = parent; consume(child) }";
    let mut sources = SourceMap::new();
    let (source, parsed) =
        unit_lower_test_support::parsed(&mut sources, "parent-child-lifecycle.ko", text);
    let (environment, types) = standard_environments();
    let inputs = [SourceUnitInput::memory_document(source, &parsed)];
    let (names, typed, owned) =
        unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
    println!(
        "actions={:#?}",
        owned.ownership().loan_action_observations()
    );
    println!("handoffs={:#?}", owned.ownership().borrow_call_handoffs());
    println!(
        "transports={:#?}",
        owned.ownership().borrow_binding_transports()
    );
    println!(
        "bindings={:#?}",
        owned.ownership().borrow_results().bindings()
    );
    println!(
        "binding_ends={:#?}",
        owned.ownership().borrow_results().ends()
    );
    let run = names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "run")
        .unwrap();
    let (lowered, routes) = crate::ssa::refine::observe_routing(|| {
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &types,
            &typed,
            &owned,
            run.id(),
        )
    });
    let (program, entry) = lowered.expect("original ordinary parent-child source lowers");
    verify_program(&program).unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
    println!("routes={routes:?}; entry={entry:?}");
    println!("{}", crate::ssa::render_program(&program));

    let caller = program.modules[0].function(entry).unwrap();
    let calls: Vec<_> = caller
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            if let Operation::BorrowCall { source, .. } = &instruction.operation {
                Some((index, *source, instruction))
            } else {
                None
            }
        })
        .collect();
    let [(handoff_index, source, handoff)] = calls.as_slice() else {
        panic!("one actual borrowed return")
    };
    let [EntityId::Loan(parent)] = handoff.results.as_slice() else {
        panic!("actual parent result")
    };
    let children: Vec<_> = caller
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            if let Operation::SharedReborrow { source } = &instruction.operation {
                Some((index, *source, instruction))
            } else {
                None
            }
        })
        .collect();
    let [(child_index, actual_parent, reborrow)] = children.as_slice() else {
        panic!("one actual child reborrow")
    };
    assert_eq!(actual_parent, parent);
    let [EntityId::Loan(child)] = reborrow.results.as_slice() else {
        panic!("actual child result")
    };
    assert_ne!(source, parent);
    assert_ne!(parent, child);
    assert_ne!(source, child);
    let consumers: Vec<_> = caller
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            if let Operation::DirectCall {
                receiver,
                arguments,
                ..
            } = &instruction.operation
            {
                Some((index, receiver, arguments, instruction))
            } else {
                None
            }
        })
        .collect();
    let [(consumer_index, receiver, arguments, consumer)] = consumers.as_slice() else {
        panic!("one ordinary consumer")
    };
    assert!(receiver.is_none());
    assert_eq!(arguments.as_slice(), &[EntityId::Loan(*child)]);
    assert!(consumer.results.is_empty());
    let ends: Vec<_> = caller
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            if let Operation::BorrowEnd { loan } = &instruction.operation {
                Some((index, *loan))
            } else {
                None
            }
        })
        .collect();
    let [
        (child_end, ended_child),
        (parent_end, ended_parent),
        (source_end, ended_source),
    ] = ends.as_slice()
    else {
        panic!("exactly child, parent and source retirements")
    };
    assert_eq!(
        (ended_child, ended_parent, ended_source),
        (child, parent, source)
    );
    assert!(handoff_index < child_index && child_index < consumer_index);
    assert!(consumer_index < child_end && child_end < parent_end && parent_end < source_end);
    let roots: Vec<_> = caller
        .instructions
        .iter()
        .filter_map(|instruction| {
            if let Operation::RootPlace { owner } = &instruction.operation {
                Some(*owner)
            } else {
                None
            }
        })
        .collect();
    let [owner] = roots.as_slice() else {
        panic!("one original owner place")
    };
    let drops: Vec<_> = caller
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, instruction)| {
            if let Operation::Drop { owner } = &instruction.operation {
                Some((index, *owner))
            } else {
                None
            }
        })
        .collect();
    let [(drop_index, dropped)] = drops.as_slice() else {
        panic!("one original owner drop")
    };
    assert_eq!(dropped, owner);
    assert!(source_end < drop_index);
}
