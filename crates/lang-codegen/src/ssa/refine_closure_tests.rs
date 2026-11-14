//! SPEC-0364, SPEC-0365, SPEC-0370 closures in the refinement layer (ADR-0034): Borrow
//! function parameters are callable views, lambda literals and bodies (thunks) lower in
//! `ssa::refine`, and local callable bindings call via `CallableInvoke` or lend as views.
use lang_frontend::{
    name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
};

use super::{
    model::{Operation, Program},
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

/// Lowers `source` (package `p`, entry `main`) and returns the program and the functions
/// the refinement layer lowered.
fn try_lower(
    source: &str,
) -> Result<(Program, Vec<String>, crate::ssa::model::FunctionId), crate::ssa::LoweringError> {
    let (lowered, routed) = super::refine::observe_routing(|| {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let inputs = [SourceUnitInput::new("root", "p/main.ko", id, &file)];
        let (name_environment, type_environment) = standard_environments();
        let (names, typed, owned) =
            analyze(&sources, &inputs, &name_environment, &type_environment);
        let entry = declaration(&names, "p", "main");
        lower_scalar_unit_with_entry(
            &sources,
            &inputs,
            &names,
            &type_environment,
            &typed,
            &owned,
            entry,
        )
    });
    let (program, entry) = lowered?;
    Ok((program, routed, entry))
}

fn lower(source: &str) -> (Program, Vec<String>, crate::ssa::model::FunctionId) {
    try_lower(source).expect("closures lower and verify")
}

fn run_counted(
    program: &Program,
    entry: crate::ssa::model::FunctionId,
    allocations: usize,
    stdout: &[u8],
) {
    let llvm =
        crate::llvm::render_verified_program_with_entry(program, entry).expect("verified LLVM");
    let output = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, allocations);
    crate::native_tests::boxed_enum_tests::assert_success(&output, stdout);
}

#[test]
fn lambda_arguments_reach_callees_as_views() {
    let source = r#"class Data(val v: Int)
    fun apply(f: (Int) -> Int, x: Int): Int = f(x)
    fun twice(f: (Int) -> Int): Int = f(f(1))
    fun relay(f: (Int) -> Int): Int = apply(f, 10)
    fun main(): Unit {
        val k = 3
        if (apply({ n -> n * k }, 7) == 21 && twice({ n -> n + 1 }) == 3) { println("views") }
        if (relay({ n -> n + k }) == 13) { println("forward") }
        val d = Data(5)
        if (apply({ n -> n + d.v }, 1) == 6) { println("shared") }
        val e = Data(9)
        if (apply(move { n -> n + e.v }, 1) == 10) { println("owned") }
    }"#;
    let (program, routed, entry) = lower(source);
    for function in [
        "koven.p.main.d",
        "koven.p.apply.d",
        "koven.p.twice.d",
        "koven.p.relay.d",
    ] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        routed
            .iter()
            .filter(|name| name.ends_with(".thunk"))
            .count(),
        5,
        "all five lambda thunks lower in ssa::refine: {routed:?}"
    );
    let module = &program.modules[0];
    let count = |predicate: fn(&Operation) -> bool| {
        module
            .functions
            .iter()
            .flat_map(|function| &function.instructions)
            .filter(|instruction| predicate(&instruction.operation))
            .count()
    };
    // Five lambda arguments become views; `relay` forwards its view by re-borrowing it.
    assert_eq!(
        count(|op| matches!(op, Operation::CallableViewOf { .. })),
        5
    );
    assert_eq!(
        count(|op| matches!(op, Operation::CallableInvokeView { .. })),
        3
    );
    assert_eq!(
        count(|op| matches!(op, Operation::FunctionAddress { .. })),
        1,
        "only the capture-free lambda is a bare function pointer"
    );
    // `d` and the moved `e` are each freed exactly once.
    run_counted(&program, entry, 2, b"views\nforward\nshared\nowned\n");
}

#[test]
fn a_factory_returns_a_move_closure_with_its_concrete_layout() {
    let source = r#"class Data(val v: Int)
    fun make(own s: String): () -> Boolean = move { s == "x" }
    fun adder(own d: Data): (Int) -> Int = move { n -> n + d.v }
    fun main(): Unit {
        val f = make("x")
        if (f()) { println("move") }
        val g = adder(Data(4))
        if (g(1) == 5) { println("data") }
    }"#;
    let (program, routed, entry) = lower(source);
    for function in ["koven.p.main.d", "koven.p.make.d", "koven.p.adder.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        routed
            .iter()
            .filter(|name| name.ends_with(".thunk"))
            .count(),
        2,
        "both factory thunks lower in ssa::refine: {routed:?}"
    );
    let module = &program.modules[0];
    for name in ["koven.p.make.d", "koven.p.adder.d"] {
        let function = module
            .functions
            .iter()
            .find(|function| function.name.starts_with(name))
            .expect("factory");
        assert!(
            matches!(
                function.return_types.as_slice(),
                [ty] if module.concrete_closure(*ty).is_some()
            ),
            "{name} returns its concrete closure"
        );
    }
    // String literals do not allocate; the Data captured by `g` is freed exactly once, with
    // its closure.
    run_counted(&program, entry, 1, b"move\ndata\n");
}

#[test]
fn local_callable_bindings_and_views_lower_in_refine() {
    let source = r#"class Data(val v: Int)
    fun apply(f: (Int) -> Int, x: Int): Int = f(x)
    fun main(): Unit {
        val d = Data(6)
        val local: (Int) -> Int = { n -> n + d.v }
        if (local(4) == 10 && apply(local, 9) == 15) { println("local") }
        val e = Data(8)
        val owned: (Int) -> Int = move { n -> n + e.v }
        val moved = owned
        if (moved(2) == 10 && apply(moved, 7) == 15) { println("moved") }
    }"#;
    let (program, routed, entry) = lower(source);
    for function in ["koven.p.main.d", "koven.p.apply.d"] {
        assert!(
            routed.iter().any(|name| name.starts_with(function)),
            "{function}: {routed:?}"
        );
    }
    assert_eq!(
        routed
            .iter()
            .filter(|name| name.ends_with(".thunk"))
            .count(),
        2,
        "both local callable thunks lower in ssa::refine: {routed:?}"
    );
    run_counted(&program, entry, 2, b"local\nmoved\n");
}

#[test]
fn a_thunk_keeps_the_gate_for_its_own_while_and_named_iteration() {
    let routable = r#"fun main(): Unit {
                val run: () -> Unit = {
                    val xs = listOf("kept")
                    for (item in xs) { println(item) }
                }
                run()
            }"#;
    let (program, routed, _) = lower(routable);
    assert!(
        routed.iter().any(|name| name.ends_with(".thunk")),
        "a named sequential for routes its thunk to ssa::refine: {routed:?}"
    );
    crate::ssa::verify::verify_program(&program).expect("the callable and iteration verify");

    // SPEC-0397 retires the capture-free thunk fallback: a thunk whose own `while`
    // keeps it off the refinement layer is now an explicit unsupported boundary.
    let rejected = r#"fun main(): Unit {
                val run: () -> Unit = {
                    while (false) {}
                    val xs = listOf("kept")
                    for (item in xs) { println(item) }
                }
                run()
            }"#;
    let error = match try_lower(rejected) {
        Err(error) => error,
        Ok(_) => panic!("no legacy thunk fallback remains"),
    };
    assert_eq!(
        error.kind,
        crate::ssa::LoweringErrorKind::UnsupportedNode,
        "{error:?}"
    );
    assert!(error.span.is_some(), "{error:?}");
}

mod source_closure_tests {
    //! Source behavior beyond the public matrix: ordering, callback reuse and iteration ASAP.

    use crate::ssa::{
        lower_frontend::{LoweringErrorKind, orchestrate::lower_scalar_file},
        model::{Operation, Program, SsaTypeKind, TerminatorKind},
    };
    use crate::test_support::diagnostics::assert_diagnostic;
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
        check: impl FnOnce(
            &SourceMap,
            &ParsedFile,
            &NameResolution,
            &TypedFile,
            &OwnershipCheckedFile,
        ) -> T,
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
                                        |terminator| matches!(
                                            terminator.kind,
                                            TerminatorKind::Abort
                                        )
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
        assert_diagnostic!(
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
        use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
        use crate::ssa::model::Origin;
        use lang_frontend::name_resolution::SymbolKind;

        let text = "fun factory(): (Int)->Int { println(\"factory\")\nreturn ({ index -> index }) }\nfun entry(size: Int): Int { val items = List<Int>(size, factory())\nreturn items[0] }";
        let unit = UnitSource::analyze("single-runtime-intent.ko", text).unwrap();
        unit.assert_frontend_clean();
        let entry_declaration = unit.declaration("entry", SymbolKind::Function);
        let factory_declaration = unit.declaration("factory", SymbolKind::Function);
        let arena = unit.types().types().len();
        let (program, root) = unit.lower_entry(entry_declaration).unwrap();
        assert_eq!(
            unit.types().types().len(),
            arena,
            "backend only queries canonical types"
        );
        crate::llvm::render_verified_program(&program).unwrap();
        let module = program.module(root.module()).unwrap();
        let factory = module
            .functions
            .iter()
            .find(|function| {
                function.origin == Origin::Source(unit.declaration_span(factory_declaration))
            })
            .expect("original factory identity is selected");
        let entry = module.function(root).unwrap();
        assert_eq!(
            entry.origin,
            Origin::Source(unit.declaration_span(entry_declaration))
        );
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

    fn lower_unit_entry(text: &str) -> (Program, crate::ssa::model::FunctionId) {
        use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
        use lang_frontend::name_resolution::SymbolKind;

        let unit = UnitSource::analyze("single-runtime-intent.ko", text).unwrap();
        unit.assert_frontend_clean();
        let declaration = unit.declaration("entry", SymbolKind::Function);
        let arena = unit.types().types().len();
        let (program, root) = unit.lower_entry(declaration).unwrap();
        assert_eq!(
            unit.types().types().len(),
            arena,
            "backend only queries canonical types"
        );
        assert_eq!(
            program
                .module(root.module())
                .unwrap()
                .function(root)
                .unwrap()
                .origin,
            crate::ssa::model::Origin::Source(unit.declaration_span(declaration))
        );
        crate::llvm::render_verified_program(&program).unwrap();
        (program, root)
    }

    #[test]
    fn single_runtime_named_shared_callback_survives_constructor_and_arithmetic_cfg() {
        let (program, root) = lower_unit_entry(
            "fun entry(): Int { val scale = 7\nval callback: (Int)->Int = { index -> index + scale }\nval first = Array<Int>(1, callback)\nval length = first.size + 2\nval second = List<Int>(length, callback)\nreturn second[2] }",
        );
        let module = program.module(root.module()).unwrap();
        let entry = module.function(root).unwrap();
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
        let (program, root) = lower_unit_entry(
            "fun entry(): Int { val source = arrayOf<Int>(7)\nvar result = 0\nfor (element in source) { val items = List<Int>(1, { index -> index + element })\nresult += items[0] }\nreturn result }",
        );
        let entry = program
            .module(root.module())
            .unwrap()
            .function(root)
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
        let [crate::ssa::model::ClosureCaptureOperand::Shared(capture)] = captures.as_slice()
        else {
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
                        Err(error) => {
                            failures.push(format!("{container}/{environment}: {error:?}"))
                        }
                    }
                    assert_eq!(
                        typed.types().len(),
                        arena,
                        "the backend must only read canonical nominal and function types"
                    );
                });
            }
        }
        assert_diagnostic!(
            failures.is_empty(),
            "nominal generic helper must preserve each selected callback ABI: {failures:?}"
        );
    }

    #[test]
    fn single_runtime_named_shared_callback_survives_when_sibling_edges() {
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
            // Strict Unit preserves the original frontend/arena/LLVM checks.
            let _ = lower_unit_entry(&text);
        }
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
}

// Keep the legacy static-capture/callable-origin witness above: Unit has a different
// fact model. This ordinary-source control checks only the production Unit outcome.
#[test]
fn unit_unreachable_lambdas_do_not_register_storage_or_nested_layouts() {
    use super::model::{Origin, SsaTypeKind};
    use crate::native_tests::boxed_enum_tests::unit_source::UnitSource;
    use lang_frontend::name_resolution::SymbolKind;

    for body in [
        "val unused: ()->Int = { val value: Int? = 1\nvalue!! }",
        "val unused: ()->Unit = { val nested: ()->Unit = {} }",
    ] {
        let text = format!("fun entry(): Int {{ return 0\n{body} }}");
        let unit = UnitSource::analyze("single-runtime-intent.ko", &text)
            .expect("original unreachable lambda source passes strict Unit admission");
        unit.assert_frontend_clean();
        let declaration = unit.declaration("entry", SymbolKind::Function);
        let (program, root) = unit.lower_entry(declaration).unwrap();
        super::verify::verify_program(&program).unwrap();
        let module = program.module(root.module()).unwrap();
        assert_eq!(module.functions.len(), 1, "no unreachable lambda thunk");
        let entry = module.function(root).unwrap();
        assert_eq!(
            entry.origin,
            Origin::Source(unit.declaration_span(declaration))
        );
        assert!(
            !module
                .types
                .iter()
                .any(|ty| matches!(ty, SsaTypeKind::ConcreteClosure { .. }))
        );
        assert!(!entry.instructions.iter().any(|instruction| matches!(
            instruction.operation,
            Operation::ClosureConstruct { .. }
                | Operation::CallableInvoke { .. }
                | Operation::CallableInvokeView { .. }
                | Operation::CallableViewOf { .. }
        )));
        crate::llvm::render_verified_program(&program)
            .expect("reachable Unit entry verifies through LLVM");
    }
}
