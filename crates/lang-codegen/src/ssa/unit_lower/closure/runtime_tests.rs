//! Concrete constructor callbacks require resource thunks and source-address ABI slots.
use crate::ssa::{
    model::{EntityId, EntityType, Operation, SsaTypeKind},
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};
use lang_frontend::{
    name_resolution::SourceUnitInput,
    ownership_checking::UnitCallableOrigin,
    parser::Expression,
    source::SourceMap,
    type_checking::{UnitCallableTarget, standard_environments},
};

#[test]
fn unit_runtime_leaf_callback_reaches_deinit_body_and_borrowed_capture_fields() {
    for (environment, literal) in [
        (0, "{ index -> Leaf(index) }"),
        (1, "{ index -> Leaf(index + scale.number) }"),
        (2, "move { index -> Leaf(index + scale.number) }"),
    ] {
        let text = format!(
            "package test\nclass Leaf(val number: Int) {{ deinit() {{ println(\"leaf\") }} }}\nfun entry(): Int {{ val scale = Leaf(7)\nval callback: (Int)->Leaf = {literal}\nval items = List<Leaf>(3, callback)\nreturn items.size }}"
        );
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "test/leaf-thunk.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "test/leaf-thunk.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let arena = typed.types().types().len();
        let (program, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        )
        .unwrap_or_else(|error| panic!("Leaf environment {environment}: {error:?}"));
        let module = &program.modules[0];
        let thunk = module
            .functions
            .iter()
            .find(|function| function.name.contains(".thunk"))
            .expect("the selected constructor initializer has a concrete thunk");
        assert!(
            thunk
                .instructions
                .iter()
                .any(|instruction| matches!(instruction.operation, Operation::HeapAllocate { .. })),
            "each logical callback returns a fresh complete Resource owner"
        );
        assert!(
            thunk.instructions.iter().any(|instruction| matches!(instruction.operation, Operation::HeapAllocate { owner, .. } if module.deinit(owner).is_some())),
            "lambda-only Resource construction must schedule the existing hidden deinit body"
        );
        if environment != 0 {
            assert!(
                thunk.instructions.iter().any(|instruction| matches!(
                    instruction.operation, Operation::HeapFieldRead { receiver, .. }
                        if matches!(thunk.entity(EntityId::Loan(receiver)).map(|entity| entity.ty),
                            Some(EntityType::Loan { .. }))
                )),
                "captured MoveOnly receiver is read through its shared view without moving it"
            );
        }
        crate::llvm::render_verified_program(&program)
            .expect("Resource callback reaches verified LLVM");
        assert_eq!(typed.types().types().len(), arena);
    }
}

#[test]
fn unit_runtime_bare_known_function_initializer_uses_a_typed_borrow_slot() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "test/known-function.ko",
        "package test\nfun identity(index: Int): Int = index\nfun entry(): Int { val items = List<Int>(3, identity)\nreturn items[2] }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/known-function.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let construction = &typed.types().container_constructions()[0];
    let Expression::Call { arguments, .. } = file
        .ast()
        .expressions()
        .get(construction.expression().expression())
        .expect("typed constructor AST")
        .payload()
    else {
        panic!("constructor descriptor identifies a Call")
    };
    let initializer = lang_frontend::type_checking::UnitExpressionId::new(
        construction.expression().source_unit(),
        arguments[1].value,
    );
    assert_eq!(
        owned
            .ownership()
            .callable_origin(initializer)
            .expect("sealed named origin")
            .origin(),
        UnitCallableOrigin::KnownFunction(UnitCallableTarget::Declaration(declaration(
            &names, "test", "identity"
        )))
    );
    let arena = typed.types().types().len();
    let (program, _) = lower_scalar_unit_with_entry(
        &sources, &inputs, &names, &type_environment, &typed, &owned,
        declaration(&names, "test", "entry"),
    ).expect("a canonical source address needs a bounded ABI loan, without invented frontend storage facts");
    let module = &program.modules[0];
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.instructions.iter().any(|instruction| {
                matches!(instruction.operation, Operation::FunctionAddress { .. })
            }))
    );
    assert!(
        module
            .types
            .iter()
            .any(|ty| matches!(ty, SsaTypeKind::FunctionPointer { .. }))
    );
    assert!(
        !module
            .types
            .iter()
            .any(|ty| matches!(ty, SsaTypeKind::ConcreteClosure { .. }))
    );
    crate::llvm::render_verified_program(&program)
        .expect("known function initializer reaches LLVM");
    assert_eq!(typed.types().types().len(), arena);
}

#[test]
fn unit_runtime_borrowed_resource_parameter_reads_copyable_field() {
    let mut sources = SourceMap::new();
    let (source, file) = parsed(
        &mut sources,
        "test/borrowed-resource-field.ko",
        "package test\nclass Leaf(val number: Int) { deinit() {} }\nfun verify(value: Leaf): Int = value.number\nfun entry(): Int { val item = Leaf(7)\nreturn verify(item) }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/borrowed-resource-field.ko",
        source,
        &file,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let arena = typed.types().types().len();
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("Borrow Resource parameters expose Copyable fields through their active heap loan");
    let verify = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains("test.verify"))
        .expect("selected verifier body");
    assert!(
        verify
            .instructions
            .iter()
            .any(|instruction| matches!(instruction.operation, Operation::HeapFieldRead { .. })),
        "reading a Copyable field must retain the complete Resource receiver loan"
    );
    crate::llvm::render_verified_program(&program).expect("borrowed resource field reaches LLVM");
    assert_eq!(typed.types().types().len(), arena);
}

#[test]
fn unit_runtime_captured_resource_string_field_borrows_through_println() {
    for literal in [
        "{ index -> println(scale.name)\nindex }",
        "move { index -> println(scale.name)\nindex }",
    ] {
        let text = format!(
            "package test\nclass Resource(val name: String) {{ deinit() {{ println(this.name) }} }}\nfun entry(): Int {{ val scale = Resource(\"scale\")\nval callback: (Int)->Int = {literal}\nval items = List<Int>(3, callback)\nreturn items.size }}"
        );
        let mut sources = SourceMap::new();
        let (source, file) = parsed(&mut sources, "test/captured-resource-field.ko", &text);
        let inputs = [SourceUnitInput::new(
            "root",
            "test/captured-resource-field.ko",
            source,
            &file,
        )];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let arena = typed.types().types().len();
        let (program, _) = lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            declaration(&names, "test", "entry"),
        )
        .expect(
            "captured Resource String fields use a child loan through the synchronous print call",
        );
        let thunk = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.contains(".thunk"))
            .expect("real capture thunk");
        assert!(
            thunk.instructions.iter().any(|instruction| matches!(
                instruction.operation,
                Operation::SharedHeapFieldLoan { .. }
            )),
            "MoveOnly String payload must be borrowed from its live captured Resource view"
        );
        assert!(
            !thunk
                .instructions
                .iter()
                .any(|instruction| instruction.results.iter().any(|&result| {
                    let Some(EntityType::Value(ty)) = thunk.entity(result).map(|entity| entity.ty)
                    else {
                        return false;
                    };
                    matches!(
                        program.modules[0].types.get(ty.index()),
                        Some(SsaTypeKind::StringOwner)
                    )
                })),
            "each callback preserves the captured Resource and String owners"
        );
        crate::llvm::render_verified_program(&program).expect("capture field loan reaches LLVM");
        assert_eq!(typed.types().types().len(), arena);
    }
}
