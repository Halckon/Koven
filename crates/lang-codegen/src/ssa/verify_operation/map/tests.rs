//! 普通静态 verifier 正负例；不生成或执行故障程序。
use super::*;
use crate::ssa::model::{Origin, Program, SequentialContainerKind};

fn origin() -> Origin {
    let mut sources = lang_frontend::source::SourceMap::new();
    let source = sources.add_source("map.ko", "map").unwrap();
    Origin::Source(sources.span(source, 0, 3).unwrap())
}

#[test]
fn map_owned_result_requires_conditional_consuming_extraction() {
    // Local contracts only: no malformed LLVM, fault artifact, or injected execution.
    let mut program = Program::default();
    let module = program.add_module("test");
    let module = program.module_mut(module).unwrap();
    let string = module.add_string_owner_type();
    let result = module.add_map_result_type(string).unwrap();
    assert_eq!(module.nullable_inner(result), Some(string));
    assert_eq!(module.type_ownership(result), Some(Ownership::MoveOnly));
    assert!(module.add_nullable_handle_type(string).is_err());
    let id = module.add_function("read", vec![], origin()).unwrap();
    let function = module.function_mut(id).unwrap();
    let block = function
        .add_block(vec![EntityType::Value(result)], origin())
        .unwrap();
    let EntityId::Value(owner) = function.block(block).unwrap().parameters[0] else {
        panic!("owner")
    };
    let function = module.function(id).unwrap();
    assert!(!verify_map_operation(
        module,
        function,
        &Operation::MapResultUnwrap { result: owner },
        &[EntityType::Value(string)]
    ));
    let boolean = module.aggregate_fields(result).unwrap()[0];
    assert!(!super::super::aggregate_explode_contract(
        module,
        function,
        owner,
        &[EntityType::Value(boolean), EntityType::Value(string)]
    ));
}

#[test]
fn map_get_rejects_an_owned_copy_of_a_move_only_value() {
    let mut program = Program::default();
    let module = program.add_module("test");
    let module = program.module_mut(module).unwrap();
    let string = module.add_string_owner_type();
    let map = module
        .add_map_container_type(MapContainerKind::Map, string, string)
        .unwrap();
    let id = module.add_function("read", vec![], origin()).unwrap();
    let function = module.function_mut(id).unwrap();
    let block = function
        .add_block(
            vec![EntityType::Value(map), EntityType::Value(string)],
            origin(),
        )
        .unwrap();
    let operands = function.block(block).unwrap().parameters.clone();
    let operation = Operation::MapGet {
        owner: operands[0],
        key: operands[1],
    };
    assert!(!verify_map_operation(
        module,
        module.function(id).unwrap(),
        &operation,
        &[EntityType::Value(string)]
    ));
}

#[test]
fn map_queries_accept_a_shared_string_key_and_reject_exclusive_key() {
    for kind in [LoanKind::Shared, LoanKind::Exclusive] {
        let mut program = Program::default();
        let module = program.add_module("test");
        let module = program.module_mut(module).unwrap();
        let string = module.add_string_owner_type();
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let map = module
            .add_map_container_type(MapContainerKind::Map, string, boolean)
            .unwrap();
        let id = module.add_function("read", vec![], origin()).unwrap();
        let function = module.function_mut(id).unwrap();
        let block = function
            .add_block(
                vec![
                    EntityType::Value(map),
                    EntityType::Loan {
                        kind,
                        target: string,
                    },
                ],
                origin(),
            )
            .unwrap();
        let operands = function.block(block).unwrap().parameters.clone();
        let operation = Operation::MapContains {
            owner: operands[0],
            key: operands[1],
        };
        assert_eq!(
            verify_map_operation(
                module,
                module.function(id).unwrap(),
                &operation,
                &[EntityType::Value(boolean)]
            ),
            kind == LoanKind::Shared
        );
    }
}

#[test]
fn map_type_verifier_rejects_a_non_hashable_key() {
    let mut program = Program::default();
    let module = program.add_module("test");
    let module = program.module_mut(module).unwrap();
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, integer)
        .unwrap();
    module
        .add_map_container_type(MapContainerKind::Map, list, integer)
        .unwrap();
    assert!(crate::ssa::verify::verify_program(&program).is_err());
}

#[test]
fn map_get_requires_registered_result_identity_instead_of_bare_payload() {
    let mut program = Program::default();
    let module = program.add_module("test");
    let module = program.module_mut(module).unwrap();
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let map = module
        .add_map_container_type(MapContainerKind::Map, integer, integer)
        .unwrap();
    let result = module.add_map_result_type(integer).unwrap();
    assert_eq!(module.add_map_result_type(integer).unwrap(), result);
    let lookalike = module
        .add_aggregate_type("ordinary", vec![boolean, integer])
        .unwrap();
    let id = module.add_function("read", vec![], origin()).unwrap();
    let function = module.function_mut(id).unwrap();
    let block = function
        .add_block(
            vec![EntityType::Value(map), EntityType::Value(integer)],
            origin(),
        )
        .unwrap();
    let operands = function.block(block).unwrap().parameters.clone();
    let operation = Operation::MapGet {
        owner: operands[0],
        key: operands[1],
    };
    for (ty, expected) in [(integer, false), (lookalike, false), (result, true)] {
        assert_eq!(
            verify_map_operation(
                module,
                module.function(id).unwrap(),
                &operation,
                &[EntityType::Value(ty)]
            ),
            expected
        );
    }
}

#[test]
fn map_result_registry_checks_presence_and_preserves_payload_ownership() {
    for move_only in [false, true] {
        let mut program = Program::default();
        let module = program.add_module("test");
        let module = program.module_mut(module).unwrap();
        let integer = module.intern_type(SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        });
        let boolean = module.intern_type(SsaTypeKind::Boolean);
        let payload = if move_only {
            module.add_string_owner_type()
        } else {
            integer
        };
        let presence = if move_only { boolean } else { integer };
        let result = module
            .add_aggregate_type("invalid query identity", vec![presence, payload])
            .unwrap();
        module.map_results.insert(result, payload);
        if move_only {
            assert_eq!(module.type_ownership(result), Some(Ownership::MoveOnly));
            crate::ssa::verify::verify_program(&program).unwrap();
            continue;
        }
        let errors = crate::ssa::verify::verify_program(&program).unwrap_err();
        assert!(
            format!("{errors:?}")
                .contains("Map result requires Boolean presence and matching payload ownership")
        );
    }
}
