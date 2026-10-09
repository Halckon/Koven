//! 正常源码产物的真实 slot、共享 callback 与清理合同；不改写 IR。
use crate::ssa::{model::*, verify::verify_program};
#[test]
fn map_with_single_and_unit_verify_real_slot_callback_and_temporary_cleanup() {
    for (name, text, _) in crate::native_map_with_cases::cases() {
        for program in [
            crate::ssa::borrow_result_contract_tests::single(&text).unwrap(),
            crate::ssa::borrow_storage_contract_tests::unit(&text).unwrap(),
        ] {
            verify_program(&program).unwrap();
            let module = &program.modules[0];
            let llvm = crate::llvm::render_verified_program(&program).unwrap();
            let mut queries = 0;
            for function in &module.functions {
                for (index, i) in function.instructions.iter().enumerate() {
                    let Operation::MapWithValue {
                        source,
                        key,
                        action,
                    } = i.operation
                    else {
                        continue;
                    };
                    queries += 1;
                    let result = function.entity(i.results[0]).unwrap().ty;
                    assert!(super::verify_map_operation(
                        module,
                        function,
                        &i.operation,
                        &[result]
                    ));
                    assert!(!super::verify_map_operation(
                        module,
                        function,
                        &Operation::MapWithValue {
                            source: action,
                            key,
                            action
                        },
                        &[result]
                    ));
                    assert!(!super::verify_map_operation(
                        module,
                        function,
                        &Operation::MapWithValue {
                            source,
                            key,
                            action: source
                        },
                        &[result]
                    ));
                    assert!(!super::verify_map_operation(
                        module,
                        function,
                        &i.operation,
                        &[function.entity(EntityId::Loan(source)).unwrap().ty]
                    ));
                    let source_type = function
                        .entity(EntityId::Loan(source))
                        .unwrap()
                        .ty
                        .semantic_type();
                    let value_type = module.map_container(source_type).unwrap().2;
                    let action_type = function
                        .entity(EntityId::Loan(action))
                        .unwrap()
                        .ty
                        .semantic_type();
                    let signature = module.callable_signature(action_type).unwrap();
                    assert_eq!(
                        signature.parameters,
                        [EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: value_type
                        }]
                    );
                    assert!(signature.returns.is_empty());
                    let end = |target| {
                        function.instructions.iter().position(
                            |j| matches!(j.operation,Operation::BorrowEnd {loan} if loan==target),
                        )
                    };
                    let action_end = end(action).unwrap();
                    assert!(index < action_end);
                    let EntityId::Loan(key_loan) = key else {
                        unreachable!()
                    };
                    let key_end = end(key_loan).unwrap();
                    assert!(index < key_end);
                    if let Some(source_end) = end(source) {
                        assert!(index < source_end);
                    }
                    // A real temporary key/callback must be dropped after its loan has ended.
                    for (loan, ended) in [(key_loan, key_end), (action, action_end)] {
                        let place = function
                            .instructions
                            .iter()
                            .find_map(|j| match j.operation {
                                Operation::BorrowBegin { place, .. }
                                    if j.results == [EntityId::Loan(loan)] =>
                                {
                                    Some(place)
                                }
                                _ => None,
                            });
                        let owner = place.and_then(|place| {
                            function
                                .instructions
                                .iter()
                                .find_map(|j| match j.operation {
                                    Operation::RootPlace { owner }
                                        if j.results == [EntityId::Place(place)] =>
                                    {
                                        Some(owner)
                                    }
                                    _ => None,
                                })
                        });
                        if let Some(owner)=owner.and_then(|owner|function.instructions.iter().position(|j|matches!(j.operation,Operation::Drop {owner:current} if current==owner))) {assert!(ended<owner,"{name}");}
                    }
                }
            }
            assert!(queries > 0);
            assert_eq!(
                llvm.lines()
                    .filter(|l| l.contains("with.value.slot")
                        && l.contains(" = getelementptr")
                        && l.ends_with(", i32 0, i32 2"))
                    .count(),
                queries
            );
            for body in llvm
                .split("with.found")
                .skip(1)
                .filter(|s| s.starts_with(':'))
            {
                let found = body.split("with.missing").next().unwrap();
                assert_eq!(found.lines().filter(|l| l.contains("call void")).count(), 1);
            }
        }
    }
}
