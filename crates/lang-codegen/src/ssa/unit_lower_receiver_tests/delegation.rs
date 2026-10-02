use super::*;

#[test]
fn borrow_delegation_projects_one_heap_field_loan_and_forwards_it_directly() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(message: String): Int = 1 }\n\
         class Reader: Readable { override fun read(message: String): Int = 7 }\n\
         class Host(val tag: Int, val delegate: Reader): Readable by delegate {}\n\
         fun entry(): Int = Host(0, Reader()).read(\"argument\")",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("validated Borrow delegation must lower");

    let module = &program.modules[0];
    let entry = function(module.functions.iter(), ".entry.d");
    let reader = function(module.functions.iter(), ".Reader.read.s");
    let field_loans = entry
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::SharedHeapFieldLoan { base, field: 1 } => {
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    panic!("heap field loan result");
                };
                Some((base, *result))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(field_loans.len(), 1);
    let (outer, delegate) = field_loans[0];
    let (call, argument) = entry
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                callee,
                receiver: Some(EntityId::Loan(receiver)),
                arguments,
            } if *callee == reader.id() && *receiver == delegate => {
                let [EntityId::Loan(argument)] = arguments.as_slice() else {
                    panic!("one Borrow argument");
                };
                Some((instruction, *argument))
            }
            _ => None,
        })
        .expect("delegate implementation direct call");
    let field_loan_index = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::SharedHeapFieldLoan { .. })
        })
        .expect("field loan index");
    let call_index = entry
        .instructions
        .iter()
        .position(|instruction| instruction.id == call.id)
        .expect("call index");
    let argument_begin_index = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::BorrowBegin { .. })
                && instruction.results == [EntityId::Loan(argument)]
        })
        .expect("argument loan begin");
    let argument_end_index = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == argument)
        })
        .expect("argument loan end");
    let delegate_end_index = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == delegate)
        })
        .expect("delegate field loan end");
    let outer_end_index = entry
        .instructions
        .iter()
        .position(|instruction| {
            matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == outer)
        })
        .expect("temporary outer receiver loan end");
    assert!(
        field_loan_index < argument_begin_index
            && argument_begin_index < call_index
            && call_index < argument_end_index
            && argument_end_index < delegate_end_index
            && delegate_end_index < outer_end_index
    );
    assert_ne!(outer, delegate);
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::HeapFieldRead { .. }))
            .count(),
        0,
        "delegation must forward the field place, not read/copy the delegate owner"
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
            .count(),
        2,
        "only the source Reader and Host constructions may allocate"
    );
    assert!(entry.instructions.iter().all(|instruction| !matches!(
        instruction.operation,
        Operation::Copy { .. } | Operation::SharedAllocate { .. } | Operation::SharedRetain { .. }
    )));
    render_verified_program(&program).expect("delegation route must lower to LLVM");
}

#[test]
fn delegation_chain_projects_each_heap_field_loan_in_source_order() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "p/main.ko",
        "package p\n\
         interface Readable { fun read(message: String): Int }\n\
         class Reader: Readable { override fun read(message: String): Int = 7 }\n\
         class Middle(val tag: Int, val reader: Reader): Readable by reader {}\n\
         class Host(val tag: Int, val middle: Middle): Readable by middle {}\n\
         fun entry(): Int = Host(0, Middle(1, Reader())).read(\"argument\")",
    );
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &parsed)];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "p", "entry"),
    )
    .expect("same-requirement delegation chain must lower");

    let module = &program.modules[0];
    let entry = function(module.functions.iter(), ".entry.d");
    let reader = function(module.functions.iter(), ".Reader.read.s");
    let field_loans = entry
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::SharedHeapFieldLoan { base, field: 1 } => {
                let [EntityId::Loan(result)] = instruction.results.as_slice() else {
                    panic!("heap field loan result");
                };
                Some((instruction.id, base, *result))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [
        (outer_instruction, outer, middle),
        (inner_instruction, inner_base, reader_loan),
    ] = field_loans.as_slice()
    else {
        panic!("exactly two chained heap field loans: {field_loans:?}");
    };
    assert_eq!(
        middle, inner_base,
        "the second hop must derive from the first"
    );
    let (call, argument) = entry
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::DirectCall {
                callee,
                receiver: Some(EntityId::Loan(receiver)),
                arguments,
            } if *callee == reader.id() && receiver == reader_loan => {
                let [EntityId::Loan(argument)] = arguments.as_slice() else {
                    panic!("one Borrow argument");
                };
                Some((instruction.id, *argument))
            }
            _ => None,
        })
        .expect("chain endpoint direct call");
    let position = |id| {
        entry
            .instructions
            .iter()
            .position(|instruction| instruction.id == id)
            .expect("instruction position")
    };
    let end_position = |loan| {
        entry
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.operation, Operation::BorrowEnd { loan: ended } if ended == loan)
            })
            .expect("loan end")
    };
    assert!(
        position(*outer_instruction) < position(*inner_instruction)
            && position(*inner_instruction) < position(call)
            && position(call) < end_position(argument)
            && end_position(argument) < end_position(*reader_loan)
            && end_position(*reader_loan) < end_position(*middle)
            && end_position(*middle) < end_position(*outer)
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::HeapFieldRead { .. }))
            .count(),
        0,
        "each hop must forward a field place instead of reading its owner value"
    );
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
            .count(),
        3,
        "only Reader, Middle and Host source constructions may allocate"
    );
    assert!(entry.instructions.iter().all(|instruction| !matches!(
        instruction.operation,
        Operation::Copy { .. } | Operation::SharedAllocate { .. } | Operation::SharedRetain { .. }
    )));
    render_verified_program(&program).expect("delegation loan chain must lower to LLVM");
}
