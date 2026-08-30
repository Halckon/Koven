use lang_frontend::{
    lexer::lex, name_resolution::SourceUnitInput, parser::parse_file, source::SourceMap,
    type_checking::standard_environments,
};

use super::{
    LoweringErrorKind,
    model::{
        ClosureCaptureMode, EntityId, EntityType, Function, LoanKind, Operation, SsaTypeKind,
        TerminatorKind, ValueId,
    },
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
fn lowers_copyable_callable_parameters_and_results_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun combine(seed: Int, own delta: Int): Int {\n\
             val action: (Int, own Int) -> Int = { left, right -> left + right }\n\
             return action(seed, delta)\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val offset = 2\n\
             val action: move (borrow Int) -> Int = move { item -> item + offset }\n\
             val first = action(40)\n\
             val second = action(first)\n\
             val nestedBreak = p.combine(if (true) {\n\
                 loop { break }\n\
                 41\n\
             } else { 0 }, 1)\n\
             val nestedContinue = p.combine(if (true) {\n\
                 while (false) { continue }\n\
                 42\n\
             } else { 0 }, 1)\n\
             val combined = p.combine(second + nestedBreak + nestedContinue, 1)\n\
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
    .expect("Copyable callable parameters and results lower to verified unit SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("input permutation preserves parameterized callable identities");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let pointer_signatures = module
        .types
        .iter()
        .filter_map(|kind| match kind {
            SsaTypeKind::FunctionPointer { signature } => Some(signature),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(pointer_signatures.len(), 1);
    let pointer = pointer_signatures[0];
    assert_eq!(pointer.parameters.len(), 2);
    assert!(matches!(
        pointer.parameters[0],
        EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        }
    ));
    assert!(matches!(pointer.parameters[1], EntityType::Value(_)));
    assert_eq!(pointer.returns.len(), 1);

    let closure_signatures = module
        .types
        .iter()
        .filter_map(|kind| match kind {
            SsaTypeKind::ConcreteClosure {
                signature,
                captures,
                ..
            } => Some((signature, captures)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(closure_signatures.len(), 1);
    let (closure, captures) = closure_signatures[0];
    assert_eq!(closure.parameters.len(), 1);
    assert!(matches!(
        closure.parameters[0],
        EntityType::Loan {
            kind: LoanKind::Shared,
            ..
        }
    ));
    assert_eq!(closure.returns.len(), 1);
    assert_eq!(captures.len(), 1);

    let combine = function(module, "p.combine");
    assert_eq!(operation_count(combine, is_function_address), 1);
    assert_eq!(operation_count(combine, is_callable_invoke), 1);
    let entry = function(module, "q.entry");
    assert_eq!(operation_count(entry, is_closure_construct), 1);
    assert_eq!(operation_count(entry, is_callable_invoke), 2);
    let thunks = module
        .functions
        .iter()
        .filter(|function| function.name.contains(".thunk"))
        .collect::<Vec<_>>();
    assert_eq!(thunks.len(), 2);
    assert!(thunks.iter().all(|thunk| thunk.return_types.len() == 1));
    assert!(thunks.iter().all(|thunk| {
        thunk
            .entry_block()
            .and_then(|entry| thunk.block(entry))
            .is_some_and(|entry| entry.parameters.len() == 2)
    }));
}

#[test]
fn lowers_move_only_value_lambda_parameters_from_exact_drop_facts() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun inspect(message: String): Unit {}\n\
         fun consume(own message: String): Unit {}\n\
         fun exercise(): Unit {\n\
             val unused: move (own String) -> Unit = move { }\n\
             val read: move (own String) -> Unit = move { item -> inspect(item) }\n\
             val consumed: move (own String) -> Unit = move { item -> consume(item) }\n\
             val implicit: move (own String) -> String = move { item -> item }\n\
             val explicit: move (own String) -> String = move { item -> return item }\n\
             val unusedCall = unused(\"unused\")\n\
             val readCall = read(\"read\")\n\
             val consumedCall = consumed(\"consumed\")\n\
             val implicitResult = implicit(\"implicit\")\n\
             val implicitSeen = inspect(implicitResult)\n\
             val explicitResult = explicit(\"explicit\")\n\
             val explicitSeen = inspect(explicitResult)\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val marker = 1\n\
             val captured: move (own String) -> Unit = move { item ->\n\
                 p.inspect(item)\n\
                 val observed = marker\n\
             }\n\
             val invoked = captured(\"captured\")\n\
             val exercised = p.exercise()\n\
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
    .expect("MoveOnly Value lambda parameters lower from exact frontend drop facts");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("input permutation preserves MoveOnly Value parameter lowering");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let exercise = function(module, "p.exercise");
    assert_eq!(operation_count(exercise, is_callable_invoke), 5);
    let entry = function(module, "q.entry");
    assert_eq!(operation_count(entry, is_callable_invoke), 1);
    assert_eq!(operation_count(entry, is_closure_construct), 1);
    let thunks = module
        .functions
        .iter()
        .filter(|function| function.name.contains(".thunk"))
        .collect::<Vec<_>>();
    assert_eq!(thunks.len(), 6);
    assert_eq!(
        thunks
            .iter()
            .map(|thunk| operation_count(thunk, is_drop))
            .sum::<usize>(),
        3,
        "only unused and last-read parameters drop; Value deliveries and returns transfer"
    );
    assert_eq!(
        thunks
            .iter()
            .map(|thunk| operation_count(thunk, is_direct_call))
            .sum::<usize>(),
        3
    );
    assert_eq!(
        thunks
            .iter()
            .filter(|thunk| !thunk.return_types.is_empty())
            .count(),
        2
    );
    assert!(
        thunks
            .iter()
            .filter(|thunk| !thunk.return_types.is_empty())
            .all(|thunk| dropped(thunk).is_empty() && returned(thunk).len() == 1),
        "implicit and explicit returns transfer the parameter owner"
    );
    assert_eq!(
        thunks
            .iter()
            .filter_map(|thunk| thunk.entry_block().and_then(|entry| thunk.block(entry)))
            .filter(|entry| entry.parameters.len() == 2)
            .count(),
        1,
        "the captured thunk keeps environment-first followed by the Value parameter"
    );
}

#[test]
fn lowers_direct_move_only_callable_results_deterministically() {
    let mut sources = SourceMap::new();
    let (provider_source, provider) = parsed(
        &mut sources,
        "p/provider.ko",
        "package p\n\
         fun inspect(message: String): Unit {}\n\
         fun make(number: Int): String = \"captured\"\n\
         fun create(): String {\n\
             val factory: move () -> String = move { \"left\" + \"right\" }\n\
             return factory()\n\
         }\n\
         fun explicit(): String {\n\
             val factory: move () -> String = move {\n\
                 val result = \"explicit\"\n\
                 return result\n\
             }\n\
             return factory()\n\
         }",
    );
    let (consumer_source, consumer) = parsed(
        &mut sources,
        "q/consumer.ko",
        "package q\n\
         fun entry(): Unit {\n\
             val offset = 2\n\
             val action: move (borrow Int) -> String = move { item -> p.make(item + offset) + \"suffix\" }\n\
             val first = action(2)\n\
             val firstSeen = p.inspect(first)\n\
             val second = action(3)\n\
             val secondSeen = p.inspect(second)\n\
             val created = p.create()\n\
             val createdSeen = p.inspect(created)\n\
             val explicit = p.explicit()\n\
             val explicitSeen = p.inspect(explicit)\n\
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
    .expect("direct MoveOnly callable results lower to verified unit SSA");
    let (backward, _) = lower_scalar_unit_with_entry(
        &sources,
        &reversed,
        &reverse_names,
        &type_environment,
        &reverse_typed,
        &reverse_owned,
        declaration(&reverse_names, "q", "entry"),
    )
    .expect("input permutation preserves MoveOnly callable result identities");
    assert_eq!(render_program(&forward), render_program(&backward));

    let module = &forward.modules[0];
    let entry = function(module, "q.entry");
    assert_eq!(operation_count(entry, is_callable_invoke), 2);
    assert_eq!(operation_count(entry, is_drop), 5);
    let create = function(module, "p.create");
    assert_eq!(operation_count(create, is_callable_invoke), 1);
    assert_eq!(operation_count(create, is_drop), 1);
    let explicit = function(module, "p.explicit");
    assert_eq!(operation_count(explicit, is_callable_invoke), 1);
    assert_eq!(operation_count(explicit, is_drop), 1);
    let thunks = module
        .functions
        .iter()
        .filter(|function| function.name.contains(".thunk"))
        .collect::<Vec<_>>();
    assert_eq!(thunks.len(), 3);
    assert!(thunks.iter().all(|thunk| thunk.return_types.len() == 1));
    assert_eq!(
        thunks
            .iter()
            .map(|thunk| operation_count(thunk, is_string_literal))
            .sum::<usize>(),
        4
    );
    assert_eq!(
        thunks
            .iter()
            .map(|thunk| operation_count(thunk, is_direct_call))
            .sum::<usize>(),
        1
    );
    assert_eq!(
        thunks
            .iter()
            .map(|thunk| operation_count(thunk, is_string_concat))
            .sum::<usize>(),
        2
    );
    assert_eq!(
        thunks
            .iter()
            .map(|thunk| operation_count(thunk, is_drop))
            .sum::<usize>(),
        4,
        "only composite operands drop inside the thunks; returned owners transfer"
    );
    let composite_thunks = thunks
        .iter()
        .copied()
        .filter(|thunk| operation_count(thunk, is_string_concat) == 1)
        .collect::<Vec<_>>();
    assert_eq!(composite_thunks.len(), 2);
    for thunk in composite_thunks {
        let concat = thunk
            .instructions
            .iter()
            .find_map(|instruction| {
                matches!(instruction.operation, Operation::StringConcat { .. })
                    .then(|| instruction.results[0])
            })
            .expect("composite thunk has one concat result");
        let EntityId::Value(concat) = concat else {
            panic!("concat result is a value");
        };
        assert_eq!(returned(thunk), &[concat]);
        let dropped = dropped(thunk);
        assert_eq!(dropped.len(), 2);
        assert!(
            !dropped.contains(&concat),
            "the lambda result transfers instead of being dropped"
        );
    }
    let explicit_thunk = thunks
        .iter()
        .copied()
        .find(|thunk| {
            operation_count(thunk, is_string_concat) == 0
                && operation_count(thunk, is_direct_call) == 0
        })
        .expect("explicit-return thunk exists");
    assert_eq!(returned(explicit_thunk).len(), 1);
    assert!(dropped(explicit_thunk).is_empty());
}

#[test]
fn lowers_fact_backed_lambda_body_owner_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "test/body-owner.ko",
        "package test\n\
         fun inspect(message: String): Unit {}\n\
         fun entry(): Unit {\n\
             val action: move () -> Unit = move {\n\
                 val local = \"inside\"\n\
                 val seen = inspect(local)\n\
             }\n\
             val invoked = action()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/body-owner.ko",
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
    .expect("lambda body owner lowers only with its frontend drop fact");

    let thunk = program.modules[0]
        .functions
        .iter()
        .find(|function| function.name.contains(".thunk"))
        .expect("lambda thunk exists");
    assert_eq!(operation_count(thunk, is_string_literal), 1);
    assert_eq!(operation_count(thunk, is_direct_call), 1);
    assert_eq!(operation_count(thunk, is_drop), 1);
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
            "test/inout-parameter.ko",
            "package test\n\
             fun entry(): Unit {\n\
                 val action: (inout Int) -> Unit = { item -> }\n\
             }",
        ),
        (
            "test/move-only-borrow-parameter.ko",
            "package test\n\
             fun inspect(message: String): Unit {}\n\
             fun entry(): Unit {\n\
                 val action: move (borrow String) -> Unit = move { item -> inspect(item) }\n\
             }",
        ),
        (
            "test/direct-argument-return.ko",
            "package test\n\
             fun select(first: Int, own second: Int): Int = second\n\
             fun entry(): Int {\n\
                 val number = 1\n\
                 return select(number, return 7)\n\
             }",
        ),
        (
            "test/callable-argument-return.ko",
            "package test\n\
             fun entry(): Int {\n\
                 val action: (Int, own Int) -> Int = { left, right -> left + right }\n\
                 val number = 1\n\
                 return action(number, return 7)\n\
             }",
        ),
        (
            "test/direct-argument-break.ko",
            "package test\n\
             fun select(first: Int, own second: Int): Int = second\n\
             fun entry(): Unit {\n\
                 val number = 1\n\
                 loop { val selected = select(number, break) }\n\
             }",
        ),
        (
            "test/callable-argument-continue.ko",
            "package test\n\
             fun entry(): Unit {\n\
                 val action: (Int, own Int) -> Int = { left, right -> left + right }\n\
                 val number = 1\n\
                 loop { val selected = action(number, continue) }\n\
             }",
        ),
        (
            "test/direct-argument-loop-condition-break.ko",
            "package test\n\
             fun select(first: Int, own second: Int): Int = second\n\
             fun entry(): Unit {\n\
                 val number = 1\n\
                 loop {\n\
                     val selected = select(number, if (true) {\n\
                         while (break) {}\n\
                         7\n\
                     } else { 8 })\n\
                 }\n\
             }",
        ),
        (
            "test/callable-argument-internal-loop.ko",
            "package test\n\
             fun entry(): Unit {\n\
                 val action: (Int, own Int) -> Int = { left, right -> left + right }\n\
                 val selected = action(1, if (true) {\n\
                     loop { break }\n\
                     2\n\
                 } else { 3 })\n\
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

fn returned(function: &Function) -> &[ValueId] {
    function
        .blocks
        .iter()
        .find_map(
            |block| match block.terminator.as_ref().map(|term| &term.kind) {
                Some(TerminatorKind::Return { values }) => Some(values.as_slice()),
                _ => None,
            },
        )
        .expect("function returns")
}

fn dropped(function: &Function) -> Vec<ValueId> {
    function
        .instructions
        .iter()
        .filter_map(|instruction| match instruction.operation {
            Operation::Drop { owner } => Some(owner),
            _ => None,
        })
        .collect()
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

fn is_string_literal(operation: &Operation) -> bool {
    matches!(operation, Operation::StringLiteral { .. })
}

fn is_string_concat(operation: &Operation) -> bool {
    matches!(operation, Operation::StringConcat { .. })
}
