//! SPEC-0331 的普通 Map provider 成功合同；六源 × MemoryDocument/rooted Unit 全采后断。
//! 不执行 native，不以当前 lower 的 UnsupportedNode 债务作为通过条件。
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};

pub(in crate::ssa) mod borrowed;
mod ci_baseline {
    //! 原真实单 Map plan 的 MemoryDocument/rooted Unit baseline；不混入六源 Guard oracle。
    use super::{Report, pipeline};
    use crate::ssa::{model::Program, verify::verify_program};
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
    use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

    const SOURCE: &str = r#"
    fun scan(first: Map<String, String>): Unit {
      for ((key, value) in first) { println(key); println(value) }
    }
    fun entry(): Unit {
      val first = mapOf<String, String>()
      scan(first)
    }
    "#;

    pub(super) fn baseline(rooted: bool) -> Program {
        let mut sources = SourceMap::new();
        let id = sources.add_source("provider.ko", SOURCE).unwrap();
        let lexed = lex(&sources, id).unwrap();
        assert!(lexed.diagnostics().is_empty());
        let parsed = parse_file(&sources, &lexed).unwrap();
        assert!(parsed.diagnostics().is_empty());
        let mut report = Report::default();
        let (program, _) = if rooted {
            pipeline::unit_observed(
                &sources,
                &parsed,
                "provider.ko",
                &mut report,
                |_, _, owned, r| {
                    r.check(
                        "original rooted Unit Map provider",
                        owned.map_iterations().len() == 1,
                        owned.map_iterations(),
                    );
                },
            )
        } else {
            pipeline::memory_document_observed(&sources, &parsed, &mut report, |_, _, owned, r| {
                r.check(
                    "original MemoryDocument Unit Map provider",
                    owned.map_iterations().len() == 1,
                    owned.map_iterations(),
                );
            })
        }
        .unwrap();
        assert_diagnostic!(report.passed(), "{report:#?}");
        verify_program(&program).expect("unmodified real SSA must pass before corruption");
        crate::llvm::render_verified_program(&program).expect("unmodified real LLVM must pass");
        program
    }

    #[test]
    fn both_real_provider_baselines_reach_verified_llvm() {
        for rooted in [false, true] {
            baseline(rooted);
            diagnostic!(
                "SPEC0331 ORDINARY CI BASELINE input={} SSA=verified LLVM=verified",
                super::input_label(rooted)
            );
        }
    }
}
#[allow(unused_imports)] pub(in crate::ssa) use local_future::ci_contracts;
mod conditional;
mod control_flow;
mod d10_temporary_carry;
mod facts;
mod fixture;
#[allow(unused_imports)] pub(in crate::ssa) use scope_value_births::formation_runtime;
mod future_use;
#[allow(unused_imports)] pub(in crate::ssa) use local_future::history;
#[allow(unused_imports)] pub(in crate::ssa) use resource::identity;
mod local_arguments;
pub(crate) use local_arguments::{
    generic_prerequisite_sources as local_generic_argument_prerequisite_sources,
    generic_sources as local_generic_argument_sources, sources as local_argument_sources,
};

pub(in crate::ssa) mod local_sources;
mod nested;
pub(in crate::ssa) mod nested_exits;
#[allow(unused_imports)] pub(in crate::ssa) use resource_forms::pipeline;
#[allow(unused_imports)] pub(in crate::ssa) use scope_value_births::query_cleanup;
#[allow(unused_imports)] pub(in crate::ssa) use scope_lowering::query_graph;
pub(in crate::ssa) use scope_lowering::query_graph::Graph as OrdinaryGraph;
pub(in crate::ssa) use borrowed::query_inputs::one as one_query;
#[allow(unused_imports)] pub(in crate::ssa) use borrowed::query_inputs;
mod query_inventory {
    //! 完整定义库存只排除额外能力；expected 必须来自调用方的真实 birth 与逐边身份证明。
    use super::{
        fixture::{Failure, failure, need},
        query_graph::Graph,
    };
    use crate::ssa::model::*;
    use std::collections::BTreeSet;

    pub(super) fn check(
        graph: &Graph<'_>,
        relevant_semantic_types: &[SsaTypeId],
        expected_entities: &BTreeSet<EntityId>,
    ) -> Result<(), Failure> {
        let f = graph.function;
        let blocks = graph.blocks.iter().copied().collect::<BTreeSet<_>>();
        let instructions = graph
            .instructions
            .iter()
            .map(|i| i.id)
            .collect::<BTreeSet<_>>();
        need(
            blocks.len() == graph.blocks.len()
                && blocks.len() == f.blocks.len()
                && f.blocks.iter().enumerate().all(|(index, block)| {
                    block.id.function() == f.id
                        && block.id.index() == index
                        && blocks.contains(&block.id)
                })
                && instructions.len() == graph.instructions.len()
                && instructions.len() == f.instructions.len()
                && f.instructions
                    .iter()
                    .enumerate()
                    .all(|(index, instruction)| {
                        instruction.id.function() == f.id
                            && instruction.id.index() == index
                            && instructions.contains(&instruction.id)
                    }),
            "Unit identity inventory covers exact reachable block/instruction arenas",
            (&graph.blocks, &instructions),
        )?;
        let mut entities = BTreeSet::new();
        for block in &f.blocks {
            for (index, &entity) in block.parameters.iter().enumerate() {
                need(
                    entity.function() == f.id
                        && entities.insert(entity)
                        && f.entity(entity).is_some_and(|data| {
                            data.definition
                                == Definition::BlockParameter {
                                    block: block.id,
                                    index,
                                }
                                && data.origin == block.origin
                        }),
                    "Unit identity inventory exact parameter definition and origin",
                    (block.id, index, entity),
                )?;
            }
        }
        for instruction in &f.instructions {
            need(
                f.block(instruction.block).is_some_and(|block| {
                    block
                        .instructions
                        .iter()
                        .filter(|&&id| id == instruction.id)
                        .count()
                        == 1
                }),
                "Unit identity inventory exact instruction block membership",
                instruction.id,
            )?;
            for (index, &entity) in instruction.results.iter().enumerate() {
                need(
                    entity.function() == f.id
                        && entities.insert(entity)
                        && f.entity(entity).is_some_and(|data| {
                            data.definition
                                == Definition::InstructionResult {
                                    instruction: instruction.id,
                                    index,
                                }
                                && data.origin == instruction.origin
                        }),
                    "Unit identity inventory exact result definition and origin",
                    (instruction.id, index, entity),
                )?;
            }
        }
        need(
            entities.len() == f.values.len() + f.loans.len() + f.places.len(),
            "Unit identity inventory excludes every orphan entity",
            &entities,
        )?;
        for &entity in expected_entities {
            let data = f
                .entity(entity)
                .ok_or_else(|| failure("Unit identity inventory actual proven entity", entity))?;
            need(
                entities.contains(&entity)
                    && relevant_semantic_types.contains(&data.ty.semantic_type()),
                "Unit identity inventory expected roles belong to exact relevant types",
                (entity, data),
            )?;
        }
        let actual = entities
            .into_iter()
            .filter(|&entity| {
                f.entity(entity)
                    .is_some_and(|data| relevant_semantic_types.contains(&data.ty.semantic_type()))
            })
            .collect::<BTreeSet<_>>();
        need(
            &actual == expected_entities,
            "Unit identity complete related owner/loan/place inventory has no extra capability",
            (&actual, expected_entities),
        )
    }
}
#[allow(unused_imports)] pub(in crate::ssa) use scope_value_births::query_map_normal_body;
#[allow(unused_imports)] pub(in crate::ssa) use borrowed::query_owner;
mod query_ssa;
#[allow(unused_imports)] pub(in crate::ssa) use scope_value_births::query_temporary_cleanup;
mod query_test {
    //! 独立 ordinary exact：原六源码 × MemoryDocument/rooted Unit 全采后断，不变异、不执行 native。
    use super::{
        Observation, Report,
        fixture::{Fixture, failure, verify_failure},
        identity, input_label, pipeline, query_ssa,
    };
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};

    fn observe(label: &'static str, text: &str, rooted: bool) -> Observation {
        let mut report = Report {
            identity_requested: true,
            ..Report::default()
        };
        let input = input_label(rooted);
        let outcome = Fixture::new(label, text).and_then(|fixture| {
            diagnostic!(
                "SPEC0331 EXIT IDENTITY INPUT case={label} input={input} path={:?} text={text:?}",
                fixture.path
            );
            let expected = identity::source_functions(&fixture);
            report.query("identity original source functions", &expected);
            let (program, entry) = if rooted {
                pipeline::unit(&fixture, &mut report)
            } else {
                pipeline::memory_document(&fixture, &mut report)
            }?;
            diagnostic!(
                "SPEC0331 EXIT IDENTITY RAW SSA case={label} input={input}\n{}",
                crate::ssa::render::render_program(&program)
            );
            let reachable = expected
                .as_ref()
                .map_err(|e| failure("identity AST prerequisite", e))
                .and_then(|functions| {
                    identity::entry_sources(&fixture.sources, &program, entry, functions)
                });
            report.query(
                "identity actual entry and reachable source functions",
                &reachable,
            );
            let guard_operand = expected
                .and_then(|functions| identity::guard_and_operand(&fixture, &program, &functions));
            report.query(
                "identity existing actual Guard/operand query",
                &guard_operand,
            );
            let inputs = report.fe_identity.take().unwrap_or_else(|| {
                Err(failure(
                    "identity same-run FE bridge missing",
                    (label, input),
                ))
            });
            report.query(
                "identity FE original parameter/call/cleanup bridge",
                &inputs,
            );
            let entities = inputs
                .as_ref()
                .map_err(|e| failure("identity FE prerequisite", e))
                .and_then(|inputs| query_ssa::check(&program, inputs));
            report.query(
                "identity complete SSA entity/cleanup/all-incoming proof",
                &entities,
            );
            // Independent query errors never hide either original verifier or LLVM outcome.
            let verified = crate::ssa::verify::verify_program(&program)
                .map_err(|errors| verify_failure(&fixture.sources, errors));
            report.query("identity pipeline original SSA verifier", &verified);
            let rendered = crate::llvm::render_verified_program(&program)
                .map_err(|error| failure("LLVM render/verify", error));
            report.query(
                "identity pipeline original LLVM render/verify",
                &rendered
                    .as_ref()
                    .map(|s| s.len())
                    .map_err(|e| failure(e.stage, &e.original)),
            );
            if let Ok(llvm) = &rendered {
                diagnostic!("SPEC0331 EXIT IDENTITY RAW LLVM case={label} input={input}\n{llvm}");
            }
            // This exact adds SSA identity evidence only; LLVM coverage remains render/verify.
            verified?;
            rendered?;
            Ok(())
        });
        Observation {
            label,
            rooted,
            report,
            outcome,
        }
    }

    #[test]
    fn unconditional_map_exits_preserve_cleanup_and_carrier_identity() {
        let mut observations = Vec::new();
        for (label, source, _golden) in super::sources() {
            for rooted in [false, true] {
                observations.push(observe(label, source, rooted));
            }
        }
        for observation in &observations {
            diagnostic!(
                "SPEC0331 EXIT IDENTITY OUTCOME case={} input={} report={:#?} result={:#?}",
                observation.label,
                input_label(observation.rooted),
                observation.report,
                observation.outcome
            );
        }
        assert_diagnostic!(
            observations.len() == 12
                && observations
                    .iter()
                    .all(|o| o.report.passed() && o.outcome.is_ok()),
            "原六源×MemoryDocument/rooted Unit必须完整采齐，FE/SSA身份查询和原verify/LLVM分别全绿。\n{observations:#?}"
        );
    }
}
pub(in crate::ssa) mod resource;
pub(in crate::ssa) mod resource_forms;
pub(crate) mod scope_future_observer;
pub(crate) use scope_future_observer::cases as scope_future_sources;
mod sequential;
mod source_forms;
mod stack;
mod temporary;
mod temporary_argument_cfg {
    //! Ordinary argument CFG delivers its actual Call owner to the existing provider entry.
    use super::{
        Report,
        fixture::{Failure, clean, failure},
        pipeline, source_forms,
    };
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
    use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

    pub(super) fn source(mutable: bool, control: bool) -> String {
        let kind = if mutable { "MutableMap" } else { "Map" };
        let factory = if mutable { "mutableMapOf" } else { "mapOf" };
        let body = if control {
            "val source=produce(if(flag){1}else{2});if(source.size==0){println(\"control\")}else{error(\"bad-size\")}"
        } else {
            "for((key,value) in produce(if(flag){1}else{2})){read(key,value)}"
        };
        format!(
            "fun produce(input:Int):{kind}<String,String>{{if(input==1){{println(\"one\")}}else{{println(\"two\")}};return {factory}<String,String>()}}\nfun read(key:String,value:String):Unit{{println(key);println(value)}}\nfun entry(flag:Boolean):Unit{{{body}}}\n"
        )
    }

    fn observe(mutable: bool, rooted: bool, control: bool) -> Result<(), Failure> {
        let text = source(mutable, control);
        diagnostic!(
            "ARGUMENT CFG INPUT mutable={mutable} rooted={rooted} control={control} source={text:?}"
        );
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("provider.ko", &text)
            .map_err(|e| failure("source", e))?;
        let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
        clean(&sources, "lex", lexed.diagnostics())?;
        let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
        clean(&sources, "parse", parsed.diagnostics())?;
        let mut report = Report::default();
        let lowered = pipeline::unit_input_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            rooted,
            |n, t, o, r| source_forms::facts::unit(&sources, &parsed, n, t, o, r),
        );
        diagnostic!(
            "ARGUMENT CFG LOWER mutable={mutable} rooted={rooted} control={control} report={report:#?} outcome={:?}",
            lowered.as_ref().map(|(_, entry)| entry)
        );
        if !report.passed() {
            return Err(failure("original FE queries", report));
        }
        let (program, _) = lowered?;
        report.query(
            "Unit original temporary entry and actual canonical owner acquisition",
            &source_forms::graph::temporary_entries(&parsed, &program),
        );
        report.query(
            "actual ordinary argument CFG and returned owner identity",
            &super::temporary_argument_cfg_query::check(&sources, &parsed, &program),
        );
        crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
        if !report.passed() {
            return Err(failure("original CFG argument and owner queries", report));
        }
        Ok(())
    }

    fn check(control: bool) {
        let mut outcomes = Vec::new();
        for mutable in [false, true] {
            for rooted in [false, true] {
                let result = observe(mutable, rooted, control);
                diagnostic!(
                    "ARGUMENT CFG CASE mutable={mutable} rooted={rooted} control={control} result={result:?}"
                );
                outcomes.push(result);
            }
        }
        assert_eq!(outcomes.len(), 4);
        assert_diagnostic!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
    }

    #[test]
    fn ordinary_call_with_argument_cfg_reaches_both_pipelines() {
        check(true);
    }

    #[test]
    fn provider_argument_cfg_reaches_both_pipelines() {
        check(false);
    }
}
mod temporary_argument_cfg_query;
mod temporary_argument_cfg_runtime {
    //! Execute both argument CFG edges through the original empty-Map source and ordinary call.
    use super::{
        Report,
        fixture::{Failure, clean, failure},
        pipeline, source_forms, temporary_argument_cfg,
    };
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
    use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

    pub(crate) struct Case {
        pub label: String,
        pub source: String,
        pub golden: String,
    }

    pub(crate) fn sources() -> Vec<Case> {
        let mut cases = Vec::new();
        for mutable in [false, true] {
            for control in [false, true] {
                for flag in [false, true] {
                    let original = temporary_argument_cfg::source(mutable, control);
                    let declaration = "fun entry(flag:Boolean)";
                    assert_eq!(original.matches(declaration).count(), 1);
                    let old_guard = "if(input==1){println(\"one\")}else{println(\"two\")}";
                    assert_eq!(original.matches(old_guard).count(), 1);
                    let scan = original
                        .replace(declaration, "fun scan(flag:Boolean)")
                        .replace(old_guard, "if(input==1){println(\"one\")}else{if(input==2){println(\"two\")}else{error(\"bad-input\")}}");
                    let source = format!(
                        "{scan}fun entry():Unit{{println(\"begin\");scan({flag});println(\"after\")}}\n"
                    );
                    let kind = if mutable { "MutableMap" } else { "Map" };
                    let branch = if flag { "one" } else { "two" };
                    let middle = if control { "control\n" } else { "" };
                    cases.push(Case {
                        label: format!("temporary-argument-CFG-{kind}-flag-{flag}-control-{control}"),
                        source,
                        golden: format!("begin\n{branch}\n{middle}after\n"),
                    });
                }
            }
        }
        assert_eq!(cases.len(), 8);
        cases
    }

    fn observe(case: &Case, rooted: bool) -> Result<(), Failure> {
        diagnostic!(
            "ARGUMENT CFG RUNTIME INPUT case={} rooted={rooted} source={:?} golden={:?}",
            case.label,
            case.source,
            case.golden
        );
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("provider.ko", &case.source)
            .map_err(|e| failure("source", e))?;
        let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
        clean(&sources, "lex", lexed.diagnostics())?;
        let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
        clean(&sources, "parse", parsed.diagnostics())?;
        let mut report = Report::default();
        let result = pipeline::unit_input_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            rooted,
            |n, t, o, r| {
                source_forms::facts::unit(&sources, &parsed, n, t, o, r);
            },
        );
        diagnostic!(
            "ARGUMENT CFG RUNTIME LOWER {} rooted={rooted} report={report:#?} outcome={:?}",
            case.label,
            result.as_ref().map(|(_, entry)| entry)
        );
        if !report.passed() {
            return Err(failure("original runtime FE queries", report));
        }
        let (program, _) = result?;
        report.query(
            "Unit original temporary entry and actual canonical owner acquisition",
            &source_forms::graph::temporary_entries(&parsed, &program),
        );
        report.query(
            "actual runtime argument CFG and hidden owner identity",
            &super::temporary_argument_cfg_query::check(&sources, &parsed, &program),
        );
        crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
        if !report.passed() {
            return Err(failure(
                "original runtime CFG argument and owner queries",
                report,
            ));
        }
        Ok(())
    }

    #[test]
    fn both_argument_cfg_edges_reach_both_pipelines() {
        let mut outcomes = Vec::new();
        for case in sources() {
            for rooted in [false, true] {
                let result = observe(&case, rooted);
                diagnostic!(
                    "ARGUMENT CFG RUNTIME CASE {} rooted={rooted} result={result:?}",
                    case.label
                );
                outcomes.push(result);
            }
        }
        assert_eq!(outcomes.len(), 16);
        assert_diagnostic!(outcomes.iter().all(Result::is_ok), "{outcomes:#?}");
    }
}
mod temporary_arguments;
pub(crate) mod temporary_delivery;
mod temporary_delivery_boundaries {
    //! Ordinary invalid aliasing stays separate from the supported local Borrow-result continuation.
    use super::{
        Report,
        fixture::{Failure, clean, failure},
        pipeline,
        temporary_delivery::{Delivery, sources},
    };
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
    use lang_frontend::{lexer::lex, parser::parse_file, source::SourceMap};

    fn observe(text: &str, rooted: bool) -> Result<(), Failure> {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("provider.ko", text)
            .map_err(|e| failure("source", e))?;
        let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
        clean(&sources, "lex", lexed.diagnostics())?;
        let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
        clean(&sources, "parse", parsed.diagnostics())?;
        let mut report = Report::default();
        let result = pipeline::unit_input_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            rooted,
            |_, _, _, _| {},
        );
        let input = super::input_label(rooted);
        diagnostic!(
            "DELIVERY BOUNDARY INPUT input={input} source={text:?} report={report:#?} outcome={:?}",
            result.as_ref().map(|(_, entry)| entry)
        );
        let (program, _) = result?;
        if !report.passed() {
            return Err(failure("original boundary FE queries", report));
        }
        crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
        Ok(())
    }

    #[test]
    fn same_root_borrow_then_owned_delivery_is_rejected_before_lowering() {
        let mut outcomes = Vec::new();
        for case in sources()
            .into_iter()
            .filter(|case| matches!(case.delivery, Delivery::BorrowAndOwnedResource))
        {
            for control in [false, true] {
                let source = if control {
                    case.control.clone()
                } else {
                    case.source.clone()
                };
                let call = "produce(borrowedArgument(),ownedArgument())";
                assert_eq!(source.matches(call).count(), 1);
                let text = source
                    .replace(
                        "println(\"begin\");",
                        "println(\"begin\");val shared=borrowedArgument();",
                    )
                    .replace(call, "produce(shared,shared)");
                for rooted in [false, true] {
                    let result = observe(&text, rooted);
                    let input = super::input_label(rooted);
                    diagnostic!(
                        "DELIVERY BOUNDARY SAME ROOT {} input={input} control={control} result={result:?}",
                        case.label
                    );
                    outcomes.push(result.is_err_and(|e| {
                        e.stage.ends_with("ownership diagnostics")
                            && e.original.matches("code: L0135").count() == 1
                            && e.original.contains("move conflicts with an active loan")
                    }));
                }
            }
        }
        assert_eq!(outcomes.len(), 8);
        assert_diagnostic!(outcomes.iter().all(|&passed| passed), "{outcomes:?}");
    }

    // This observer is exclusive to the retired Legacy-rejection contract below.
    // Each layout keeps its own original SourceMap/parsed/name/type/ownership chain.
    fn observe_local_result_unit(text: &str, rooted: bool) -> Result<(), Failure> {
        let mut sources = SourceMap::new();
        let id = sources
            .add_source("provider.ko", text)
            .map_err(|e| failure("source", e))?;
        let lexed = lex(&sources, id).map_err(|e| failure("lex", e))?;
        clean(&sources, "lex", lexed.diagnostics())?;
        let parsed = parse_file(&sources, &lexed).map_err(|e| failure("parse", e))?;
        clean(&sources, "parse", parsed.diagnostics())?;
        let mut report = Report::default();
        let result = pipeline::unit_input_observed(
            &sources,
            &parsed,
            "provider.ko",
            &mut report,
            rooted,
            |_, _, _, _| {},
        );
        let input = super::input_label(rooted);
        diagnostic!(
            "DELIVERY LOCAL RESULT INPUT input={input} stages_passed={} outcome={:?}",
            report.passed(),
            result.as_ref().map(|(_, entry)| entry)
        );
        if result.is_err() || !report.passed() {
            diagnostic!(
                "DELIVERY LOCAL RESULT FAILURE input={input} source={text:?} report={report:#?}"
            );
        }
        let (program, _) = result?;
        if !report.passed() {
            return Err(failure(
                "local Borrow-result Unit FE and entry identity",
                report,
            ));
        }
        crate::ssa::verify::verify_program(&program).map_err(|e| failure("SSA", e))?;
        crate::llvm::render_verified_program(&program).map_err(|e| failure("LLVM", e))?;
        Ok(())
    }

    // Historical exact name retained for CI identity. The retired Legacy pipeline
    // rejected `result.consume()` as a non-entry slot; this is not a Unit boundary.
    // Both original single-source layouts must pass full FE admission and SSA/LLVM.
    #[test]
    fn local_borrow_result_is_not_an_original_entry_slot() {
        let source = "class Resource(val tag:String)\nfun view(input:Resource):borrow Resource from input=input\nfun observe(input:Resource):Unit{println(input.tag)}\nfun entry():Unit{val input=Resource(\"input\".clone());borrow val alias=view(input);var result=mutableMapOf<String,String>();result.put(\"key\".clone(),\"value\".clone());val output=result.consume();observe(alias);println(\"done\")}\n";
        let mut outcomes = Vec::new();
        for rooted in [false, true] {
            let result = observe_local_result_unit(source, rooted);
            let input = super::input_label(rooted);
            diagnostic!("DELIVERY BOUNDARY LOCAL RESULT input={input} result={result:?}");
            outcomes.push(result.is_ok());
        }
        assert_eq!(outcomes.len(), 2);
        assert_diagnostic!(outcomes.iter().all(|&passed| passed), "{outcomes:?}");
    }
}
pub(in crate::ssa) mod temporary_exits;
pub(in crate::ssa) mod temporary_forms;

pub(crate) use borrowed::sources as borrowed_exit_sources;
pub(crate) use conditional::sources as conditional_sources;
pub(crate) use control_flow::future_transfer_sources;
pub(crate) use control_flow::neighbor_sources as ordinary_control_flow_neighbor_sources;
pub(crate) use control_flow::populated_future_transfer_sources;
pub(crate) use control_flow::return_sources as ordinary_return_sources;
pub(crate) use control_flow::sources as ordinary_control_flow_sources;
pub(crate) use scope_value_births::formation_runtime::sources as formation_runtime_sources;
pub(crate) use future_use::sources as future_use_sources;
pub(crate) use local_future::history::{sanitizer_ir as history_sanitizer_ir, sources as history_sources};
pub(crate) use local_sources::native_sources as local_native_sources;
pub(crate) use nested::sources as nested_sources;
pub(crate) use nested_exits::native_sources as nested_exit_sources;
pub(crate) use resource::continuation::sources as resource_continuation_sources;
pub(crate) use resource::continued::sources as resource_continue_sources;
pub(crate) use resource::nullable::sources as resource_nullable_sources;
pub(crate) use resource::sources as resource_sources;
pub(crate) use resource::terminated::returning::sources as resource_return_sources;
pub(crate) use resource::terminated::sources as resource_break_sources;
pub(crate) use resource_forms::sources as resource_form_sources;
pub(crate) use sequential::sources as sequential_sources;
pub(crate) use source_forms::sources as source_form_sources;
pub(crate) use stack::{
    Case as StackCase, owned_resource_sources as stack_owned_resource_sources,
    runtime_sources as stack_runtime_sources, variant_sources as stack_variant_sources,
};
pub(crate) use temporary::sources as temporary_sources;
pub(crate) use temporary_argument_cfg_runtime::sources as temporary_argument_cfg_sources;
pub(crate) use temporary_arguments::sources as temporary_argument_sources;
pub(crate) use temporary_delivery::sources as temporary_delivery_sources;
pub(crate) use temporary_exits::native_sources as temporary_exit_sources;
pub(crate) use temporary_forms::sources as temporary_form_sources;

use fixture::{Failure, Fixture};

/// 同字节源码和拟议 stdout，供后续独立获准的 native 测试复用。
/// 本 exact 只消费源码；golden 是期望，不是执行结果。
pub(crate) fn sources() -> [(&'static str, &'static str, &'static str); 6] {
    [
        (
            "map-continue",
            include_str!("../../tests/fixtures/map-provider-exits-0331/map-continue.ko"),
            include_str!("../../tests/fixtures/map-provider-exits-0331/map-continue.stdout"),
        ),
        (
            "map-break",
            include_str!("../../tests/fixtures/map-provider-exits-0331/map-break.ko"),
            include_str!("../../tests/fixtures/map-provider-exits-0331/map-break.stdout"),
        ),
        (
            "map-return",
            include_str!("../../tests/fixtures/map-provider-exits-0331/map-return.ko"),
            include_str!("../../tests/fixtures/map-provider-exits-0331/map-return.stdout"),
        ),
        (
            "mutable-map-continue",
            include_str!("../../tests/fixtures/map-provider-exits-0331/mutable-map-continue.ko"),
            include_str!(
                "../../tests/fixtures/map-provider-exits-0331/mutable-map-continue.stdout"
            ),
        ),
        (
            "mutable-map-break",
            include_str!("../../tests/fixtures/map-provider-exits-0331/mutable-map-break.ko"),
            include_str!("../../tests/fixtures/map-provider-exits-0331/mutable-map-break.stdout"),
        ),
        (
            "mutable-map-return",
            include_str!("../../tests/fixtures/map-provider-exits-0331/mutable-map-return.ko"),
            include_str!("../../tests/fixtures/map-provider-exits-0331/mutable-map-return.stdout"),
        ),
    ]
}

#[derive(Debug, Default)]
struct Report {
    checks: Vec<(&'static str, bool, String)>,
    identity_requested: bool,
    fe_identity: Option<Result<query_inputs::Inputs, Failure>>,
}

impl Report {
    fn check(&mut self, name: &'static str, passed: bool, detail: impl std::fmt::Debug) {
        self.checks.push((name, passed, format!("{detail:?}")));
    }

    fn query<T: std::fmt::Debug>(&mut self, name: &'static str, result: &Result<T, Failure>) {
        self.check(name, result.is_ok(), result);
    }

    fn passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|(_, passed, _)| *passed)
    }
}

#[derive(Debug)]
struct Observation {
    label: &'static str,
    rooted: bool,
    report: Report,
    outcome: Result<(), Failure>,
}

fn input_label(rooted: bool) -> &'static str {
    if rooted {
        "rooted Unit"
    } else {
        "MemoryDocument Unit"
    }
}

fn observe(label: &'static str, text: &str, rooted: bool) -> Observation {
    let mut report = Report::default();
    let input = input_label(rooted);
    diagnostic!(
        "SPEC0331 PROVIDER EXIT INPUT case={label} input={input} path=provider.ko text={text:?}"
    );
    let outcome = Fixture::new(label, text).and_then(|fixture| {
        let expected_functions = identity::source_functions(&fixture);
        report.query("source function AST identity", &expected_functions);
        let lowered = if rooted {
            pipeline::unit(&fixture, &mut report)
        } else {
            pipeline::memory_document(&fixture, &mut report)
        }?;
        let (program, entry) = lowered;
        diagnostic!(
            "SPEC0331 PROVIDER EXIT SSA case={label} input={input}\n{}",
            super::render::render_program(&program)
        );
        // 只读 query 的失败留在独立 report，不吞 lower 已到达的 verify/LLVM 原错误。
        let identity = expected_functions
            .as_ref()
            .map_err(|error| fixture::failure("AST identities", error))
            .and_then(|expected| {
                identity::entry_sources(&fixture.sources, &program, entry, expected)
            });
        report.query("returned entry and reachable source functions", &identity);
        let entities = expected_functions
            .and_then(|expected| identity::guard_and_operand(&fixture, &program, &expected));
        report.query(
            "reachable actual Guard and operand result entities",
            &entities,
        );
        super::verify::verify_program(&program)
            .map_err(|errors| fixture::verify_failure(&fixture.sources, errors))?;
        report.check("SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|error| fixture::failure("LLVM render/verify", error))?;
        report.check("LLVM render/verify", true, llvm.len());
        diagnostic!("SPEC0331 PROVIDER EXIT LLVM case={label} input={input}\n{llvm}");
        Ok(())
    });
    Observation {
        label,
        rooted,
        report,
        outcome,
    }
}

#[test]
fn map_and_mutable_map_unconditional_exits_reach_verified_llvm() {
    let mut observations = Vec::new();
    for (label, source, _golden) in sources() {
        for rooted in [false, true] {
            observations.push(observe(label, source, rooted));
        }
    }
    // 所有真实观察结束后只有这一个硬成功断言，首红不能短路另外十一格。
    for observation in &observations {
        diagnostic!(
            "SPEC0331 PROVIDER EXIT OUTCOME case={} input={} report={:#?}",
            observation.label,
            input_label(observation.rooted),
            observation.report
        );
        match &observation.outcome {
            Ok(()) => {
                diagnostic!("stage=LLVM render/verify result=ok")
            }
            Err(error) => diagnostic!("stage={} original={}", error.stage, error.original),
        }
    }
    assert_diagnostic!(
        observations.len() == 12
            && observations
                .iter()
                .all(|observation| { observation.report.passed() && observation.outcome.is_ok() }),
        "十二格必须是同源完整 frontend、真实 entry/CFG 可达身份、SSA 与 LLVM 全成功；\
         原阶段失败保持失败，UnsupportedNode 不是成功 oracle。\n{observations:#?}"
    );
}

pub(in crate::ssa) mod local_exits;

pub(crate) use local_exits::sources as local_exits_sources;

pub(in crate::ssa) mod local_resources;
pub(crate) use local_resources::sources as local_resource_sources;

pub(in crate::ssa) mod local_future;
pub(crate) use local_future::sources as local_future_sources;

mod local_reassignment;
pub(crate) use local_reassignment::grouped_sources as local_grouped_reassignment_sources;
pub(crate) use local_reassignment::sources as local_reassignment_sources;

pub(crate) use local_reassignment::ordinary_epoch_sources as local_ordinary_epoch_sources;

mod local_resource_future;
pub(crate) use local_resource_future::sources as local_resource_future_sources;

pub(crate) use local_reassignment::{
    ordinary_route_controls as local_ordinary_route_controls,
    ordinary_route_sources as local_ordinary_route_sources,
};

pub(crate) use local_reassignment::completed_epoch_sources as local_completed_epoch_sources;

pub(crate) use local_exits::alias_rebound_source as local_alias_rebound_source;
pub(crate) use local_exits::alternate_sources as alternate_local_sources;

pub(in crate::ssa) mod births;
pub(crate) use births::{Kind as MapBirthKind, sources as map_birth_sources};

pub(in crate::ssa) mod scope_lowering;
pub(crate) use scope_lowering::sources as scope_a1_sources;

pub(in crate::ssa) mod scope_lambda_body;
pub(crate) use scope_lambda_body::sources as scope_a2_sources;

pub(in crate::ssa) mod scope_member_parameter;
pub(in crate::ssa) mod scope_value_births;
pub(crate) use scope_member_parameter::sources as scope_member_parameter_sources;
pub(crate) use scope_value_births::int_storage_sources as scope_int_storage_sources;

pub(crate) use scope_value_births::borrow_alias_sources as scope_borrow_alias_sources;
pub(crate) use scope_value_births::conditional_owner_sources as scope_if_owner_sources;
pub(crate) use scope_value_births::owner_update_sources as scope_owner_update_sources;
