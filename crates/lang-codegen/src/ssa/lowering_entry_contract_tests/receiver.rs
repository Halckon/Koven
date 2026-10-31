//! Original receiver source, Unit-only resource oracle. Keep the legacy witness.
use super::*;
use crate::ssa::model::{EntityType, Instruction, LoanKind};
use lang_frontend::name_resolution::SymbolKind;

const SOURCE: &str = "package pair\nclass Reader { fun read(text: String): Unit { val printed = println(text) } }\nfun entry(): Unit { val reader = Reader()\nval ignored = reader.read(\"receiver\0\") }";

fn producer(function: &Function, entity: EntityId) -> (usize, &Instruction) {
    let producers = function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| instruction.results.contains(&entity))
        .collect::<Vec<_>>();
    assert_eq!(producers.len(), 1, "one producer for {entity:?}");
    producers[0]
}

fn assert_receiver_contract(sources: &SourceMap, program: &Program, entry: FunctionId) {
    crate::ssa::verify::verify_program(program).unwrap();
    let module = program.module(entry.module()).unwrap();
    let function = module.function(entry).unwrap();
    assert_eq!(
        function.blocks.len(),
        1,
        "original source has no control split"
    );
    let calls = function
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| {
            matches!(
                i.operation,
                Operation::DirectCall {
                    receiver: Some(_),
                    ..
                }
            )
        })
        .collect::<Vec<_>>();
    let [(call_index, call)] = calls.as_slice() else {
        panic!("one instance call")
    };
    let Operation::DirectCall {
        callee,
        receiver: Some(receiver),
        arguments,
    } = &call.operation
    else {
        unreachable!()
    };
    assert_eq!(
        sources.slice(call.origin.span()),
        Ok("reader.read(\"receiver\0\")")
    );
    let source = call.origin.span().source_id();
    assert_eq!(sources.source_name(source), Ok("pair/entry.ko"));
    assert_eq!(sources.source_text(source), Ok(SOURCE));
    assert!(call.results.is_empty(), "Unit result has no owner");
    assert_eq!(arguments.len(), 1);
    assert_ne!(
        *receiver, arguments[0],
        "receiver and text have distinct loans"
    );

    let mut owners = Vec::new();
    let mut loans = Vec::new();
    for (operand, is_text) in [(*receiver, false), (arguments[0], true)] {
        let EntityId::Loan(loan) = operand else {
            panic!("Borrow ABI operand")
        };
        let (begin_index, begin) = producer(function, operand);
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            panic!("original owned roots use shared loans")
        };
        let (place_index, place_instruction) = producer(function, EntityId::Place(place));
        let Operation::RootPlace { owner } = place_instruction.operation else {
            panic!("loan targets its exact owner root")
        };
        let (owner_index, creation) = producer(function, EntityId::Value(owner));
        if is_text {
            let Operation::StringLiteral { string, bytes } = &creation.operation else {
                panic!("text owner")
            };
            assert_eq!(bytes.as_slice(), b"receiver\0");
            assert_eq!(sources.slice(creation.origin.span()), Ok("\"receiver\0\""));
            let EntityType::Value(ty) = function.entity(EntityId::Value(owner)).unwrap().ty else {
                panic!("value owner")
            };
            assert_eq!(ty, *string);
            assert_eq!(module.types[ty.index()], SsaTypeKind::StringOwner);
        } else {
            assert!(matches!(creation.operation, Operation::HeapAllocate { .. }));
            assert_eq!(sources.slice(creation.origin.span()), Ok("Reader()"));
        }
        let ends = function.instructions.iter().enumerate().filter(|(_, i)|
            matches!(i.operation, Operation::BorrowEnd { loan: actual } if actual == loan))
            .collect::<Vec<_>>();
        let [(end_index, _)] = ends.as_slice() else {
            panic!("exact loan ends once")
        };
        let drops = function.instructions.iter().enumerate().filter(|(_, i)|
            matches!(i.operation, Operation::Drop { owner: actual } if actual == owner))
            .collect::<Vec<_>>();
        let [(drop_index, _)] = drops.as_slice() else {
            panic!("exact owner drops once")
        };
        assert!(owner_index < place_index && place_index < begin_index);
        assert!(begin_index < *call_index && *call_index < *end_index && *end_index < *drop_index);
        owners.push(owner);
        loans.push(loan);
    }
    assert_ne!(owners[0], owners[1]);
    let trace = function
        .instructions
        .iter()
        .filter_map(|i| match i.operation {
            Operation::HeapAllocate { .. } => Some("receiver-owner"),
            Operation::StringLiteral { .. } => Some("text-owner"),
            Operation::BorrowBegin { .. } if i.results == [EntityId::Loan(loans[0])] => {
                Some("receiver-begin")
            }
            Operation::BorrowBegin { .. } if i.results == [EntityId::Loan(loans[1])] => {
                Some("text-begin")
            }
            Operation::DirectCall { .. } => Some("call"),
            Operation::BorrowEnd { loan } if loan == loans[0] => Some("receiver-end"),
            Operation::BorrowEnd { loan } if loan == loans[1] => Some("text-end"),
            Operation::Drop { owner } if owner == owners[0] => Some("receiver-drop"),
            Operation::Drop { owner } if owner == owners[1] => Some("text-drop"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        trace,
        [
            "receiver-owner",
            "receiver-begin",
            "text-owner",
            "text-begin",
            "call",
            "text-end",
            "receiver-end",
            "text-drop",
            "receiver-drop"
        ]
    );
    // This frozen source has no other operations or hidden resource-producing calls.
    assert!(function.instructions.iter().all(|i| matches!(
        i.operation,
        Operation::AggregateConstruct { .. }
            | Operation::HeapAllocate { .. }
            | Operation::RootPlace { .. }
            | Operation::BorrowBegin { .. }
            | Operation::StringLiteral { .. }
            | Operation::DirectCall { .. }
            | Operation::BorrowEnd { .. }
            | Operation::Drop { .. }
    )));
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::RootPlace { .. }))
            .count(),
        2
    );
    // Exact inventories, not a positive .any() that permits extra resource events.
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::BorrowBegin { .. }))
            .count(),
        2
    );
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
            .count(),
        2
    );
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::Drop { .. }))
            .count(),
        2
    );
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::StringLiteral { .. }))
            .count(),
        1
    );
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::HeapAllocate { .. }))
            .count(),
        1
    );
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
            .count(),
        1
    );
    assert!(function.instructions.iter().all(|i| !matches!(
        i.operation,
        Operation::PrintString { .. } | Operation::StringConcat { .. } | Operation::Consume { .. }
    )));
    assert!(
        matches!(&function.blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.is_empty())
    );

    // The actual call target, not any function with a matching printed string.
    let target = module.function(*callee).unwrap();
    assert_eq!(target.blocks.len(), 1);
    assert_eq!(target.origin.span().source_id(), source);
    assert_eq!(
        sources.slice(target.origin.span()),
        Ok("fun read(text: String): Unit { val printed = println(text) }")
    );
    let print = target
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::PrintString { .. }))
        .collect::<Vec<_>>();
    assert_eq!(print.len(), 1);
    assert_eq!(
        target.instructions.len(),
        1,
        "callee only prints its borrowed parameter"
    );
    assert_eq!(print[0].origin.span().source_id(), source);
    assert_eq!(sources.slice(print[0].origin.span()), Ok("println(text)"));
    let parameters = &target.blocks[0].parameters;
    assert_eq!(
        parameters.len(),
        2,
        "hidden receiver plus Borrow String argument"
    );
    let EntityId::Loan(text_parameter) = parameters[1] else {
        panic!("Borrow parameter")
    };
    assert_eq!(
        target
            .instructions
            .iter()
            .filter_map(|i| match i.operation {
                Operation::PrintString { value } => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [text_parameter]
    );
    assert!(target.instructions.iter().all(|i| !matches!(
        i.operation,
        Operation::Drop { .. }
            | Operation::BorrowEnd { .. }
            | Operation::StringLiteral { .. }
            | Operation::DirectCall { .. }
            | Operation::Consume { .. }
    )));
    assert!(
        matches!(&target.blocks[0].terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.is_empty())
    );
}

#[test]
fn receiver_original_source_preserves_exact_unit_owners_and_call_loans() {
    for entry_first in [false, true] {
        paired_in_order_observed(SOURCE, false, entry_first, assert_receiver_contract);
    }
    let unit = crate::native_tests::boxed_enum_tests::unit_source::UnitSource::analyze(
        "pair/entry.ko",
        SOURCE,
    )
    .expect("original MemoryDocument frontend admission");
    unit.assert_frontend_clean();
    let declaration = unit.declaration("entry", SymbolKind::Function);
    let (program, entry) = unit.lower_entry(declaration).unwrap();
    assert_receiver_contract(unit.sources(), &program, entry);
}
