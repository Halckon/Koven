//! SPEC-0291: String value observation after nullable consume, ordinary entries only.
//! Collect both complete FE/ownership/SSA/LLVM outcomes before any assertion.

use super::map_consume_fixture_tests::{Fixture, observe_single_fixture, observe_unit_fixture};
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};

mod inputs {
    //! Read-only qualification of this fixed String fixture, on the harness's actual analysis.

    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::{
            SourceUnitId, SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames,
        },
        ownership_checking::{
            CompilationUnitOwnership, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
        },
        parser::{Expression, ParsedFile},
        type_checking::{
            BuiltinType, CompilationUnitTypes, ExpressionCategory, IntrinsicTypeConstructor,
            UnitCallReceiverOrigin, UnitCallTarget, UnitExpressionId, UnitTypeKind,
        },
    };

    use super::super::map_consume_fixture_tests::{Failure, Outcome, gate};

    fn failure(stage: &str, detail: impl std::fmt::Debug) -> Failure {
        Failure {
            stage: stage.to_owned(),
            detail: format!("{detail:#?}"),
        }
    }

    fn receiver_chain(
        parsed: &ParsedFile,
        call: ExpressionId,
        receiver: ExpressionId,
        stages: &mut Vec<String>,
    ) -> Outcome<[ExpressionId; 3]> {
        let node = parsed
            .ast()
            .expressions()
            .get(call)
            .map_err(|error| failure("real consume call AST", error))?;
        let Expression::Call {
            callee,
            arguments,
            type_arguments,
            ..
        } = node.payload()
        else {
            return Err(failure("real consume call AST", node));
        };
        let member = parsed
            .ast()
            .expressions()
            .get(*callee)
            .map_err(|error| failure("real consume member AST", error))?;
        gate(
            "exact consume Call and original grouped receiver",
            arguments.is_empty()
                && type_arguments.is_empty()
                && matches!(member.payload(), Expression::Member { receiver: actual, safe: false, .. }
                    if *actual == receiver),
            (node, member),
            stages,
        )?;
        let mut expression = receiver;
        let mut chain = Vec::new();
        loop {
            let node = parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|error| failure("real receiver chain AST", error))?;
            gate(
                "same-file receiver AST",
                node.span().source_id() == parsed.source_id(),
                node,
                stages,
            )?;
            chain.push(expression);
            match node.payload() {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Name => break,
                _ => return Err(failure("receiver must end at a Name", node)),
            }
        }
        match chain.as_slice() {
            [outer, inner, name] => Ok([*outer, *inner, *name]),
            _ => Err(failure("exact two Groups then Name", chain)),
        }
    }

    // The original single source now has real Unit facts. These are the same exact
    // root/storage/KV, three Name/Group proofs, null comparison, and Map descriptor
    // oracles as before, with source qualification and the receiver Move fact added.
    pub(super) fn inspect_single(
        source: SourceUnitId,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        stages: &mut Vec<String>,
    ) -> Outcome<()> {
        let [consume] = typed.map_consumes() else {
            return Err(failure(
                "one real qualified nullable String consume",
                typed.map_consumes(),
            ));
        };
        let Some((root, storage)) = consume.source() else {
            return Err(failure(
                "real qualified source root and nullable storage",
                consume,
            ));
        };
        gate(
            "consume call/receiver/root belong to the actual fixture source",
            consume.expression().source_unit() == source
                && consume.receiver().source_unit() == source
                && root.source_unit() == source,
            (source, consume),
            stages,
        )?;
        let chain = receiver_chain(
            parsed,
            consume.expression().expression(),
            consume.receiver().expression(),
            stages,
        )?;
        let roots: Vec<_> = names
            .names()
            .source_units()
            .iter()
            .filter(|unit| unit.source_unit() == source)
            .flat_map(|unit| unit.resolution().symbols())
            .filter(|symbol| symbol.id() == root.symbol())
            .collect();
        let leaf = parsed
            .ast()
            .expressions()
            .get(chain[2])
            .map_err(|error| failure("qualified receiver Name AST", error))?;
        let references: Vec<_> = names
            .names()
            .references()
            .iter()
            .filter(|reference| {
                reference.source_unit() == source && reference.span() == leaf.span()
            })
            .collect();
        let string = typed.types().builtin(BuiltinType::String);
        let pair = [consume.key_type(), consume.value_type()];
        gate(
            "qualified nullable String source/root/storage/read/result/KV identity",
            matches!(roots.as_slice(), [symbol]
                if symbol.name() == "source" && symbol.kind() == SymbolKind::Variable
                    && !symbol.is_synthetic() && symbol.span().source_id() == parsed.source_id())
                && matches!(references.as_slice(), [reference]
                    if matches!(reference.target(), UnitReferenceTarget::Symbol(actual) if *actual == root))
                && typed.symbol_type(root) == Some(storage)
                && storage != consume.receiver_type()
                && typed.types().get(storage)
                    == Some(&UnitTypeKind::Nullable(consume.receiver_type()))
                && string.is_some_and(|string| pair == [string, string])
                && matches!(typed.types().get(consume.receiver_type()),
                    Some(UnitTypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::MutableMap, arguments })
                        if arguments.as_slice() == pair)
                && matches!(typed.types().get(consume.result_type()),
                    Some(UnitTypeKind::Intrinsic { constructor: IntrinsicTypeConstructor::Map, arguments })
                        if arguments.as_slice() == pair)
                && typed.expression_type(consume.expression()) == Some(consume.result_type()),
            (consume, &roots, &references, storage, string),
            stages,
        )?;
        for expression in chain {
            let expression = UnitExpressionId::new(source, expression);
            let proof = typed.non_null_use(expression);
            gate(
                "original qualified Name/Group non-null use and Place identity",
                proof.is_some_and(|proof| {
                    proof.expression() == expression
                        && proof.symbol() == root
                        && proof.declared_type() == storage
                        && proof.narrowed_type() == consume.receiver_type()
                }) && typed.expression_type(expression) == Some(consume.receiver_type())
                    && typed.expression_category(expression) == Some(ExpressionCategory::Place),
                (expression, proof),
                stages,
            )?;
        }
        gate(
            "real qualified non-null true edge for the same source",
            matches!(typed.null_comparisons(), [comparison]
                if comparison.expression().source_unit() == source && comparison.symbol() == root
                    && comparison.nullable_type() == storage && comparison.non_null_when_true()),
            typed.null_comparisons(),
            stages,
        )?;
        let actual = owned.receiver_fact(consume.expression());
        gate(
            "real qualified consume receiver Move fact",
            actual.is_some_and(|fact| {
                fact.call() == consume.expression()
                    && fact.source() == UnitCallReceiverOrigin::Expression(consume.receiver())
                    && fact.receiver_type() == consume.receiver_type()
                    && fact.kind() == UnitReceiverOwnershipKind::Move
                    && matches!(fact.target(), UnitReceiverOwnershipTarget::Place(place)
                    if place.is_root() && place.root() == root)
            }),
            (consume, actual),
            stages,
        )?;
        gate(
            "fixed qualified overwrite/size/two String borrow-read descriptors",
            typed.map_constructions().len() == 1
                && typed.map_puts().len() == 2
                && typed.map_removes().is_empty()
                && typed.map_sizes().len() == 1
                && typed.map_with_values().len() == 2
                && typed
                    .map_constructions()
                    .iter()
                    .all(|construction| construction.expression().source_unit() == source)
                && typed
                    .map_puts()
                    .iter()
                    .all(|put| put.expression().source_unit() == source)
                && typed.map_sizes().iter().all(|size| {
                    size.expression().source_unit() == source
                        && size.receiver().source_unit() == source
                        && size.map_type() == consume.result_type()
                })
                && typed.map_with_values().iter().all(|read| {
                    read.expression().source_unit() == source
                        && read.receiver().source_unit() == source
                        && [read.key_type(), read.value_type()] == pair
                        && typed.expression_type(read.receiver()) == Some(consume.result_type())
                }),
            (
                typed.map_constructions(),
                typed.map_puts(),
                typed.map_sizes(),
                typed.map_with_values(),
            ),
            stages,
        )
    }

    pub(super) fn inspect_unit(
        source: SourceUnitId,
        parsed: &ParsedFile,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        stages: &mut Vec<String>,
    ) -> Outcome<()> {
        let index = names.names().index();
        gate(
            "actual p/provider.ko source for the cross-file consume",
            index.source_units().iter().any(|unit| {
                unit.id() == source
                    && unit.source_id() == parsed.source_id()
                    && unit
                        .key()
                        .logical_path()
                        .is_some_and(|path| path.as_str() == "p/provider.ko")
            }),
            index.source_units(),
            stages,
        )?;
        let mains: Vec<_> = index
            .declarations()
            .iter()
            .filter(|declaration| {
                declaration.name() == "main"
                    && declaration.kind() == SymbolKind::Function
                    && declaration.source_unit() == source
                    && declaration.name_span().source_id() == parsed.source_id()
            })
            .collect();
        let consumers: Vec<_> = index
            .source_units()
            .iter()
            .filter(|unit| {
                unit.key()
                    .logical_path()
                    .is_some_and(|path| path.as_str() == "q/consumer.ko")
            })
            .collect();
        let ([main], [consumer]) = (mains.as_slice(), consumers.as_slice()) else {
            return Err(failure(
                "unique real p.main and q consumer",
                (&mains, &consumers),
            ));
        };
        let calls: Vec<_> = typed
            .calls()
            .iter()
            .filter(|call| call.expression().source_unit() == consumer.id())
            .collect();
        gate(
            "real q.entry call selects actual p.main",
            matches!(calls.as_slice(), [call]
                if call.target() == UnitCallTarget::Declaration(main.id())
                    && call.arguments().is_empty() && call.receiver().is_none()),
            (&mains, &consumers, &calls),
            stages,
        )?;
        inspect_single(source, parsed, names, typed, owned, stages)
    }
}

const FIXTURE: Fixture = Fixture {
    source_name: "nullable-map-consume-strings.ko",
    source: include_str!("../../tests/fixtures/map-consume-0291/nullable-strings.ko"),
    consumer: "package q\nfun entry(): Unit { p.main() }\n",
    consumes: 1,
};

#[test]
fn complete_nullable_map_consume_string_fixture_reaches_verified_llvm() {
    let mut observations = Vec::with_capacity(2);
    for unit in [false, true] {
        let mut stages = Vec::new();
        let outcome = if unit {
            observe_unit_fixture(&FIXTURE, inputs::inspect_unit, &mut stages)
        } else {
            observe_single_fixture(&FIXTURE, inputs::inspect_single, &mut stages)
        };
        let entry = if unit {
            "unit q.entry -> p.main"
        } else {
            "single main"
        };
        observations.push((entry, stages, outcome));
    }
    let failed = observations.iter().any(|(_, _, outcome)| outcome.is_err());
    for (entry, stages, outcome) in &observations {
        if !failed {
            diagnostic!(
                "NULLABLE MAP CONSUME STRING FIXTURE {entry}: verified LLVM ({} recorded stages)",
                stages.len(),
            );
            continue;
        }
        diagnostic!(
            "NULLABLE MAP CONSUME STRING FIXTURE {entry}\n{}",
            stages.join("\n")
        );
        match outcome {
            Ok(()) => diagnostic!("outcome=verified LLVM"),
            Err(failure) => {
                diagnostic!("failed stage={}; detail={}", failure.stage, failure.detail)
            }
        }
    }
    assert_eq!(observations.len(), 2);
    assert_diagnostic!(
        observations.iter().all(|(_, _, outcome)| outcome.is_ok()),
        "both complete ordinary nullable String fixture entries must reach verified LLVM: {observations:#?}"
    );
}

pub(in crate::ssa) mod refine_string_when_tests {
    //! SPEC-0360: real Unit sources consume the SPEC-0357 comparison/edge facts.
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };

    use super::super::{
        model::{Operation, Program},
        refine::observe_routing,
        render::render_program,
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze, declaration, parsed},
    };

    /// Same source in a single unit and a two-source unit, both source orders. Each product
    /// passes the public Unit qualification, complete SSA verifier and LLVM verifier.
    fn lower(source: &str) -> (Program, String) {
        let mut sources = SourceMap::new();
        let text = format!("package p\n{source}");
        let (id, file) = parsed(&mut sources, "p/main.ko", &text);
        let (other_id, other) = parsed(
            &mut sources,
            "q/other.ko",
            "package q\nfun unrelated(): Int = 7",
        );
        let main = SourceUnitInput::new("root", "p/main.ko", id, &file);
        let other = SourceUnitInput::new("root", "q/other.ko", other_id, &other);
        let mut last = None;
        let mut ordered = None;
        for inputs in [vec![main], vec![main, other], vec![other, main]] {
            let (ne, te) = standard_environments();
            let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
            let entry = declaration(&names, "p", "main");
            let (result, routed) = observe_routing(|| {
                lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
            });
            let (program, entry) = result.unwrap_or_else(|error| panic!("{source}: {error:?}"));
            assert!(
                !routed.is_empty(),
                "String when must use refinement: {source}"
            );
            let rendered = render_program(&program);
            if inputs.len() == 2 {
                if let Some(expected) = &ordered {
                    assert_eq!(expected, &rendered, "source-order determinism");
                }
                ordered = Some(rendered);
            }
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry)
                .expect("String when LLVM verifies");
            last = Some((program, llvm));
        }
        last.expect("three input shapes")
    }

    fn counted(source: &str, allocations: usize, stdout: &[u8]) {
        let (_, llvm) = lower(source);
        let run =
            crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, allocations);
        crate::native_tests::boxed_enum_tests::assert_success(&run, stdout);
    }

    #[test]
    fn p41_original_source_runs_through_refinement() {
        let source = include_str!("../../../../conformance/probes/p41_when_string.ko");
        lower(source);
        let run = crate::native_tests::emit_link_and_run("p41.ko", source, "main");
        crate::native_tests::boxed_enum_tests::assert_success(&run, b"when-str\n");
    }

    #[test]
    fn temporary_subject_early_late_miss_and_grouped_candidates_free_once() {
        for (subject, expected) in [("a", "early"), ("b", "late"), ("z", "miss")] {
            let source = format!(
                r#"fun make(): String {{ println("subject"); return "{subject}" + "!" }}
        fun main(): Unit {{
            val n = when ((make())) {{ (("a!")), "c!" -> 1; "b!" -> 2; else -> 0 }}
            if (n == 1) {{ println("early") }}
            if (n == 2) {{ println("late") }}
            if (n == 0) {{ println("miss") }}
        }}"#
            );
            counted(&source, 1, format!("subject\n{expected}\n").as_bytes());
        }
    }

    #[test]
    fn named_subject_stays_live_for_body_and_after_when() {
        for value in ["a", "z"] {
            let source = format!(
                r#"fun main(): Unit {{
            val s = "{value}" + "!"
            when ((s)) {{ "a!" -> println(s); else -> println(s) }}
            println(s)
        }}"#
            );
            counted(&source, 1, format!("{value}!\n{value}!\n").as_bytes());
        }
    }

    #[test]
    fn no_else_and_zero_comparison_paths_free_temporary_once() {
        for body in [
            r#"when ("z" + "!") { "a!", "b!" -> println("unreachable") }; println("done")"#,
            r#"when (("z" + "!")) { else -> println("done") }"#,
            r#"when ("z" + "!") {}; println("done")"#,
        ] {
            counted(&format!("fun main(): Unit {{ {body} }}"), 1, b"done\n");
        }
    }

    #[test]
    fn borrowed_subject_uses_callers_loan_without_owner_drop() {
        let source = r#"fun select(s: String): Int = when ((s)) { "a!", "b!" -> 1; else -> 0 }
    fun main(): Unit {
        val s = "b" + "!"
        if (select(s) == 1) { println("match") }
        println(s)
        if (select("z" + "!") == 0) { println("miss") }
    }"#;
        let (program, _) = lower(source);
        let select = program.modules[0]
            .functions
            .iter()
            .find(|function| function.name.starts_with("koven.p.select.d"))
            .expect("select");
        assert_eq!(
            select
                .instructions
                .iter()
                .filter(|instruction| matches!(instruction.operation, Operation::Drop { .. }))
                .count(),
            2,
            "only the two literal candidates are dropped; the subject is a loan"
        );
        assert!(
            select
                .instructions
                .iter()
                .all(|instruction| !matches!(instruction.operation, Operation::Read { .. })),
            "a String loan cannot be read into an owned value"
        );
        counted(source, 2, b"match\nb!\nmiss\n");
    }

    #[test]
    fn body_return_break_and_continue_do_not_repeat_subject_cleanup() {
        let source = r#"fun choose(): Int {
        when ("b" + "!") { "a!" -> return 0; "b!" -> return 1 }; return 2
    }
    fun main(): Unit {
        if (choose() == 1) { println("return") }
        var i = 0
        loop {
            i = i + 1
            when ("b" + "!") { "b!" -> { if (i == 1) continue else break } }
        }
        println("loop")
    }"#;
        counted(source, 3, b"return\nloop\n");
    }

    #[test]
    fn each_evaluated_candidate_drops_after_comparison_and_suffix_is_not_evaluated() {
        use super::super::model::{EntityId, SsaTypeKind};
        use std::{fs, process::Command};
        // Three distinct source candidates; early, late and no-match execute 1/3/3.
        for (subject, comparisons, expected) in [("a", 1, 1), ("b", 3, 2), ("z", 3, 0)] {
            let source = format!(
                r#"fun main(): Unit {{
            val n = when ("{subject}" + "!") {{ "a!", "c!" -> 1; "b!" -> 2; else -> 0 }}
            if (n == {expected}) {{ println("ok") }}
        }}"#
            );
            let (program, llvm) = lower(&source);
            let function = program.modules[0]
                .functions
                .iter()
                .find(|function| function.name.starts_with("koven.p.main.d"))
                .unwrap();
            let mut sites = 0;
            for block in &function.blocks {
                for (index, id) in block.instructions.iter().enumerate() {
                    let instruction = function.instruction(*id).unwrap();
                    if let Operation::StringEqual {
                        right: EntityId::Value(candidate),
                        ..
                    } = instruction.operation
                    {
                        sites += 1;
                        let next = function.instruction(block.instructions[index + 1]).unwrap();
                        assert!(
                            matches!(next.operation, Operation::Drop { .. }),
                            "Completed drops the candidate immediately after equality"
                        );
                        // The precise owner ID is printed in debug output on failure; the
                        // complete verifier additionally rejects any read after this drop.
                        assert!(
                            matches!(next.operation, Operation::Drop { owner } if owner == candidate)
                        );
                    }
                }
            }
            assert_eq!(sites, 3, "one comparison per source candidate");
            let string = program.modules[0]
                .types
                .iter()
                .position(|ty| *ty == SsaTypeKind::StringOwner)
                .unwrap();
            let mut instrumented = String::new();
            let mut drop_sites = 0;
            for line in llvm.lines() {
                instrumented.push_str(line);
                instrumented.push('\n');
                if line.contains(&format!("call void @koven.drop.t{string}(")) {
                    instrumented.push_str("  call void @counted_drop()\n");
                    drop_sites += 1;
                }
            }
            assert!(drop_sites > 0);
            instrumented.push_str("declare void @counted_drop()\n");
            let directory = crate::native_tests::TestDirectory::create();
            let ir = directory.join("when.ll");
            let counter = directory.join("counter.c");
            let executable = directory.join("when");
            fs::write(&ir, instrumented).unwrap();
            // Two concat operands, one subject, evaluated candidates, and the output literal.
            fs::write(
                &counter,
                format!(
                    r#"#include <assert.h>
static int drops;
void counted_drop(void) {{ ++drops; }}
__attribute__((destructor)) static void verify(void) {{ assert(drops == {}); }}
"#,
                    comparisons + 4
                ),
            )
            .unwrap();
            let linked = Command::new(crate::test_support::ir_clang())
                .arg(&ir)
                .arg(&counter)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(linked.status.success(), "{linked:?}");
            let run = Command::new(executable).output().unwrap();
            crate::native_tests::boxed_enum_tests::assert_success(&run, b"ok\n");
        }
    }

    #[test]
    fn named_owner_cleanup_follows_selected_body_liveness() {
        for (value, expected) in [("a", "a!"), ("z", "miss")] {
            let source = format!(
                r#"fun main(): Unit {{
            val s = "{value}" + "!"
            when (s) {{ "a!" -> println(s); else -> println("miss") }}
        }}"#
            );
            counted(&source, 1, format!("{expected}\n").as_bytes());
        }
    }

    #[test]
    fn borrowed_subject_cross_source_call_verifies_and_preserves_caller_owner() {
        let mut sources = SourceMap::new();
        let (provider_id, provider) = parsed(
            &mut sources,
            "p/select.ko",
            "package p\nfun select(s: String): Int = when (s) { \"b!\" -> 1; else -> 0 }",
        );
        let (consumer_id, consumer) = parsed(
            &mut sources,
            "q/main.ko",
            r#"package q
    fun main(): Unit {
        val s = "b" + "!"
        if (p.select(s) == 1) { println(s) }
        println(s)
    }"#,
        );
        let inputs = [
            SourceUnitInput::new("root", "p/select.ko", provider_id, &provider),
            SourceUnitInput::new("root", "q/main.ko", consumer_id, &consumer),
        ];
        let mut expected = None;
        for inputs in [inputs, [inputs[1], inputs[0]]] {
            let (ne, te) = standard_environments();
            let (names, typed, owned) = analyze(&sources, &inputs, &ne, &te);
            let entry = declaration(&names, "q", "main");
            let (program, entry) =
                lower_scalar_unit_with_entry(&sources, &inputs, &names, &te, &typed, &owned, entry)
                    .expect("source-qualified borrowed subject lowers");
            let rendered = render_program(&program);
            if let Some(expected) = &expected {
                assert_eq!(&rendered, expected);
            }
            expected = Some(rendered);
            let llvm = crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
            let run = crate::native_tests::boxed_enum_tests::run_counted_allocations(&llvm, 1);
            crate::native_tests::boxed_enum_tests::assert_success(&run, b"b!\nb!\n");
        }
    }

    #[test]
    fn original_fact_gate_sources_succeed_on_public_routing() {
        for source in [
            "fun entry(): Int { when (\"subject\") { \"first\", \"second\" -> {} }\nreturn 0 }",
            "fun entry(): Int { when (\"subject\") { else -> {} }\nreturn 0 }",
        ] {
            lower(&format!(
                "{source}\nfun main(): Unit {{ if (entry() != 0) {{ println(\"bad\") }} }}"
            ));
        }
    }
}
