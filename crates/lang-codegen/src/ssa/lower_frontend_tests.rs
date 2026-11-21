mod frontend_boundaries {
    //! Frozen ordinary frontend boundaries and successful original roots through strict Unit.

    use lang_frontend::{
        diagnostic::{DiagnosticDetail, Severity},
        name_resolution::SymbolKind,
    };

    use crate::{
        llvm::render_verified_program,
        native_tests::boxed_enum_tests::unit_source::{SourceRejectionKind, UnitSource},
        ssa::{
            model::{EntityId, EntityType, Operation, Origin, SsaTypeKind, TerminatorKind},
            verify::verify_program,
        },
    };

    struct ExpectedDiagnostic {
        stage: &'static str,
        code: &'static str,
        message: &'static str,
        range: (usize, usize),
        label: Option<(usize, usize)>,
    }

    // Frozen from the original Legacy consumers at a4b72d35, before their migration.
    // Preserve severity, code, full message, source identity, primary/label ranges and detail order.
    fn assert_rejected(source: &str, stages: &[(&str, usize)], expected: &[ExpectedDiagnostic]) {
        let rejected = UnitSource::analyze("lowering.ko", source)
            .err()
            .expect("the original frontend error cannot acquire any executable Unit capability");
        assert!(matches!(rejected.kind, SourceRejectionKind::Diagnostics));
        assert_eq!(rejected.report.stages, stages);
        assert_eq!(rejected.report.diagnostics.len(), expected.len());
        for ((stage, diagnostic), expected) in rejected.report.diagnostics.iter().zip(expected) {
            assert_eq!(*stage, expected.stage);
            assert_eq!(diagnostic.severity(), Severity::Error);
            assert_eq!(diagnostic.code().to_string(), expected.code);
            assert_eq!(diagnostic.message(), expected.message);
            let primary = diagnostic.primary_span();
            assert_eq!((primary.start(), primary.end()), expected.range);
            assert_eq!(
                rejected.sources.source_name(primary.source_id()),
                Ok("lowering.ko")
            );
            assert_eq!(
                rejected.sources.source_text(primary.source_id()),
                Ok(source)
            );
            assert_eq!(
                rejected.sources.slice(primary),
                Ok(&source[expected.range.0..expected.range.1])
            );
            match (diagnostic.details(), expected.label) {
                ([DiagnosticDetail::Label(label)], Some((start, end))) => {
                    assert_eq!(label.message(), "expected type introduced here");
                    assert_eq!(label.span().source_id(), primary.source_id());
                    assert_eq!((label.span().start(), label.span().end()), (start, end));
                    assert_eq!(
                        rejected.sources.slice(label.span()),
                        Ok(&source[start..end])
                    );
                }
                ([], None) => {}
                _ => panic!(
                    "complete ordered diagnostic details changed: {:?}",
                    diagnostic.details()
                ),
            }
        }
    }

    pub(super) fn assert_integer_literal_rejection(source: &str) {
        assert_rejected(
            source,
            &[("parse", 0), ("names", 0), ("types", 2)],
            &[
                ExpectedDiagnostic {
                    stage: "types",
                    code: "L0090",
                    message: "numeric literal is outside the representable range",
                    range: (26, 29),
                    label: Some((18, 22)),
                },
                ExpectedDiagnostic {
                    stage: "types",
                    code: "L0090",
                    message: "numeric literal is outside the representable range",
                    range: (58, 80),
                    label: Some((50, 55)),
                },
            ],
        );
    }

    pub(super) fn assert_missing_name_rejection(source: &str) {
        assert_rejected(
            source,
            &[("parse", 0), ("names", 1)],
            &[ExpectedDiagnostic {
                stage: "names",
                code: "L0080",
                message: "unresolved name",
                range: (30, 37),
                label: None,
            }],
        );
    }

    pub(super) fn assert_jump_rejection(source: &str) {
        assert_rejected(
            source,
            &[("parse", 0), ("names", 0), ("types", 1)],
            &[ExpectedDiagnostic {
                stage: "types",
                code: "L0142",
                message: "break is not inside an enclosing loop in this callable",
                range: (22, 27),
                label: None,
            }],
        );
    }

    #[test]
    fn top_level_runtime_value_stops_before_unit_lowering() {
        // Single parsed and typed this source, then reported UnsupportedNode in lowering.
        // Ordinary Unit rejects the exact same source during type checking.
        assert_rejected(
            "val state = 1\nfun entry(): Unit {}",
            &[("parse", 0), ("names", 0), ("types", 1)],
            &[ExpectedDiagnostic {
                stage: "types",
                code: "L0167",
                message: "top-level runtime values are not supported; use `const val` or construct the value in `main` and pass it as an argument",
                range: (0, 3),
                label: None,
            }],
        );
    }

    fn analyze_original(source: &str) -> UnitSource {
        let unit = UnitSource::analyze("lowering.ko", source).expect("strict original Unit source");
        unit.assert_frontend_clean();
        assert!(unit.ownership().deferred().is_empty());
        assert_eq!(
            unit.sources().source_text(unit.parsed().source_id()),
            Ok(source)
        );
        unit
    }

    pub(super) fn assert_original_for_loop(source: &str) {
        let unit = analyze_original(source);
        assert_eq!(unit.ownership().iterations().len(), 1);
        let declaration = unit.declaration("iterate", SymbolKind::Function);
        let (program, entry) = unit
            .lower_entry(declaration)
            .expect("original for loop lowers");
        verify_program(&program).expect("original for loop SSA verifies");
        let module = program.module(entry.module()).unwrap();
        let function = module.function(entry).unwrap();
        assert_eq!(
            function.origin,
            Origin::Source(unit.declaration_span(declaration))
        );
        assert_eq!(module.functions.len(), 1);
        assert!(function.blocks[0].parameters.is_empty());
        assert!(function.return_types.is_empty());
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::ContainerConstruct { .. }))
                .count(),
            1
        );
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::Drop { .. }))
                .count(),
            1
        );
        render_verified_program(&program).expect("original for loop LLVM verifies");
    }

    pub(super) fn assert_original_string_instance(source: &str) {
        let unit = analyze_original(source);
        let declaration = unit.declaration("text", SymbolKind::Function);
        let (program, entry) = unit
            .lower_entry(declaration)
            .expect("original String generic instance lowers");
        verify_program(&program).expect("original String generic SSA verifies");
        let module = program.module(entry.module()).unwrap();
        let function = module.function(entry).unwrap();
        assert_eq!(
            function.origin,
            Origin::Source(unit.declaration_span(declaration))
        );
        assert_eq!(module.functions.len(), 2);
        let input = function.blocks[0].parameters.as_slice();
        let [EntityId::Value(_)] = input else {
            panic!("original owned String input")
        };
        let [returned] = function.return_types.as_slice() else {
            panic!("one original String return")
        };
        assert_eq!(
            function.entity(input[0]).unwrap().ty,
            EntityType::Value(*returned)
        );
        assert_eq!(module.type_kind(*returned), Some(&SsaTypeKind::StringOwner));
        let calls = function
            .instructions
            .iter()
            .filter_map(|i| match &i.operation {
                Operation::DirectCall {
                    callee,
                    receiver,
                    arguments,
                } => Some((i, callee, receiver, arguments)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let [(call, callee, receiver, arguments)] = calls.as_slice() else {
            panic!("one concrete generic call")
        };
        assert!(receiver.is_none());
        assert_eq!(arguments.as_slice(), input);
        let callee = module.function(**callee).unwrap();
        assert_eq!(
            callee.origin,
            Origin::Source(
                unit.declaration_span(unit.declaration("identity", SymbolKind::Function))
            )
        );
        assert_eq!(callee.return_types, function.return_types);
        for current in [function, callee] {
            assert!(
                !current
                    .instructions
                    .iter()
                    .any(|i| matches!(i.operation, Operation::Drop { .. }))
            );
            let returns = current
                .blocks
                .iter()
                .filter_map(|block| match &block.terminator.as_ref()?.kind {
                    TerminatorKind::Return { values } => Some(
                        values
                            .iter()
                            .copied()
                            .map(EntityId::Value)
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let expected = if current.id() == function.id() {
                &call.results
            } else {
                &callee.blocks[0].parameters
            };
            assert_eq!(
                returns.as_slice(),
                std::slice::from_ref(expected),
                "each original owner is returned, never dropped"
            );
        }
        // Unit has source-qualified generated names. Preserve the original generic-instance
        // observation through its exact declaration and concrete owned String ABI instead.
        let [parameter] = callee.blocks[0].parameters.as_slice() else {
            panic!("the original generic identity has one concrete parameter");
        };
        assert!(matches!(parameter, EntityId::Value(_)));
        assert_eq!(
            callee.entity(*parameter).unwrap().ty,
            EntityType::Value(*returned)
        );
        render_verified_program(&program).expect("original String generic LLVM verifies");
    }
}

use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic, assert_text_eq};
#[path = "boxed_signature_storage_tests.rs"]
pub(in crate::ssa) mod boxed_signature_storage_tests;

#[path = "lower_frontend_map_tests.rs"]
pub(in crate::ssa) mod map_result_tests;

#[path = "lower_frontend_resource_deinit_tests.rs"]
mod resource_deinit_tests;

#[allow(unused_imports)]
pub(in crate::ssa) use map_result_tests::return_control_tests;

#[path = "lower_frontend_root_boundary_tests.rs"]
pub(in crate::ssa) mod root_boundaries;

use lang_frontend::{
    lexer::lex,
    name_resolution::{DeclarationId, NameResolution, SourceUnitKey, SymbolKind, resolve_names},
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
use crate::{
    llvm::render_verified_program, native_tests::boxed_enum_tests::unit_source::UnitSource,
};

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

// These complete single-root sources retain their original test identities through Unit.
fn analyze_single_root(text: &str, name: &str) -> (UnitSource, DeclarationId) {
    let unit = UnitSource::analyze("lowering.ko", text).expect("strict original Unit admission");
    unit.assert_frontend_clean();
    assert!(unit.ownership().deferred().is_empty());
    assert_eq!(unit.parsed().roots().len(), 1);
    let [document] = unit.names().names().index().source_units() else {
        panic!("one original MemoryDocument");
    };
    assert_eq!(document.key(), &SourceUnitKey::MemoryDocument);
    assert_eq!(document.source_id(), unit.parsed().source_id());
    assert_eq!(
        unit.sources().source_name(document.source_id()),
        Ok("lowering.ko")
    );
    assert_eq!(unit.sources().source_text(document.source_id()), Ok(text));
    let declaration = unit.declaration(name, SymbolKind::Function);
    assert_eq!(
        unit.sources().slice(unit.declaration_span(declaration)),
        Ok(text)
    );
    (unit, declaration)
}

#[test]
fn rejects_nested_interpolation_in_error_at_whole_file_boundary() {
    let text = "fun abortInterpolated(): Unit { error(\"${1}\") }";
    let (unit, declaration) = analyze_single_root(text, "abortInterpolated");
    let error = unit
        .lower_entry(declaration)
        .err()
        .expect("String interpolation remains outside this native slice");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    let span = error
        .span
        .expect("nested interpolation retains source span");
    assert_eq!(span.source_id(), unit.parsed().source_id());
    assert_eq!(unit.sources().slice(span), Ok("\"${1}\""));
    assert_eq!(span.start(), text.find('"').unwrap());
    assert_eq!(span.end(), text.rfind('"').unwrap() + 1);
}

#[test]
fn rejects_nested_interpolation_in_println_at_whole_file_boundary() {
    let text = "fun output(): Unit { println(\"${1}\") }";
    let (unit, declaration) = analyze_single_root(text, "output");
    let error = unit
        .lower_entry(declaration)
        .err()
        .expect("interpolated println remains outside this native slice");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    let span = error
        .span
        .expect("nested interpolation retains source span");
    assert_eq!(span.source_id(), unit.parsed().source_id());
    assert_eq!(unit.sources().slice(span), Ok("\"${1}\""));
    assert_eq!(span.start(), text.find('"').unwrap());
    assert_eq!(span.end(), text.rfind('"').unwrap() + 1);
}

#[path = "inline_nullable_unit_contract.rs"]
mod inline_nullable_contract;

#[test]
fn rejects_inline_nullable_lowering_without_panicking() {
    let text = "value class Token(val item: Int)\n\
         fun inspect(own token: Token?): Unit {\n\
             if (token != null) {}\n\
         }";
    // Keep the historical FQN and original source; the supported Unit ABI/CFG
    // contract supersedes the obsolete Legacy implementation rejection.
    inline_nullable_contract::assert_unit(text);
}

#[test]
fn rejects_borrowed_nullable_when_and_non_null_assertion_without_panicking() {
    for source in [
        "class Node {}\nclass Holder(val item: Node?)\nfun inspect(holder: Holder): Unit { when (holder.item) { null -> {}; else -> {} } }",
        "class Node {}\nfun inspect(items: List<Node?>): Unit { when (items[0]) { null -> {}; else -> {} } }",
        "value class Token(val item: Int)\nfun inspect(own token: Token?): Unit { when (token) { null -> {}; else -> {} } }",
    ] {
        // These original source forms stay valid in the frontend and fail at the
        // same whole When boundary as the corresponding strict Unit negative set.
        let unit = UnitSource::analyze("lowering.ko", source)
            .expect("original nullable When source passes strict Unit admission");
        unit.assert_frontend_clean();
        let document = unit.parsed().source_id();
        assert_eq!(unit.sources().source_text(document), Ok(source));
        let entry = unit.declaration("inspect", SymbolKind::Function);
        let error = unit
            .lower_entry(entry)
            .err()
            .expect("ordinary Unit still refuses this subject category");
        assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
        let origin = error.span.expect("exact source-qualified backend boundary");
        let start = source.find("when (").unwrap();
        let end = source.strip_suffix(" }").unwrap().len();
        assert_eq!(origin.source_id(), document);
        assert_eq!((origin.start(), origin.end()), (start, end));
        assert_eq!(unit.sources().slice(origin), Ok(&source[start..end]));
    }
    for source in [
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
fn nullable_when_inout_class_subject_rejects_at_unit_parameter() {
    use lang_frontend::type_checking::NullableWhenSubjectCategory::InoutRoot;
    let source = "class Node {}\nfun inspect(inout node: Node?): Unit { when (node) { null -> {}; else -> {} } }";
    assert_class_loan_subject(source, InoutRoot, "inout node: Node?");
}

fn assert_class_loan_subject(
    source: &str,
    category: lang_frontend::type_checking::NullableWhenSubjectCategory,
    rejected_origin: &str,
) {
    // A named class loan can have a real Unit ownership plan while its
    // representation remains unsupported by the backend.
    let unit = UnitSource::analyze("lowering.ko", source).expect("strict Unit admission");
    unit.assert_frontend_clean();
    let [plan] = unit.ownership().nullable_whens() else {
        panic!("one original nullable When ownership plan");
    };
    assert_eq!(plan.descriptor().source_category(), category);
    let entry = unit.declaration("inspect", SymbolKind::Function);
    let error = unit
        .lower_entry(entry)
        .err()
        .expect("unsupported class loan root");
    assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode);
    let span = error
        .span
        .expect("source-qualified Unit lowering rejection");
    assert_eq!(span.source_id(), unit.parsed().source_id());
    assert_eq!(unit.sources().slice(span), Ok(rejected_origin));
}

#[path = "nominal_multiroot_unit_contract.rs"]
mod nominal_multiroot_unit_contract;

#[test]
fn declarative_type_roots_do_not_enter_the_scalar_instance_graph() {
    root_boundaries::assert_unused_declarations(
        "public value class Pair<A, B>(val first: A, val second: B)\n\
         class Holder(val item: Int)\n\
         interface Marker\n\
         enum class Outcome<T, E> { Ok(item: T), Err(failure: E) }\n\
         fun entry(): Unit {}",
    );
    nominal_multiroot_unit_contract::assert_original_nominal_roots();
}

#[test]
fn lowers_real_scalar_expression_functions_through_verified_ssa() {
    super::unit_lower_scalar_tests::assert_real_scalar_expression_roots_through_verified_unit_ssa_and_llvm();
}

#[test]
fn rejects_out_of_range_integer_literals_before_ssa_construction() {
    frontend_boundaries::assert_integer_literal_rejection(
        "fun byteTooLow(): Byte = -129\n\
         fun ulongTooHigh(): ULong = 18446744073709551616uL",
    );
}

#[path = "generic_recursive_unit_contract.rs"]
mod generic_recursive_unit_contract;

#[test]
fn lowers_reachable_scalar_generic_instances_once_and_keeps_recursive_identity() {
    generic_recursive_unit_contract::assert_reachable_instances();
}

#[path = "generic_overload_unit_contract.rs"]
mod generic_overload_unit_contract;

#[test]
fn generic_overloads_with_the_same_type_arguments_keep_distinct_targets() {
    generic_overload_unit_contract::assert_distinct_targets();
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
    frontend_boundaries::assert_missing_name_rejection("fun broken(input: Int): Int = missing");

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

    frontend_boundaries::assert_jump_rejection("fun invalid(): Unit { break }");
    frontend_boundaries::assert_original_for_loop(
        "fun iterate(): Unit { for (item in listOf(1)) {} }",
    );
    frontend_boundaries::assert_original_string_instance(
        "fun <T> identity(own input: T): T = input\n\
         fun text(own input: String): String = identity(input)",
    );
}

#[test]
fn non_null_assertion_does_not_register_unreachable_inline_types() {
    let text = "fun inspect(): Int { return 0\nval unused: Int? = 1\nval asserted = unused!! }";
    let (unit, declaration) = analyze_single_root(text, "inspect");
    assert!(unit.types().diagnostics().is_empty());
    assert_eq!(unit.types().non_null_assertions().len(), 1);
    assert!(unit.ownership().non_null_assertions().is_empty());
    let (program, root) = unit
        .lower_entry(declaration)
        .expect("unreachable typed facts cannot enable or reject runtime layouts");
    super::verify::verify_program(&program).expect("original single-root SSA verifies");
    render_verified_program(&program).expect("original single-root LLVM verifies");
    let module = program.module(root.module()).expect("returned module");
    assert_eq!(module.functions.len(), 1);
    assert_eq!(
        module.types,
        [super::model::SsaTypeKind::Integer {
            bits: 32,
            signed: true
        }]
    );
    let function = module.function(root).expect("returned root");
    assert_eq!(
        function.origin,
        super::model::Origin::Source(unit.declaration_span(declaration))
    );
}

#[path = "constant_lowering_tests.rs"]
pub(in crate::ssa) mod constant_lowering_tests;

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
                block { f = ({ read(xs) }) }
                block { g = ({ read(xs) }) }
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
pub(in crate::ssa) mod root_primitive_tests;

#[path = "field_replace_lower_tests.rs"]
mod field_replace_tests;

#[path = "lower_frontend_owned_source_continue_tests.rs"]
mod owned_source_continue_tests;

#[path = "lower_frontend_owned_source_return_tests.rs"]
mod owned_source_return_tests;
