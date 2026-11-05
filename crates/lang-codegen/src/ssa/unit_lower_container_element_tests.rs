use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{EntityId, Function, Operation, PlaceAccess, TerminatorKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_cross_file_container_element_read_borrow_and_replacement_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         class Resource {}\n\
         fun make(): Resource = Resource()\n\
         fun inspect(item: Resource): Int = 1\n\
         fun takeInt(own item: Int): Int = item\n\
         fun forward(items: List<Resource>, index: Int): Int = inspect(items[index])",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Int {\n\
             val resources = listOf(p.make(), p.make())\n\
             val numbers = mutableListOf(1, 2)\n\
             val first = p.inspect(resources[0])\n\
             val second = p.forward(resources, 1)\n\
             val copied = p.takeInt(numbers[0])\n\
             val write = (numbers[0] = 3)\n\
             val compound = (numbers[1] += 4)\n\
             val array = arrayOf(5, 6)\n\
             val arrayWrite = (array[0] = 7)\n\
             val replaced = mutableListOf(p.make())\n\
             val moveWrite = (replaced[0] = p.make())\n\
             val after = p.inspect(replaced[0])\n\
             val temporary = p.inspect(listOf(p.make())[0])\n\
             val groupedTemporary = p.inspect((listOf(p.make()))[0])\n\
             return first + second + copied + numbers[0] + array[0] + after + temporary + groupedTemporary\n\
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
    .expect("container element core lowers to verified unit SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("input permutation preserves container element identities");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let forwarder = function(module, "p.forward");
    let EntityId::Loan(items) = forwarder.blocks[0].parameters[0] else {
        panic!("Borrow container parameter must be a function-scoped loan");
    };
    assert!(forwarder.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::ContainerElementPlace {
            owner: EntityId::Loan(owner),
            ..
        } if owner == items
    )));

    let entry = function(module, "q.entry");
    assert_eq!(operation_count(entry, is_element_place), 8);
    assert_eq!(operation_count(entry, is_replace), 4);
    assert_eq!(operation_count(entry, is_place_read), 4);
    assert_eq!(operation_count(entry, is_borrow_begin), 6);
    assert!(entry.instructions.windows(2).any(|instructions| matches!(
        (&instructions[0].operation, &instructions[1].operation),
        (
            Operation::DirectCall { .. },
            Operation::ContainerReplace { .. }
        )
    )));
    let (failure, success) = entry
        .blocks
        .iter()
        .find_map(|block| {
            let TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } = &block.terminator.as_ref()?.kind
            else {
                return None;
            };
            let success = entry.block(when_false.target)?;
            success
                .instructions
                .iter()
                .filter_map(|instruction| entry.instruction(*instruction))
                .any(|instruction| {
                    matches!(instruction.operation, Operation::ContainerReplace { .. })
                })
                .then_some((when_true, when_false))
        })
        .expect("compound replacement has one checked success edge");
    assert!(!failure.arguments.is_empty());
    assert_eq!(failure.arguments.len(), success.arguments.len());
    let success_block = entry.block(success.target).expect("success block exists");
    let carried_owner = success_block
        .parameters
        .iter()
        .find_map(|param| {
            let EntityId::Value(val) = param else { return None; };
            success_block
                .instructions
                .iter()
                .filter_map(|instruction| entry.instruction(*instruction))
                .find_map(|instruction| match instruction.operation {
                    Operation::ContainerReplace { owner, .. } if owner == *val => Some(*val),
                    _ => None,
                })
        })
        .expect("checked success carries the live MoveOnly container owner");
    assert!(
        success_block
            .instructions
            .iter()
            .filter_map(|instruction| entry.instruction(*instruction))
            .any(|instruction| matches!(
                instruction.operation,
                Operation::ContainerReplace { owner, .. } if owner == carried_owner
            ))
    );
}

#[test]
fn field_backed_elements_and_temporary_compound_remain_atomic_boundaries() {
    for (path, source) in [
        (
            "test/field.ko",
            "package test\n\
             class Resource {}\n\
             class Holder(val items: List<Resource>)\n\
             fun inspect(item: Resource): Int = 1\n\
             fun entry(own holder: Holder): Int = inspect(holder.items[0])",
        ),
        (
            "test/temporary-compound.ko",
            "package test\n\
             fun entry(): Unit {\n\
                 val result = (mutableListOf(1)[0] += 2)\n\
             }",
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
            Ok(_) => panic!("{path} remains outside the root container element core"),
            Err(error) => error,
        };
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{path}");
    }
}

#[test]
fn named_nested_element_borrow_passes_p3_but_keeps_the_p4_receiver_boundary() {
    use lang_frontend::{
        ownership_checking::{ElementIndexIdentity, LoanKind, UnitLoanTarget},
        type_checking::complete_compilation_unit_types,
    };

    let mut sources = SourceMap::new();
    let text = "package test\n\
                fun inspect(n: Int): Int = 1\n\
                fun entry(own xs: List<List<Int>>): Int = inspect(xs[0][1])";
    let (source, parsed) = parsed(&mut sources, "test/nested.ko", text);
    let inputs = [SourceUnitInput::new(
        "root",
        "test/nested.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    complete_compilation_unit_types(&sources, &inputs, &names, &type_environment, typed.types())
        .expect("legal nested Borrow also passes the final type gate");
    let [loan] = owned.ownership().loans() else {
        panic!("actual P3 publishes exactly the nested Borrow");
    };
    assert_eq!(loan.kind(), LoanKind::Shared);
    let UnitLoanTarget::Place(place) = loan.target() else {
        panic!("named container keeps its place");
    };
    assert_eq!(place.root().source_unit(), loan.argument().source_unit());
    assert_eq!(
        place.elements(),
        &[
            ElementIndexIdentity::Known(0),
            ElementIndexIdentity::Known(1)
        ]
    );
    let error = match lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    ) {
        Ok(_) => panic!("P3 path support does not grant a nested P4 container owner"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    let receiver = text.find("xs[0]").unwrap();
    assert_eq!(
        error.span,
        Some(
            sources
                .span(source, receiver, receiver + "xs[0]".len())
                .unwrap()
        )
    );
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

fn is_element_place(operation: &Operation) -> bool {
    matches!(operation, Operation::ContainerElementPlace { .. })
}

fn is_replace(operation: &Operation) -> bool {
    matches!(operation, Operation::ContainerReplace { .. })
}

fn is_place_read(operation: &Operation) -> bool {
    matches!(
        operation,
        Operation::Read {
            source: PlaceAccess::Place(_)
        }
    )
}

fn is_borrow_begin(operation: &Operation) -> bool {
    matches!(operation, Operation::BorrowBegin { .. })
}
