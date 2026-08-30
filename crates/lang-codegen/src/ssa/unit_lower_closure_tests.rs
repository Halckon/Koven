use lang_frontend::{
    lexer::lex, name_resolution::SourceUnitInput, parser::parse_file, source::SourceMap,
    type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{ClosureCaptureMode, Function, Operation, SsaTypeKind},
    render::render_program,
    unit_lower::lower_scalar_unit_with_entry,
    unit_lower_test_support::{analyze, declaration, parsed},
};

#[test]
fn lowers_no_capture_lambdas_as_deterministic_function_pointers() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun observe(): Unit {}\n\
         fun create(): Unit {\n\
             val action: () -> Unit = { observe() }\n\
             val moved = action\n\
             val first = moved()\n\
             val second = moved()\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val action: move () -> Unit = move { p.observe() }\n\
             val invoked = action()\n\
             val created = p.create()\n\
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
    .expect("no-capture lambdas lower as function pointers");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("input permutation preserves function-pointer identities");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    assert_eq!(
        module
            .types
            .iter()
            .filter(|kind| matches!(kind, SsaTypeKind::FunctionPointer { .. }))
            .count(),
        1,
        "the zero-parameter Unit signature has one canonical pointer type"
    );
    assert!(
        !module
            .types
            .iter()
            .any(|kind| matches!(kind, SsaTypeKind::ConcreteClosure { .. }))
    );
    let create = function(module, "p.create");
    assert_eq!(operation_count(create, is_function_address), 1);
    assert_eq!(operation_count(create, is_callable_invoke), 2);
    assert_eq!(operation_count(create, is_drop), 1);
    let entry = function(module, "q.entry");
    assert_eq!(operation_count(entry, is_function_address), 1);
    assert_eq!(operation_count(entry, is_callable_invoke), 1);
    assert_eq!(operation_count(entry, is_drop), 1);
    assert_eq!(operation_count(entry, is_closure_construct), 0);
    let thunks = module
        .functions
        .iter()
        .filter(|function| function.name.contains(".thunk"))
        .collect::<Vec<_>>();
    assert_eq!(thunks.len(), 2);
    assert!(thunks.iter().all(|thunk| {
        thunk
            .entry_block()
            .and_then(|entry| thunk.block(entry))
            .is_some_and(|entry| entry.parameters.is_empty())
    }));
    assert_eq!(
        thunks
            .iter()
            .map(|function| operation_count(function, is_direct_call))
            .sum::<usize>(),
        2
    );
}

#[test]
fn lowers_cross_file_owned_move_closure_and_thunk_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun inspectText(message: String): Unit {}\n\
         fun inspectInt(number: Int): Unit {}\n\
         fun captureCopy(number: Int): Unit {\n\
             val action: move () -> Unit = move { val read = inspectInt(number) }\n\
             val invoked = action()\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val message = \"owned\"\n\
             val count = 7\n\
             val action: move () -> Unit = move {\n\
                 val first = p.inspectText(message)\n\
                 val second = p.inspectInt(count)\n\
             }\n\
             val moved = action\n\
             val firstRun = moved()\n\
             val secondRun = moved()\n\
             val copiedCapture = p.captureCopy(9)\n\
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
    .expect("owned move closure lowers to verified unit SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("input permutation preserves closure identities");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let closures = module
        .types
        .iter()
        .filter_map(|kind| match kind {
            SsaTypeKind::ConcreteClosure { captures, .. } => Some(captures),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(closures.len(), 2);
    assert!(closures.iter().any(|captures| captures.len() == 2));
    assert!(closures.iter().any(|captures| captures.len() == 1));
    assert!(
        closures
            .iter()
            .flat_map(|captures| captures.iter())
            .all(|capture| { capture.mode == ClosureCaptureMode::Owned })
    );

    let entry = function(module, "q.entry");
    assert_eq!(operation_count(entry, is_closure_construct), 1);
    assert_eq!(operation_count(entry, is_callable_invoke), 2);
    assert_eq!(operation_count(entry, is_drop), 1);
    let capture_copy = function(module, "p.captureCopy");
    assert_eq!(operation_count(capture_copy, is_closure_construct), 1);
    assert_eq!(operation_count(capture_copy, is_callable_invoke), 1);
    let thunks = module
        .functions
        .iter()
        .filter(|function| function.name.contains(".thunk"))
        .collect::<Vec<_>>();
    assert_eq!(thunks.len(), 2);
    assert_eq!(
        thunks
            .iter()
            .map(|function| operation_count(function, is_shared_field_loan))
            .sum::<usize>(),
        3
    );
    assert_eq!(
        thunks
            .iter()
            .map(|function| operation_count(function, is_direct_call))
            .sum::<usize>(),
        3
    );
}

#[test]
fn restores_owned_closure_provenance_across_control_flow() {
    let mut sources = SourceMap::new();
    let (source_id, parsed) = parsed(
        &mut sources,
        "test/branch.ko",
        "package test\n\
         fun inspect(message: String): Unit {}\n\
         fun branch(flag: Boolean): Unit {\n\
             val message = \"branch\"\n\
             val action: move () -> Unit = move { val read = inspect(message) }\n\
             if (flag) { val first = action() } else { val second = action() }\n\
         }\n\
         fun loopDrop(flag: Boolean): Unit {\n\
             val message = \"loop\"\n\
             val action: move () -> Unit = move { val read = inspect(message) }\n\
             while (flag) {}\n\
         }\n\
         fun allBreaksConsume(): Unit {\n\
             val message = \"break\"\n\
             val action: move () -> Unit = move { val read = inspect(message) }\n\
             loop {\n\
                 val moved = action\n\
                 break\n\
             }\n\
         }\n\
         fun entry(): Unit {\n\
             val branched = branch(true)\n\
             val looped = loopDrop(false)\n\
             val consumed = allBreaksConsume()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/branch.ko",
        source_id,
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
    .expect("each branch restores and consumes the same closure provenance");

    let branch = function(&program.modules[0], "test.branch");
    assert_eq!(operation_count(branch, is_callable_invoke), 2);
    assert_eq!(operation_count(branch, is_drop), 2);
    let loop_drop = function(&program.modules[0], "test.loopDrop");
    assert_eq!(operation_count(loop_drop, is_closure_construct), 1);
    assert_eq!(operation_count(loop_drop, is_drop), 1);
    let all_breaks = function(&program.modules[0], "test.allBreaksConsume");
    assert_eq!(operation_count(all_breaks, is_closure_construct), 1);
    assert_eq!(operation_count(all_breaks, is_drop), 1);
}

#[test]
fn unsupported_closure_surfaces_remain_atomic_boundaries() {
    for (path, source) in [
        (
            "test/borrowed.ko",
            "package test\n\
             fun inspect(message: String): Unit {}\n\
             fun entry(): Unit {\n\
                 val message = \"borrowed\"\n\
                 val action: () -> Unit = { -> val read = inspect(message) }\n\
                 val invoked = action()\n\
             }",
        ),
        (
            "test/body-owner.ko",
            "package test\n\
             fun inspect(message: String): Unit {}\n\
             fun entry(): Unit {\n\
                 val captured = \"capture\"\n\
                 val action: move () -> Unit = move {\n\
                     val local = \"inside\"\n\
                     val read = inspect(captured)\n\
                 }\n\
                 val invoked = action()\n\
             }",
        ),
        (
            "test/temporary.ko",
            "package test\n\
             fun inspect(message: String): Unit {}\n\
             fun consume(own action: move () -> Unit): Unit {}\n\
             fun entry(): Unit {\n\
                 val message = \"temporary\"\n\
                 val consumed = consume(move { inspect(message) })\n\
             }",
        ),
        (
            "test/nested.ko",
            "package test\n\
             fun inspect(message: String): Unit {}\n\
             fun consume(own action: move () -> Unit): Unit {}\n\
             fun entry(): Unit {\n\
                 val message = \"nested\"\n\
                 val action: move () -> Unit = move {\n\
                     val consumed = consume(move { inspect(message) })\n\
                 }\n\
             }",
        ),
        (
            "test/parameter.ko",
            "package test\n\
             fun inspect(message: String): Unit {}\n\
             fun entry(): Unit {\n\
                 val message = \"parameter\"\n\
                 val action: move (borrow Int) -> Unit = move { item -> inspect(message) }\n\
             }",
        ),
        (
            "test/return.ko",
            "package test\n\
             fun entry(): Unit {\n\
                 val number = 1\n\
                 val action: move () -> Int = move { number }\n\
             }",
        ),
    ] {
        let mut sources = SourceMap::new();
        let source_id = sources.add_source(path, source).expect("unique source");
        let lexed = lex(&sources, source_id).expect("lexing succeeds internally");
        let parsed = parse_file(&sources, &lexed).expect("parsing succeeds internally");
        assert!(
            parsed.diagnostics().is_empty(),
            "{path}: {:?}",
            parsed.diagnostics()
        );
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
            Ok(_) => panic!("unsupported closure surface must fail before publishing a program"),
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

fn is_closure_construct(operation: &Operation) -> bool {
    matches!(operation, Operation::ClosureConstruct { .. })
}

fn is_function_address(operation: &Operation) -> bool {
    matches!(operation, Operation::FunctionAddress { .. })
}

fn is_callable_invoke(operation: &Operation) -> bool {
    matches!(operation, Operation::CallableInvoke { .. })
}

fn is_shared_field_loan(operation: &Operation) -> bool {
    matches!(operation, Operation::SharedFieldLoan { .. })
}

fn is_direct_call(operation: &Operation) -> bool {
    matches!(operation, Operation::DirectCall { .. })
}

fn is_drop(operation: &Operation) -> bool {
    matches!(operation, Operation::Drop { .. })
}
