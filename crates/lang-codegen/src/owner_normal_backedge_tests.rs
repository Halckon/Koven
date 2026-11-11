//! Ordinary finite original-source probe for a replaced owner crossing a normal backedge.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    var again = flag
    while(again) { val previous = replace(&held, "next"); again = false }
    return held
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn normal_backedge_original_source_probe() {
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    let source = UnitSource::analyze("owner-normal-backedge.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (program, entry) = source.lower_entry(source.entry()).unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    eprintln!("OWNERSHIP {:#?}", source.ownership());
    eprintln!("SSA {}", crate::ssa::render_program(&program));
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
    let replacements = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
        .collect::<Vec<_>>();
    let [replacement] = replacements.as_slice() else {
        panic!("one real body Replace");
    };
    let body = run.block(replacement.block).unwrap();
    let TerminatorKind::Branch(backedge) = &body.terminator.as_ref().unwrap().kind else {
        panic!("body must take the actual normal backedge");
    };
    let header = run.block(backedge.target).unwrap();
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("normal backedge targets the real condition header");
    };
    assert_eq!(when_true.target, body.id);
    assert_eq!(backedge.arguments[0], replacement.results[0]);
    assert_eq!(when_true.arguments, [header.parameters[0]]);
    assert_eq!(when_false.arguments, [header.parameters[0]]);
    let Operation::RootReplace { owner, .. } = replacement.operation else {
        unreachable!()
    };
    assert_eq!(body.parameters, [EntityId::Value(owner)]);
    let incoming = run
        .blocks
        .iter()
        .filter_map(|b| match &b.terminator.as_ref()?.kind {
            TerminatorKind::Branch(edge) if edge.target == header.id => Some((b.id, edge)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(incoming.len(), 2, "entry and normal backedge only");
    let entry_edge = incoming.iter().find(|(id, _)| *id != body.id).unwrap().1;
    let initial = run
        .instructions
        .iter()
        .find(
            |i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"outer"),
        )
        .unwrap();
    assert_eq!(entry_edge.arguments[0], initial.results[0]);
    let drops = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("only actual detached old owner drops in run");
    };
    let Operation::Drop { owner: retired } = drop.operation else {
        unreachable!()
    };
    assert_eq!(EntityId::Value(retired), replacement.results[1]);
    assert_eq!(drop.block, body.id);
    let position = |id| body.instructions.iter().position(|i| *i == id).unwrap();
    assert!(position(replacement.id) < position(drop.id));
    let exit = run.block(when_false.target).unwrap();
    let TerminatorKind::Return { values } = &exit.terminator.as_ref().unwrap().kind else {
        panic!("condition false returns current header owner");
    };
    assert_eq!(values.len(), 1);
    assert_eq!(exit.parameters, [EntityId::Value(values[0])]);
    assert_eq!(
        run.blocks
            .iter()
            .filter(|b| matches!(
                b.terminator.as_ref().unwrap().kind,
                TerminatorKind::Return { .. }
            ))
            .count(),
        1
    );
}

#[test]
fn normal_backedge_requires_actual_header_return_authority() {
    let source = UnitSource::analyze("normal-backedge-receipt.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [receipt] = receipts.as_slice() else {
        panic!("one real owned Return");
    };
    assert!(
        receipt.2.is_some(),
        "actual header current owner needs a Return authorization receipt"
    );
    assert_eq!(receipt.1, receipt.2);
}

#[test]
fn normal_backedge_both_units_authorize_current_header() {
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                SOURCE.replace("run(true)", "run(ON)")
            )
        } else {
            SOURCE.to_owned()
        };
        let source = UnitSource::analyze("normal-backedge-dual.ko", &text).unwrap();
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
            panic!("one real header Return");
        };
        assert!(receipt.2.is_some());
        assert_eq!(receipt.1, receipt.2);
    }
}

#[test]
fn normal_backedge_native_preserves_zero_and_one_iteration() {
    let text = SOURCE.replace(
        "val first = run(false); val second = run(true)",
        "val first = run(false); println(first); val second = run(true); println(second)",
    );
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                text.replace("run(true)", "run(ON)")
            )
        } else {
            text.clone()
        };
        let source = UnitSource::analyze("normal-backedge-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        super::assert_native_output(&source, b"outer\nnext\n");
    }
}

#[test]
fn ci_only_normal_backedge_rejects_extra_header_definition() {
    let source = UnitSource::analyze("normal-backedge-extra-definition.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let result = crate::ssa::with_owner_fault(crate::ssa::OwnerFault::ExtraHeaderAlias, || {
        source.lower_entry(source.entry())
    });
    assert!(
        result.is_err(),
        "a hidden H redefinition must not receive header Return authority"
    );
}

/// Ordinary source/SSA probe only: four finite caller inputs are not an induction proof.
#[test]
fn counter_backedge_requires_actual_header_return_authority() {
    let source = UnitSource::analyze(
        "owner-counter-backedge.ko",
        r#"fun run(limit: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) { val previous = replace(&held, "next"); index = index + 1 }
    return held
}
fun entry(): Unit {
    val zero = run(0)
    val one = run(1)
    val two = run(2)
    val seven = run(7)
}
"#,
    )
    .unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    assert!(!llvm.is_empty());
    let [receipt] = receipts.as_slice() else {
        panic!("one real counter-loop owned Return");
    };
    assert!(
        receipt.2.is_some(),
        "counter-loop current header owner lacks Return authorization"
    );
    assert_eq!(receipt.1, receipt.2);
}

const COUNTER_SOURCE: &str = r#"fun run(limit: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) { val previous = replace(&held, "next"); index = index + 1 }
    return held
}
fun entry(): Unit { val a = run(0); val b = run(1); val c = run(2); val d = run(7) }
"#;

#[test]
fn counter_backedge_real_ssa_preserves_header_and_abort() {
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    let source = UnitSource::analyze("counter-cfg.ko", COUNTER_SOURCE).unwrap();
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
    let replacements = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
        .collect::<Vec<_>>();
    let [replace] = replacements.as_slice() else {
        panic!("one Replace");
    };
    let Operation::RootReplace { owner, .. } = replace.operation else {
        unreachable!()
    };
    let body = run.block(replace.block).unwrap();
    let owner_index = body
        .parameters
        .iter()
        .position(|p| *p == EntityId::Value(owner))
        .unwrap();
    let (header, on_true, on_false) = run
        .blocks
        .iter()
        .find_map(|b| match &b.terminator.as_ref()?.kind {
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } if when_true.target == body.id => Some((b, when_true, when_false)),
            _ => None,
        })
        .unwrap();
    let h = on_true.arguments[owner_index];
    let header_index = header.parameters.iter().position(|p| *p == h).unwrap();
    let incoming = run
        .blocks
        .iter()
        .filter_map(|b| match &b.terminator.as_ref()?.kind {
            TerminatorKind::Branch(edge) if edge.target == header.id => Some((b, edge)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(incoming.len(), 2);
    let initial = run
        .instructions
        .iter()
        .find(|i| matches!(&i.operation, Operation::StringLiteral {bytes,..} if bytes == b"outer"))
        .unwrap();
    assert!(
        incoming
            .iter()
            .any(|(_, edge)| edge.arguments[header_index] == initial.results[0])
    );
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &body.terminator.as_ref().unwrap().kind
    else {
        panic!("checked Add split");
    };
    assert!(matches!(
        run.block(when_true.target)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind,
        TerminatorKind::Abort
    ));
    let (success, backedge) = incoming
        .iter()
        .find(|(block, _)| block.id == when_false.target)
        .unwrap();
    let forwarded = when_false
        .arguments
        .iter()
        .position(|argument| *argument == replace.results[0])
        .unwrap();
    assert_eq!(
        backedge.arguments[header_index],
        success.parameters[forwarded]
    );
    let drops = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one detached Drop");
    };
    assert_eq!(drop.block, body.id);
    assert!(
        matches!(drop.operation, Operation::Drop {owner} if EntityId::Value(owner) == replace.results[1])
    );
    let exit = run.block(on_false.target).unwrap();
    let TerminatorKind::Return { values } = &exit.terminator.as_ref().unwrap().kind else {
        panic!("false exit Return");
    };
    let position = exit
        .parameters
        .iter()
        .position(|p| *p == EntityId::Value(values[0]))
        .unwrap();
    assert_eq!(on_false.arguments[position], h);
}

#[test]
fn counter_backedge_both_units_run_finite_normal_examples() {
    let source = COUNTER_SOURCE.replace("val a = run(0); val b = run(1); val c = run(2); val d = run(7)",
        "val a = run(0); println(a); val b = run(1); println(b); val c = run(2); println(c); val d = run(7); println(d)");
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val SEVEN: Int = 7\n{}",
                source.replace("run(7)", "run(SEVEN)")
            )
        } else {
            source.clone()
        };
        let source = UnitSource::analyze("counter-native.ko", &text).unwrap();
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
            panic!("one Return");
        };
        assert!(receipt.2.is_some());
        assert_eq!(receipt.1, receipt.2);
        super::assert_native_output(&source, b"outer\nnext\nnext\nnext\n");
    }
}
