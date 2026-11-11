//! Conditional Break and implicit normal backedge require distinct owner authority.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(limit: Int, stop: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
        if(index < stop) { break }
    }
    return held
}
fun entry(): Unit { val a = run(0, 2); val b = run(1, 2); val c = run(7, 2); val d = run(2, 7) }
"#;

#[test]
fn conditional_break_requires_actual_exit_return_authority() {
    let source = UnitSource::analyze("owner-conditional-break.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.expect("real conditional Break source must lower");
    crate::ssa::verify::verify_program(&program).expect("real SSA must verify");
    crate::llvm::render_verified_program_with_entry(&program, entry)
        .expect("verified SSA must render LLVM");
    let [receipt] = receipts.as_slice() else {
        panic!("one owned Return");
    };
    assert!(
        receipt.2.is_some(),
        "conditional Break and normal backedge lack real Return authority"
    );
    assert_eq!(receipt.1, receipt.2);
}

#[test]
fn conditional_break_complete_incoming_and_parameter_sources() {
    use crate::ssa::model::{
        ComparisonOperator, EntityId, EntityType, LoanKind, Operation, SsaTypeKind, TerminatorKind,
    };
    let source = UnitSource::analyze("conditional-break-cfg.ko", SOURCE).unwrap();
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
    let branch = |id| {
        let TerminatorKind::Branch(edge) =
            &run.block(id).unwrap().terminator.as_ref().unwrap().kind
        else {
            panic!(
                "expected Branch at {id:?}, got {:?}; SSA: {run:#?}",
                run.block(id).unwrap().terminator
            )
        };
        edge
    };
    let break_edge = branch(broken.target);
    assert_eq!(
        source
            .sources
            .slice(
                run.block(broken.target)
                    .unwrap()
                    .terminator
                    .as_ref()
                    .unwrap()
                    .origin
                    .span()
            )
            .unwrap(),
        "break"
    );
    // Refine witnesses false -> join -> header before emission. The emitter
    // folds the single-predecessor join into this implicit false block.
    let backedge = branch(implicit.target);
    assert_eq!(
        backedge.target, header.id,
        "implicit false must feed header directly after folding; SSA: {run:#?}"
    );
    assert_ne!(break_edge.target, header.id);
    let exit = break_edge.target;
    assert_eq!(branch(finished.target).target, exit);
    assert!(matches!(
        run.block(exit).unwrap().terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { .. }
    ));
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
        [run.entry_block().unwrap(), implicit.target],
        "header must have exactly entry and implicit-backedge predecessors; SSA: {run:#?}"
    );
    assert_eq!(incoming(body.target), [header.id]);
    assert_eq!(incoming(finished.target), [header.id]);
    assert_eq!(incoming(broken.target), [split.id]);
    assert_eq!(incoming(implicit.target), [split.id]);
    let mut expected = vec![finished.target, broken.target];
    expected.sort_by_key(|b| b.index());
    assert_eq!(incoming(exit), expected);
}

#[test]
fn conditional_break_basic_const_finite_native() {
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
        let source = UnitSource::analyze("conditional-break-native.ko", &text).unwrap();
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
            panic!("one Return")
        };
        assert!(receipt.2.is_some());
        assert_eq!(receipt.1, receipt.2);
        super::assert_native_output(&source, b"outer\nnext\nnext\nnext\nnext\n");
    }
}

#[test]
fn ci_only_conditional_break_rejects_missing_duplicate_or_wrong_normal_edge() {
    for fault in [
        crate::ssa::OwnerFault::MissingConditionalNormal,
        crate::ssa::OwnerFault::DuplicateConditionalNormal,
        crate::ssa::OwnerFault::WrongConditionalNormalTarget,
    ] {
        let source = UnitSource::analyze("conditional-break-fault.ko", SOURCE).unwrap();
        source.assert_frontend_clean();
        let result = crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()));
        assert!(
            result.is_err(),
            "normal edge must have a unique exact receipt"
        );
    }
}
