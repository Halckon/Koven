//! Exact original control/member sources; compare ordinary Unit before retiring Single.
use super::analyze_single_root;
use crate::{
    llvm::render_verified_program,
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::{
        LoweringErrorKind,
        model::{
            ComparisonOperator, Edge, EntityId, Function, Operation, PlaceAccess, ScalarConstant,
            TerminatorKind, ValueId,
        },
        verify::verify_program,
    },
};
use lang_frontend::{name_resolution::SymbolKind, type_checking::UnitCallTarget};

#[test]
fn original_numeric_when_matches_one_and_returns_the_corresponding_arm() {
    let text = "fun numeric(input: Int): Int = when (input) {\n\
             1 -> 2\n\
             else -> 3\n\
         }";
    let (unit, entry) = analyze_single_root(text, "numeric");
    let (program, entry) = unit
        .lower_entry(entry)
        .expect("ordinary numeric when lowers");
    verify_program(&program).unwrap();
    render_verified_program(&program).unwrap();
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    let [EntityId::Loan(input)] = function.blocks[0].parameters.as_slice() else {
        panic!("the original Int parameter is borrowed");
    };
    let [branch] = function
        .blocks
        .iter()
        .filter_map(|block| {
            let term = block.terminator.as_ref().unwrap();
            matches!(term.kind, TerminatorKind::Conditional { .. }).then_some(term)
        })
        .collect::<Vec<_>>()[..]
    else {
        panic!("one numeric match");
    };
    let TerminatorKind::Conditional {
        condition,
        ref when_true,
        ref when_false,
    } = branch.kind
    else {
        unreachable!();
    };
    let Operation::Compare {
        operator: ComparisonOperator::Equal,
        left,
        right,
    } = definition(function, condition)
    else {
        panic!("match dispatch uses actual equality");
    };
    assert!(
        (reads(function, *left, *input) && literal(function, *right, 1))
            || (reads(function, *right, *input) && literal(function, *left, 1))
    );
    assert!(literal(function, returned_on_edge(function, when_true), 2));
    assert!(literal(function, returned_on_edge(function, when_false), 3));
}

fn definition(function: &Function, value: ValueId) -> &Operation {
    &function
        .instructions
        .iter()
        .find(|instruction| instruction.results.contains(&EntityId::Value(value)))
        .expect("value has an instruction definition")
        .operation
}

fn reads(function: &Function, value: ValueId, parameter: crate::ssa::model::LoanId) -> bool {
    matches!(definition(function, value), Operation::Read { source: PlaceAccess::Loan(loan) } if *loan == parameter)
}

fn literal(function: &Function, value: ValueId, expected: i128) -> bool {
    matches!(definition(function, value), Operation::Constant(ScalarConstant::Integer(integer)) if *integer == expected)
}

fn returned_on_edge<'a>(function: &'a Function, mut edge: &'a Edge) -> ValueId {
    let mut forwarded = std::collections::BTreeMap::<ValueId, ValueId>::new();
    let resolve = |forwarded: &std::collections::BTreeMap<ValueId, ValueId>, value| {
        forwarded.get(&value).copied().unwrap_or(value)
    };
    for _ in 0..function.blocks.len() {
        let block = function.block(edge.target).unwrap();
        assert_eq!(block.parameters.len(), edge.arguments.len());
        let next: Vec<_> = block
            .parameters
            .iter()
            .zip(&edge.arguments)
            .filter_map(|(parameter, argument)| match (parameter, argument) {
                (EntityId::Value(parameter), EntityId::Value(argument)) => {
                    Some((*parameter, resolve(&forwarded, *argument)))
                }
                _ => None,
            })
            .collect();
        forwarded.extend(next);
        match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(next) => edge = next,
            TerminatorKind::Return { values } => {
                let [value] = values.as_slice() else {
                    panic!("one Int result");
                };
                return resolve(&forwarded, *value);
            }
            other => panic!("expected straight path from selected arm: {other:?}"),
        }
    }
    panic!("numeric match arm unexpectedly cycles");
}

#[test]
fn original_elvis_stops_at_the_exact_ordinary_unit_expression() {
    let text = "fun choose(input: Int?): Int = input ?: 0";
    let (unit, entry) = analyze_single_root(text, "choose");
    let error = unit
        .lower_entry(entry)
        .err()
        .expect("Elvis exposes no partially lowered Unit program");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    let origin = error.span.expect("Unit boundary has a source span");
    assert_eq!(origin.source_id(), unit.parsed().source_id());
    assert_eq!(unit.sources().slice(origin), Ok("input ?: 0"));
}

// Temporary characterization: the frontend contract is checked in both cases;
// the real Unit backend outcome must be observed before choosing the final oracle.
#[test]
fn original_ordinary_and_inv_are_source_members_not_integer_intrinsics() {
    for method in ["ordinary", "inv"] {
        let text = format!(
            "class Sample {{ fun {method}(): Int = 7 }}\nfun entry(): Int = Sample().{method}()"
        );
        let unit = UnitSource::analyze("lowering.ko", &text).expect("strict Unit admission");
        unit.assert_frontend_clean();
        assert!(unit.types().integer_operations().is_empty());
        let [call] = unit.types().calls() else {
            panic!("one actual member-call descriptor for {method}");
        };
        let UnitCallTarget::Symbol(callee) = call.target() else {
            panic!("source-local selected member, never an intrinsic");
        };
        assert!(call.receiver().is_some());
        let names = &unit.names().names().source_units()[callee.source_unit().index()];
        let declaration = names
            .resolution()
            .symbols()
            .get(callee.symbol().index())
            .unwrap();
        assert_eq!(declaration.name(), method);
        let entry = unit.declaration("entry", SymbolKind::Function);
        match unit.lower_entry(entry) {
            Err(error) => {
                assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
                let origin = error.span.expect("real backend rejection location");
                assert_eq!(origin.source_id(), unit.parsed().source_id());
                eprintln!(
                    "{method}: Unit rejected at {:?}",
                    unit.sources().slice(origin)
                );
            }
            Ok((program, entry)) => {
                verify_program(&program).unwrap();
                render_verified_program(&program).unwrap();
                let module = program.module(entry.module()).unwrap();
                let function = module.function(entry).unwrap();
                assert!(!function.instructions.iter().any(|instruction| {
                    matches!(
                        instruction.operation,
                        Operation::IntegerNot { .. } | Operation::IntegerBitwise { .. }
                    )
                }));
                let calls = function
                    .instructions
                    .iter()
                    .filter_map(|instruction| match instruction.operation {
                        Operation::DirectCall {
                            callee,
                            receiver: Some(_),
                            ..
                        } => Some(callee),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [target] = calls.as_slice() else {
                    panic!("one actual method call");
                };
                let member = module.function(*target).unwrap();
                assert!(
                    unit.sources()
                        .slice(member.origin.span())
                        .unwrap()
                        .contains(&format!("fun {method}()"))
                );
                eprintln!(
                    "{method}: Unit calls the selected source member {}",
                    member.name
                );
            }
        }
    }
}
