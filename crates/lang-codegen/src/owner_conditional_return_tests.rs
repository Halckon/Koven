//! Conditional Return and implicit normal backedge require distinct owner authority.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(limit: Int, stop: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
        if(index < stop) { return held }
    }
    return held
}
fun entry(): Unit { val a = run(0, 2); val b = run(1, 2); val c = run(7, 2); val d = run(2, 7) }
"#;

#[test]
fn conditional_return_requires_both_actual_return_authorities() {
    let source = UnitSource::analyze("owner-conditional-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.expect("real conditional Return source must lower");
    crate::ssa::verify::verify_program(&program).expect("real SSA must verify");
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("verified SSA must render LLVM");
    let [conditional, tail] = receipts.as_slice() else {
        panic!("exactly two actual owned Returns, got {receipts:?}");
    };
    assert_ne!(conditional.0, tail.0, "distinct source Return identities");
    for (role, receipt) in [("conditional", conditional), ("tail", tail)] {
        assert!(
            receipt.1.is_some(),
            "{role} Return must have a real payload"
        );
        assert!(
            receipt.2.is_some(),
            "{role} Return lacks actual owner authority"
        );
        assert_eq!(
            receipt.1, receipt.2,
            "{role} authority must match its own payload"
        );
    }
}

#[test]
fn conditional_return_complete_incoming_and_parameter_sources() {
    use crate::ssa::model::{
        ComparisonOperator, EntityId, EntityType, LoanKind, Operation, SsaTypeKind, TerminatorKind,
    };
    let source = UnitSource::analyze("conditional-return-cfg.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (program, entry) = source.lower_entry(source.entry()).unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let run = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| {
            source
                .sources
                .slice(f.origin.span())
                .unwrap()
                .starts_with("fun run(")
        })
        .unwrap();
    let [limit, stop] = run.blocks.first().unwrap().parameters.as_slice() else {
        panic!("two parameters")
    };
    assert_ne!(limit, stop);
    for parameter in [*limit, *stop] {
        assert!(matches!(parameter, EntityId::Loan(_)));
        let EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        } = run.entity(parameter).unwrap().ty
        else {
            panic!("shared loan")
        };
        assert_eq!(
            program.module(target.module()).unwrap().types[target.index()],
            SsaTypeKind::Integer {
                bits: 32,
                signed: true
            }
        );
    }
    let compare = |text: &str| {
        run.instructions
            .iter()
            .find(|i| {
                matches!(i.operation, Operation::Compare { .. })
                    && source.sources.slice(i.origin.span()).unwrap() == text
            })
            .unwrap()
    };
    let hc = compare("index < limit");
    let sc = compare("index < stop");
    for (instruction, parameter) in [(hc, *limit), (sc, *stop)] {
        let Operation::Compare {
            operator, right, ..
        } = instruction.operation
        else {
            unreachable!()
        };
        assert_eq!(operator, ComparisonOperator::LessThan);
        assert_eq!(
            super::owner_parameter_mixed_tests::parameter_roots(run, EntityId::Value(right)),
            [parameter]
        );
    }
    let header = run.block(hc.block).unwrap();
    let split = run.block(sc.block).unwrap();
    let TerminatorKind::Conditional {
        condition,
        when_true: body,
        when_false: finished,
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("header")
    };
    assert_eq!(hc.results, [EntityId::Value(*condition)]);
    let TerminatorKind::Conditional {
        condition,
        when_true: broken,
        when_false: implicit,
    } = &split.terminator.as_ref().unwrap().kind
    else {
        panic!("split")
    };
    assert_eq!(sc.results, [EntityId::Value(*condition)]);
    let direct = run.block(broken.target).unwrap();
    let tail = run.block(finished.target).unwrap();
    let TerminatorKind::Return {
        values: direct_values,
    } = &direct.terminator.as_ref().unwrap().kind
    else {
        panic!("true arm returns directly; {run:#?}")
    };
    let TerminatorKind::Return {
        values: tail_values,
    } = &tail.terminator.as_ref().unwrap().kind
    else {
        panic!("false loop exit returns directly after folding; {run:#?}")
    };
    assert_eq!(direct_values.len(), 1);
    assert_eq!(tail_values.len(), 1);
    assert_ne!(direct.id, tail.id);
    let TerminatorKind::Branch(backedge) = &run
        .block(implicit.target)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("implicit normal backedge; {run:#?}")
    };
    assert_eq!(backedge.target, header.id);
    let incoming = |target| {
        run.blocks
            .iter()
            .filter(|b| {
                b.terminator
                    .as_ref()
                    .is_some_and(|t| t.kind.targets().contains(&target))
            })
            .map(|b| b.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        incoming(header.id),
        [run.entry_block().unwrap(), implicit.target]
    );
    assert_eq!(incoming(body.target), [header.id]);
    assert_eq!(incoming(direct.id), [split.id]);
    assert_eq!(incoming(tail.id), [header.id]);
    assert_eq!(incoming(implicit.target), [split.id]);
    // Each endpoint must receive the current owner from its own predecessor.
    let direct_index = direct
        .parameters
        .iter()
        .position(|p| *p == EntityId::Value(direct_values[0]))
        .unwrap();
    let tail_index = tail
        .parameters
        .iter()
        .position(|p| *p == EntityId::Value(tail_values[0]))
        .unwrap();
    let current_h = finished.arguments[tail_index];
    assert!(header.parameters.contains(&current_h));
    let current_q = broken.arguments[direct_index];
    assert!(split.parameters.contains(&current_q));
    let normal = run.block(implicit.target).unwrap();
    let header_index = header
        .parameters
        .iter()
        .position(|p| *p == current_h)
        .unwrap();
    let normal_index = normal
        .parameters
        .iter()
        .position(|p| *p == backedge.arguments[header_index])
        .unwrap();
    assert_eq!(implicit.arguments[normal_index], current_q);
}

#[test]
fn conditional_return_basic_const_finite_native() {
    let printed=SOURCE.replace("val a = run(0, 2); val b = run(1, 2); val c = run(7, 2); val d = run(2, 7)",
        "val a = run(0, 2); println(a); val b = run(1, 2); println(b); val c = run(7, 2); println(c); val d = run(2, 0); println(d); val e = run(7, -1); println(e)");
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val SEVEN: Int = 7\n{}",
                printed.replace("run(7,", "run(SEVEN,")
            )
        } else {
            printed.clone()
        };
        let source = UnitSource::analyze("conditional-return-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        let (lowered, receipts) =
            crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let [first, second] = receipts.as_slice() else {
            panic!("two Returns")
        };
        assert_ne!(first.0, second.0);
        for receipt in [first, second] {
            assert!(receipt.2.is_some());
            assert_eq!(receipt.1, receipt.2);
        }
        super::assert_native_output(&source, b"outer\nnext\nnext\nnext\nnext\n");
    }
}

#[test]
fn ci_only_conditional_return_rejects_missing_duplicate_or_wrong_receipt() {
    for fault in [
        crate::ssa::OwnerFault::MissingConditionalReturn,
        crate::ssa::OwnerFault::DuplicateConditionalReturn,
        crate::ssa::OwnerFault::WrongConditionalReturnSlot,
    ] {
        let source = UnitSource::analyze("conditional-return-fault.ko", SOURCE).unwrap();
        source.assert_frontend_clean();
        let result = crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()));
        assert!(result.is_err(), "each Return must retain its exact receipt");
    }
}
