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
    let mut observations = Vec::new();
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
        let (program, entry) = lower_scalar_unit_with_entry(
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
        if environment == 1 {
            // Catch only the new read-only query; all three original source products
            // still complete their real frontend/lowering/SSA/LLVM gates directly.
            observations.push(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || {
                    assert_shared_capture_current_loans(&sources, &program, entry, literal);
                },
            )));
        }
        println!("Leaf environment {environment}: original SSA/LLVM gates passed");
    }
    assert_eq!(observations.len(), 1, "one Shared product observation");
    let mut failures = Vec::new();
    for result in observations {
        match result {
            Ok(()) => println!("Shared capture current-loan observation: ok"),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string query panic payload");
                let failure = format!("Shared capture current-loan observation: {message}");
                println!("{failure}");
                failures.push(failure);
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn assert_shared_capture_current_loans(
    sources: &SourceMap,
    program: &crate::ssa::model::Program,
    entry: crate::ssa::model::FunctionId,
    literal: &str,
) {
    use crate::ssa::model::{
        CheckedArithmeticOperator, ClosureCaptureOperand, Definition, LoanKind, Origin,
        TerminatorKind,
    };

    let module = &program.modules[0];
    let entry = module.function(entry).unwrap();
    let formations = entry
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::ClosureConstruct { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        formations.len(),
        1,
        "one actual captured callback formation"
    );
    let formation = formations[0];
    let Operation::ClosureConstruct {
        thunk, captures, ..
    } = &formation.operation
    else {
        unreachable!()
    };
    assert!(matches!(
        captures.as_slice(),
        [ClosureCaptureOperand::Shared(_)]
    ));
    let function = module.function(*thunk).unwrap();
    let Origin::Source(lambda_span) = &function.origin else {
        panic!("the actual closure thunk must retain its source lambda");
    };
    assert_eq!(
        sources.source_name(lambda_span.source_id()).unwrap(),
        "test/leaf-thunk.ko"
    );
    let source = sources.source_text(lambda_span.source_id()).unwrap();
    let start = source
        .find(literal)
        .expect("the exact Shared lambda exists");
    assert_eq!(
        (lambda_span.start(), lambda_span.end()),
        (start, start + literal.len())
    );
    assert_eq!(sources.slice(*lambda_span).unwrap(), literal);
    assert_eq!(formation.origin.span(), *lambda_span);
    let entry_block = function.block(function.entry_block().unwrap()).unwrap();
    let EntityId::Loan(environment) = entry_block.parameters[0] else {
        panic!("the actual thunk receives its borrowed environment first");
    };
    let parents = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.operation,
            Operation::SharedFieldLoan { base, field: 0 } if base == environment)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        parents.len(),
        1,
        "field zero has one actual environment-slot loan"
    );
    let parent_instruction = parents[0];
    let [EntityId::Loan(parent)] = parent_instruction.results.as_slice() else {
        panic!("SharedFieldLoan produces its parent slot loan");
    };
    let follows = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.operation,
            Operation::SharedReferenceFollow { source } if source == *parent)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        follows.len(),
        1,
        "follow the actual parent, not a same-typed loan"
    );
    let follow = follows[0];
    let [EntityId::Loan(child)] = follow.results.as_slice() else {
        panic!("the Follow produces its distinct child view");
    };
    assert_ne!(parent, child);
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: storage,
    } = function.entity(EntityId::Loan(*parent)).unwrap().ty
    else {
        panic!("the environment slot is a shared loan");
    };
    let Some(SsaTypeKind::SharedReference { target }) = module.type_kind(storage) else {
        panic!("the parent holds the actual SharedReference storage");
    };
    assert_eq!(
        function.entity(EntityId::Loan(*child)).unwrap().ty,
        EntityType::Loan {
            kind: LoanKind::Shared,
            target: *target
        }
    );
    let checked = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::CheckedArithmetic { .. }))
        .collect::<Vec<_>>();
    assert_eq!(checked.len(), 1);
    let checked = checked[0];
    assert!(matches!(
        checked.operation,
        Operation::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Add,
            ..
        }
    ));
    assert_eq!(
        sources.slice(checked.origin.span()).unwrap(),
        "index + scale.number"
    );
    assert_eq!(checked.origin.span().source_id(), lambda_span.source_id());
    assert!(
        lambda_span.start() <= checked.origin.span().start()
            && checked.origin.span().end() <= lambda_span.end()
    );
    let [EntityId::Value(_), EntityId::Value(failed)] = checked.results.as_slice() else {
        panic!("checked Add returns a scalar and a failure flag");
    };
    let before = function.block(checked.block).unwrap();
    let before_position = |id| {
        before
            .instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    assert_eq!(parent_instruction.block, before.id);
    assert_eq!(follow.block, before.id);
    assert!(before_position(parent_instruction.id) < before_position(follow.id));
    assert!(before_position(follow.id) < before_position(checked.id));
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &before.terminator.as_ref().unwrap().kind
    else {
        panic!("the checked flag selects failure or success");
    };
    assert_eq!(condition, failed);
    assert_eq!(when_true.arguments, when_false.arguments);
    assert_ne!(when_true.target, when_false.target);
    let failure = function.block(when_true.target).unwrap();
    let success = function.block(when_false.target).unwrap();
    assert!(matches!(
        failure.terminator.as_ref().unwrap().kind,
        TerminatorKind::Abort
    ));
    assert!(
        failure.instructions.is_empty(),
        "Abort does not unwind capture views"
    );
    assert_eq!(failure.parameters.len(), when_true.arguments.len());
    assert_eq!(success.parameters.len(), when_false.arguments.len());
    let current = |source| {
        let columns = when_false
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, argument)| (*argument == source).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(
            columns.len(),
            1,
            "one physical column for this exact source loan"
        );
        let index = columns[0];
        let EntityId::Loan(loan) = success.parameters[index] else {
            panic!("the current capture column is a Loan parameter");
        };
        let parameter = EntityId::Loan(loan);
        assert_ne!(parameter, source, "success must use the rebound identity");
        for (block, parameter) in [(success, parameter), (failure, failure.parameters[index])] {
            let data = function.entity(parameter).unwrap();
            assert_eq!(
                data.definition,
                Definition::BlockParameter {
                    block: block.id,
                    index
                }
            );
            assert_eq!(data.ty, function.entity(source).unwrap().ty);
        }
        (index, loan)
    };
    let (parent_column, current_parent) = current(EntityId::Loan(*parent));
    let (child_column, current_child) = current(EntityId::Loan(*child));
    assert_ne!(
        parent_column, child_column,
        "a parent and its Follow child stay distinct"
    );
    let ends = function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::BorrowEnd { loan } => Some((instruction, loan)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        ends.iter().all(|(_, loan)| loan != parent && loan != child),
        "neither pre-CFG loan may be ended instead of its current parameter"
    );
    let end_position = |loan| {
        let matching = ends
            .iter()
            .filter(|(_, actual)| *actual == loan)
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "end this exact current loan once");
        let instruction = matching[0].0;
        assert_eq!(instruction.block, success.id);
        success
            .instructions
            .iter()
            .position(|id| *id == instruction.id)
            .unwrap()
    };
    assert!(
        end_position(current_child) < end_position(current_parent),
        "end the current Follow child before the current parent slot"
    );
    assert!(
        matches!(
            success.terminator.as_ref().unwrap().kind,
            TerminatorKind::Return { .. }
        ),
        "both current loans end on the normal returning path"
    );
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
