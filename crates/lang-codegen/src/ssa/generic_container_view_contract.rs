//! Independent Unit view ABI contract; the original Legacy concrete helper contract is retained.
use crate::ssa::model::{Operation, SsaTypeKind};
use crate::test_support::diagnostics::assert_diagnostic;
#[test]
fn generic_nominal_container_callbacks_preserve_concrete_to_view_transport() {
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;
    let mut failures = Vec::new();
    for container in ["Array", "List"] {
        for (environment, initializer) in [
            ("pointer", "{ index -> Leaf(index) }"),
            ("shared", "{ index -> Leaf(index + scale) }"),
            ("owned", "move { index -> Leaf(index + scale) }"),
        ] {
            let text = format!(
                "class Leaf(val number: Int) {{ deinit() {{ println(\"leaf\") }} }}\nfun <T> generate(size: Int, callback: (Int)->T): {container}<T> = {container}<T>(size, callback)\nfun entry(): Int {{ val scale = 7\nval callback: (Int)->Leaf = {initializer}\nval items = generate<Leaf>(3, callback)\nreturn items.size }}"
            );
            {
                let unit = UnitSource::analyze("single-runtime-intent.ko", &text).unwrap();
                unit.assert_frontend_clean();
                let entry = unit.declaration("entry", SymbolKind::Function);
                let helper = unit.declaration("generate", SymbolKind::Function);
                let arena = unit.types().types().len();
                match unit.lower_entry(entry) {
                    Ok((program, root)) => {
                        assert_eq!(
                            program
                                .module(root.module())
                                .unwrap()
                                .function(root)
                                .unwrap()
                                .origin,
                            crate::ssa::model::Origin::Source(unit.declaration_span(entry))
                        );
                        let module = &program.modules[0];
                        let helpers = module
                            .functions
                            .iter()
                            .filter(|function| {
                                function.origin
                                    == crate::ssa::model::Origin::Source(
                                        unit.declaration_span(helper),
                                    )
                            })
                            .collect::<Vec<_>>();
                        assert_eq!(helpers.len(), 1, "one actual nominal helper instance");
                        let generate = helpers[0];
                        let operation = generate
                            .instructions
                            .iter()
                            .find(|instruction| {
                                matches!(
                                    instruction.operation,
                                    Operation::ContainerGenerateBorrowed { .. }
                                )
                            })
                            .expect("helper must execute the borrowed runtime constructor");
                        let Operation::ContainerGenerateBorrowed {
                            container,
                            initializer: helper_initializer,
                            ..
                        } = operation.operation
                        else {
                            unreachable!()
                        };
                        let SsaTypeKind::SequentialContainer { element, .. } =
                            module.type_kind(container).unwrap()
                        else {
                            panic!("concrete sequential identity")
                        };
                        assert!(
                            matches!(
                                module.type_kind(*element),
                                Some(SsaTypeKind::HeapOwner { .. })
                            ),
                            "ordinary Resource class remains its concrete heap-owner element identity"
                        );
                        let crate::ssa::model::EntityType::Loan { target, .. } = generate
                            .entity(crate::ssa::model::EntityId::Loan(helper_initializer))
                            .unwrap()
                            .ty
                        else {
                            panic!("borrowed initializer ABI")
                        };
                        let SsaTypeKind::CallableView { signature } =
                            module.type_kind(target).unwrap()
                        else {
                            panic!("Unit helper accepts a callable view")
                        };
                        assert_eq!(signature.returns.as_slice(), [*element]);
                        assert_eq!(
                            generate
                                .block(generate.entry_block().unwrap())
                                .unwrap()
                                .parameters
                                .len(),
                            2
                        );
                        let callback_parameter = generate
                            .block(generate.entry_block().unwrap())
                            .unwrap()
                            .parameters[1];
                        assert_original_parameter_transport(
                            generate,
                            callback_parameter,
                            crate::ssa::model::EntityId::Loan(helper_initializer),
                        );
                        let caller = module.function(root).unwrap();
                        let calls = caller.instructions.iter().filter(|i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == generate.id)).collect::<Vec<_>>();
                        assert_eq!(calls.len(), 1);
                        let call = calls[0];
                        let Operation::DirectCall {
                            arguments,
                            receiver,
                            ..
                        } = &call.operation
                        else {
                            unreachable!()
                        };
                        assert_eq!(arguments.len(), 2);
                        assert!(receiver.is_none());
                        let crate::ssa::model::Origin::Source(call_span) = call.origin else {
                            panic!("original helper call")
                        };
                        assert_eq!(
                            unit.sources().slice(call_span),
                            Ok("generate<Leaf>(3, callback)")
                        );
                        let view = arguments[1];
                        assert_eq!(
                            caller.entity(view).unwrap().ty,
                            crate::ssa::model::EntityType::Loan {
                                kind: crate::ssa::model::LoanKind::Shared,
                                target
                            }
                        );
                        let crate::ssa::model::Definition::InstructionResult {
                            instruction, ..
                        } = caller.entity(view).unwrap().definition
                        else {
                            panic!("actual view definition")
                        };
                        let view_instruction = caller.instruction(instruction).unwrap();
                        let Operation::CallableViewOf { source } = view_instruction.operation
                        else {
                            panic!("actual view transport")
                        };
                        let source_entity = crate::ssa::model::EntityId::Loan(source);
                        let crate::ssa::model::EntityType::Loan {
                            kind: crate::ssa::model::LoanKind::Shared,
                            target: concrete,
                        } = caller.entity(source_entity).unwrap().ty
                        else {
                            panic!("concrete shared source")
                        };
                        let crate::ssa::model::Definition::InstructionResult {
                            instruction, ..
                        } = caller.entity(source_entity).unwrap().definition
                        else {
                            panic!("source borrow definition")
                        };
                        let begin = caller.instruction(instruction).unwrap();
                        let Operation::BorrowBegin { place, .. } = begin.operation else {
                            panic!("concrete source borrow")
                        };
                        let crate::ssa::model::Definition::InstructionResult {
                            instruction, ..
                        } = caller
                            .entity(crate::ssa::model::EntityId::Place(place))
                            .unwrap()
                            .definition
                        else {
                            panic!("root place definition")
                        };
                        let root_place = caller.instruction(instruction).unwrap();
                        let Operation::RootPlace { owner } = root_place.operation else {
                            panic!("actual callback owner")
                        };
                        assert_eq!(
                            caller
                                .entity(crate::ssa::model::EntityId::Value(owner))
                                .unwrap()
                                .ty,
                            crate::ssa::model::EntityType::Value(concrete)
                        );
                        let crate::ssa::model::Definition::InstructionResult {
                            instruction, ..
                        } = caller
                            .entity(crate::ssa::model::EntityId::Value(owner))
                            .unwrap()
                            .definition
                        else {
                            panic!("actual lambda construction")
                        };
                        let formed = caller.instruction(instruction).unwrap();
                        let crate::ssa::model::Origin::Source(formed_span) = formed.origin else {
                            panic!("original lambda expression")
                        };
                        assert_eq!(unit.sources().slice(formed_span), Ok(initializer));
                        match (&formed.operation, environment) {
                            (Operation::FunctionAddress { target: thunk }, "pointer") => {
                                assert!(module.function(*thunk).is_some());
                            }
                            (
                                Operation::ClosureConstruct {
                                    closure,
                                    thunk,
                                    captures,
                                },
                                "shared" | "owned",
                            ) => {
                                assert_eq!(*closure, concrete);
                                assert!(module.function(*thunk).is_some());
                                assert_eq!(captures.len(), 1);
                                assert!(matches!(
                                    (environment, &captures[0]),
                                    (
                                        "shared",
                                        crate::ssa::model::ClosureCaptureOperand::Shared(_)
                                    ) | (
                                        "owned",
                                        crate::ssa::model::ClosureCaptureOperand::Owned(_)
                                    )
                                ));
                            }
                            _ => panic!("original concrete lambda creation: {environment}"),
                        }
                        let ends = |loan| {
                            caller.instructions.iter().filter(|i| matches!(i.operation, Operation::BorrowEnd { loan: actual } if actual == loan)).collect::<Vec<_>>()
                        };
                        let crate::ssa::model::EntityId::Loan(view_loan) = view else {
                            unreachable!()
                        };
                        let view_ends = ends(view_loan);
                        let source_ends = ends(source);
                        assert_eq!(view_ends.len(), 1);
                        assert_eq!(source_ends.len(), 1);
                        let block = caller
                            .blocks
                            .iter()
                            .find(|b| b.instructions.contains(&call.id))
                            .expect("actual call block");
                        let positions = [
                            formed,
                            root_place,
                            begin,
                            view_instruction,
                            call,
                            view_ends[0],
                            source_ends[0],
                        ]
                        .map(|instruction| {
                            block
                                .instructions
                                .iter()
                                .position(|id| *id == instruction.id)
                                .expect("same actual caller block")
                        });
                        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
                        match (environment, module.type_kind(concrete).unwrap()) {
                            ("pointer", SsaTypeKind::FunctionPointer { signature }) => {
                                assert_eq!(signature.returns.as_slice(), [*element])
                            }
                            (
                                "shared" | "owned",
                                SsaTypeKind::ConcreteClosure {
                                    signature,
                                    captures,
                                    ..
                                },
                            ) => {
                                assert_eq!(signature.returns.as_slice(), [*element]);
                                assert_eq!(captures.len(), 1);
                                assert_eq!(
                                    captures[0].mode,
                                    if environment == "shared" {
                                        crate::ssa::model::ClosureCaptureMode::Shared
                                    } else {
                                        crate::ssa::model::ClosureCaptureMode::Owned
                                    }
                                );
                            }
                            _ => panic!("concrete helper callback environment: {environment}"),
                        }
                        crate::llvm::render_verified_program(&program)
                            .expect("nominal generic helper must reach verified LLVM");
                    }
                    Err(error) => failures.push(format!("{container}/{environment}: {error:?}")),
                }
                assert_eq!(
                    unit.types().types().len(),
                    arena,
                    "the backend must only read canonical nominal and function types"
                );
            }
        }
    }
    assert_diagnostic!(
        failures.is_empty(),
        "nominal generic helper must preserve each selected callback ABI: {failures:?}"
    );
}

fn assert_original_parameter_transport(
    function: &crate::ssa::model::Function,
    original: crate::ssa::model::EntityId,
    actual: crate::ssa::model::EntityId,
) {
    use crate::ssa::model::TerminatorKind;
    let edges = |block: &crate::ssa::model::Block| -> Vec<crate::ssa::model::Edge> {
        match &block.terminator.as_ref().unwrap().kind {
            TerminatorKind::Branch(edge) => vec![edge.clone()],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true.clone(), when_false.clone()],
            TerminatorKind::Return { .. } | TerminatorKind::Abort => vec![],
            _ => panic!("unclaimed helper control flow"),
        }
    };
    let mut known = std::collections::HashSet::from([original]);
    for _ in 0..function.blocks.len() {
        for block in &function.blocks {
            for edge in edges(block) {
                let parameters = &function.block(edge.target).unwrap().parameters;
                assert_eq!(edge.arguments.len(), parameters.len());
                for (argument, parameter) in edge.arguments.iter().zip(parameters) {
                    if known.contains(argument) {
                        known.insert(*parameter);
                    }
                }
            }
        }
    }
    for block in &function.blocks {
        for edge in edges(block) {
            for (argument, parameter) in edge
                .arguments
                .iter()
                .zip(&function.block(edge.target).unwrap().parameters)
            {
                if known.contains(parameter) {
                    assert!(
                        known.contains(argument),
                        "every predecessor preserves callback source"
                    );
                }
            }
        }
    }
    assert!(
        known.contains(&actual),
        "actual initializer is transported original callback"
    );
    assert_eq!(
        function.entity(original).unwrap().ty,
        function.entity(actual).unwrap().ty
    );
}
