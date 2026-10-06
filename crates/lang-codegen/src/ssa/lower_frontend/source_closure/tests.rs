//! Source behavior beyond the public matrix: ordering, callback reuse and iteration ASAP.

use crate::ssa::{
    lower_frontend::{LoweringErrorKind, orchestrate::lower_scalar_file},
    model::{Operation, Program, SsaTypeKind, TerminatorKind},
};
use lang_frontend::{
    lexer::lex,
    name_resolution::{NameResolution, resolve_names},
    ownership_checking::{LoanTarget, OwnershipCheckedFile, check_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{DeferredReason, TypeKind, TypedFile, check_types, standard_environments},
};

fn analyze<T>(
    text: &str,
    check: impl FnOnce(&SourceMap, &ParsedFile, &NameResolution, &TypedFile, &OwnershipCheckedFile) -> T,
) -> T {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("single-runtime-intent.ko", text)
        .expect("source");
    let lexed = lex(&sources, source).expect("lexer");
    let parsed = parse_file(&sources, &lexed).expect("parser");
    let (environment, types) = standard_environments();
    let names = resolve_names(&sources, &parsed, &environment).expect("names");
    let typed = check_types(&sources, &parsed, &names, &types).expect("types");
    let owned = check_ownership(&sources, &parsed, &names, &typed).expect("ownership");
    assert!(
        parsed.diagnostics().is_empty(),
        "{text}: {:?}",
        parsed.diagnostics()
    );
    assert!(
        names.diagnostics().is_empty(),
        "{text}: {:?}",
        names.diagnostics()
    );
    assert!(
        typed.diagnostics().is_empty(),
        "{text}: {:?}",
        typed.diagnostics()
    );
    assert!(
        owned.diagnostics().is_empty(),
        "{text}: {:?}",
        owned.diagnostics()
    );
    assert!(owned.is_compatible_with(&names, &typed));
    check(&sources, &parsed, &names, &typed, &owned)
}

fn lower(text: &str) -> Program {
    analyze(text, |sources, parsed, names, typed, owned| {
        let arena = typed.types().len();
        let program = lower_scalar_file(sources, parsed, names, typed, owned)
            .expect("source must reach verified SSA");
        assert_eq!(
            typed.types().len(),
            arena,
            "backend only queries canonical types"
        );
        crate::llvm::render_verified_program(&program).expect("source must reach verified LLVM");
        program
    })
}

#[test]
fn single_runtime_nothing_operand_stops_before_borrow_and_initializer() {
    let mut failures = Vec::new();
    for container in ["Array", "List"] {
        for (operands, size_loans) in [
            ("error(\"size\"), { index -> index }", 0),
            ("1, error(\"initializer\")", 1),
        ] {
            let text = format!(
                "fun entry(): Int {{ val unused = {container}<Int>({operands})\nreturn 0 }}"
            );
            analyze(&text, |sources, parsed, names, typed, owned| {
                let constructors = typed.container_constructions();
                assert_eq!(constructors.len(), 1);
                assert_eq!(
                    owned
                        .loans()
                        .iter()
                        .filter(|loan| loan.call() == constructors[0].expression())
                        .count(),
                    size_loans
                );
                let arena = typed.types().len();
                match lower_scalar_file(sources, parsed, names, typed, owned) {
                    Ok(program) => {
                        assert!(
                            program.modules[0]
                                .functions
                                .iter()
                                .flat_map(|function| &function.instructions)
                                .all(|instruction| !matches!(
                                    instruction.operation,
                                    Operation::ContainerGenerateBorrowed { .. }
                                ))
                        );
                        assert!(
                            program.modules[0]
                                .functions
                                .iter()
                                .flat_map(|function| &function.blocks)
                                .any(|block| block.terminator.as_ref().is_some_and(
                                    |terminator| matches!(terminator.kind, TerminatorKind::Abort)
                                ))
                        );
                        crate::llvm::render_verified_program(&program)
                            .expect("ordinary source Abort verifies through LLVM");
                    }
                    Err(error) => failures.push(format!("{container}/{operands}: {error:?}")),
                }
                assert_eq!(typed.types().len(), arena);
            });
        }
    }
    assert!(
        failures.is_empty(),
        "Nothing must stop before creating later operand facts: {failures:?}"
    );
}

/// A clean diagnostic list does not select an unresolved source callable for its ABI.
fn assert_deferred_boundary(
    text: &str,
    operand: &str,
    reason: DeferredReason,
    temporary_loan: bool,
) {
    analyze(text, |sources, parsed, names, typed, owned| {
        let expressions = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| &text[node.span().start()..node.span().end()] == operand)
            .collect::<Vec<_>>();
        assert_eq!(expressions.len(), 1, "one exact deferred operand");
        let (expression, node) = expressions[0];
        assert_eq!(
            typed
                .expression_type(expression)
                .and_then(|ty| typed.types().get(ty)),
            Some(&TypeKind::Deferred(reason))
        );
        assert!(
            owned.callable_origin(expression).is_none(),
            "no selected source/environment fact"
        );
        if temporary_loan {
            assert!(
                matches!(owned.loan_begin(expression).map(|fact| fact.target()),
                Some(LoanTarget::Temporary(origin)) if *origin == expression),
                "the real frontend temporary loan is retained, not fabricated or discarded"
            );
        } else {
            assert!(owned.loan_begin(expression).is_none());
        }
        let arena = typed.types().len();
        let failure = match lower_scalar_file(sources, parsed, names, typed, owned) {
            Err(failure) => failure,
            Ok(_) => panic!("deferred source facts cannot select a concrete callable ABI"),
        };
        assert_eq!(failure.kind, LoweringErrorKind::UnsupportedNode);
        assert_eq!(failure.span, Some(node.span()));
        assert_eq!(
            typed.types().len(),
            arena,
            "backend must not resolve deferred types"
        );
    });
}

#[test]
fn single_runtime_guard_keeps_size_loan_live_before_initializer_factory() {
    let program = lower(
        "fun factory(): (Int)->Int { println(\"factory\")\nreturn ({ index -> index }) }\nfun entry(size: Int): Int { val items = List<Int>(size, factory())\nreturn items[0] }",
    );
    let module = &program.modules[0];
    let factory = module
        .functions
        .iter()
        .find(|function| function.name == "factory")
        .unwrap();
    let entry = module
        .functions
        .iter()
        .find(|function| function.name == "entry")
        .unwrap();
    let guard = entry.block(entry.entry_block().unwrap()).unwrap();
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &guard.terminator.as_ref().unwrap().kind
    else {
        panic!("source guard must precede initializer evaluation");
    };
    assert!(matches!(
        entry
            .block(when_true.target)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind,
        TerminatorKind::Abort
    ));
    assert!(guard.instructions.iter().all(|instruction| !matches!(
        entry.instruction(*instruction).unwrap().operation,
        Operation::DirectCall { .. } | Operation::ContainerGenerateBorrowed { .. }
    )));
    let success = entry.block(when_false.target).unwrap();
    let factory_index = success.instructions.iter().position(|instruction| matches!(entry.instruction(*instruction).unwrap().operation, Operation::DirectCall { callee, .. } if callee == factory.id)).unwrap();
    let generation_index = success
        .instructions
        .iter()
        .position(|instruction| {
            matches!(
                entry.instruction(*instruction).unwrap().operation,
                Operation::ContainerGenerateBorrowed { .. }
            )
        })
        .unwrap();
    assert!(
        factory_index < generation_index,
        "one factory evaluation occurs only on nonnegative path before generation"
    );
}

#[test]
fn single_runtime_named_shared_callback_survives_constructor_and_arithmetic_cfg() {
    let program = lower(
        "fun entry(): Int { val scale = 7\nval callback: (Int)->Int = { index -> index + scale }\nval first = Array<Int>(1, callback)\nval length = first.size + 2\nval second = List<Int>(length, callback)\nreturn second[2] }",
    );
    let module = &program.modules[0];
    let entry = module
        .functions
        .iter()
        .find(|function| function.name == "entry")
        .unwrap();
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::ContainerGenerateBorrowed { .. }
            ))
            .count(),
        2
    );
    let closure_drops = entry
        .instructions
        .iter()
        .filter(|instruction| match instruction.operation {
            Operation::Drop { owner } => match entry
                .entity(crate::ssa::model::EntityId::Value(owner))
                .unwrap()
                .ty
            {
                crate::ssa::model::EntityType::Value(ty) => matches!(
                    module.type_kind(ty),
                    Some(SsaTypeKind::ConcreteClosure { .. })
                ),
                _ => false,
            },
            _ => false,
        })
        .count();
    assert_eq!(
        closure_drops, 1,
        "constructor only ends its argument loan; named callback owns its environment until last use"
    );
}

#[test]
fn single_runtime_temporary_shared_callback_ends_capture_before_iteration_element() {
    let program = lower(
        "fun entry(): Int { val source = arrayOf<Int>(7)\nvar result = 0\nfor (element in source) { val items = List<Int>(1, { index -> index + element })\nresult += items[0] }\nreturn result }",
    );
    let entry = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name == "entry")
        .unwrap();
    assert_eq!(
        entry
            .instructions
            .iter()
            .filter(|instruction| matches!(
                instruction.operation,
                Operation::ContainerGenerateBorrowed { .. }
            ))
            .count(),
        1
    );
    let formation = entry
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::ClosureConstruct { .. }))
        .unwrap();
    let Operation::ClosureConstruct { captures, .. } = &formation.operation else {
        unreachable!()
    };
    let [crate::ssa::model::ClosureCaptureOperand::Shared(capture)] = captures.as_slice() else {
        panic!("one real shared element capture")
    };
    let crate::ssa::model::EntityId::Value(callback) = formation.results[0] else {
        panic!("callback owner")
    };
    let drop_index = entry.instructions.iter().position(|instruction| matches!(instruction.operation, Operation::Drop { owner } if owner == callback)).unwrap();
    assert!(entry.instructions.iter().all(|instruction| !matches!(instruction.operation, Operation::BorrowEnd { loan } if loan == *capture)), "SSA closure Drop already releases the capture; the source bridge must not end it twice");
    assert!(
        drop_index > formation.id.index(),
        "the temporary owner reaches the constructor before its ASAP drop"
    );
}

#[test]
fn single_runtime_deferred_named_initializer_cannot_select_source_identity() {
    assert_deferred_boundary(
        "fun identity(index: Int): Int = index\nfun entry(): Int { val items = List<Int>(3, identity)\nreturn items[2] }",
        "identity",
        DeferredReason::OverloadSelection,
        true,
    );
}

#[test]
fn single_runtime_deferred_named_helper_with_inner_break_never_selects_slot() {
    let call = "helper(identity, if (true) { while (true) { break }\n3 } else { 0 })";
    let text = format!(
        "fun identity(index: Int): Int = index\nfun helper(callback: (Int)->Int, count: Int): Int {{ val items = List<Int>(count, callback)\nreturn items[2] }}\nfun entry(): Int = {call}"
    );
    assert_deferred_boundary(&text, call, DeferredReason::Call, false);
}

#[test]
fn single_runtime_deferred_named_helper_with_inner_continue_never_selects_slot() {
    let call = "helper(identity, if (true) { var step = 0\nwhile (step < 1) { step += 1\ncontinue }\n3 } else { 0 })";
    let text = format!(
        "fun identity(index: Int): Int = index\nfun helper(callback: (Int)->Int, count: Int): Int {{ val items = List<Int>(count, callback)\nreturn items[2] }}\nfun entry(): Int = {call}"
    );
    assert_deferred_boundary(&text, call, DeferredReason::Call, false);
}

#[test]
fn single_runtime_generic_helper_preserves_nominal_element_and_three_environments() {
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
            analyze(&text, |sources, parsed, names, typed, owned| {
                let arena = typed.types().len();
                match lower_scalar_file(sources, parsed, names, typed, owned) {
                    Ok(program) => {
                        let module = &program.modules[0];
                        let generate = module
                            .functions
                            .iter()
                            .find(|function| function.name.starts_with("generate<"))
                            .expect("one actual nominal helper instance");
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
                            initializer,
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
                            .entity(crate::ssa::model::EntityId::Loan(initializer))
                            .unwrap()
                            .ty
                        else {
                            panic!("borrowed initializer ABI")
                        };
                        match (environment, module.type_kind(target).unwrap()) {
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
                    typed.types().len(),
                    arena,
                    "the backend must only read canonical nominal and function types"
                );
            });
        }
    }
    assert!(
        failures.is_empty(),
        "nominal generic helper must preserve each selected callback ABI: {failures:?}"
    );
}

#[test]
fn single_runtime_named_shared_callback_survives_when_sibling_edges() {
    let mut failures = Vec::new();
    for branches in [
        "flag -> println(\"then\")\nelse -> println(\"else\")",
        "flag, other -> println(\"then\")\nelse -> println(\"else\")",
        "flag -> println(\"then\")",
        "flag -> println(\"then\")\nother -> println(\"second\")\nelse -> println(\"else\")",
    ] {
        let text = format!(
            "fun entry(flag: Boolean, other: Boolean): Int {{ val scale = 7\n\
             val callback: (Int)->Int = {{ index -> index + scale }}\n\
             when {{ {branches} }}\nval items = List<Int>(3, callback)\nreturn items[2] }}"
        );
        analyze(&text, |sources, parsed, names, typed, owned| {
            let before = typed.types().len();
            match lower_scalar_file(sources, parsed, names, typed, owned) {
                Ok(program) => {
                    crate::llvm::render_verified_program(&program)
                        .expect("when siblings retain valid capture identities");
                }
                Err(error) => failures.push(format!("{branches}: {error:?}")),
            }
            assert_eq!(typed.types().len(), before);
        });
    }
    assert!(
        failures.is_empty(),
        "every when sibling must transport its own active Shared capture loan: {failures:?}"
    );
}

#[test]
fn single_runtime_unreachable_lambdas_do_not_register_storage_or_nested_layouts() {
    for body in [
        "val unused: ()->Int = { val value: Int? = 1\nvalue!! }",
        "val unused: ()->Unit = { val nested: ()->Unit = {} }",
    ] {
        let text = format!("fun entry(): Int {{ return 0\n{body} }}");
        analyze(&text, |sources, parsed, names, typed, owned| {
            for (expression, node) in parsed.ast().expressions().iter() {
                if matches!(
                    node.payload(),
                    lang_frontend::parser::Expression::Lambda { .. }
                ) {
                    assert!(
                        owned.closure(expression).is_some(),
                        "static capture analysis still describes the AST lambda"
                    );
                    assert!(
                        owned.callable_origin(expression).is_none(),
                        "Phase 3 never evaluated this lambda after return"
                    );
                }
            }
            let before = typed.types().len();
            let program = lower_scalar_file(sources, parsed, names, typed, owned)
                .expect("unreachable lambda metadata cannot demand storage or nested layouts");
            assert_eq!(
                program.modules[0].functions.len(),
                1,
                "only entry runs; no lambda thunk is declared"
            );
            assert_eq!(typed.types().len(), before);
            crate::llvm::render_verified_program(&program)
                .expect("reachable entry verifies through LLVM");
        });
    }
}

#[test]
fn single_runtime_reachable_nested_lambda_remains_a_precise_boundary() {
    let text = "fun entry(): Unit { val callback: ()->Unit = { val nested: ()->Unit = {}\nnested() }\ncallback() }";
    analyze(text, |sources, parsed, names, typed, owned| {
        let lambdas = parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| {
                matches!(
                    node.payload(),
                    lang_frontend::parser::Expression::Lambda { .. }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(lambdas.len(), 2);
        assert!(lambdas.iter().all(|(expression, _)| {
            matches!(
                owned.callable_origin(*expression).map(|fact| fact.origin()),
                Some(lang_frontend::ownership_checking::CallableOrigin::Lambda(origin))
                    if origin == *expression
            )
        }));
        let failure = match lower_scalar_file(sources, parsed, names, typed, owned) {
            Ok(_) => panic!("evaluated nested closure layouts remain outside the Single slice"),
            Err(error) => error,
        };
        assert_eq!(failure.kind, LoweringErrorKind::UnsupportedNode);
        assert!(
            lambdas
                .iter()
                .any(|(_, node)| Some(node.span()) == failure.span)
        );
    });
}
