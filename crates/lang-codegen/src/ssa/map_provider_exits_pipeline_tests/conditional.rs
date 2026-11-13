//! Two unchanged conditional sources × MemoryDocument/rooted Unit; four observations before assert.
//! No native run and no expectation that UnsupportedNode is a successful outcome.
use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
mod ast;
mod query_calls;
pub(in crate::ssa) mod query_cleanup;
mod query_conditions {
    //! 按真实 AST 独立列出 Unit 五出口；条件路径和 owner 入边由实际 SSA 身份查询等强承接。
    use super::{
        super::{
            fixture::{Failure, failure, need},
            query_inputs::one,
        },
        ast::{Anchors, Input},
    };
    use lang_frontend::{
        name_resolution::UnitSymbolId,
        ownership_checking::{
            CompilationUnitOwnership, UnitDropPoint, UnitDropTarget,
            UnitIterationCleanupAction as UA, UnitIterationExitKind as UE,
        },
        source::Span,
        type_checking::{UnitExpressionId, UnitStatementId},
    };

    pub(super) fn unit(
        f: &Input,
        a: &Anchors,
        owned: &CompilationUnitOwnership,
        source: UnitSymbolId,
        local: UnitSymbolId,
        parameter: Span,
    ) -> Result<(), Failure> {
        need(
            a.branches.len() == 3 && source.source_unit() == local.source_unit(),
            "conditional FE unit actual controls and source unit",
            (&a.branches, source, local),
        )?;
        let unit = source.source_unit();
        let expression = |id| UnitExpressionId::new(unit, id);
        let statement = UnitStatementId::new(unit, a.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("conditional FE unit cleanup plan", statement))?;
        let d = p.descriptor();
        let exits = [
            (
                UE::Continue(expression(a.branches[0].transfer)),
                UnitDropPoint::ControlTransfer(expression(a.branches[0].transfer)),
            ),
            (
                UE::Break(expression(a.branches[1].transfer)),
                UnitDropPoint::ControlTransfer(expression(a.branches[1].transfer)),
            ),
            (
                UE::Return(expression(a.branches[2].transfer)),
                UnitDropPoint::ControlTransfer(expression(a.branches[2].transfer)),
            ),
            (
                UE::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, a.body)),
            ),
            (UE::Exhaustion, UnitDropPoint::LoopExit(statement)),
        ];
        need(
            p.exits().len() == exits.len(),
            "conditional FE unit exact five-exit set",
            p.exits(),
        )?;
        let guards = owned
            .drops()
            .iter()
            .filter(|v| v.target() == UnitDropTarget::Named(local))
            .collect::<Vec<_>>();
        need(
            guards.len() == 4 && guards.iter().all(|v| v.value_origin() == a.local),
            "conditional FE unit four source-qualified lexical Guard drops",
            &guards,
        )?;
        let drops = owned
            .drops()
            .iter()
            .filter(|v| v.target() == UnitDropTarget::Named(source))
            .collect::<Vec<_>>();
        need(
            drops.len() == if f.map { 3 } else { 0 }
                && drops.iter().all(|v| v.value_origin() == parameter),
            "conditional FE unit source drops from actual parameter",
            &drops,
        )?;
        for (index, (kind, point)) in exits.into_iter().enumerate() {
            let exit = one(
                p.exits()
                    .iter()
                    .filter(|v| v.kind() == kind && v.point() == point),
                "conditional FE unit unique expected exit",
            )?;
            let mut expected = Vec::new();
            if index != 4 {
                expected.extend([
                    UA::Drop(*one(
                        guards.iter().copied().filter(|v| v.point() == point),
                        "conditional FE unit actual per-path Guard drop",
                    )?),
                    UA::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UA::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ]);
            }
            if [1, 2, 4].contains(&index) {
                expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
                if f.map {
                    expected.push(UA::Drop(*one(
                        drops.iter().copied().filter(|v| v.point() == point),
                        "conditional FE unit actual per-path source drop",
                    )?));
                }
            }
            need(
                exit.actions() == expected,
                "conditional FE unit independently expected complete cleanup order",
                (exit, &expected),
            )?;
            // 公开 unit plan 没有 owner/condition/phi 表；不得在此补造对应事实。
            // 此 getter 与 exit.actions 同表，只是附加一致性检查。
            need(
                owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
                "conditional FE unit cleanup accessor consistency",
                (point, expected),
            )?;
        }
        Ok(())
    }
}
mod query_inputs {
    //! 同次公开 FE 身份桥；AST anchors、原始 facts 与 returned SSA 分别保持独立。
    //! 原 collector 继续硬核全部 type/loan/delivery 合同；本查询仅增加身份连接与精确条件/出口。
    use super::{
        super::{
            fixture::{Failure, failure, need},
            query_inputs::one,
        },
        ast::{Anchors, Input},
        query_conditions,
    };
    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::{
            NameResolution, SymbolId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
            ValidatedCompilationUnitNames,
        },
        ownership_checking::{
            CompilationUnitOwnership, IterationSource, OwnershipBindingKind,
            UnitIterationSourceAccess, UnitLoanTarget,
        },
        parser::{Item, NameMarker, ValueParameter},
        source::Span,
        type_checking::{
            CompilationUnitTypes, ParameterMode, UnitCallTarget, UnitCallableSignature,
            UnitCallableTarget, UnitConstructionTarget, UnitExpressionId, UnitStatementId,
        },
    };

    #[derive(Debug)]
    pub(super) struct Inputs {
        pub parameter: Span,
        pub parameter_slot: usize,
        pub source_drop: Option<Span>,
    }

    fn parameters<'a>(
        f: &'a Input,
        a: &Anchors,
        name: &str,
    ) -> Result<&'a [ValueParameter], Failure> {
        let anchor = one(
            a.functions.iter().filter(|v| v.name == name),
            "conditional FE AST function",
        )?;
        let node = f
            .parsed
            .ast()
            .items()
            .get(anchor.item)
            .map_err(|e| failure("conditional FE AST item", e))?;
        let Item::Function {
            name: NameMarker::Present(actual),
            parameters,
            ..
        } = node.payload()
        else {
            return Err(failure("conditional FE actual function declaration", node));
        };
        need(
            *actual == anchor.name_span && node.span() == anchor.span,
            "conditional FE function anchor identity",
            (anchor, node),
        )?;
        Ok(parameters)
    }

    fn symbol(names: &NameResolution, span: Span) -> Result<SymbolId, Failure> {
        Ok(one(
            names
                .symbols()
                .iter()
                .filter(|v| v.span() == span && !v.is_synthetic()),
            "conditional FE unique declaration symbol",
        )?
        .id())
    }

    fn unit_reference(
        f: &Input,
        names: &ValidatedCompilationUnitNames,
        id: ExpressionId,
    ) -> Result<UnitSymbolId, Failure> {
        let (_, span) = f.expression(id)?;
        let reference = one(
            names
                .names()
                .references()
                .iter()
                .filter(|v| v.span() == span),
            "conditional FE unit reference",
        )?;
        match reference.target() {
            UnitReferenceTarget::Symbol(id) => Ok(*id),
            _ => Err(failure(
                "conditional FE unit source-local target",
                reference,
            )),
        }
    }

    fn unit_callable<'t>(
        f: &Input,
        a: &Anchors,
        name: &str,
        names: &ValidatedCompilationUnitNames,
        typed: &'t CompilationUnitTypes,
    ) -> Result<&'t UnitCallableSignature, Failure> {
        let source = one(
            names.names().source_units().iter(),
            "conditional FE unit source",
        )?;
        let anchor = one(
            a.functions.iter().filter(|v| v.name == name),
            "conditional FE unit function anchor",
        )?;
        let declaration = one(
            names.names().index().declarations().iter().filter(|v| {
                v.source_unit() == source.source_unit()
                    && v.root() == anchor.item
                    && v.name_span() == anchor.name_span
                    && v.kind() == SymbolKind::Function
            }),
            "conditional FE unit function declaration",
        )?;
        let callable = one(
            typed
                .signatures()
                .declarations()
                .iter()
                .filter_map(|v| v.callable())
                .filter(|v| v.target() == UnitCallableTarget::Declaration(declaration.id())),
            "conditional FE unit actual declaration signature",
        )?;
        let ast_parameters = parameters(f, a, name)?;
        need(
            callable.name_span() == anchor.name_span
                && callable.receiver().is_none()
                && callable.parameters().len() == ast_parameters.len(),
            "conditional FE unit actual signature shape",
            (declaration, callable),
        )?;
        for (index, parameter) in ast_parameters.iter().enumerate() {
            let NameMarker::Present(span) = parameter.name else {
                return Err(failure(
                    "conditional FE unit real parameter name",
                    parameter,
                ));
            };
            let id = callable.parameters()[index]
                .symbol()
                .ok_or_else(|| failure("conditional FE unit parameter symbol", callable))?;
            need(
                id.source_unit() == source.source_unit()
                    && id.symbol() == symbol(source.resolution(), span)?,
                "conditional FE unit parameter declaration and slot",
                (index, parameter, callable),
            )?;
        }
        Ok(callable)
    }

    pub(super) fn unit(
        f: &Input,
        a: &Anchors,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
    ) -> Result<Inputs, Failure> {
        let source_names = one(
            names.names().source_units().iter(),
            "conditional FE single actual source unit",
        )?;
        let unit = source_names.source_unit();
        let expression = |id| UnitExpressionId::new(unit, id);
        let statement = UnitStatementId::new(unit, a.statement);
        let d = typed
            .map_iteration(statement)
            .ok_or_else(|| failure("conditional FE unit Map descriptor", statement))?;
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("conditional FE unit Map plan", statement))?;
        let source = unit_reference(f, names, a.source)?;
        let parameter = one(
            source_names
                .resolution()
                .symbols()
                .iter()
                .filter(|v| v.id() == source.symbol()),
            "conditional FE unit source parameter",
        )?;
        let scan = unit_callable(f, a, "scan", names, typed)?;
        let parameter_slot = one(
            scan.parameters()
                .iter()
                .enumerate()
                .filter_map(|(i, v)| (v.symbol() == Some(source)).then_some(i)),
            "conditional FE unit parameter slot",
        )?;
        let mode = if f.map {
            ParameterMode::Value
        } else {
            ParameterMode::Borrow
        };
        let capability = if f.map {
            OwnershipBindingKind::Owned
        } else {
            OwnershipBindingKind::Shared
        };
        need(
            source_names.resolution().source_id() == f.parsed.source_id()
                && source.source_unit() == unit
                && d.statement() == statement
                && d.source() == expression(a.source)
                && p.descriptor() == d
                && typed.map_iterations().len() == 1
                && owned.map_iterations().len() == 1
                && parameter.kind() == SymbolKind::ValueParameter
                && !parameter.is_synthetic()
                && matches!(scan.parameters(), [parameter] if parameter.symbol() == Some(source)
                && parameter.mode() == mode && parameter.ty() == d.source_type())
                && typed.expression_type(expression(a.source)) == Some(d.source_type())
                && owned.binding_kind(source) == Some(capability)
                && matches!(p.source(), Some(UnitLoanTarget::Place(path)) if path.is_root() && path.root() == source)
                && matches!(p.source_kind(), IterationSource::BorrowedSource(_))
                && p.source_access()
                    == if f.map {
                        UnitIterationSourceAccess::Owned
                    } else {
                        UnitIterationSourceAccess::Shared
                    },
            "conditional FE unit actual parameter to Map plan",
            (parameter, scan, d, p),
        )?;
        for (index, binding) in d.bindings().into_iter().enumerate() {
            need(
                binding.source_unit() == unit
                    && symbol(source_names.resolution(), a.bindings[index])? == binding.symbol()
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[index].symbol() == binding
                    && p.loans()[index].binding() == binding
                    && p.loans()[index].source() == expression(a.source),
                "conditional FE unit component identity",
                (index, binding, p),
            )?;
        }
        need(
            a.branches.len() == 3,
            "conditional FE unit three actual branches",
            &a.branches,
        )?;
        for branch in &a.branches {
            need(
                unit_reference(f, names, branch.key)? == d.key_binding(),
                "conditional FE unit three If reference the actual key",
                branch,
            )?;
        }
        let local_symbol = symbol(source_names.resolution(), a.local)?;
        let local = one(
            owned
                .formed_owned_locals()
                .iter()
                .copied()
                .filter(|id| id.source_unit() == unit && id.symbol() == local_symbol),
            "conditional FE unit Guard local",
        )?;
        let construction = typed
            .construction(expression(a.guard))
            .ok_or_else(|| failure("conditional FE unit Guard constructor", a.guard))?;
        let guard = one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|v| v.source_unit() == unit && v.name_span() == a.guard_name),
            "conditional FE unit Guard declaration",
        )?;
        need(
            construction.expression() == expression(a.guard)
                && construction.arguments().is_empty()
                && typed.expression_type(expression(a.guard)) == Some(construction.result_type())
                && typed.symbol_type(local) == Some(construction.result_type())
                && typed.is_resource_type(construction.result_type()) == Some(true)
                && matches!(construction.target(), UnitConstructionTarget::Nominal(id) if id == guard.id()),
            "conditional FE unit actual Guard construction and local",
            (local, construction),
        )?;
        let read = unit_callable(f, a, "read", names, typed)?;
        let operand = unit_callable(f, a, "operand", names, typed)?;
        need(
            read.parameters().len() == 2 && operand.parameters().len() == 2,
            "conditional FE unit exact two consumer parameters before indexed queries",
            (read, operand),
        )?;
        need(
            a.component_calls.len() == 3,
            "conditional FE unit component call set",
            &a.component_calls,
        )?;
        for (name, id, arguments) in &a.component_calls {
            let expected = if *name == "operand" { operand } else { read };
            let call = typed
                .call(expression(*id))
                .ok_or_else(|| failure("conditional FE unit component call", id))?;
            need(
                matches!(call.target(), UnitCallTarget::Declaration(id)
                if expected.target() == UnitCallableTarget::Declaration(id))
                    && call.receiver().is_none()
                    && call.arguments().len() == 2,
                "conditional FE unit component callee identity",
                call,
            )?;
            for (index, argument) in call.arguments().iter().enumerate() {
                need(
                    argument.argument_index() == index
                        && argument.parameter_index() == index
                        && argument.mode() == ParameterMode::Borrow
                        && argument.parameter_type() == expected.parameters()[index].ty()
                        && unit_reference(f, names, arguments[index])? == d.bindings()[index],
                    "conditional FE unit component argument identity",
                    (index, call),
                )?;
            }
        }
        let forwarding = typed
            .call(expression(a.operand_read.0))
            .ok_or_else(|| failure("conditional FE unit operand read", a.operand_read))?;
        need(
            matches!(forwarding.target(), UnitCallTarget::Declaration(id)
            if read.target() == UnitCallableTarget::Declaration(id))
                && forwarding.receiver().is_none()
                && forwarding.arguments().len() == 2,
            "conditional FE unit operand actual read target",
            forwarding,
        )?;
        for (index, argument) in forwarding.arguments().iter().enumerate() {
            need(
                argument.argument_index() == index
                    && argument.parameter_index() == index
                    && argument.mode() == ParameterMode::Borrow
                    && Some(unit_reference(f, names, a.operand_read.1[index])?)
                        == operand.parameters()[index].symbol(),
                "conditional FE unit operand own parameters",
                (index, argument),
            )?;
        }
        unit_callable(f, a, "entry", names, typed)?;
        need(
            a.entry_calls.len() == 5,
            "conditional FE unit actual entry calls",
            &a.entry_calls,
        )?;
        for entry in &a.entry_calls {
            let call_id = expression(entry.call);
            let call = typed
                .call(call_id)
                .ok_or_else(|| failure("conditional FE unit entry scan call", entry))?;
            let argument = unit_reference(f, names, entry.argument)?;
            let result = unit_reference(f, names, entry.result_reference)?;
            need(
                matches!(call.target(), UnitCallTarget::Declaration(id)
                if scan.target() == UnitCallableTarget::Declaration(id))
                    && call.receiver().is_none()
                    && matches!(call.arguments(), [arg] if arg.argument_index() == 0 && arg.parameter_index() == parameter_slot
                    && arg.mode() == mode && arg.parameter_type() == d.source_type())
                    && argument.source_unit() == unit
                    && argument.symbol()
                        == symbol(source_names.resolution(), entry.source_binding)?
                    && result.source_unit() == unit
                    && result.symbol() == symbol(source_names.resolution(), entry.result_binding)?
                    && typed.expression_type(expression(entry.source_initializer))
                        == Some(d.source_type()),
                "conditional FE unit entry source and scan slot",
                (entry, call),
            )?;
        }
        query_conditions::unit(f, a, owned, source, local, parameter.span())?;
        Ok(Inputs {
            parameter: parameter.span(),
            parameter_slot,
            source_drop: f.map.then_some(parameter.span()),
        })
    }
}
mod query_ssa;
mod query_test {
    //! 四次 fresh MemoryDocument/rooted Unit 流水线；身份查询只读同次公开产物，原阶段错误全部保留。
    use crate::test_support::diagnostics::{assert_diagnostic, diagnostic};
    #[test]
    fn conditional_map_exits_preserve_all_edge_identity() {
        let mut observations = Vec::new();
        for (label, map, hash, text, _golden) in super::sources() {
            for rooted in [false, true] {
                observations.push(super::observe(label, map, hash, text, rooted, true));
            }
        }
        for observation in &observations {
            diagnostic!(
                "SPEC0331 CONDITIONAL IDENTITY OUTCOME case={} input={} report={:#?} result={:#?}",
                observation.label,
                super::input_label(observation.rooted),
                observation.report,
                observation.outcome,
            );
        }
        assert_diagnostic!(
            observations.len() == 4
                && observations.iter().all(|observation| {
                    observation.report.passed() && observation.outcome.is_ok()
                }),
            "原两源×MemoryDocument/rooted Unit四fresh必须完成同次FE、完整validator、独立全edge身份query、SSA和LLVM；\
             查询失败不隐藏原流水线阶段错误，未运行不作通过。\n{observations:#?}"
        );
    }
}
#[allow(unused_imports)]
pub(in crate::ssa) use query_cleanup::unit;

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
    diagnostic!(
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
        diagnostic!(
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
        diagnostic!("SPEC0331 CONDITIONAL LLVM case={label} input={layout}\n{llvm}");
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
        diagnostic!(
            "SPEC0331 CONDITIONAL OUTCOME case={} input={} report={:#?}",
            observation.label,
            input_label(observation.rooted),
            observation.report
        );
        match &observation.outcome {
            Ok(()) => diagnostic!("stage=LLVM render/verify result=ok"),
            Err(error) => diagnostic!("stage={} original={}", error.stage, error.original),
        }
    }
    assert_diagnostic!(
        observations.len() == 4
            && observations
                .iter()
                .all(|o| o.report.passed() && o.outcome.is_ok()),
        "两源×MemoryDocument/rooted Unit四格必须完成同源真实frontend、完整validator/sealed view、实际entry、SSA和LLVM；\
         query失败与原阶段失败分别保留，UnsupportedNode不是成功oracle。\n{observations:#?}"
    );
}
