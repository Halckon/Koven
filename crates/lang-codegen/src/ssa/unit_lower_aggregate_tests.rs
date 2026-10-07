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
fn direct_slot_generic_class_instances_keep_distinct_layout_identities() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "test/generic-layout.ko",
        "package test\n\
         class Cell<T>(val item: T)\n\
         fun entry(): Int {\n\
             val narrow = Cell<Int>(20)\n\
             val wide = Cell<Long>(22L)\n\
             return narrow.item + 2\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/generic-layout.ko",
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
    .expect("direct-slot generic class instances must lower");
    let module = &program.modules[0];
    assert_eq!(
        module
            .types
            .iter()
            .filter(|ty| matches!(ty, SsaTypeKind::HeapOwner { .. }))
            .count(),
        2,
        "Cell<Int> and Cell<Long> keep distinct owner/layout identities"
    );
    let entry = function(module, "test.entry");
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. }))
            .count(),
        2
    );
}

#[test]
fn lowers_frontend_authorized_nested_generic_class_field_layouts() {
    let (name_environment, type_environment) = standard_environments();
    for (name, text, heap_owners, containers) in [
        (
            "intrinsic",
            "package test\n\
             class Dependent<T>(val items: List<T>)\n\
             fun entry(own input: Dependent<Int>): Int = 1",
            1,
            1,
        ),
        (
            "nominal",
            "package test\n\
             class Wrapper<T>(val marker: Int)\n\
             class Dependent<T>(val item: Wrapper<T>)\n\
             fun entry(own input: Dependent<Int>): Int = 1",
            2,
            0,
        ),
        (
            "deep-intrinsic",
            "package test\n\
             class Dependent<T>(val items: List<List<T>>)\n\
             fun entry(own input: Dependent<Int>): Int = 1",
            1,
            2,
        ),
        (
            "deep-nominal",
            "package test\n\
             class Wrapper<T>(val item: T)\n\
             class Dependent<T>(val item: Wrapper<List<T>>)\n\
             fun entry(own input: Dependent<Int>): Int = 1",
            2,
            1,
        ),
    ] {
        let mut sources = SourceMap::new();
        let path = format!("test/{name}.ko");
        let (source, parsed) = parsed(&mut sources, &path, text);
        let inputs = [SourceUnitInput::new("root", &path, source, &parsed)];
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let (program, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        )
        .expect("frontend-authorized nested generic field layout must lower");
        let module = &program.modules[0];
        assert_eq!(
            module
                .types
                .iter()
                .filter(|ty| matches!(ty, SsaTypeKind::HeapOwner { .. }))
                .count(),
            heap_owners,
            "{name}"
        );
        assert_eq!(
            module
                .types
                .iter()
                .filter(|ty| matches!(ty, SsaTypeKind::SequentialContainer { .. }))
                .count(),
            containers,
            "{name}"
        );
    }
}

#[test]
fn lowers_pointer_like_nullable_generic_class_field_layouts() {
    let (name_environment, type_environment) = standard_environments();
    for (name, declarations, actual) in [
        ("class", "class Node", "Node"),
        ("box", "value class Token(val item: Int)", "Box<Token>"),
        ("rc", "", "Rc<Int>"),
    ] {
        let mut sources = SourceMap::new();
        let path = format!("test/nullable-{name}.ko");
        let text = format!(
            "package test\n{declarations}\n\
             class Holder<T>(val item: T?)\n\
             fun entry(own input: Holder<{actual}>): Int = 1"
        );
        let (source, parsed) = parsed(&mut sources, &path, &text);
        let inputs = [SourceUnitInput::new("root", &path, source, &parsed)];
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let (program, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        )
        .unwrap_or_else(|error| panic!("{name} nullable layout must lower: {error:?}"));
        assert_eq!(
            program.modules[0]
                .types
                .iter()
                .filter(|ty| matches!(ty, SsaTypeKind::NullableHandle { .. }))
                .count(),
            1,
            "{name}"
        );
    }
}

#[test]
fn adapts_pointer_like_nullable_storage_at_each_owned_value_boundary() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "test/nullable-boundaries.ko",
        "package test\n\
         class Node\n\
         fun make(): Node? = Node()\n\
         fun pass(own input: Node?): Node? = input\n\
         fun entry(): Unit {\n\
             var current: Node? = Node()\n\
             { current = Node() }\n\
             val source = Node()\n\
             val passed = pass(source)\n\
             val made = make()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/nullable-boundaries.ko",
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
    .expect("nullable local/call/assignment/return boundaries must lower");

    assert_eq!(
        program.modules[0]
            .functions
            .iter()
            .flat_map(|function| function.instructions.iter())
            .filter(|instruction| matches!(instruction.operation, Operation::NullableWrap { .. }))
            .count(),
        4,
        "local initialization, root assignment, Value call and return each consume one inner owner"
    );
}

#[test]
fn rejects_unauthorized_nested_generic_class_field_layouts() {
    let (name_environment, type_environment) = standard_environments();
    for (name, declarations, actual) in [
        ("inline-nullable", "class Dependent<T>(val item: T?)", "Int"),
        (
            "string-nullable",
            "class Dependent<T>(val item: T?)",
            "String",
        ),
        (
            "other-intrinsic",
            "class Dependent<T>(val item: Array<T>)",
            "Int",
        ),
        (
            "value-wrapper",
            "value class Wrapper<T>(val item: T)\nclass Dependent<T>(val item: Wrapper<T>)",
            "Int",
        ),
        (
            "growing-owner",
            "class Dependent<T>(val item: Dependent<List<T>>)",
            "Int",
        ),
    ] {
        let mut sources = SourceMap::new();
        let path = format!("test/{name}.ko");
        let text = format!(
            "package test\n{declarations}\n\
             fun entry(own input: Dependent<{actual}>): Int = 1"
        );
        let (source, parsed) = parsed(&mut sources, &path, &text);
        let inputs = [SourceUnitInput::new("root", &path, source, &parsed)];
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
            Ok(_) => panic!("unauthorized nested runtime recipe must remain unsupported: {name}"),
            Err(error) => error,
        };
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{name}");
        assert!(error.span.is_some(), "{name}");
    }
}

#[test]
fn rejects_recursive_nullable_owner_until_ssa_declaration_cycles_are_supported() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "test/recursive-nullable.ko",
        "package test\n\
         class Node(val next: Node?)\n\
         fun entry(own input: Node): Int = 1",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/recursive-nullable.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let error = match lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    ) {
        Ok(_) => panic!("recursive nullable owner requires a separate SSA type-cycle slice"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
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

#[test]
fn lowers_compilation_unit_copyable_and_move_only_destructuring() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         value class Pair(val first: Int, val second: Int)\n\
         class Token(val id: Int)\n\
         value class ResourcePair(val left: Token, val right: Token)\n\
         fun makePair(): Pair = Pair(10, 20)\n\
         fun makeResourcePair(): ResourcePair = ResourcePair(Token(1), Token(2))",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun copyableEntry(): Int {\n\
             val (a, b) = p.makePair()\n\
             return a + b\n\
         }\n\
         fun moveOnlyEntry(): Int {\n\
             val (x, y) = p.makeResourcePair()\n\
             return x.id + y.id\n\
         }",
    );
    let inputs = [
        SourceUnitInput::new("root", "p/provider.ko", provider_source, &provider),
        SourceUnitInput::new("root", "q/consumer.ko", consumer_source, &consumer),
    ];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);

    let entry_copy = declaration(&names, "q", "copyableEntry");
    let (program_copy, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry_copy,
    )
    .expect("copyable destructuring lowers to verified SSA");
    let rendered_copy = render_program(&program_copy);
    assert!(rendered_copy.contains("aggregate.copy_explode"));

    let entry_move = declaration(&names, "q", "moveOnlyEntry");
    let (program_move, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry_move,
    )
    .expect("move-only destructuring lowers to verified SSA");
    let rendered_move = render_program(&program_move);
    assert!(rendered_move.contains("aggregate.explode"));
}

fn function<'a>(module: &'a super::model::Module, name: &str) -> &'a super::model::Function {
    module
        .functions
        .iter()
        .find(|function| function.name.contains(name))
        .expect("reachable function exists")
}
