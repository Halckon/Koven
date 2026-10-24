//! Two unchanged conditional sources × MemoryDocument/rooted Unit; four observations before assert.
//! No native run and no expectation that UnsupportedNode is a successful outcome.
mod ast;
mod query_calls;
mod query_cleanup;
mod query_conditions;
mod query_inputs;
mod query_ssa;
mod query_test;
mod unit;

use super::{
    Report,
    fixture::{Failure, failure, verify_failure},
    identity, input_label, pipeline,
};

pub(crate) fn sources() -> [(&'static str, bool, &'static str, &'static str, &'static str); 2] {
    [
        (
            "map-conditional",
            true,
            "279f8ebc33ceabb3e0538823a6a7b2efaa3eec274f4e1e33b22042c947ffb9bb",
            include_str!(
                "../../../tests/fixtures/map-provider-conditional-0331/map-conditional.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-conditional-0331/map-conditional.stdout"
            ),
        ),
        (
            "mutable-map-conditional",
            false,
            "c2f6da3f52e873287c263d3291ee6a9d1bc78c77d8dfc440b62fa996a9b6faaa",
            include_str!(
                "../../../tests/fixtures/map-provider-conditional-0331/mutable-map-conditional.ko"
            ),
            include_str!(
                "../../../tests/fixtures/map-provider-conditional-0331/mutable-map-conditional.stdout"
            ),
        ),
    ]
}

#[derive(Debug)]
struct Observation {
    label: &'static str,
    rooted: bool,
    report: Report,
    outcome: Result<(), Failure>,
}

fn observe(
    label: &'static str,
    map: bool,
    source_hash: &str,
    text: &str,
    rooted: bool,
    deep_identity: bool,
) -> Observation {
    let mut report = Report::default();
    let layout = input_label(rooted);
    // Hash is the externally verified frozen-fixture identity, not a runtime hashing claim.
    println!(
        "SPEC0331 CONDITIONAL INPUT case={label} input={layout} path=provider.ko frozen_sha256={source_hash} text={text:?}"
    );
    let outcome = ast::Input::new(text, map).and_then(|input| {
        let anchors = ast::collect(&input);
        report.query("conditional actual source AST anchors", &anchors);
        let mut identity_inputs = None;
        let (program, entry) = if rooted {
            pipeline::unit_observed(
                &input.sources,
                &input.parsed,
                "provider.ko",
                &mut report,
                |names, typed, owned, report| {
                    unit::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
                    if deep_identity {
                        let queried = anchors
                            .as_ref()
                            .map_err(|e| failure("conditional identity AST prerequisite", e))
                            .and_then(|a| query_inputs::unit(&input, a, names, typed, owned));
                        report.query("conditional identity same-run FE bridge", &queried);
                        identity_inputs = Some(queried);
                    }
                },
            )
        } else {
            pipeline::memory_document_observed(
                &input.sources,
                &input.parsed,
                &mut report,
                |names, typed, owned, report| {
                    unit::observe(&input, anchors.as_ref().ok(), names, typed, owned, report);
                    if deep_identity {
                        let queried = anchors
                            .as_ref()
                            .map_err(|e| failure("conditional identity AST prerequisite", e))
                            .and_then(|a| query_inputs::unit(&input, a, names, typed, owned));
                        report.query("conditional identity same-run FE bridge", &queried);
                        identity_inputs = Some(queried);
                    }
                },
            )
        }?;
        println!(
            "SPEC0331 CONDITIONAL SSA case={label} input={layout}\n{}",
            crate::ssa::render::render_program(&program)
        );
        // Reuse only the shape-independent reachable source-function helper.
        let reachable = anchors
            .as_ref()
            .map_err(|e| failure("conditional AST identity", e))
            .and_then(|a| {
                identity::entry_sources(&input.sources, &program, entry, &a.functions_for_ssa())
            });
        report.query(
            "conditional returned entry and reachable source functions",
            &reachable,
        );
        if deep_identity {
            let inputs = identity_inputs.unwrap_or_else(|| {
                Err(failure(
                    "conditional identity same-run bridge missing",
                    (label, rooted),
                ))
            });
            let queried = anchors
                .as_ref()
                .map_err(|e| failure("conditional identity AST prerequisite", e))
                .and_then(|a| {
                    inputs
                        .as_ref()
                        .map_err(|e| failure("conditional identity FE prerequisite", e))
                        .and_then(|i| query_ssa::check(&program, &input, a, i))
                });
            report.query(
                "conditional identity complete SSA roles and all incoming",
                &queried,
            );
            let calls = anchors
                .as_ref()
                .map_err(|e| failure("conditional identity AST prerequisite", e))
                .and_then(|a| query_calls::check(&program, &input, a));
            report.query(
                "conditional identity actual entry and consumer operands",
                &calls,
            );
        }
        crate::ssa::verify::verify_program(&program)
            .map_err(|e| verify_failure(&input.sources, e))?;
        report.check("conditional SSA verifier", true, entry);
        let llvm = crate::llvm::render_verified_program(&program)
            .map_err(|e| failure("LLVM render/verify", e))?;
        report.check("conditional LLVM render/verify", true, llvm.len());
        println!("SPEC0331 CONDITIONAL LLVM case={label} input={layout}\n{llvm}");
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
fn map_and_mutable_map_conditional_exits_reach_verified_llvm() {
    let mut observations = Vec::new();
    for (label, map, hash, source, _golden) in sources() {
        for rooted in [false, true] {
            observations.push(observe(label, map, hash, source, rooted, false));
        }
    }
    for observation in &observations {
        println!(
            "SPEC0331 CONDITIONAL OUTCOME case={} input={} report={:#?}",
            observation.label,
            input_label(observation.rooted),
            observation.report
        );
        match &observation.outcome {
            Ok(()) => println!("stage=LLVM render/verify result=ok"),
            Err(error) => println!("stage={} original={}", error.stage, error.original),
        }
    }
    assert!(
        observations.len() == 4
            && observations
                .iter()
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "两源×MemoryDocument/rooted Unit四格必须完成同源真实frontend、完整validator/sealed view、实际entry、SSA和LLVM；\
         query失败与原阶段失败分别保留，UnsupportedNode不是成功oracle。\n{observations:#?}"
    );
}
