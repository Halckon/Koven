#[path = "boxed_signature_storage_tests.rs"]
mod boxed_signature_storage_tests;

#[path = "lower_frontend_map_tests.rs"]
mod map_result_tests;

#[path = "lower_frontend_resource_deinit_tests.rs"]
mod resource_deinit_tests;

#[path = "lower_frontend_return_tests.rs"]
mod return_control_tests;

use lang_frontend::{
    lexer::lex,
    name_resolution::{NameResolution, resolve_names},
    ownership_checking::{OwnershipCheckedFile, check_ownership},
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::{TypeEnvironment, TypedFile, check_types, standard_environments},
};

use super::{
    lower_frontend::{LoweringErrorKind, orchestrate::lower_scalar_file},
    model::Operation,
    render::render_program,
};
use crate::llvm::render_verified_program;

#[path = "call_lifetimes_tests.rs"]
mod call_lifetimes_tests;

struct Analysis {
    sources: SourceMap,
    parsed: ParsedFile,
    names: NameResolution,
    types: TypeEnvironment,
    typed: TypedFile,
    owned: OwnershipCheckedFile,
}

fn analyze(text: &str) -> Analysis {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("lowering.ko", text)
        .expect("source must be unique");
    let lexed = lex(&sources, source).expect("lexing must succeed internally");
    let parsed = parse_file(&sources, &lexed).expect("parsing must succeed internally");
    let (environment, types) = standard_environments();
    let names =
        resolve_names(&sources, &parsed, &environment).expect("names must resolve internally");
    let typed =
        check_types(&sources, &parsed, &names, &types).expect("types must check internally");
    let owned = check_ownership(&sources, &parsed, &names, &typed)
        .expect("ownership must check internally");
    Analysis {
        sources,
        parsed,
        names,
        types,
        typed,
        owned,
    }
}

// Preserve the original whole-file rejection independently of the Unit positive oracles.
#[test]
fn rejects_nested_interpolation_in_error_at_whole_file_boundary() {
    let unsupported = analyze("fun abortInterpolated(): Unit { error(\"${1}\") }");
    let error = match lower_scalar_file(
        &unsupported.sources,
        &unsupported.parsed,
        &unsupported.names,
        &unsupported.typed,
        &unsupported.owned,
    ) {
        Ok(_) => panic!("String interpolation remains outside this native slice"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());
}

#[test]
fn rejects_nested_interpolation_in_println_at_whole_file_boundary() {
    let unsupported = analyze("fun output(): Unit { println(\"${1}\") }");
    let error = match lower_scalar_file(
        &unsupported.sources,
        &unsupported.parsed,
        &unsupported.names,
        &unsupported.typed,
        &unsupported.owned,
    ) {
        Ok(_) => panic!("interpolated println remains outside this native slice"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

#[test]
fn rejects_inline_nullable_lowering_without_panicking() {
    let analysis = analyze(
        "value class Token(val item: Int)\n\
         fun inspect(own token: Token?): Unit {\n\
             if (token != null) {}\n\
         }",
    );
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let error = match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Ok(_) => panic!("inline nullable ABI remains outside SPEC-0196"),
        Err(error) => error,
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
}

#[test]
fn rejects_borrowed_nullable_when_and_non_null_assertion_without_panicking() {
    for source in [
        "class Node {}\nfun inspect(inout node: Node?): Unit { when (node) { null -> {}; else -> {} } }",
        "class Node {}\nclass Holder(val item: Node?)\nfun inspect(holder: Holder): Unit { when (holder.item) { null -> {}; else -> {} } }",
        "class Node {}\nfun inspect(items: List<Node?>): Unit { when (items[0]) { null -> {}; else -> {} } }",
        "value class Token(val item: Int)\nfun inspect(own token: Token?): Unit { when (token) { null -> {}; else -> {} } }",
        "class Node {}\n\
         fun inspect(node: Node?): Unit {\n\
             when (node) { null -> {}; else -> {} }\n\
         }",
        "fun inspect(node: Int?): Int = node!!",
    ] {
        let analysis = analyze(source);
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{:?}",
            analysis.typed.diagnostics()
        );
        assert!(analysis.owned.diagnostics().is_empty());
        let error = match lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        ) {
            Ok(_) => panic!("unimplemented nullable control form must not silently lower"),
            Err(error) => error,
        };
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    }
}

#[test]
fn declarative_type_roots_do_not_enter_the_scalar_instance_graph() {
    let analysis = analyze(
        "public value class Pair<A, B>(val first: A, val second: B)\n\
         class Holder(val item: Int)\n\
         interface Marker\n\
         enum class Outcome<T, E> { Ok(item: T), Err(failure: E) }\n\
         fun entry(): Unit {}",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());

    let lower = || {
        lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .expect("declarative type roots have no module initialization action")
    };
    let program = lower();
    let repeated = lower();
    let ssa = render_program(&program);
    assert_eq!(ssa, render_program(&repeated));
    assert_eq!(ssa.matches("func \"").count(), 1, "{ssa}");
    assert!(ssa.contains("func \"entry\""), "{ssa}");
    for declaration in ["Pair", "Holder", "Marker", "Outcome"] {
        assert!(!ssa.contains(declaration), "{ssa}");
    }

    let llvm = render_verified_program(&program).expect("declarative roots must reach LLVM");
    assert_eq!(llvm.matches("define internal").count(), 1, "{llvm}");
    for declaration in ["Pair", "Holder", "Marker", "Outcome"] {
        assert!(!llvm.contains(declaration), "{llvm}");
    }

    {
        let rejected = analyze("val state = 1\nfun entry(): Unit {}");
        assert!(rejected.names.diagnostics().is_empty());
        assert!(rejected.typed.diagnostics().is_empty());
        let error = match lower_scalar_file(
            &rejected.sources,
            &rejected.parsed,
            &rejected.names,
            &rejected.typed,
            &rejected.owned,
        ) {
            Ok(_) => panic!("runtime-bearing roots remain outside this lowering slice"),
            Err(error) => error,
        };
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        assert!(error.span.is_some());
    }

    let nominal_use = analyze(
        "value class Wrapped(val item: Int)\n\
         class Holder(val item: Int)\n\
         class Resource {}\n\
         enum class Flag { On, Off }\n\
         enum class Maybe<T> { Some(item: T), None }\n\
         enum class Owned<T> { Some(item: T), None }\n\
         fun wrapped(): Wrapped = Wrapped(1)\n\
         fun holder(): Holder = Holder(2)\n\
         fun boxed(): Box<Wrapped> = Box(Wrapped(3))\n\
         fun flag(): Flag = Flag.On\n\
         fun some(): Maybe<Int> = Maybe.Some(4)\n\
         fun enumProjected(): Int { val maybe: Maybe<Int> = Maybe.Some(8) val result: Int = when (maybe) {\n\
             is Maybe.Some<Int> -> maybe.item\n\
             is Maybe.None<Int> -> 0\n\
         } return result }\n\
         fun projected(): Int { val wrapped = Wrapped(5) return wrapped.item }\n\
         fun destructured(): Int { val (item) = Wrapped(6) return item }\n\
         fun classProjected(): Int { val holder = Holder(7) return holder.item }\n\
         fun dropEnum(): Unit { val event: Owned<Resource> = Owned.Some(Resource()) }",
    );
    assert!(
        nominal_use.typed.diagnostics().is_empty(),
        "{:?}",
        nominal_use.typed.diagnostics()
    );
    assert!(nominal_use.owned.diagnostics().is_empty());
    let program = lower_scalar_file(
        &nominal_use.sources,
        &nominal_use.parsed,
        &nominal_use.names,
        &nominal_use.typed,
        &nominal_use.owned,
    )
    .expect("value/class/Box construction facts must lower to verified SSA");
    let ssa = render_program(&program);
    assert_eq!(ssa.matches("aggregate.construct").count(), 11, "{ssa}");
    assert_eq!(ssa.matches("heap.allocate").count(), 4, "{ssa}");
    assert_eq!(ssa.matches("tagged.construct").count(), 4, "{ssa}");
    assert_eq!(ssa.matches("aggregate.project").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("aggregate.copy_explode").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("heap.payload_place").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("tagged.payload_place").count(), 1, "{ssa}");
    assert_eq!(ssa.matches("tagged.discriminant").count(), 2, "{ssa}");
    assert_eq!(ssa.matches("drop ").count(), 2, "{ssa}");
    assert!(ssa.contains("Wrapped#t"), "{ssa}");
    assert!(ssa.contains("Holder#t"), "{ssa}");
    assert!(ssa.contains("Box#t"), "{ssa}");
    assert!(ssa.contains("Flag#t"), "{ssa}");
    assert!(ssa.contains("Maybe#t"), "{ssa}");
    assert!(ssa.contains("Owned#t"), "{ssa}");

    let llvm = render_verified_program(&program).expect("nominal SSA must lower to LLVM");
    assert_eq!(llvm.matches("call ptr @malloc").count(), 4, "{llvm}");
    assert_eq!(llvm.matches("call void @free").count(), 2, "{llvm}");
    assert!(llvm.contains("%koven.enum.t"), "{llvm}");
    assert!(!llvm.contains("retain"), "{llvm}");
    assert!(!llvm.contains("clone"), "{llvm}");
}

#[test]
fn lowers_real_scalar_expression_functions_through_verified_ssa() {
    super::unit_lower_scalar_tests::assert_real_scalar_expression_roots_through_verified_unit_ssa_and_llvm();
}

#[test]
fn rejects_out_of_range_integer_literals_before_ssa_construction() {
    let invalid = analyze(
        "fun byteTooLow(): Byte = -129\n\
         fun ulongTooHigh(): ULong = 18446744073709551616uL",
    );
    assert_eq!(invalid.typed.diagnostics().len(), 2);
    assert!(
        invalid
            .typed
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.code().to_string() == "L0090")
    );
    let error = lower_scalar_file(
        &invalid.sources,
        &invalid.parsed,
        &invalid.names,
        &invalid.typed,
        &invalid.owned,
    )
    .err()
    .expect("out-of-range literals must be rejected before SSA construction");
    assert_eq!(error.kind, LoweringErrorKind::FrontendDiagnostics);
}

#[test]
fn lowers_reachable_scalar_generic_instances_once_and_keeps_recursive_identity() {
    let analysis = analyze(
        "fun <T> identity(own input: T): T = input\n\
         fun <T> relay(own input: T): T = identity(input)\n\
         fun <T> recurse(own input: T): T = recurse(input)\n\
         fun <T> unused(own input: T): T = input\n\
         fun useInt(input: Int): Int = relay<Int>(input)\n\
         fun useIntAgain(input: Int): Int = relay(input)\n\
         fun useLong(input: Long): Long = relay(input)\n\
         fun recursive(input: Int): Int = recurse(input)",
    );
    assert!(
        analysis.parsed.diagnostics().is_empty(),
        "{:?}",
        analysis.parsed.diagnostics()
    );
    assert!(
        analysis.names.diagnostics().is_empty(),
        "{:?}",
        analysis.names.diagnostics()
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );

    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("reachable concrete scalar generic instances must lower");
    let rendered = render_program(&program);
    let repeated = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("repeated instance planning must lower");
    assert_eq!(rendered, render_program(&repeated));
    assert_eq!(rendered.matches("func \"identity<Int>\"").count(), 1);
    assert_eq!(rendered.matches("func \"identity<Long>\"").count(), 1);
    assert_eq!(rendered.matches("func \"relay<Int>\"").count(), 1);
    assert_eq!(rendered.matches("func \"relay<Long>\"").count(), 1);
    assert_eq!(rendered.matches("func \"recurse<Int>\"").count(), 1);
    assert!(!rendered.contains("unused<"));

    let module = &program.modules[0];
    let recursive = module
        .functions
        .iter()
        .find(|function| function.name == "recurse<Int>")
        .expect("recursive generic instance must exist");
    let recursive_body = rendered
        .split("func \"recurse<Int>\"")
        .nth(1)
        .and_then(|body| body.split("\n\n  func").next())
        .expect("recursive instance body must render");
    assert!(recursive_body.contains(&format!("call @f{}(", recursive.id.index())));
}

#[test]
fn generic_overloads_with_the_same_type_arguments_keep_distinct_targets() {
    let analysis = analyze(
        "fun <T> choose(own input: T): T = input\n\
         fun <T> choose(own input: T, fallback: Boolean): T = input\n\
         fun one(input: Int): Int = choose(input)\n\
         fun two(input: Int): Int = choose(input, true)",
    );
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("generic overload instances must lower independently");
    let module = &program.modules[0];
    let choices = module
        .functions
        .iter()
        .filter(|function| function.name == "choose<Int>")
        .map(|function| function.id)
        .collect::<Vec<_>>();
    assert_eq!(choices.len(), 2);
    let target_of = |name: &str| {
        let function = module
            .functions
            .iter()
            .find(|function| function.name == name)
            .expect("caller must exist");
        function
            .instructions
            .iter()
            .find_map(|instruction| match instruction.operation {
                Operation::DirectCall { callee, .. } => Some(callee),
                _ => None,
            })
            .expect("caller must contain one direct call")
    };
    let one = target_of("one");
    let two = target_of("two");
    assert_ne!(one, two);
    assert!(choices.contains(&one));
    assert!(choices.contains(&two));
    let llvm = render_verified_program(&program).expect("distinct overload instances must lower");
    assert_eq!(
        llvm.matches("define internal i32 @\"f0.choose<Int>\"")
            .count(),
        1
    );
    assert_eq!(
        llvm.matches("define internal i32 @\"f1.choose<Int>\"")
            .count(),
        1
    );
    assert!(llvm.contains("call i32 @\"f0.choose<Int>\""));
    assert!(llvm.contains("call i32 @\"f1.choose<Int>\""));
}

#[test]
fn rejects_mixed_analysis_chains_before_constructing_ssa() {
    let first = analyze("fun value(input: Int): Int = input");
    let second = analyze("fun value(input: Int): Int = input");
    let error = lower_scalar_file(
        &first.sources,
        &first.parsed,
        &first.names,
        &first.typed,
        &second.owned,
    )
    .err()
    .expect("foreign ownership analysis must be rejected");
    assert_eq!(error.kind, LoweringErrorKind::MismatchedSource);

    let repeated_typed = check_types(&first.sources, &first.parsed, &first.names, &first.types)
        .expect("repeated type analysis must succeed");
    let repeated_owned =
        check_ownership(&first.sources, &first.parsed, &first.names, &repeated_typed)
            .expect("ownership analysis for repeated typed product must succeed");
    let error = lower_scalar_file(
        &first.sources,
        &first.parsed,
        &first.names,
        &first.typed,
        &repeated_owned,
    )
    .err()
    .expect("ownership from another typed product must not match the original typed chain");
    assert_eq!(error.kind, LoweringErrorKind::MismatchedAnalysis);
}

#[test]
fn lowers_straight_line_blocks_locals_assignments_and_returns() {
    super::unit_lower_straight_line_tests::assert_original_straight_line_roots_through_strict_memory_document_unit();
}

#[test]
fn diagnostics_and_unsupported_bodies_fail_without_partial_programs() {
    let diagnostic = analyze("fun broken(input: Int): Int = missing");
    let error = lower_scalar_file(
        &diagnostic.sources,
        &diagnostic.parsed,
        &diagnostic.names,
        &diagnostic.typed,
        &diagnostic.owned,
    )
    .err()
    .expect("frontend diagnostics must gate lowering");
    assert_eq!(error.kind, LoweringErrorKind::FrontendDiagnostics);

    let numeric_when = analyze(
        "fun numeric(input: Int): Int = when (input) {\n\
             1 -> 2\n\
             else -> 3\n\
         }",
    );
    assert!(numeric_when.typed.diagnostics().is_empty());
    let error = lower_scalar_file(
        &numeric_when.sources,
        &numeric_when.parsed,
        &numeric_when.names,
        &numeric_when.typed,
        &numeric_when.owned,
    )
    .err()
    .expect("non-Boolean when remains outside the scalar control-flow slice");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());

    let elvis = analyze("fun choose(input: Int?): Int = input ?: 0");
    assert!(elvis.typed.diagnostics().is_empty());
    assert!(elvis.owned.diagnostics().is_empty());
    assert!(elvis.owned.deferred().is_empty());
    let error = lower_scalar_file(
        &elvis.sources,
        &elvis.parsed,
        &elvis.names,
        &elvis.typed,
        &elvis.owned,
    )
    .err()
    .expect("Elvis has valid frontend facts but remains outside native lowering");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert!(error.span.is_some());

    let invalid_jump = analyze("fun invalid(): Unit { break }");
    assert_eq!(invalid_jump.typed.diagnostics().len(), 1);
    assert_eq!(
        invalid_jump.typed.diagnostics()[0].code().to_string(),
        "L0142"
    );
    let error = lower_scalar_file(
        &invalid_jump.sources,
        &invalid_jump.parsed,
        &invalid_jump.names,
        &invalid_jump.typed,
        &invalid_jump.owned,
    )
    .err()
    .expect("Phase 2 jump diagnostics must gate SSA construction");
    assert_eq!(error.kind, LoweringErrorKind::FrontendDiagnostics);

    let for_loop = analyze("fun iterate(): Unit { for (item in listOf(1)) {} }");
    assert!(for_loop.typed.diagnostics().is_empty());
    assert!(for_loop.owned.diagnostics().is_empty());
    assert_eq!(for_loop.owned.iterations().len(), 1);
    let program = lower_scalar_file(
        &for_loop.sources,
        &for_loop.parsed,
        &for_loop.names,
        &for_loop.typed,
        &for_loop.owned,
    )
    .expect("for loops are now lowered to valid SSA");
    super::verify::verify_program(&program).expect("for loop SSA must verify");

    let string_instance = analyze(
        "fun <T> identity(own input: T): T = input\n\
         fun text(own input: String): String = identity(input)",
    );
    assert!(string_instance.typed.diagnostics().is_empty());
    assert!(string_instance.owned.diagnostics().is_empty());
    let program = lower_scalar_file(
        &string_instance.sources,
        &string_instance.parsed,
        &string_instance.names,
        &string_instance.typed,
        &string_instance.owned,
    )
    .expect("String is now a first-class concrete generic instance");
    assert!(render_program(&program).contains("identity<String>"));
}

#[test]
fn non_null_assertion_does_not_register_unreachable_inline_types() {
    let analysis =
        analyze("fun inspect(): Int { return 0\nval unused: Int? = 1\nval asserted = unused!! }");
    assert!(analysis.typed.diagnostics().is_empty());
    assert_eq!(analysis.typed.non_null_assertions().len(), 1);
    assert!(analysis.owned.non_null_assertions().is_empty());
    lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .expect("unreachable typed facts cannot enable or reject runtime layouts");
}

#[path = "constant_lowering_tests.rs"]
mod constant_lowering_tests;

#[test]
fn rejects_produced_guarded_cleanup_before_lowering_bindings() {
    let analysis = analyze(
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, own ys: List<Int>, flag: Boolean) { val f: () -> Unit = if (flag) ({ read(xs) }) else ({ read(ys) })\nval used = f() }",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    assert!(
        analysis
            .owned
            .drops()
            .iter()
            .any(|fact| fact.condition().is_some()),
        "frontend must produce real guarded cleanup"
    );
    let first = analysis
        .owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            lang_frontend::ownership_checking::IterationCleanupAction::SaveOwnerSnapshot {
                owner,
                ..
            } => analysis.owned.cleanup_conditions().owner_snapshot(*owner),
            _ => None,
        })
        .expect("frontend must publish the saved owner choices");
    let error = match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Err(error) => error,
        Ok(_) => panic!("saved choices require an explicit SSA carrier"),
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(first.origin()));
}

#[test]
fn rejects_conditional_capture_instance_before_narrow_closure_lowering() {
    use lang_frontend::ownership_checking::{CleanupCondition, IterationCleanupAction};

    let analysis = analyze(
        "fun run(flag: Boolean) { val text = \"x\"\nif (flag) { val action: move () -> Unit = move { println(text) }\nval used = action() } }",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let input = analysis
        .owned
        .cleanup_steps()
        .iter()
        .find_map(|(_, action)| match action {
            IterationCleanupAction::SaveClosureCapture { input, .. }
                if !matches!(
                    analysis.owned.cleanup_conditions().get(input.condition()),
                    Some(CleanupCondition::Always)
                ) =>
            {
                Some(*input)
            }
            _ => None,
        })
        .expect("the branch must publish a guarded capture instance");
    let error = match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Err(error) => error,
        Ok(_) => panic!("the narrow native bridge cannot select a dynamic capture source"),
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(error.span, Some(input.origin()));
}

#[test]
fn rejects_loop_carried_last_capture_loan_before_lowering_bindings() {
    let analysis = analyze(
        "fun read(xs: List<Int>) {}\nfun run(flags: List<Boolean>) {
            var f: () -> Unit = {}
            var g: () -> Unit = {}
            for (_ in flags) {
                val xs = listOf(1)
                { f = ({ read(xs) }) }
                { g = ({ read(xs) }) }
                break
            }
            val first = f()
            val second = g()
        }",
    );
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    assert!(analysis.owned.cleanup_steps().iter().any(|(_, action)| matches!(
        action,
        lang_frontend::ownership_checking::IterationCleanupAction::TestLastCaptureLoan { .. }
    )));
    let error = match lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    ) {
        Err(error) => error,
        Ok(_) => panic!("last capture loan needs an executable runtime selector"),
    };
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    assert_eq!(
        analysis.sources.slice(error.span.unwrap()).unwrap(),
        "xs",
        "native must reject the unavailable runtime last-loan choice"
    );
}

#[test]
fn bitwise_inv_does_not_reinterpret_unsupported_single_file_source_members() {
    for method in ["ordinary", "inv"] {
        let analysis = analyze(&format!(
            "class Sample {{ fun {method}(): Int = 7 }}\nfun entry(): Int = Sample().{method}()"
        ));
        assert!(
            analysis.typed.diagnostics().is_empty(),
            "{:?}",
            analysis.typed.diagnostics()
        );
        assert!(
            analysis.owned.diagnostics().is_empty(),
            "{:?}",
            analysis.owned.diagnostics()
        );
        assert!(analysis.typed.integer_operations().is_empty());
        let error = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .err()
        .expect("source member call retains single-file lowering boundary");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    }
}

#[path = "root_primitive_lower_tests.rs"]
mod root_primitive_tests;

#[path = "field_replace_lower_tests.rs"]
mod field_replace_tests;

#[path = "lower_frontend_owned_source_continue_tests.rs"]
mod owned_source_continue_tests;

#[path = "lower_frontend_owned_source_return_tests.rs"]
mod owned_source_return_tests;
