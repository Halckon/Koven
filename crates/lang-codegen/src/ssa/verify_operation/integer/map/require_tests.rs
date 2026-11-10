//! 正常源码生成的实际 Map slot loan 及纯合同查询，不改写 IR。
use crate::ssa::{
    borrow_memory_test_support::single as memory_unit, borrow_storage_contract_tests::unit,
    model::*, verify::verify_program,
};

#[test]
fn map_require_memory_and_rooted_verify_real_slot_and_source_end_order() {
    for (_, text, _) in crate::native_map_require_cases::cases() {
        for program in [memory_unit(text).unwrap(), unit(text).unwrap()] {
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

const READONLY_WRAPPER: &str = "fun view(source: Map<String, String>): borrow String from source = source.requireValue(\"key\")\nfun entry() {}";

#[test]
fn map_require_readonly_source_can_return_its_slot_through_ordinary_wrapper() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    let unit = UnitSource::analyze("require-wrapper.ko", READONLY_WRAPPER).unwrap();
    unit.assert_frontend_clean();
    let declaration = unit.declaration("view", SymbolKind::Function);
    let (program, view) = unit.lower_entry(declaration).unwrap();
    verify_program(&program).unwrap();
    let module = program.module(view.module()).unwrap();
    let function = module.function(view).unwrap();
    let view_origin = Origin::Source(unit.declaration_span(declaration));
    assert_eq!(function.origin, view_origin);
    assert!(function.borrow_return.is_some());
    let query = function
        .instructions
        .iter()
        .find(|i| matches!(i.operation, Operation::MapRequireValue { .. }))
        .expect("the selected wrapper borrows the original slot");
    let [EntityId::Loan(slot)] = query.results.as_slice() else {
        panic!("the slot is a loan");
    };
    assert!(function.blocks.iter().any(|block| matches!(
        block.terminator.as_ref().map(|term| &term.kind),
        Some(TerminatorKind::BorrowReturn { loan }) if loan == slot
    )));
    assert!(!function.instructions.iter().any(|i| matches!(
        i.operation,
        Operation::Copy { .. } | Operation::SharedRetain { .. }
    )));

    let entry = unit.declaration("entry", SymbolKind::Function);
    let (program, _) = unit.lower_entry(entry).unwrap();
    verify_program(&program).unwrap();
    assert!(
        program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .all(|function| function.origin != view_origin && function.borrow_return.is_none())
    );
}

#[test]
fn legacy_whole_file_emits_unselected_map_require_wrapper() {
    let program = crate::ssa::borrow_result_contract_tests::single(READONLY_WRAPPER).unwrap();
    verify_program(&program).unwrap();
    assert!(
        program.modules[0]
            .functions
            .iter()
            .any(|f| f.borrow_return.is_some())
    );
}
