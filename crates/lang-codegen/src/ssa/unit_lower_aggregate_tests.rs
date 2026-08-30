use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{EntityId, Operation, SsaTypeKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_cross_file_value_class_class_and_box_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         value class Coordinates(val x: Int, val y: Int)\n\
         value class Token(val item: Int)\n\
         class Bundle(val text: String, val count: Int)\n\
         fun makeCoordinates(): Coordinates = Coordinates(20, 22)\n\
         fun sum(own coordinates: Coordinates): Int = coordinates.x + coordinates.y\n\
         fun makeBundle(): Bundle = Bundle(count = 7, text = \"owned\")\n\
         fun count(own bundle: Bundle): Int = bundle.count\n\
         fun boxed(): Box<Token> = Box(Token(9))\n\
         fun inspectBox(own resource: Box<Token>): Int = 1",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Int = p.sum(p.makeCoordinates()) + p.count(p.makeBundle()) + p.inspectBox(p.boxed())",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let reversed = [inputs[1], inputs[0]];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let entry = declaration(&names, "q", "entry");
    let (backward_names, backward_typed, backward_owned) =
        analyze(&sources, &reversed, &name_environment, &type_environment);
    let backward_entry = declaration(&backward_names, "q", "entry");
    let (forward, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
    )
    .expect("concrete aggregate and Box facts lower to verified SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &backward_names,
        &type_environment,
        &backward_typed,
        &backward_owned,
        backward_entry,
    )
    .expect("input permutation preserves aggregate identities");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    assert_eq!(
        module
            .types
            .iter()
            .filter(|ty| matches!(ty, SsaTypeKind::HeapOwner { .. }))
            .count(),
        2,
        "Bundle and Box<Token> have distinct heap-owner identities"
    );
    let sum = function(module, "p.sum");
    assert_eq!(
        sum.instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::AggregateProject { .. }
            ))
            .count(),
        2
    );
    let make_bundle = function(module, "p.makeBundle");
    let (text, count) = make_bundle
        .instructions
        .iter()
        .find_map(|instruction| match &instruction.operation {
            Operation::AggregateConstruct { fields, .. } => Some((fields[0], fields[1])),
            _ => None,
        })
        .expect("named arguments are assembled in declaration order");
    let count_evaluation = make_bundle
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::Constant(_)))
        .expect("count argument is evaluated");
    let text_evaluation = make_bundle
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
        .expect("text argument is evaluated");
    assert!(
        count_evaluation < text_evaluation,
        "named operands preserve source evaluation order"
    );
    assert!(make_bundle.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::StringLiteral { .. } if instruction.results[0] == EntityId::Value(text)
    )));
    assert!(make_bundle.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::Constant(_) if instruction.results[0] == EntityId::Value(count)
    )));
    assert!(
        make_bundle
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
    );
    assert!(
        !make_bundle
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
    );

    let count = function(module, "p.count");
    assert!(matches!(
        count.instructions.as_slice(),
        [
            super::model::Instruction {
                operation: Operation::HeapPayloadPlace { .. },
                ..
            },
            super::model::Instruction {
                operation: Operation::FieldPlace { .. },
                ..
            },
            super::model::Instruction {
                operation: Operation::Read { .. },
                ..
            },
            super::model::Instruction {
                operation: Operation::Drop { .. },
                ..
            }
        ]
    ));
    let boxed = function(module, "p.boxed");
    let token = boxed
        .instructions
        .iter()
        .find_map(|instruction| match instruction.operation {
            Operation::AggregateConstruct { .. } => Some(instruction.results[0]),
            _ => None,
        })
        .expect("Token aggregate is constructed");
    assert!(boxed.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::HeapAllocate { payload, .. } if EntityId::Value(payload) == token
    )));
    let discard = function(module, "p.inspectBox");
    assert_eq!(
        discard
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
            .count(),
        1
    );
}

#[test]
fn finite_value_class_recursion_through_a_heap_handle_lowers() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "test/recursive.ko",
        "package test\n\
         value class Value(val owner: Owner)\n\
         class Owner(val item: Value)\n\
         class Payload(val item: Int)\n\
         fun entry(own item: Value) {}\n\
         fun classRc(own owner: Rc<Payload>) {}\n\
         fun nestedRc(own owner: Rc<Rc<Int>>) {}",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/recursive.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("a heap handle breaks the inline value-class recursion");

    let module = &program.modules[0];
    assert_eq!(
        module
            .types
            .iter()
            .filter(|ty| matches!(ty, SsaTypeKind::HeapOwner { .. }))
            .count(),
        1
    );
    assert_eq!(
        module
            .types
            .iter()
            .filter(|ty| matches!(ty, SsaTypeKind::Aggregate { .. }))
            .count(),
        2,
        "Value and Owner.payload are distinct finite inline layouts"
    );

    let (class_rc, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "classRc"),
    )
    .expect("Rc<Class> defines its heap payload before the shared owner");
    assert_eq!(
        class_rc.modules[0]
            .types
            .iter()
            .filter(|ty| matches!(ty, SsaTypeKind::SharedOwner { .. }))
            .count(),
        1
    );

    let (nested_rc, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "nestedRc"),
    )
    .expect("nested Rc payload owners are defined from the inside out");
    assert_eq!(
        nested_rc.modules[0]
            .types
            .iter()
            .filter(|ty| matches!(ty, SsaTypeKind::SharedOwner { .. }))
            .count(),
        2
    );
}

#[test]
fn rejects_generic_nominal_and_move_only_field_read_without_partial_ssa() {
    let (name_environment, type_environment) = standard_environments();
    let mut sources = SourceMap::new();
    let (generic_source, generic) = parsed(
        &mut sources,
        "test/generic.ko",
        "package test\n\
         value class Wrapper<T>(val item: T)\n\
         fun entry(): Wrapper<Int> = Wrapper(1)",
    );
    let generic_inputs = [SourceUnitInput::new(
        "root",
        "test/generic.ko",
        generic_source,
        &generic,
    )];
    let (names, typed, owned) = analyze(
        &sources,
        &generic_inputs,
        &name_environment,
        &type_environment,
    );
    let error = match lower_scalar_unit_with_entry(
        &sources,
        &generic_inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    ) {
        Ok(_) => panic!("generic nominal layout remains an explicit boundary"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);

    let mut sources = SourceMap::new();
    let (field_source, field) = parsed(
        &mut sources,
        "test/field.ko",
        "package test\n\
         class Bundle(val text: String, val count: Int)\n\
         fun entry(): Boolean {\n\
             val bundle = Bundle(\"owned\", 1)\n\
             return bundle.text == \"owned\"\n\
         }\n\
         fun temporary(): Int = Bundle(\"owned\", 7).count",
    );
    let field_inputs = [SourceUnitInput::new(
        "root",
        "test/field.ko",
        field_source,
        &field,
    )];
    let (names, typed, owned) = analyze(
        &sources,
        &field_inputs,
        &name_environment,
        &type_environment,
    );
    let error = match lower_scalar_unit_with_entry(
        &sources,
        &field_inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    ) {
        Ok(_) => panic!("MoveOnly field projection is not guessed into an owned read"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);

    let error = match lower_scalar_unit_with_entry(
        &sources,
        &field_inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "temporary"),
    ) {
        Ok(_) => panic!("MoveOnly temporary receiver needs an exact outer drop fact"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

fn function<'a>(module: &'a super::model::Module, name: &str) -> &'a super::model::Function {
    module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .expect("reachable function exists")
}
