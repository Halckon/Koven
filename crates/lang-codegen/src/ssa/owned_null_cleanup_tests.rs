//! SPEC-0296: inspect unmodified ordinary-source SSA, never fabricate IR.
use super::{
    borrow_result_contract_tests::single,
    borrow_storage_contract_tests::unit,
    model::{EntityId, Operation},
};

#[test]
fn owned_null_cleanup_presence_precedes_unique_asap_drop() {
    for payload in ["String", "Text"] {
        let value = if payload == "String" {
            "\"text\".clone()"
        } else {
            "Text(\"text\".clone())"
        };
        let source = format!(
            "value class Text(val text: String)\nfun entry() {{ var m = mutableMapOf<Int, {payload}>(); m.put(1, {value}); val removed = m.remove(1); val present = ((removed)) != null; if (present) {{ println(\"present\") }} }}"
        );
        for (label, program) in [("single", single(&source)), ("unit", unit(&source))] {
            let program = program.unwrap_or_else(|error| panic!("{label}/{payload}: {error:?}"));
            super::verify::verify_program(&program).unwrap();
            let function = program.modules[0]
                .functions
                .iter()
                .find(|function| function.name.contains("entry"))
                .unwrap();
            let removed = function
                .instructions
                .iter()
                .find(|instruction| matches!(instruction.operation, Operation::MapRemove { .. }))
                .unwrap();
            let [EntityId::Value(_), EntityId::Value(owner)] = removed.results.as_slice() else {
                panic!("owned result")
            };
            let reads = function.instructions.iter().enumerate().filter_map(|(index, instruction)| matches!(instruction.operation, Operation::AggregateProject { aggregate, field: 0 } if aggregate == *owner).then_some(index)).collect::<Vec<_>>();
            let drops = function.instructions.iter().enumerate().filter_map(|(index, instruction)| matches!(instruction.operation, Operation::Drop { owner: dropped } if dropped == *owner).then_some(index)).collect::<Vec<_>>();
            assert_eq!(reads.len(), 1, "{label}/{payload}");
            assert_eq!(drops.len(), 1, "{label}/{payload}");
            assert!(
                reads[0] < drops[0],
                "presence read must precede original ASAP cleanup"
            );
            assert!(!function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::Copy { source } if source == *owner)));
            crate::llvm::render_verified_program(&program).unwrap();
        }
    }
}
