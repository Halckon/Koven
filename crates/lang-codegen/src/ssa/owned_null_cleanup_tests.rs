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

#[test]
fn temporary_null_cleanup_presence_precedes_unique_result_drop() {
    for (payload, value) in [
        ("String", "\"text\".clone()"),
        ("Text", "Text(\"text\".clone())"),
        ("Resource", "Resource()"),
    ] {
        for condition in ["m.remove(1) != null", "null == ((m.remove(1)))"] {
            let source = format!(
                "value class Text(val text: String)\nclass Resource {{ deinit() {{ println(\"drop\") }} }}\nfun entry() {{ var m = mutableMapOf<Int, {payload}>(); m.put(1, {value}); val present = {condition}; if (present) {{ println(\"present\") }} }}"
            );
            for (label, program) in [("single", single(&source)), ("unit", unit(&source))] {
                let program =
                    program.unwrap_or_else(|error| panic!("{label}/{payload}: {error:?}"));
                super::verify::verify_program(&program).unwrap();
                let function = program.modules[0]
                    .functions
                    .iter()
                    .find(|function| function.name.contains("entry"))
                    .unwrap();
                let removed = function
                    .instructions
                    .iter()
                    .find(|instruction| {
                        matches!(instruction.operation, Operation::MapRemove { .. })
                    })
                    .unwrap();
                let [EntityId::Value(_), EntityId::Value(owner)] = removed.results.as_slice()
                else {
                    panic!("updated map and owned nullable result")
                };
                let reads = function
                    .instructions
                    .iter()
                    .enumerate()
                    .filter_map(|(index, instruction)| match instruction.operation {
                        Operation::AggregateProject {
                            aggregate,
                            field: 0,
                        } if aggregate == *owner => Some(index),
                        Operation::NullableIsNull { owner: nullable } if nullable == *owner => {
                            Some(index)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let drops = function.instructions.iter().enumerate().filter_map(|(index, instruction)| matches!(instruction.operation, Operation::Drop { owner: dropped } if dropped == *owner).then_some(index)).collect::<Vec<_>>();
                assert_eq!(reads.len(), 1, "{label}/{payload}/{condition}");
                assert_eq!(drops.len(), 1, "{label}/{payload}/{condition}");
                assert!(reads[0] < drops[0]);
                assert!(!function.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::Copy { source } if source == *owner)));
                crate::llvm::render_verified_program(&program).unwrap();
            }
        }
    }
}

#[test]
fn single_nullable_factory_planning_boundary_precedes_any_comparison() {
    let source = "class Resource(val text: String) { deinit() { println(this.text) } }\nfun make(): Resource? = Resource(\"plain nullable drop\")\nfun entry() { val value = make() }";
    let error = single(source)
        .err()
        .expect("single arbitrary nullable factory remains unsupported");
    assert_eq!(error.kind, super::LoweringErrorKind::UnsupportedNode);
    let origin = error.span.unwrap();
    assert_eq!(
        &source[origin.start()..origin.end()],
        "fun make(): Resource? = Resource(\"plain nullable drop\")"
    );
    let program = unit(source).unwrap();
    super::verify::verify_program(&program).unwrap();
}
