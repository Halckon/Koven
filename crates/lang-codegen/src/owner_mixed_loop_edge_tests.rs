//! A Continue backedge and Break exit must retain distinct actual owner authority.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(limit: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
        if(index < 2) { continue } else { break }
    }
    return held
}
fun entry(): Unit { val a = run(0); val b = run(1); val c = run(2); val d = run(7) }
"#;

#[test]
fn mixed_continue_break_requires_actual_exit_return_authority() {
    let source = UnitSource::analyze("owner-mixed-loop-edges.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [receipt] = receipts.as_slice() else {
        panic!("one owned Return");
    };
    assert!(
        receipt.2.is_some(),
        "mixed Continue/Break exit lacks real Return authority"
    );
    assert_eq!(receipt.1, receipt.2);
}

#[test]
fn mixed_basic_and_const_finite_native_return_authority() {
    let printed = SOURCE.replace(
        "val a = run(0); val b = run(1); val c = run(2); val d = run(7)",
        "val a = run(0); println(a); val b = run(1); println(b); val c = run(2); println(c); val d = run(7); println(d)",
    );
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val SEVEN: Int = 7\n{}",
                printed.replace("run(7)", "run(SEVEN)")
            )
        } else {
            printed.clone()
        };
        let source = UnitSource::analyze("mixed-native.ko", &text).unwrap();
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
        let [receipt] = receipts.as_slice() else {
            panic!("one mixed Return")
        };
        assert!(receipt.2.is_some());
        assert_eq!(receipt.1, receipt.2);
        super::assert_native_output(&source, b"outer\nnext\nnext\nnext\n");
    }
}

#[test]
fn mixed_ssa_distinct_continue_header_and_break_exit_inputs() {
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    let source = UnitSource::analyze("mixed-ssa.ko", SOURCE).unwrap();
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
    let jump = |name: &str| {
        run.blocks
            .iter()
            .find(|b| {
                b.terminator
                    .as_ref()
                    .is_some_and(|t| source.sources.slice(t.origin.span()).unwrap() == name)
            })
            .unwrap()
    };
    let continued = jump("continue");
    let broken = jump("break");
    let TerminatorKind::Branch(backedge) = &continued.terminator.as_ref().unwrap().kind else {
        panic!("Continue branch")
    };
    let TerminatorKind::Branch(breakedge) = &broken.terminator.as_ref().unwrap().kind else {
        panic!("Break branch")
    };
    assert_ne!(backedge.target, breakedge.target);
    let header = run.block(backedge.target).unwrap();
    let exit = run.block(breakedge.target).unwrap();
    let TerminatorKind::Return { values } = &exit.terminator.as_ref().unwrap().kind else {
        panic!("shared exit Return")
    };
    let exit_index = exit
        .parameters
        .iter()
        .position(|v| *v == EntityId::Value(values[0]))
        .unwrap();
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("header split")
    };
    let body = run.block(when_true.target).unwrap();
    let replace = run
        .instructions
        .iter()
        .find(|i| i.block == body.id && matches!(i.operation, Operation::RootReplace { .. }))
        .unwrap();
    let Operation::RootReplace { owner, .. } = replace.operation else {
        unreachable!()
    };
    let body_index = body
        .parameters
        .iter()
        .position(|v| *v == EntityId::Value(owner))
        .unwrap();
    let h = when_true.arguments[body_index];
    let header_index = header.parameters.iter().position(|v| *v == h).unwrap();
    let TerminatorKind::Conditional {
        when_true: overflow,
        when_false: success,
        ..
    } = &body.terminator.as_ref().unwrap().kind
    else {
        panic!("checked Add")
    };
    assert!(matches!(
        run.block(overflow.target)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind,
        TerminatorKind::Abort
    ));
    let selected = run.block(success.target).unwrap();
    let q_index = success
        .arguments
        .iter()
        .position(|v| *v == replace.results[0])
        .unwrap();
    let q = selected.parameters[q_index];
    let TerminatorKind::Conditional {
        when_true: to_continue,
        when_false: to_break,
        ..
    } = &selected.terminator.as_ref().unwrap().kind
    else {
        panic!("real mixed if")
    };
    assert_eq!(to_continue.target, continued.id);
    assert_eq!(to_break.target, broken.id);
    for (arm, edge, forwarded) in [
        (continued, to_continue, backedge.arguments[header_index]),
        (broken, to_break, breakedge.arguments[exit_index]),
    ] {
        let position = edge.arguments.iter().position(|v| *v == q).unwrap();
        assert_eq!(arm.parameters[position], forwarded);
    }
    let finished = run.block(when_false.target).unwrap();
    let TerminatorKind::Branch(false_exit) = &finished.terminator.as_ref().unwrap().kind else {
        panic!("false H exit")
    };
    assert_eq!(false_exit.target, exit.id);
    let position = finished
        .parameters
        .iter()
        .position(|v| *v == false_exit.arguments[exit_index])
        .unwrap();
    assert_eq!(when_false.arguments[position], h);
}

#[test]
fn ci_only_mixed_rejects_missing_duplicate_or_wrong_jump_receipts() {
    for fault in [
        crate::ssa::OwnerFault::MissingMixedJump,
        crate::ssa::OwnerFault::DuplicateMixedJump,
        crate::ssa::OwnerFault::WrongMixedJumpTarget,
        crate::ssa::OwnerFault::WrongMixedJumpIdentity,
    ] {
        let source = UnitSource::analyze("mixed-fault.ko", SOURCE).unwrap();
        source.assert_frontend_clean();
        let result = crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()));
        assert!(result.is_err(), "mixed role receipt must remain exact");
    }
}
