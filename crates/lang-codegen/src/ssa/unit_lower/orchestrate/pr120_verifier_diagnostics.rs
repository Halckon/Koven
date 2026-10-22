//! 七份完整原源的真实 final-verifier 观察；五份 take 闭包仅允许 CI 执行。
//! 不注入 SSA、不重跑 verifier、不改变返回值，不进入 LLVM/native。
use std::{
    collections::BTreeMap,
    fmt::Debug,
    fmt::Write as _,
    io::Write as _,
    panic::{AssertUnwindSafe, catch_unwind},
};

use lang_frontend::{
    analysis::{
        UnitNameSnapshot, UnitSourceDescriptor, analyze_basic_unit_ownership,
        analyze_parsed_unit_names,
    },
    lexer::lex,
    name_resolution::{DeclarationId, SymbolKind, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, OwnedCompilationUnitView, owned_compilation_unit_view,
    },
    parser::parse_file,
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, UnitCallableTarget, UnitTypeKind,
        check_compilation_unit_types, complete_compilation_unit_types, standard_environments,
    },
};

use super::super::type_lower::UnitTypeLowering;
use crate::ssa::{
    LoweringError,
    model::{Definition, Edge, EntityId, Function, FunctionId, Module, Program, TerminatorKind},
    render::render_program,
    unit_plan::{UnitCallablePlan, UnitFunctionInstanceKey},
    verify::{VerifyErrors, VerifyLocation},
};

const STANDARD_PATH: &str = "koven/algorithms/ranges.ko";
const STANDARD_TEXT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../lang-std/koven/algorithms/ranges.ko"
));
const STANDARD_SHA256: &str = "953aa75735718f0b0099d8e1abb30641c317cca5e5f66fdad5403bd123b9b603";

struct Case {
    id: &'static str,
    path: &'static str,
    text: &'static str,
    bytes: usize,
    sha256: &'static str,
    scope: &'static str,
}

/// 每次调用显式传递；从不存放全局或 thread-local 状态。
#[derive(Default)]
pub(super) struct Capture {
    text: String,
    verify_calls: usize,
    verifier_ok: Option<bool>,
    missing_sections: Vec<&'static str>,
}

impl Capture {
    fn section(&mut self, label: &str, text: &str) {
        writeln!(self.text, "SECTION {label} bytes={}", text.len()).unwrap();
        self.text.push_str(text);
        writeln!(self.text, "\nEND_SECTION {label}").unwrap();
    }

    fn debug(&mut self, label: &str, value: &impl Debug) {
        self.section(label, &format!("{value:#?}"));
    }

    fn note(&mut self, label: &str, value: impl Debug) {
        writeln!(self.text, "{label}={value:?}").unwrap();
    }

    fn component(&mut self, label: &'static str, observe: impl FnOnce(&mut Self)) {
        // 仅保护只读观察与本地文本缓冲，不捕获或替换 lowering/verify 的真实结果。
        // 若无效 SSA 使 renderer/关系查询 panic，标缺项后仍收另一布局；OOM/abort 不可恢复。
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| observe(self))) {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("non-string observer panic payload");
            self.missing_sections.push(label);
            self.note("observer_component_incomplete", (label, message));
        }
    }

    // 只接受同一 driver 留存的不可变对象；真实 verify 已执行且只执行一次。
    pub(super) fn observe(
        &mut self,
        program: &Program,
        verification: &Result<(), VerifyErrors>,
        facts: (
            &SourceMap,
            &ValidatedCompilationUnitNames,
            &CompilationUnitTypes,
            &CompilationUnitOwnership,
        ),
        function_ids: &BTreeMap<UnitFunctionInstanceKey, FunctionId>,
        types: &UnitTypeLowering,
        source_plan: &UnitCallablePlan,
    ) {
        let (sources, names, typed, owned) = facts;
        self.verify_calls += 1;
        self.verifier_ok = Some(verification.is_ok());
        self.debug("final.verify_result", verification);
        self.component("final.facts", |capture| {
            capture.debug("final.names", names.names());
            capture.debug("final.typed", typed);
            capture.debug("final.owned", owned);
        });
        self.component("final.routes", |capture| {
            capture.debug("final.function_ids", function_ids);
            capture.debug("final.source_callable_plan", source_plan);
            capture.debug("final.type_ids", types.type_ids());
            capture.debug("final.heap_payloads", types.heap_payloads());
            capture.debug("final.field_indices", types.field_indices());
            capture.debug("final.enum_payloads", types.enum_payloads());
        });
        self.component("final.modules", |capture| {
            capture.debug("final.modules", &program.modules);
        });
        self.component("final.ssa", |capture| {
            capture.section("final.ssa", &render_program(program));
        });
        self.section(
            "verifier_gate_scope",
            "Original structure/type/deinit/CFG/dominance/ownership/closure gates unchanged. \
             This is the complete returned error vector, not a replay of skipped gates. \
             Internal per-gate arrival and ownership BlockState are not instrumented; \
             later gates may not have been reached after an earlier error.",
        );
        if let Err(errors) = verification {
            self.note("verify_error_count", errors.errors.len());
            for (ordinal, error) in errors.errors.iter().enumerate() {
                self.note("verify_error_ordinal", ordinal);
                self.debug("verify_error", error);
                if let Some(origin) = &error.origin {
                    self.span(sources, "verify_error_origin", origin.span());
                }
                self.component("verify_error_location", |capture| {
                    capture.location(program, sources, &error.location);
                });
            }
        } else {
            self.note("verify_error_count", 0);
        }
        self.component("final.cfg", |capture| capture.cfg(program, sources));
    }

    fn span(&mut self, sources: &SourceMap, label: &str, span: Span) {
        self.note(label, span);
        self.note("span_source_name", sources.source_name(span.source_id()));
        self.note("span_source_text", sources.slice(span));
    }

    fn entity(&mut self, module: &Module, function: &Function, id: EntityId) {
        self.note("entity_id", id);
        self.note("entity_function_matches", id.function() == function.id);
        let data = function.entity(id);
        self.debug("entity_data", &data);
        if let Some(data) = data {
            let ty = data.ty.semantic_type();
            self.note("entity_type_module_matches", ty.module() == module.id);
            self.debug("entity_type_definition", &module.type_kind(ty));
            self.debug("entity_type_origin", &module.type_origin(ty));
            let exact_definition = match data.definition {
                Definition::BlockParameter { block, index } => {
                    function
                        .block(block)
                        .and_then(|block| block.parameters.get(index))
                        == Some(&id)
                }
                Definition::InstructionResult { instruction, index } => {
                    function
                        .instruction(instruction)
                        .and_then(|instruction| instruction.results.get(index))
                        == Some(&id)
                }
            };
            self.note("entity_definition_resolves_exact_id", exact_definition);
        }
    }

    fn location(&mut self, program: &Program, sources: &SourceMap, location: &VerifyLocation) {
        let function_id = match *location {
            VerifyLocation::Module(id) => {
                self.note("location_module_resolves", program.module(id).is_some());
                return;
            }
            VerifyLocation::Type(id) => {
                self.debug(
                    "location_type",
                    &program
                        .module(id.module())
                        .and_then(|module| module.type_kind(id)),
                );
                return;
            }
            VerifyLocation::Function(id) => id,
            VerifyLocation::Block(id) | VerifyLocation::Terminator(id) => id.function(),
            VerifyLocation::Instruction(id) => id.function(),
            VerifyLocation::Edge { source, .. } => source.function(),
            VerifyLocation::Entity(id) => id.function(),
        };
        let Some(module) = program.module(function_id.module()) else {
            self.note("location_module_resolves", false);
            return;
        };
        let Some(function) = module.function(function_id) else {
            self.note("location_function_resolves", false);
            return;
        };
        self.note("location_function_id", function.id);
        self.note("location_function_name", &function.name);
        self.span(sources, "location_function_origin", function.origin.span());
        match *location {
            VerifyLocation::Block(id) | VerifyLocation::Terminator(id) => {
                self.debug("location_block", &function.block(id));
            }
            VerifyLocation::Instruction(id) => {
                self.debug("location_instruction", &function.instruction(id));
                if let Some(instruction) = function.instruction(id) {
                    for entity in instruction.operation.entities() {
                        self.entity(module, function, entity);
                    }
                    for entity in &instruction.results {
                        self.entity(module, function, *entity);
                    }
                }
            }
            VerifyLocation::Edge { source, successor } => {
                self.note("location_successor_ordinal", successor);
                self.debug("location_edge_source", &function.block(source));
            }
            VerifyLocation::Entity(id) => self.entity(module, function, id),
            VerifyLocation::Module(_) | VerifyLocation::Type(_) | VerifyLocation::Function(_) => {}
        }
    }

    fn cfg(&mut self, program: &Program, sources: &SourceMap) {
        self.section(
            "cfg_contract",
            "All edges and all columns are reported, including unreachable blocks. \
             Missing/foreign IDs stay missing; no replacement entities are fabricated. \
             Range/nullable roles are read from real operations, facts and origin spans, \
             not inferred verifier execution states or saved lowerer maps.",
        );
        for module in &program.modules {
            self.note("cfg_module", module.id);
            self.note(
                "cfg_module_resolves_same_owner",
                program
                    .module(module.id)
                    .is_some_and(|found| std::ptr::eq(found, module)),
            );
            for function in &module.functions {
                self.note("cfg_function", (function.id, &function.name));
                self.note(
                    "cfg_function_resolves_same_owner",
                    module
                        .function(function.id)
                        .is_some_and(|found| std::ptr::eq(found, function)),
                );
                self.span(sources, "cfg_function_origin", function.origin.span());
                for instruction in &function.instructions {
                    self.note("cfg_instruction", instruction.id);
                    self.note(
                        "cfg_instruction_resolves_same_owner",
                        function
                            .instruction(instruction.id)
                            .is_some_and(|found| std::ptr::eq(found, instruction)),
                    );
                    self.span(sources, "cfg_instruction_origin", instruction.origin.span());
                    for entity in instruction.operation.entities() {
                        self.entity(module, function, entity);
                    }
                    for entity in &instruction.results {
                        self.entity(module, function, *entity);
                    }
                }
                for block in &function.blocks {
                    self.note("cfg_block", block.id);
                    self.note(
                        "cfg_block_resolves_same_owner",
                        function
                            .block(block.id)
                            .is_some_and(|found| std::ptr::eq(found, block)),
                    );
                    self.span(sources, "cfg_block_origin", block.origin.span());
                    for (column, entity) in block.parameters.iter().enumerate() {
                        self.note("cfg_parameter_column", column);
                        self.entity(module, function, *entity);
                    }
                    let Some(terminator) = &block.terminator else {
                        self.note("cfg_missing_terminator", true);
                        continue;
                    };
                    self.span(sources, "cfg_terminator_origin", terminator.origin.span());
                    for entity in terminator.kind.entities() {
                        self.entity(module, function, entity);
                    }
                    if let TerminatorKind::NullableBranch { view, .. } = terminator.kind {
                        self.note("cfg_nullable_branch_proof", view);
                        self.entity(module, function, EntityId::Loan(view));
                    }
                    for (successor, edge) in edges(&terminator.kind).into_iter().enumerate() {
                        self.note("cfg_edge", (block.id, successor, edge.target));
                        let target = function.block(edge.target);
                        self.note("cfg_target_resolves_same_owner", target.is_some());
                        let parameters = target.map(|target| target.parameters.as_slice());
                        self.note("cfg_argument_count", edge.arguments.len());
                        self.note("cfg_parameter_count", parameters.map(<[_]>::len));
                        let count = edge.arguments.len().max(parameters.map_or(0, <[_]>::len));
                        for column in 0..count {
                            self.note("cfg_edge_column", column);
                            let argument = edge.arguments.get(column).copied();
                            let parameter =
                                parameters.and_then(|values| values.get(column)).copied();
                            self.note("cfg_argument", argument);
                            if let Some(argument) = argument {
                                self.entity(module, function, argument);
                            }
                            self.note("cfg_parameter", parameter);
                            if let Some(parameter) = parameter {
                                self.entity(module, function, parameter);
                            }
                            self.note(
                                "cfg_exact_types_equal",
                                argument
                                    .and_then(|value| function.entity(value))
                                    .zip(parameter.and_then(|value| function.entity(value)))
                                    .map(|(argument, parameter)| argument.ty == parameter.ty),
                            );
                        }
                    }
                }
            }
        }
    }
}

fn edges(terminator: &TerminatorKind) -> Vec<&Edge> {
    match terminator {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => {
            vec![when_null, when_non_null]
        }
        TerminatorKind::Return { .. }
        | TerminatorKind::BorrowReturn { .. }
        | TerminatorKind::RangeReturn { .. }
        | TerminatorKind::Abort => Vec::new(),
    }
}

fn checked<T, E: Debug>(
    capture: &mut Capture,
    stage: &str,
    value: Result<T, E>,
) -> Result<T, String> {
    match value {
        Ok(value) => {
            capture.note(stage, "Ok");
            Ok(value)
        }
        Err(error) => {
            capture.debug(stage, &error);
            Err(format!("{stage}: {error:?}"))
        }
    }
}

fn require(capture: &mut Capture, label: &str, condition: bool) -> Result<(), String> {
    capture.note(label, condition);
    if condition {
        Ok(())
    } else {
        Err(label.to_owned())
    }
}

fn snapshot_sources(capture: &mut Capture, snapshot: &UnitNameSnapshot) {
    capture.debug("input.descriptors", &snapshot.descriptors());
    capture.debug("input.parsed_files", &snapshot.parsed_files());
    for input in snapshot.inputs() {
        capture.note("source_identity", input.source_id());
        let units = snapshot
            .names()
            .index()
            .source_units()
            .iter()
            .filter(|unit| unit.source_id() == input.source_id())
            .collect::<Vec<_>>();
        capture.debug("actual_source_units", &units);
        for (id, node) in input.parsed().ast().items().iter() {
            capture.note("ast_item", id);
            capture.span(snapshot.sources(), "ast_item_span", node.span());
        }
        for (id, node) in input.parsed().ast().statements().iter() {
            capture.note("ast_statement", id);
            capture.span(snapshot.sources(), "ast_statement_span", node.span());
        }
        for (id, node) in input.parsed().ast().expressions().iter() {
            capture.note("ast_expression", id);
            capture.span(snapshot.sources(), "ast_expression_span", node.span());
        }
        for (id, node) in input.parsed().ast().type_refs().iter() {
            capture.note("ast_type_ref", id);
            capture.span(snapshot.sources(), "ast_type_ref_span", node.span());
        }
    }
}

// 诊断入口仍只能接收 frontend 工厂封闭的真实 view，不能接受任意 raw facts。
fn lower_observed(
    view: &OwnedCompilationUnitView<'_, '_>,
    entry: DeclarationId,
    capture: &mut Capture,
) -> Result<(Program, FunctionId), LoweringError> {
    super::lower_unit_impl(
        (view.sources(), view.inputs()),
        view.names(),
        view.types(),
        view.ownership(),
        None,
        entry,
        Some(capture),
    )
}

fn collect(case: &Case, root: &str, capture: &mut Capture) -> Result<(), String> {
    require(
        capture,
        "source_byte_count_matches",
        case.text.len() == case.bytes,
    )?;
    let mut sources = SourceMap::new();
    let source = checked(
        capture,
        "add_user",
        sources.add_source(case.path, case.text),
    )?;
    let standard = checked(
        capture,
        "add_standard",
        sources.add_source("koven-std/koven/algorithms/ranges.ko", STANDARD_TEXT),
    )?;
    let (name_environment, mut environment) = standard_environments();
    checked(
        capture,
        "authorize_range_source",
        environment.authorize_range_source(&sources, standard),
    )?;
    checked(
        capture,
        "authorize_range_extension_source",
        environment.authorize_range_extension_source(&sources, standard),
    )?;
    capture.note("trusted_standard_source_only", standard);
    let descriptors = [
        UnitSourceDescriptor::new(root, case.path, source),
        UnitSourceDescriptor::new("koven-std", STANDARD_PATH, standard),
    ];
    let mut parsed_sources = Vec::new();
    for descriptor in descriptors {
        let lexed = checked(capture, "lex", lex(&sources, descriptor.source_id()))?;
        capture.debug("lex.diagnostics", &lexed.diagnostics());
        let parsed = checked(capture, "parse", parse_file(&sources, &lexed))?;
        capture.debug("parse.diagnostics", &parsed.diagnostics());
        parsed_sources.push((descriptor, parsed));
    }
    let snapshot = checked(
        capture,
        "name_snapshot",
        analyze_parsed_unit_names(sources, parsed_sources, name_environment),
    )?;
    snapshot_sources(capture, &snapshot);
    capture.debug("names.diagnostics", &snapshot.names().diagnostics());
    if !snapshot.names().diagnostics().is_empty() {
        capture.debug("rejected.names", snapshot.names());
        return Err("nonempty names diagnostics".to_owned());
    }
    let names = snapshot
        .validated_names()
        .ok_or_else(|| "unvalidated names".to_owned())?;
    capture.note("names_validated", true);
    let sources = snapshot.sources();
    let inputs = snapshot.inputs();
    let raw = checked(
        capture,
        "check_types",
        check_compilation_unit_types(sources, &inputs, names, &environment),
    )?;
    capture.debug("types.diagnostics", &raw.diagnostics());
    if !raw.diagnostics().is_empty() {
        capture.debug("rejected.typed", &raw);
        return Err("nonempty type diagnostics".to_owned());
    }
    {
        let admission =
            complete_compilation_unit_types(sources, &inputs, names, &environment, &raw);
        if admission.is_err() {
            capture.debug("rejected.typed", &raw);
        }
        let complete = checked(capture, "type_admission", admission)?;
        require(
            capture,
            "type_admission_same_analysis",
            complete.is_same_analysis(&raw) && std::ptr::eq(complete.types(), &raw),
        )?;
    }
    capture.component("admitted.typed", |capture| {
        capture.debug("admitted.typed", &raw)
    });
    let outcome = checked(
        capture,
        "basic_ownership",
        analyze_basic_unit_ownership(sources, &inputs, names, &environment, raw),
    )?;
    let (typed, owned) = checked(capture, "basic_capability", outcome.into_result())?;
    capture.debug("owned.diagnostics", &owned.diagnostics());
    capture.debug("owned.deferred", &owned.deferred());
    if !owned.diagnostics().is_empty() || !owned.deferred().is_empty() {
        capture.debug("rejected.typed", typed.types());
        capture.debug("rejected.owned", &owned);
        return Err("nonempty ownership diagnostics/deferred".to_owned());
    }
    require(
        capture,
        "owned_same_analysis",
        owned.is_compatible_with(&typed),
    )?;
    let owned = checked(capture, "owned_validation", owned.validate())?;
    let view = checked(
        capture,
        "sealed_owned_view",
        owned_compilation_unit_view(sources, &inputs, names, &environment, &typed, &owned),
    )?;
    require(
        capture,
        "sealed_types_same_object",
        std::ptr::eq(view.types(), typed.types()),
    )?;
    require(
        capture,
        "sealed_owned_same_object",
        std::ptr::eq(view.ownership(), owned.ownership()),
    )?;
    let source_units = names
        .names()
        .index()
        .source_units()
        .iter()
        .filter(|unit| unit.source_id() == source)
        .collect::<Vec<_>>();
    capture.debug("entry_source_candidates", &source_units);
    let [source_unit] = source_units.as_slice() else {
        return Err("missing/ambiguous actual user SourceUnitId".to_owned());
    };
    let entries = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|entry| {
            entry.source_unit() == source_unit.id()
                && entry.kind() == SymbolKind::Function
                && entry.name() == "main"
        })
        .collect::<Vec<_>>();
    capture.debug("entry_candidates", &entries);
    let [entry] = entries.as_slice() else {
        return Err("missing/ambiguous original main".to_owned());
    };
    let signature = typed
        .types()
        .signatures()
        .declaration(entry.id())
        .and_then(|declaration| declaration.callable())
        .ok_or_else(|| "missing real entry callable signature".to_owned())?;
    capture.debug("entry_signature", signature);
    require(
        capture,
        "entry_original_no_arguments_unit",
        signature.target() == UnitCallableTarget::Declaration(entry.id())
            && signature.parameters().is_empty()
            && signature.type_parameters().is_empty()
            && signature.receiver().is_none()
            && typed.types().types().get(signature.return_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Unit)),
    )?;
    let lowered = lower_observed(&view, entry.id(), capture);
    match lowered {
        Ok((_program, entry)) => {
            capture.note("lower_result", "Ok");
            capture.note("lower_entry", entry);
            Ok(())
        }
        Err(error) => {
            capture.debug("lower_error", &error);
            if let Some(span) = error.span {
                capture.span(sources, "lower_error_span", span);
            }
            if capture.verify_calls == 0 {
                capture.debug("preverify.names", names.names());
                capture.debug("preverify.typed", view.types());
                capture.debug("preverify.owned", view.ownership());
                capture.note("final_verifier_not_reached", true);
            }
            Err(format!("original lower error: {error:?}"))
        }
    }
}

fn write_record(case: &Case, layout: &str, capture: &Capture) -> std::io::Result<()> {
    // StdoutLock::write_all 不使用 print!/println! 的 libtest 捕获通道；成功对照也可见。
    // 固定宽度 hex 保留字节；CI 时间戳只会在行前，下载时按 case/layout/seq 解码。
    let mut output = std::io::stdout().lock();
    let payload = capture.text.as_bytes();
    writeln!(
        output,
        "PR120_DIAGNOSTIC_V1 BEGIN case={} layout={layout} scope={} source_sha256={} source_bytes={} payload_bytes={}",
        case.id,
        case.scope,
        case.sha256,
        case.bytes,
        payload.len()
    )?;
    let mut chunks = 0;
    for (sequence, bytes) in payload.chunks(480).enumerate() {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(hex, "{byte:02x}").unwrap();
        }
        writeln!(
            output,
            "PR120_DIAGNOSTIC_V1 CHUNK case={} layout={layout} seq={sequence} hex={hex}",
            case.id
        )?;
        chunks += 1;
    }
    writeln!(
        output,
        "PR120_DIAGNOSTIC_V1 END case={} layout={layout} chunks={chunks} payload_bytes={}",
        case.id,
        payload.len()
    )?;
    output.flush()
}

fn exercise(case: Case) {
    let mut outcomes = Vec::new();
    for (layout, root) in [("single", "single"), ("project", "src")] {
        let mut capture = Capture::default();
        capture.note("schema", "PR120_DIAGNOSTIC_V1");
        capture.note("case", case.id);
        capture.note("layout", layout);
        capture.note("root", root);
        capture.note("logical_path", case.path);
        capture.note("scope", case.scope);
        capture.note("expected_source_sha256_not_runtime_computed", case.sha256);
        capture.note("source_bytes", case.text.len());
        capture.section("source_user", case.text);
        capture.note(
            "expected_standard_sha256_not_runtime_computed",
            STANDARD_SHA256,
        );
        capture.note("standard_bytes", STANDARD_TEXT.len());
        capture.section("source_standard", STANDARD_TEXT);
        let result = collect(&case, root, &mut capture);
        capture.debug("pipeline_result", &result);
        capture.note("verify_calls", capture.verify_calls);
        capture.note("verifier_ok", capture.verifier_ok);
        if capture.verify_calls == 0 {
            capture
                .missing_sections
                .push("final.verifier_program_facts");
        }
        capture.debug("missing_sections", &capture.missing_sections.clone());
        let complete = capture.verify_calls == 1
            && capture.verifier_ok.is_some()
            && capture.missing_sections.is_empty();
        capture.note("snapshot_complete", complete);
        let output = write_record(&case, layout, &capture);
        outcomes.push((layout, result, complete, output));
    }
    // 两布局记录都已尝试写出后才断言。历史 InvalidSsa 绝不是新的通过 oracle。
    assert!(
        outcomes
            .iter()
            .all(|(_, result, complete, output)| { result.is_ok() && *complete && output.is_ok() }),
        "{}: original sources must lower and verify in both layouts: {outcomes:?}",
        case.id,
    );
}

macro_rules! original_case {
    ($name:ident, $id:literal, $scope:literal, $file:literal, $bytes:literal, $hash:literal) => {
        #[test]
        fn $name() {
            exercise(Case {
                id: $id,
                path: $file,
                text: include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/pr120-verifier-diagnostics/",
                    $file
                )),
                bytes: $bytes,
                sha256: $hash,
                scope: $scope,
            });
        }
    };
}

original_case!(
    ordinary_p14b,
    "p14b",
    "ordinary",
    "p14b_nullable_simple.ko",
    122,
    "cd84ceffbec73254136daecb5bf156452a8a162bd82e693475f86b1d3d0078ce"
);
original_case!(
    ordinary_q05,
    "q05",
    "ordinary",
    "q05_nullable_param.ko",
    163,
    "0d93bd32ae49bb4bbb2cb34ed293cf04e118377c58e1e8724b534774152dccba"
);
original_case!(
    ci_only_y01,
    "y01",
    "ci-only",
    "y01_import_take_for_sum.ko",
    157,
    "c1fac2dfee57b67ec279de26bfde160e623abe6372e67e08b0cd37140250ad9f"
);
original_case!(
    ci_only_y03,
    "y03",
    "ci-only",
    "y03_method_take_for.ko",
    163,
    "5db5994e2ff45cc7b6cd80c0c196e60557072279ac6558b74e8d1fb44ae816dc"
);
original_case!(
    ci_only_y04,
    "y04",
    "ci-only",
    "y04_take_for_println.ko",
    120,
    "baa58796b1332bb65f4d38bbf7c240790d3405fcdcc5af80f22f4d6d710089b0"
);
original_case!(
    ci_only_y05,
    "y05",
    "ci-only",
    "y05_take_borrow_val_call.ko",
    188,
    "3f45ef425f8520adee6c074458e555974a017f7f76e8b31c1af86c3bba14c802"
);
original_case!(
    ci_only_y06,
    "y06",
    "ci-only",
    "y06_take_borrow_val_for.ko",
    138,
    "6bc8fb147c64bfd6cd5302759952c67b8c550ca1813c046c7fcfaa9e1859eabb"
);
