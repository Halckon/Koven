//! SPEC-0331 的普通 Map provider 成功合同；六源 × MemoryDocument/rooted Unit 全采后断。
//! 不执行 native，不以当前 lower 的 UnsupportedNode 债务作为通过条件。

mod borrowed;
mod ci_baseline;
mod ci_contracts;
mod conditional;
mod control_flow;
mod d10_temporary_carry;
mod facts;
mod fixture;
mod formation_runtime;
mod future_use;
mod history;
mod identity;
mod local_arguments;
pub(crate) use local_arguments::{
    generic_prerequisite_sources as local_generic_argument_prerequisite_sources,
    generic_sources as local_generic_argument_sources, sources as local_argument_sources,
};

mod local_sources;
mod nested;
mod nested_exits;
mod pipeline;
mod query_cleanup;
mod query_graph;
pub(in crate::ssa) use query_graph::Graph as OrdinaryGraph;
pub(in crate::ssa) use query_inputs::one as one_query;
mod query_inputs;
mod query_map_normal_body;
mod query_owner;
mod query_ssa;
mod query_temporary_cleanup;
mod query_test;
mod resource;
mod resource_forms;
mod scope_future_observer;
pub(crate) use scope_future_observer::cases as scope_future_sources;
mod sequential;
mod source_forms;
mod stack;
mod temporary;
mod temporary_argument_cfg;
mod temporary_argument_cfg_query;
mod temporary_argument_cfg_runtime;
mod temporary_arguments;
mod temporary_delivery;
mod temporary_delivery_boundaries;
mod temporary_exits;
mod temporary_forms;

pub(crate) use borrowed::sources as borrowed_exit_sources;
pub(crate) use conditional::sources as conditional_sources;
pub(crate) use control_flow::future_transfer_sources;
pub(crate) use control_flow::neighbor_sources as ordinary_control_flow_neighbor_sources;
pub(crate) use control_flow::populated_future_transfer_sources;
pub(crate) use control_flow::return_sources as ordinary_return_sources;
pub(crate) use control_flow::sources as ordinary_control_flow_sources;
pub(crate) use formation_runtime::sources as formation_runtime_sources;
pub(crate) use future_use::sources as future_use_sources;
pub(crate) use history::{sanitizer_ir as history_sanitizer_ir, sources as history_sources};
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
    println!(
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
        println!(
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
        println!("SPEC0331 PROVIDER EXIT LLVM case={label} input={input}\n{llvm}");
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
        println!(
            "SPEC0331 PROVIDER EXIT OUTCOME case={} input={} report={:#?}",
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
        observations.len() == 12
            && observations
                .iter()
                .all(|observation| { observation.report.passed() && observation.outcome.is_ok() }),
        "十二格必须是同源完整 frontend、真实 entry/CFG 可达身份、SSA 与 LLVM 全成功；\
         原阶段失败保持失败，UnsupportedNode 不是成功 oracle。\n{observations:#?}"
    );
}

mod local_exits;

pub(crate) use local_exits::sources as local_exits_sources;

mod local_resources;
pub(crate) use local_resources::sources as local_resource_sources;

mod local_future;
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

mod births;
pub(crate) use births::{Kind as MapBirthKind, sources as map_birth_sources};

mod scope_lowering;
pub(crate) use scope_lowering::sources as scope_a1_sources;

mod scope_lambda_body;
pub(crate) use scope_lambda_body::sources as scope_a2_sources;

mod scope_member_parameter;
mod scope_value_births;
pub(crate) use scope_member_parameter::sources as scope_member_parameter_sources;
pub(crate) use scope_value_births::int_storage_sources as scope_int_storage_sources;

pub(crate) use scope_value_births::borrow_alias_sources as scope_borrow_alias_sources;
pub(crate) use scope_value_births::conditional_owner_sources as scope_if_owner_sources;
pub(crate) use scope_value_births::owner_update_sources as scope_owner_update_sources;
