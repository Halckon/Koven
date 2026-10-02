use super::{
    model::{
        EntityId, EntityType, IntegerBitwiseOperator, Operation, Origin, Program, SsaTypeKind,
        TerminatorKind,
    },
    render::render_program,
    verify::{VerifyErrorKind, verify_program},
};
use lang_frontend::source::SourceMap;

const OPERATORS: [IntegerBitwiseOperator; 6] = [
    IntegerBitwiseOperator::And,
    IntegerBitwiseOperator::Or,
    IntegerBitwiseOperator::Xor,
    IntegerBitwiseOperator::Shl,
    IntegerBitwiseOperator::Shr,
    IntegerBitwiseOperator::Ushr,
];

fn fixture(
    left: SsaTypeKind,
    right: SsaTypeKind,
    results: &[SsaTypeKind],
    operator: Option<IntegerBitwiseOperator>,
) -> Program {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("bitwise.ko", "fun bitwise")
        .expect("source");
    let origin = Origin::Source(sources.span(source, 0, 11).expect("span"));
    let mut program = Program::default();
    let module_id = program.add_module("bitwise");
    let module = program.module_mut(module_id).expect("module");
    let left = module.intern_type(left);
    let right = module.intern_type(right);
    let results = results
        .iter()
        .map(|ty| EntityType::Value(module.intern_type(ty.clone())))
        .collect();
    let id = module
        .add_function("bitwise", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(id).expect("function");
    let entry = function
        .add_block(
            vec![EntityType::Value(left), EntityType::Value(right)],
            origin.clone(),
        )
        .expect("entry");
    let [EntityId::Value(left), EntityId::Value(right)] =
        function.block(entry).expect("entry").parameters[..]
    else {
        panic!("value parameters");
    };
    let operation = match operator {
        Some(operator) => Operation::IntegerBitwise {
            operator,
            left,
            right,
        },
        None => Operation::IntegerNot { operand: left },
    };
    function
        .append_instruction(entry, operation, results, origin.clone())
        .expect("instruction");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return");
    program
}

#[test]
fn bitwise_ssa_contract_accepts_exact_integer_operands_and_renders_all_operators() {
    for bits in [8, 16, 32, 64] {
        for signed in [false, true] {
            let ty = SsaTypeKind::Integer { bits, signed };
            for operator in OPERATORS.into_iter().map(Some).chain([None]) {
                let program = fixture(ty.clone(), ty.clone(), std::slice::from_ref(&ty), operator);
                verify_program(&program).expect("integer bitwise contract");
                let name = match operator {
                    Some(IntegerBitwiseOperator::And) => "and",
                    Some(IntegerBitwiseOperator::Or) => "or",
                    Some(IntegerBitwiseOperator::Xor) => "xor",
                    Some(IntegerBitwiseOperator::Shl) => "shl",
                    Some(IntegerBitwiseOperator::Shr) => "shr",
                    Some(IntegerBitwiseOperator::Ushr) => "ushr",
                    None => "inv",
                };
                assert!(render_program(&program).contains(&format!("bitwise.{name}")));
            }
        }
    }
}

#[test]
fn bitwise_ssa_contract_rejects_nonintegers_mixed_types_and_bad_results() {
    let integer = SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    };
    for operator in OPERATORS.into_iter().map(Some).chain([None]) {
        let mut invalid = vec![
            (
                SsaTypeKind::Integer {
                    bits: 7,
                    signed: true,
                },
                SsaTypeKind::Integer {
                    bits: 7,
                    signed: true,
                },
                vec![SsaTypeKind::Integer {
                    bits: 7,
                    signed: true,
                }],
            ),
            (
                SsaTypeKind::Integer {
                    bits: 128,
                    signed: false,
                },
                SsaTypeKind::Integer {
                    bits: 128,
                    signed: false,
                },
                vec![SsaTypeKind::Integer {
                    bits: 128,
                    signed: false,
                }],
            ),
            (
                SsaTypeKind::Boolean,
                SsaTypeKind::Boolean,
                vec![SsaTypeKind::Boolean],
            ),
            (
                SsaTypeKind::Char,
                SsaTypeKind::Char,
                vec![SsaTypeKind::Char],
            ),
            (integer.clone(), integer.clone(), vec![SsaTypeKind::Boolean]),
            (integer.clone(), integer.clone(), Vec::new()),
            (
                integer.clone(),
                integer.clone(),
                vec![integer.clone(), integer.clone()],
            ),
        ];
        for other in [
            SsaTypeKind::Integer {
                bits: 16,
                signed: true,
            },
            SsaTypeKind::Integer {
                bits: 32,
                signed: false,
            },
        ] {
            invalid.push((integer.clone(), integer.clone(), vec![other.clone()]));
            if operator.is_some() {
                invalid.push((integer.clone(), other, vec![integer.clone()]));
            }
        }
        for (left, right, results) in invalid {
            let errors = verify_program(&fixture(left, right, &results, operator))
                .expect_err("invalid bitwise contract");
            assert!(
                errors
                    .errors
                    .iter()
                    .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. })),
                "{errors:?}"
            );
        }
    }
}
