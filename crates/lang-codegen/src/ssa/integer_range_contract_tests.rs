//! CI-only synthetic verifier negatives; never select in ordinary-source local manifests.
use super::{
    model::{EntityId, EntityType, Operation, Origin, Program, SsaTypeKind, TerminatorKind},
    verify::{VerifyErrorKind, verify_program},
};
use lang_frontend::source::SourceMap;

#[derive(Clone, Copy)]
enum Invalid {
    Unregistered,
    BadEmptyField,
    MissingField,
    UnsignedEndpoint,
    MixedConstructor,
    WrongMember,
    MixedEquality,
    NoResult,
    ExtraResult,
    PlaceResult,
}

fn fixture(case: Invalid) -> Program {
    let mut sources = SourceMap::new();
    let source = sources.add_source("range-contract.ko", "contract").unwrap();
    let origin = Origin::Source(sources.span(source, 0, 8).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("range-contract");
    let module = program.module_mut(module_id).unwrap();
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let long = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let unsigned = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: false,
    });
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let range = match case {
        Invalid::Unregistered => module
            .add_aggregate_type("lookalike", vec![int, int, boolean])
            .unwrap(),
        Invalid::BadEmptyField | Invalid::MissingField | Invalid::UnsignedEndpoint => {
            let endpoint = if matches!(case, Invalid::UnsignedEndpoint) {
                unsigned
            } else {
                int
            };
            let fields = match case {
                Invalid::BadEmptyField => vec![endpoint, endpoint, int],
                Invalid::MissingField => vec![endpoint, boolean],
                _ => vec![endpoint, endpoint, boolean],
            };
            let range = module
                .add_aggregate_type("invalid-registry", fields)
                .unwrap();
            module.integer_ranges.insert(range, endpoint);
            range
        }
        _ => module.add_integer_range_type(int).unwrap(),
    };
    let long_range = module.add_integer_range_type(long).unwrap();
    let function_id = module
        .add_function("contract", vec![], origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let block = function
        .add_block(
            vec![
                EntityType::Value(int),
                EntityType::Value(long),
                EntityType::Value(range),
                EntityType::Value(long_range),
            ],
            origin.clone(),
        )
        .unwrap();
    let values = function
        .block(block)
        .unwrap()
        .parameters
        .iter()
        .map(|entity| {
            let EntityId::Value(value) = entity else {
                panic!("value parameter");
            };
            *value
        })
        .collect::<Vec<_>>();
    let operation = match case {
        Invalid::WrongMember => Operation::IntegerRangeContains {
            value: values[1],
            range: values[2],
        },
        Invalid::MixedEquality => Operation::IntegerRangeEqual {
            left: values[2],
            right: values[3],
        },
        _ => Operation::IntegerRangeConstruct {
            first: values[0],
            bound: if matches!(case, Invalid::MixedConstructor) {
                values[1]
            } else {
                values[0]
            },
            inclusive: true,
        },
    };
    let results = match case {
        Invalid::WrongMember | Invalid::MixedEquality => vec![EntityType::Value(boolean)],
        Invalid::NoResult => vec![],
        Invalid::ExtraResult => vec![EntityType::Value(range), EntityType::Value(range)],
        Invalid::PlaceResult => vec![EntityType::Place(range)],
        _ => vec![EntityType::Value(range)],
    };
    function
        .append_instruction(block, operation, results, origin.clone())
        .unwrap();
    function
        .set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
        .unwrap();
    program
}

fn rejected(case: Invalid) {
    let errors = verify_program(&fixture(case)).expect_err("invalid numeric Range contract");
    assert!(
        errors
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. })),
        "{errors:?}"
    );
    if matches!(
        case,
        Invalid::BadEmptyField | Invalid::MissingField | Invalid::UnsignedEndpoint
    ) {
        assert!(
            errors
                .errors
                .iter()
                .any(|error| matches!(error.kind, VerifyErrorKind::InvalidTypeDefinition { .. })),
            "{errors:?}"
        );
    }
}

#[test]
fn rejects_unregistered_range_lookalike() {
    rejected(Invalid::Unregistered);
}
#[test]
fn rejects_range_registry_malformed_fields() {
    rejected(Invalid::BadEmptyField);
    rejected(Invalid::MissingField);
}
#[test]
fn rejects_range_registry_unsigned_endpoint() {
    rejected(Invalid::UnsignedEndpoint);
}
#[test]
fn rejects_mixed_constructor_endpoints() {
    rejected(Invalid::MixedConstructor);
}
#[test]
fn rejects_membership_wrong_endpoint() {
    rejected(Invalid::WrongMember);
}
#[test]
fn rejects_equality_different_range_identity() {
    rejected(Invalid::MixedEquality);
}
#[test]
fn rejects_range_result_count_and_entity_kind() {
    rejected(Invalid::NoResult);
    rejected(Invalid::ExtraResult);
    rejected(Invalid::PlaceResult);
}
