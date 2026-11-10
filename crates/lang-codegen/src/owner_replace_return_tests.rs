//! Ordinary finite suffix Replace probe through original admitted products.
use super::{OwnedProducts, UnitSource};

const SOURCE: &str = r#"fun run(flag: Boolean): String {
    var held = "outer"
    while(flag) { val previous = replace(&held, "next"); break }
    return replace(&held, "last")
}
fun entry(): Unit { val first = run(false); val second = run(true) }
"#;

#[test]
fn suffix_replace_return_original_source_probe() {
    let source = UnitSource::analyze("suffix-replace-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    assert!(matches!(source.products, OwnedProducts::Basic(..)));
    assert_suffix_shape(&source);
}

fn assert_suffix_shape(source: &UnitSource) {
    use crate::ssa::model::{EntityId, Operation, TerminatorKind};
    use lang_frontend::ownership_checking::UnitDropPoint;
    let suffix = "replace(&held, \"last\")";
    let drops = source
        .ownership()
        .drops()
        .iter()
        .filter(|drop| source.sources.slice(drop.value_origin()).unwrap() == suffix)
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one actual root cleanup");
    };
    let UnitDropPoint::CallReturn(call) = drop.point() else {
        panic!("R retires at CallReturn");
    };
    assert_eq!(
        source
            .sources
            .slice(
                source
                    .parsed
                    .ast()
                    .expressions()
                    .get(call.expression())
                    .unwrap()
                    .span()
            )
            .unwrap(),
        suffix
    );
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
    let exchanges = run
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::RootReplace { .. }))
        .collect::<Vec<_>>();
    assert_eq!(exchanges.len(), 2);
    let exchange = exchanges
        .iter()
        .find(|i| source.sources.slice(i.origin.span()).unwrap() == suffix)
        .unwrap();
    let exit = run.block(exchange.block).unwrap();
    let Operation::RootReplace { replacement, .. } = exchange.operation else {
        unreachable!()
    };
    let literal = run
        .instructions
        .iter()
        .find(
            |i| matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == b"last"),
        )
        .unwrap();
    assert_eq!(literal.results, [EntityId::Value(replacement)]);
    let drops = run
        .instructions
        .iter()
        .filter(|i| i.block == exit.id && matches!(i.operation, Operation::Drop { .. }))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        panic!("one installed R drop at exit");
    };
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    assert_eq!(EntityId::Value(owner), exchange.results[0]);
    let TerminatorKind::Return { values } = &exit.terminator.as_ref().unwrap().kind else {
        panic!("old payload Return");
    };
    assert_eq!(values.len(), 1);
    assert_eq!(EntityId::Value(values[0]), exchange.results[1]);
    let position = |id| exit.instructions.iter().position(|i| *i == id).unwrap();
    assert!(position(exchange.id) < position(drop.id));
}

#[test]
fn suffix_replace_return_actual_old_slot_requires_authorization() {
    let source = UnitSource::analyze("suffix-replace-return.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let (lowered, returns) =
        crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()));
    let (program, entry) = lowered.expect("actual detached old slot requires authorized Return");
    crate::ssa::verify::verify_program(&program).unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    let [receipt] = returns.as_slice() else {
        panic!("one explicit Return");
    };
    assert!(receipt.2.is_some());
    assert_eq!(receipt.1, receipt.2);
}

#[test]
fn suffix_replace_return_native_delivers_selected_old_value() {
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
        let source = UnitSource::analyze("suffix-replace-return-native.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        super::assert_native_output(&source, b"outer\nnext\n");
    }
}

#[test]
fn suffix_replace_return_both_units_authorize_distinct_old_and_installed_slots() {
    for constant in [false, true] {
        let text = if constant {
            format!(
                "const val ON: Boolean = true\n{}",
                SOURCE.replace("run(true)", "run(ON)")
            )
        } else {
            SOURCE.to_owned()
        };
        let source = UnitSource::analyze("suffix-replace-return-dual.ko", &text).unwrap();
        source.assert_frontend_clean();
        assert_eq!(
            matches!(source.products, OwnedProducts::Constant(..)),
            constant
        );
        assert_suffix_shape(&source);
        let ((lowered, returns), actions) = crate::ssa::observe_owner_actions(|| {
            crate::ssa::observe_owner_returns(|| source.lower_entry(source.entry()))
        });
        let (program, entry) = lowered.unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
        let [receipt] = returns.as_slice() else {
            panic!("one explicit Return");
        };
        let old = receipt.2.unwrap();
        assert_eq!(receipt.1, Some(old));
        let root = actions[0].1;
        assert_ne!(root, old);
        assert_eq!(
            actions,
            [
                ("root", root),
                ("replace_result", old),
                ("replace_installed_drop", root)
            ]
        );
    }
}

// Explicit protocol faults are compiled locally and executed only by official CI.
#[test]
fn ci_only_suffix_replace_return_rejects_incomplete_or_wrong_handoffs() {
    use crate::ssa::OwnerFault::{
        DuplicateOwnerReturn, MissingFinish, MissingOwnerReturn, WrongReturnPayload,
    };
    for fault in [
        DuplicateOwnerReturn,
        MissingOwnerReturn,
        WrongReturnPayload,
        MissingFinish,
    ] {
        let source = UnitSource::analyze("suffix-replace-return-ci.ko", SOURCE).unwrap();
        source.assert_frontend_clean();
        assert!(
            crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry())).is_err()
        );
    }
}

#[test]
fn suffix_replace_hooks_preserve_existing_ordinary_consumers() {
    let texts = [
        SOURCE.replace("return replace(&held, \"last\")", "return held"),
        SOURCE.replace(
            "return replace(&held, \"last\")",
            "val forwarded = held; return forwarded",
        ),
        SOURCE
            .replace(
                "fun run(flag: Boolean): String",
                "fun run(flag: Boolean): Unit",
            )
            .replace("return replace(&held, \"last\")", "val retired = held"),
        format!(
            "fun sink(own value: String, own count: Int): Unit {{}}\n{}",
            SOURCE
                .replace(
                    "fun run(flag: Boolean): String",
                    "fun run(flag: Boolean): Unit"
                )
                .replace("return replace(&held, \"last\")", "sink(held, 7)")
        ),
    ];
    for (index, text) in texts.iter().enumerate() {
        let source = UnitSource::analyze("existing-owner-consumer.ko", text).unwrap();
        source.assert_frontend_clean();
        let boundaries = source
            .ownership()
            .owner_observations()
            .iter()
            .filter_map(|o| o.replace_break_observation_v1())
            .collect::<Vec<_>>();
        let [boundary] = boundaries.as_slice() else {
            panic!("one original closed consumer");
        };
        match index {
            0 | 1 => assert!(boundary.suffix_return_consumer_v1().is_some()),
            2 => assert!(boundary.suffix_consumer_v1().is_some()),
            3 => assert!(boundary.suffix_call_consumer_v1().is_some()),
            _ => unreachable!(),
        }
        let (program, entry) = source.lower_entry(source.entry()).unwrap();
        crate::ssa::verify::verify_program(&program).unwrap();
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    }
}

// CI-only: never include these mutations in a local execution selection.
fn reject_suffix_receipt_fault(fault: crate::ssa::OwnerFault) {
    let source = UnitSource::analyze("suffix-receipt-ci.ko", SOURCE).unwrap();
    source.assert_frontend_clean();
    let result = crate::ssa::with_owner_fault(fault, || source.lower_entry(source.entry()));
    let Err(error) = result else {
        panic!("missing or invalid protocol receipt must fail loud");
    };
    assert_eq!(error.kind, crate::ssa::LoweringErrorKind::InvalidModel);
}

#[test]
fn ci_only_suffix_return_rejects_missing_replace_receipt() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::MissingReturnReplaceReceipt);
}

#[test]
fn ci_only_suffix_return_rejects_duplicate_replace_receipt() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::DuplicateReturnReplaceReceipt);
}

#[test]
fn ci_only_suffix_return_rejects_old_is_root() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::ReturnReplaceOldIsRoot);
}

#[test]
fn ci_only_suffix_return_rejects_drop_is_old() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::ReturnReplaceDropIsOld);
}

#[test]
fn ci_only_suffix_return_rejects_missing_drop_receipt() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::MissingReturnReplaceDropReceipt);
}

#[test]
fn ci_only_suffix_return_rejects_duplicate_drop_receipt() {
    reject_suffix_receipt_fault(crate::ssa::OwnerFault::DuplicateReturnReplaceDropReceipt);
}
