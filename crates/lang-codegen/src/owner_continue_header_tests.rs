//! Real Continue has its own source/target identity; it is not a normal fallthrough.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(limit: Int): String {
    var held = "outer"
    var index = 0
    while(index < limit) {
        val previous = replace(&held, "next")
        index = index + 1
        continue
    }
    return held
}
fun entry(): Unit { val a = run(0); val b = run(1); val c = run(2); val d = run(7) }
"#;

#[test]
fn original_continue_requires_actual_header_return_authority() {
    let source = UnitSource::analyze("owner-continue-header.ko", SOURCE).unwrap();
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
        "Continue header current owner lacks real Return authority"
    );
    assert_eq!(receipt.1, receipt.2);
}

#[test]
fn continue_real_ssa_preserves_jump_header_and_abort() {
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    let source = UnitSource::analyze("continue-cfg.ko", SOURCE).unwrap();
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
    assert_eq!(
        source
            .sources
            .slice(success.terminator.as_ref().unwrap().origin.span())
            .unwrap(),
        "continue",
        "the backedge is the actual Continue terminator, not inferred fallthrough"
    );
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
fn continue_both_units_authorize_and_run_finite_examples() {
    let source = SOURCE.replace("val a = run(0); val b = run(1); val c = run(2); val d = run(7)",
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
        let source = UnitSource::analyze("continue-native.ko", &text).unwrap();
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
            panic!("one Continue-header Return");
        };
        assert!(receipt.2.is_some());
        assert_eq!(receipt.1, receipt.2);
        super::assert_native_output(&source, b"outer\nnext\nnext\nnext\n");
    }
}

#[test]
fn boolean_continue_without_increment_authorizes_header_return() {
    use crate::ssa::model::TerminatorKind;
    let text = r#"fun run(flag: Boolean): String {
    var held = "outer"
    var again = flag
    while(again) {
        val previous = replace(&held, "next")
        again = false
        continue
    }
    return held
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;
    let source = UnitSource::analyze("boolean-continue-header.ko", text).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    let (lowered, receipts) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.unwrap();
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [receipt] = receipts.as_slice() else {
        panic!("one Boolean Continue-header Return");
    };
    assert!(receipt.2.is_some());
    assert_eq!(receipt.1, receipt.2);
    let jumps = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .flat_map(|function| &function.blocks)
        .filter_map(|block| block.terminator.as_ref())
        .filter(|term| source.sources.slice(term.origin.span()).unwrap() == "continue")
        .collect::<Vec<_>>();
    let [jump] = jumps.as_slice() else {
        panic!("one actual Boolean Continue terminator");
    };
    assert!(matches!(jump.kind, TerminatorKind::Branch(_)));
}

#[test]
fn ci_only_continue_rejects_missing_or_retargeted_receipt() {
    for fault in [
        crate::ssa::OwnerFault::MissingHeaderContinue,
        crate::ssa::OwnerFault::DuplicateHeaderContinue,
        crate::ssa::OwnerFault::WrongHeaderContinueIdentity,
        crate::ssa::OwnerFault::MissingHeaderContinueDrop,
        crate::ssa::OwnerFault::WrongHeaderContinueTarget,
        crate::ssa::OwnerFault::WrongHeaderContinueSource,
    ] {
        let source = UnitSource::analyze("continue-fault.ko", SOURCE).unwrap();
        source.assert_frontend_clean();
        let result = crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()));
        assert!(
            result.is_err(),
            "actual Continue evidence cannot be missing or retargeted"
        );
    }
}
