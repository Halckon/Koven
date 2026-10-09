//! 正常源码生成的实际 Map slot loan 及纯合同查询，不改写 IR。
use crate::ssa::{model::*, verify::verify_program};

#[test]
fn map_require_single_and_unit_verify_real_slot_and_source_end_order() {
    for (_, text, _) in crate::native_map_require_cases::cases() {
        for program in [
            crate::ssa::borrow_result_contract_tests::single(text).unwrap(),
            crate::ssa::borrow_storage_contract_tests::unit(text).unwrap(),
        ] {
            verify_program(&program).unwrap();
            let module = &program.modules[0];
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            assert!(llvm.contains("required.missing"));
            assert!(llvm.contains("required.found"));
            let slot_pointers = llvm
                .lines()
                .filter(|line| {
                    line.contains(".required")
                        && line.contains(" = getelementptr")
                        && line.ends_with(", i32 0, i32 2")
                })
                .count();
            let queries = module
                .functions
                .iter()
                .flat_map(|function| &function.instructions)
                .filter(|i| matches!(i.operation, Operation::MapRequireValue { .. }))
                .count();
            assert!(queries > 0);
            assert_eq!(
                slot_pointers, queries,
                "every query must return actual V field storage"
            );
            for function in &module.functions {
                for (index, instruction) in function.instructions.iter().enumerate() {
                    let Operation::MapRequireValue { source, key } = instruction.operation else {
                        continue;
                    };
                    let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                        unreachable!()
                    };
                    let result_type = function.entity(EntityId::Loan(*result)).unwrap().ty;
                    assert!(super::verify_map_operation(
                        module,
                        function,
                        &instruction.operation,
                        &[result_type]
                    ));
                    assert!(!super::verify_map_operation(
                        module,
                        function,
                        &instruction.operation,
                        &[EntityType::Value(result_type.semantic_type())]
                    ));
                    assert!(!super::verify_map_operation(
                        module,
                        function,
                        &Operation::MapRequireValue {
                            source: *result,
                            key
                        },
                        &[result_type]
                    ));
                    let result_end = function
                        .instructions
                        .iter()
                        .position(|i| {
                            matches!(i.operation,
         Operation::BorrowEnd { loan } if loan == *result)
                        })
                        .unwrap();
                    let source_end = function
                        .instructions
                        .iter()
                        .position(|i| {
                            matches!(i.operation,
         Operation::BorrowEnd { loan } if loan == source)
                        })
                        .unwrap();
                    assert!(index < result_end && result_end < source_end);
                    let source_type = function
                        .entity(EntityId::Loan(source))
                        .unwrap()
                        .ty
                        .semantic_type();
                    assert_eq!(
                        module.map_container(source_type).unwrap().2,
                        result_type.semantic_type()
                    );
                }
            }
        }
    }
}

#[test]
fn map_require_readonly_source_can_return_its_slot_through_ordinary_wrapper() {
    let text = "fun view(source: Map<String, String>): borrow String from source = source.requireValue(\"key\")\nfun entry() {}";
    // Without a native entry pruning the function, exercise its true borrowed return chain.
    let program = crate::ssa::borrow_result_contract_tests::single(text).unwrap();
    verify_program(&program).unwrap();
    let function = program.modules[0]
        .functions
        .iter()
        .find(|f| f.borrow_return.is_some())
        .unwrap();
    assert!(
        function
            .instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::MapRequireValue { .. }))
    );
    assert!(!function.instructions.iter().any(|i| matches!(
        i.operation,
        Operation::Copy { .. } | Operation::SharedRetain { .. }
    )));
}
