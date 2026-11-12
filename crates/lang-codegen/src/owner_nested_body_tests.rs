//! The nested permission only consumes one normal body, not a loop invariant.
use super::{UnitSource, assert_general_counter_baseline};

#[test]
fn ci_only_nested_body_missing_permits_and_foreign_parent_fail_closed() {
    use crate::ssa::OwnerFault;
    let source = UnitSource::analyze(
        "nested-permits.ko",
        r#"
        fun run(outer: Int, inner: Int) {
            var a="a"; var b="b"; var n0=outer; var g0=n0>0;
            while(g0) {
                var n1=inner; var g1=n1>0;
                while(g1) { val x=replace(&a,"new-a"); val y=replace(&b,"new-b");
                    n1=n1-1; g1=n1>0 };
                n0=n0-1; g0=n0>0
            }; println(a); println(b)
        }
        fun entry() { run(2,2) }
    "#,
    )
    .unwrap();
    source.assert_frontend_clean();
    assert_eq!(
        source
            .ownership()
            .owner_observations()
            .iter()
            .filter(|o| o.nested_normal_body_v1().is_some())
            .count(),
        1
    );
    for fault in [
        OwnerFault::MissingBridgeReplacePermit,
        OwnerFault::MissingBridgeBindingPermit,
        OwnerFault::MissingBridgeDropPermit,
        OwnerFault::BridgeDropDuplicateCommit,
        OwnerFault::NestedBodyWrongParent,
        OwnerFault::NestedBodyLateMissingDrop,
        OwnerFault::NestedBodyLateDuplicateDrop,
        OwnerFault::NestedBodyForeignLoan,
        OwnerFault::NestedBodyLateRootPlace,
        OwnerFault::NestedBodyEscapedPreparation,
        OwnerFault::NestedBodyEscapedLoanTerminator,
    ] {
        let (result, actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()))
        });
        assert!(result.is_err());
        assert!(
            actions
                .iter()
                .all(|(kind, _)| *kind != "nested-owner-normal-body-complete")
        );
        if matches!(
            fault,
            OwnerFault::NestedBodyForeignLoan
                | OwnerFault::NestedBodyLateRootPlace
                | OwnerFault::NestedBodyEscapedPreparation
                | OwnerFault::NestedBodyEscapedLoanTerminator
        ) {
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "nested-preparation-fault-hit")
                    .count(),
                1
            );
        }
        if matches!(
            fault,
            OwnerFault::NestedBodyLateMissingDrop | OwnerFault::NestedBodyLateDuplicateDrop
        ) {
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "nested-retirement-fault-hit")
                    .count(),
                1
            );
            assert!(
                actions
                    .iter()
                    .all(|(kind, _)| *kind != "nested-owner-retirement-rechecked")
            );
        }
        if fault == OwnerFault::NestedBodyWrongParent {
            assert_eq!(
                actions
                    .iter()
                    .filter(|(kind, _)| *kind == "nested-parent-fault-hit")
                    .count(),
                1
            );
        }
    }
}

#[test]
fn nested_counters_preserve_outer_live_roots_without_flat_batch() {
    let mut calls = String::new();
    let mut expected = String::new();
    for outer in [0, 1, 2] {
        for inner in [0, 1, 2] {
            calls.push_str(&format!("run({outer}, {inner}, \"seed\");"));
            let a = if outer > 0 && inner > 0 {
                "inner-a"
            } else {
                "seed"
            };
            let b = if outer > 0 { "outer-b" } else { "entry-b" };
            expected.push_str(&format!("{a}\n{b}\n"));
        }
    }
    let text = format!(
        r#"fun run(outer: Int, inner: Int, own seed: String) {{
        var a = seed; var b = "entry-b"; var n0 = outer; var g0 = n0 > 0;
        while(g0) {{
            var n1 = inner; var g1 = n1 > 0;
            while(g1) {{ val x = replace(&a, "inner-a"); val y = replace(&b, "inner-b");
                n1 = n1 - 1; g1 = n1 > 0 }};
            val old = replace(&b, "outer-b"); n0 = n0 - 1; g0 = n0 > 0
        }};
        println(a); println(b)
    }}
    fun entry() {{ {calls} }}"#
    );
    let source = UnitSource::analyze("nested-normal.ko", &text).unwrap();
    source.assert_frontend_clean();
    assert_eq!(
        source
            .ownership()
            .owner_observations()
            .iter()
            .filter(|o| o.nested_normal_body_v1().is_some())
            .count(),
        1
    );
    assert_nested_ssa_retirement(&source);
    assert_general_counter_baseline(&text, expected.as_bytes(), false);
}

fn assert_nested_ssa_retirement(source: &UnitSource) {
    use crate::ssa::model::{EntityId, Operation};
    let product = source
        .ownership()
        .owner_observations()
        .iter()
        .find_map(|o| o.nested_normal_body_v1())
        .unwrap();
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(kind, _)| *kind == "nested-owner-retirement-rechecked")
            .map(|(_, count)| *count)
            .collect::<Vec<_>>(),
        [2]
    );
    assert_existing_false_exit_resume(source, &program);
    for step in product.normal().steps() {
        let span = source
            .parsed()
            .ast()
            .expressions()
            .get(step.call().expression())
            .unwrap()
            .span();
        let matches = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .flat_map(|f| {
                f.instructions
                    .iter()
                    .filter(move |i| {
                        i.origin.span() == span
                            && matches!(i.operation, Operation::RootReplace { .. })
                    })
                    .map(move |i| (f, i))
            })
            .collect::<Vec<_>>();
        let [(function, replace)] = matches.as_slice() else {
            panic!("unique actual inner Replace")
        };
        let [new, old] = replace.results.as_slice() else {
            panic!("new/retired pair")
        };
        assert_ne!(new, old);
        let drops = function
            .instructions
            .iter()
            .filter(
                |i| matches!(i.operation, Operation::Drop {owner} if EntityId::Value(owner)==*old),
            )
            .collect::<Vec<_>>();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].block, replace.block);
        let block = function.block(replace.block).unwrap();
        assert!(
            block
                .instructions
                .iter()
                .position(|id| *id == replace.id)
                .unwrap()
                < block
                    .instructions
                    .iter()
                    .position(|id| *id == drops[0].id)
                    .unwrap()
        );
    }
}

// Audit the existing generic lowering only. This does not publish an exit/resume permit.
fn assert_existing_false_exit_resume(source: &UnitSource, program: &crate::ssa::model::Program) {
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    let functions = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .filter(|f| {
            source
                .sources
                .slice(f.origin.span())
                .unwrap()
                .starts_with("fun run(")
        })
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        panic!("unique run")
    };
    let product = source
        .ownership()
        .owner_observations()
        .iter()
        .find_map(|o| o.nested_normal_body_v1())
        .unwrap();
    let find = |literal| {
        let sites = source
            .types()
            .ownership_primitives()
            .iter()
            .filter_map(|p| {
                let call = p.expression();
                if call.source_unit() != product.control().source_unit() {
                    return None;
                }
                let span = source
                    .parsed()
                    .ast()
                    .expressions()
                    .get(call.expression())
                    .unwrap()
                    .span();
                source
                    .sources
                    .slice(span)
                    .unwrap()
                    .contains(literal)
                    .then_some((call, span))
            })
            .collect::<Vec<_>>();
        let [(call, span)] = sites.as_slice() else {
            panic!("unique original primitive")
        };
        let candidates = function
            .instructions
            .iter()
            .filter(|i| {
                i.origin.span() == *span && matches!(i.operation, Operation::RootReplace { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(candidates.len(), 1);
        (*call, candidates[0])
    };
    let (inner_call, inner) = find("\"inner-b\"");
    assert!(
        product
            .normal()
            .steps()
            .iter()
            .any(|s| s.call() == inner_call)
    );
    let (_, outer) = find("\"outer-b\"");
    let Operation::RootReplace {
        owner: inner_owner, ..
    } = inner.operation
    else {
        unreachable!()
    };
    let Operation::RootReplace {
        owner: resumed_owner,
        loan: resumed_loan,
        ..
    } = outer.operation
    else {
        unreachable!()
    };
    let body = function.block(inner.block).unwrap();
    let root_position = body
        .parameters
        .iter()
        .position(|v| *v == EntityId::Value(inner_owner))
        .unwrap();
    let headers = function
        .blocks
        .iter()
        .filter_map(|b| match &b.terminator.as_ref()?.kind {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } if when_true.target == body.id => Some((b, when_true, when_false)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(header, when_true, when_false)] = headers.as_slice() else {
        panic!("unique child header")
    };
    let mut value = when_true.arguments[root_position];
    assert!(header.parameters.contains(&value));
    // This is a finite static join of entry and normal-backedge operands, not dynamic history.
    assert_eq!(
        function
            .blocks
            .iter()
            .filter(|b| matches!(&b.terminator.as_ref().unwrap().kind,
        TerminatorKind::Branch(edge) if edge.target==header.id))
            .count(),
        2
    );
    let mut edge = *when_false;
    let mut visited = std::collections::BTreeSet::new();
    loop {
        assert!(
            visited.insert(edge.target),
            "resume audit must not cross another iteration"
        );
        let destination = function.block(edge.target).unwrap();
        let positions = edge
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(i, v)| (*v == value).then_some(i))
            .collect::<Vec<_>>();
        let [position] = positions.as_slice() else {
            panic!("one carried root operand")
        };
        value = destination.parameters[*position];
        if destination.id == outer.block {
            assert_eq!(value, EntityId::Value(resumed_owner));
            let statements = destination
                .instructions
                .iter()
                .map(|id| function.instruction(*id).unwrap())
                .collect::<Vec<_>>();
            let loans = statements
                .iter()
                .enumerate()
                .filter_map(|(i, s)| match s.operation {
                    Operation::BorrowBegin {
                        place,
                        kind: crate::ssa::model::LoanKind::Exclusive,
                    } if s.results == [EntityId::Loan(resumed_loan)] => Some((i, place)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [(loan_index, place)] = loans.as_slice() else {
                panic!("actual outer exclusive loan")
            };
            let places = statements.iter().enumerate().filter_map(|(i,s)|
                (s.results==[EntityId::Place(*place)] && matches!(s.operation,Operation::RootPlace {owner} if owner==resumed_owner)).then_some(i)).collect::<Vec<_>>();
            let [place_index] = places.as_slice() else {
                panic!("actual outer root place")
            };
            let replace_index = destination
                .instructions
                .iter()
                .position(|id| *id == outer.id)
                .unwrap();
            assert!(*place_index < *loan_index && *loan_index < replace_index);
            for (i, statement) in statements.iter().enumerate() {
                assert_eq!(
                    statement.operation.entities().contains(&value),
                    i == *place_index || i == replace_index
                );
                assert!(!statement.results.contains(&value));
            }
            break;
        }
        for id in &destination.instructions {
            let statement = function.instruction(*id).unwrap();
            assert!(!statement.operation.entities().contains(&value));
            assert!(!statement.results.contains(&value));
        }
        // Do not infer through branching or another loop. The supported fixture has a direct corridor.
        edge = match &destination.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(edge) => edge,
            other => panic!("nonlinear old resume corridor: {other:?}"),
        };
    }
}

#[test]
fn nested_false_exit_retires_child_local_root_without_resume_permission() {
    let mut calls = String::new();
    let mut expected = String::new();
    for outer in [0, 1, 2] {
        for inner in [0, 1, 2] {
            calls.push_str(&format!("run({outer},{inner});"));
            for _ in 0..outer {
                expected.push_str(if inner == 0 { "b\n" } else { "inner-b\n" });
            }
        }
    }
    let text = format!(
        r#"fun run(outer: Int, inner: Int) {{
        var n0=outer; var g0=n0>0;
        while(g0) {{
            var a="a"; var b="b"; var n1=inner; var g1=n1>0;
            while(g1) {{ val x=replace(&a,"inner-a"); val y=replace(&b,"inner-b"); n1=n1-1; g1=n1>0 }};
            println(b); n0=n0-1; g0=n0>0
        }}
    }}
    fun entry() {{ {calls} }}"#
    );
    let source = UnitSource::analyze("nested-local-exit.ko", &text).unwrap();
    source.assert_frontend_clean();
    let n = source
        .ownership()
        .owner_observations()
        .iter()
        .find_map(|o| o.nested_normal_body_v1())
        .expect("nested normal");
    let (live, drops) = n.false_exit_partition().expect("actual cleanup partition");
    assert_eq!(live.len(), 1);
    assert_eq!(drops.len(), 1);
    let (lowered, actions) =
        crate::ssa::observe_owner_actions(|| source.lower_entry(source.entry()));
    let (program, _) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    assert_eq!(
        actions
            .iter()
            .filter(|(k, _)| *k == "nested-false-cleanup-rechecked")
            .map(|(_, n)| *n)
            .collect::<Vec<_>>(),
        [1]
    );

    assert_general_counter_baseline(&text, expected.as_bytes(), false);
}

#[test]
fn ci_only_nested_false_cleanup_missing_actual_drop_fails_closed() {
    let source = UnitSource::analyze(
        "nested-exit-fault.ko",
        r#"
    fun run(outer: Int, inner: Int) {
        var n0=outer; var g0=n0>0;
        while(g0) { var a="a"; var b="b"; var n1=inner; var g1=n1>0;
            while(g1) { val x=replace(&a,"new-a"); val y=replace(&b,"new-b"); n1=n1-1; g1=n1>0 };
            println(b); n0=n0-1; g0=n0>0 }
    }
    fun entry() { run(2,2) }
    "#,
    )
    .unwrap();
    source.assert_frontend_clean();
    let (result, actions) = crate::ssa::observe_owner_actions(|| {
        crate::ssa::with_owner_fault(crate::ssa::OwnerFault::NestedFalseMissingDrop, || {
            source.lower_entry(source.entry())
        })
    });
    assert!(result.is_err());
    assert_eq!(
        actions
            .iter()
            .filter(|(k, _)| *k == "nested-false-fault-hit")
            .count(),
        1
    );
    assert!(
        actions
            .iter()
            .all(|(k, _)| *k != "nested-false-cleanup-rechecked"
                && *k != "nested-owner-normal-body-complete")
    );
}
