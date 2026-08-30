use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{EntityId, Function, Operation, PlaceAccess},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_cross_file_shared_borrows_and_loan_forwarding_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun make(): Resource = Resource()\n\
         fun inspect(resource: Resource): Int = 1\n\
         fun forward(resource: Resource): Int = inspect(resource)\n\
         fun pair(left: Resource, right: Resource, own marker: Int): Int = marker\n\
         fun read(number: Int): Int = number",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Int {\n\
             val owner = p.make()\n\
             val first = p.inspect(borrow owner)\n\
             val second = p.forward(owner)\n\
             val temporary = p.inspect(p.make())\n\
             val paired = p.pair(right = p.make(), marker = 2, left = owner)\n\
             val copied = p.read(40)\n\
             return first + second + temporary + paired + copied\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (reverse_names, reverse_typed, reverse_owned) =
        analyze(&sources, &reversed, &name_environment, &type_environment);
    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "q", "entry"),
    )
    .expect("shared Borrow parameters lower to verified unit SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("input permutation preserves shared-loan identities");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let inspect = function(module, "p.inspect");
    assert!(matches!(inspect.blocks[0].parameters[0], EntityId::Loan(_)));

    let forwarder = function(module, "p.forward");
    let EntityId::Loan(forwarded) = forwarder.blocks[0].parameters[0] else {
        panic!("Borrow parameter must use a function-scoped loan");
    };
    assert!(forwarder.instructions.iter().any(|instruction| matches!(
        &instruction.operation,
        Operation::DirectCall { arguments, .. }
            if arguments.as_slice() == [EntityId::Loan(forwarded)]
    )));
    assert_eq!(operation_count(forwarder, is_borrow_begin), 0);
    assert_eq!(operation_count(forwarder, is_borrow_end), 0);

    let read = function(module, "p.read");
    let EntityId::Loan(number) = read.blocks[0].parameters[0] else {
        panic!("Copyable Borrow parameter must use a loan");
    };
    assert!(read.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::Read {
            source: PlaceAccess::Loan(loan)
        } if loan == number
    )));

    let pair = function(module, "p.pair");
    assert!(matches!(pair.blocks[0].parameters[0], EntityId::Loan(_)));
    assert!(matches!(pair.blocks[0].parameters[1], EntityId::Loan(_)));
    assert!(matches!(pair.blocks[0].parameters[2], EntityId::Value(_)));

    let entry = function(module, "q.entry");
    let (pair_call_index, left_loan, right_loan) = entry
        .instructions
        .iter()
        .enumerate()
        .find_map(|(index, instruction)| match &instruction.operation {
            Operation::DirectCall {
                callee, arguments, ..
            } if *callee == pair.id
                && matches!(
                    arguments.as_slice(),
                    [EntityId::Loan(_), EntityId::Loan(_), EntityId::Value(_)]
                ) =>
            {
                let [
                    EntityId::Loan(left),
                    EntityId::Loan(right),
                    EntityId::Value(_),
                ] = arguments.as_slice()
                else {
                    unreachable!("guard fixes the direct-call argument shape")
                };
                Some((index, *left, *right))
            }
            _ => None,
        })
        .expect("mixed Borrow/Value call uses parameter-order slots");
    assert!(matches!(
        entry.instructions[pair_call_index + 1].operation,
        Operation::BorrowEnd { loan } if loan == left_loan
    ));
    assert!(matches!(
        entry.instructions[pair_call_index + 2].operation,
        Operation::BorrowEnd { loan } if loan == right_loan
    ));

    assert_eq!(operation_count(entry, is_borrow_begin), 6);
    assert_eq!(operation_count(entry, is_borrow_end), 6);
    assert_eq!(operation_count(entry, is_root_place), 6);
    assert_eq!(operation_count(entry, is_drop), 3);
}

#[test]
fn inout_and_non_root_borrows_remain_atomic_boundaries() {
    for (path, source) in [
        (
            "test/inout.ko",
            "package test\n\
             class Resource {}\n\
             fun mutate(inout resource: Resource): Unit {}\n\
             fun entry(): Unit {\n\
                 var owner = Resource()\n\
                 val result = mutate(&owner)\n\
             }",
        ),
        (
            "test/projection.ko",
            "package test\n\
             class Resource {}\n\
             value class Holder(val resource: Resource)\n\
             fun inspect(resource: Resource): Unit {}\n\
             fun entry(own holder: Holder): Unit {\n\
                 val result = inspect(holder.resource)\n\
             }",
        ),
        (
            "test/unit-entry.ko",
            "package test\n\
             fun entry(item: Unit): Int = 0",
        ),
        (
            "test/unit-call.ko",
            "package test\n\
             fun produce(): Unit {}\n\
             fun ignore(item: Unit): Int = 0\n\
             fun entry(): Int = ignore(produce())",
        ),
    ] {
        let mut sources = SourceMap::new();
        let (source_id, parsed) = parsed(&mut sources, path, source);
        let inputs = [SourceUnitInput::new("root", path, source_id, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let error = match lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        ) {
            Ok(_) => panic!("{path} remains outside the shared-root Borrow core"),
            Err(error) => error,
        };
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{path}");
    }
}

fn function<'a>(module: &'a super::model::Module, name: &str) -> &'a Function {
    module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .unwrap_or_else(|| panic!("reachable function {name} exists"))
}

fn operation_count(function: &Function, predicate: fn(&Operation) -> bool) -> usize {
    function
        .instructions
        .iter()
        .filter(|instruction| predicate(&instruction.operation))
        .count()
}

fn is_borrow_begin(operation: &Operation) -> bool {
    matches!(operation, Operation::BorrowBegin { .. })
}

fn is_borrow_end(operation: &Operation) -> bool {
    matches!(operation, Operation::BorrowEnd { .. })
}

fn is_root_place(operation: &Operation) -> bool {
    matches!(operation, Operation::RootPlace { .. })
}

fn is_drop(operation: &Operation) -> bool {
    matches!(operation, Operation::Drop { .. })
}
