//! 通用 Borrow storage 参数取代旧 String-only closure 布局限制。
use super::*;
use crate::ssa::verify::verify_program;

#[test]
fn move_only_borrow_parameter_uses_shared_storage_without_payload_owner() {
    // Preserve the exact former unsupported fixture as a positive ABI contract.
    let source = "package test\n\
        class Host {}\n\
        fun entry(): Unit {\n\
            val action: move (borrow Host) -> Unit = move { item -> }\n\
        }";
    let mut sources = SourceMap::new();
    let (source_id, file) = parsed(&mut sources, "test/move-only-borrow-parameter.ko", source);
    let inputs = [SourceUnitInput::new(
        "root",
        "test/move-only-borrow-parameter.ko",
        source_id,
        &file,
    )];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .unwrap();
    verify_program(&program).unwrap();
    let module = &program.modules[0];
    let thunk = module
        .functions
        .iter()
        .find(|f| f.name.contains("lambda"))
        .unwrap();
    let [EntityId::Loan(parameter)] = thunk
        .block(thunk.entry_block().unwrap())
        .unwrap()
        .parameters
        .as_slice()
    else {
        panic!("one shared storage parameter")
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = thunk.entity(EntityId::Loan(*parameter)).unwrap().ty
    else {
        panic!("borrow ABI")
    };
    assert!(matches!(
        module.type_kind(target),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    assert!(!thunk.instructions.iter().any(|i| matches!(
        i.operation,
        Operation::Drop { .. } | Operation::Copy { .. } | Operation::SharedRetain { .. }
    )));
    // Entry Borrow is a call-scoped parameter; Return ends this callee view.
    // The caller remains responsible for its actual source loan continuation.
    assert_eq!(
        operation_count(thunk, |i| matches!(i, Operation::BorrowEnd { .. })),
        0
    );
    let llvm = crate::llvm::render_verified_program(&program).unwrap();
    let header = llvm
        .lines()
        .find(|line| line.starts_with("define internal void") && line.contains("lambda"))
        .unwrap();
    assert!(header.contains("ptr %l"));
}
