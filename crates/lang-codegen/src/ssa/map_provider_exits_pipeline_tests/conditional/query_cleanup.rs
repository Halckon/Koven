//! 每个已证明路径的实际 Guard/KV/source 消费与 Return operand 的求值顺序。
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, result, value},
    query_inputs::one,
};
use super::{
    ast::{Anchors, Input},
    query_inputs::Inputs,
    query_ssa::Roles,
};
use crate::ssa::model::*;
use lang_frontend::{
    parser::{Expression, Statement},
    source::Span,
};

pub(super) struct Tail {
    pub next: Option<ValueId>,
    pub returned: Option<ValueId>,
}

pub(super) struct Context<'a, 'ir> {
    pub graph: &'a Graph<'ir>,
    pub module: &'ir Module,
    pub f: &'a Input,
    pub a: &'a Anchors,
    pub input: &'a Inputs,
}

impl Context<'_, '_> {
    fn end_source(
        &self,
        block: BlockId,
        roles: Roles,
        span: Span,
        uses: &mut Uses,
    ) -> Result<Vec<InstructionId>, Failure> {
        let (graph, f, a, input) = (self.graph, self.f, self.a, self.input);
        let end = one(
            graph
                .ops(block)?
                .into_iter()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: roles.source }),
            "conditional identity path EndSource",
        )?;
        need(
            end.origin == Origin::Source(span),
            "conditional identity exact EndSource origin",
            end,
        )?;
        uses.permit(EntityId::Loan(roles.source), end);
        let mut expected = vec![end.id];
        if f.map {
            let Some(EntityId::Value(owner)) = roles.seed else {
                return Err(failure("conditional identity current source owner", roles));
            };
            let drop = one(
                graph
                    .ops(block)?
                    .into_iter()
                    .filter(|i| i.operation == Operation::Drop { owner }),
                "conditional identity path source Drop",
            )?;
            need(
                Some(drop.origin.span()) == input.source_drop
                    && drop.origin == Origin::Source(input.parameter)
                    && graph.position(drop)? == graph.position(end)? + 1,
                "conditional identity current owner ends after its provider loan",
                (end, drop),
            )?;
            uses.permit(EntityId::Value(owner), drop);
            expected.push(drop.id);
        } else {
            need(
                input.source_drop.is_none(),
                "conditional identity Borrow has no source Drop fact",
                input,
            )?;
        }
        need(
            a.span.source_id() == span.source_id(),
            "conditional identity cleanup belongs to actual source",
            span,
        )?;
        Ok(expected)
    }

    pub(super) fn body(
        &self,
        index: usize,
        block: BlockId,
        roles: Roles,
        uses: &mut Uses,
    ) -> Result<Tail, Failure> {
        let (graph, module, f, a) = (self.graph, self.module, self.f, self.a);
        let (key, val, guard) = roles
            .body
            .ok_or_else(|| failure("conditional identity live cleanup body", roles))?;
        let transfer = if index < 3 {
            f.expression(a.branches[index].transfer)?.1
        } else {
            a.span
        };
        // Both layouts consume Unit cleanup facts with the actual For origin.
        let cleanup_span = a.span;
        let ops = graph.ops(block)?;
        let guard_drop = one(
            ops.iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner: guard }),
            "conditional identity current Guard Drop",
        )?;
        need(
            guard_drop.origin == Origin::Source(a.local),
            "conditional identity Guard lexical origin",
            guard_drop,
        )?;
        uses.permit(EntityId::Value(guard), guard_drop);
        let mut expected = vec![guard_drop.id];
        for component in [val, key] {
            let end = one(
                ops.iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: component }),
                "conditional identity current component EndBinding",
            )?;
            need(
                end.origin == Origin::Source(cleanup_span),
                "conditional identity exact component cleanup origin",
                end,
            )?;
            uses.permit(EntityId::Loan(component), end);
            expected.push(end.id);
        }
        let mut output = Tail {
            next: None,
            returned: None,
        };
        if index == 0 || index == 3 {
            let next = one(
                ops.iter()
                    .copied()
                    .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
                "conditional identity this path unique Next",
            )?;
            need(
                next.operation
                    == Operation::MapIterationNext {
                        source: roles.source,
                        cursor: roles.cursor,
                    }
                    && next.origin == Origin::Source(transfer)
                    && graph.function.entity(result(next)?).map(|e| e.ty)
                        == graph
                            .function
                            .entity(EntityId::Value(roles.cursor))
                            .map(|e| e.ty),
                "conditional identity this path current lease/cursor and Next result",
                next,
            )?;
            uses.permit(EntityId::Loan(roles.source), next);
            uses.permit(EntityId::Value(roles.cursor), next);
            uses.protect(result(next)?);
            expected.push(next.id);
            output.next = Some(value(next)?);
        } else {
            need(
                !ops.iter()
                    .any(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
                "conditional identity Break/Return has no Next",
                &ops,
            )?;
            expected.extend(self.end_source(block, roles, cleanup_span, uses)?);
        }
        let drop_position = graph.position(guard_drop)?;
        need(
            ops[drop_position..]
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>()
                == expected,
            "conditional identity full terminal cleanup suffix",
            (&ops, &expected),
        )?;
        let direct_calls = ops
            .iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
            .collect::<Vec<_>>();
        if index == 0 {
            need(
                direct_calls.is_empty(),
                "conditional identity Continue has no component call",
                &direct_calls,
            )?;
        } else {
            let call_index = index - 1;
            let (name, expression, _) = a.component_calls[call_index];
            let target = one(
                a.functions.iter().filter(|v| v.name == name),
                "conditional identity consumer AST",
            )?;
            let consumer = function(module, target.span)?;
            let call = one(
                direct_calls.into_iter(),
                "conditional identity exact body source call",
            )?;
            need(
                call.origin == Origin::Source(f.expression(expression)?.1)
                    && matches!(&call.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == consumer.id && arguments.as_slice() == [EntityId::Loan(key), EntityId::Loan(val)])
                    && graph.position(call)? < drop_position,
                "conditional identity ordered current K/V call before cleanup",
                call,
            )?;
            let entry = consumer
                .entry_block()
                .and_then(|b| consumer.block(b))
                .ok_or_else(|| failure("conditional identity consumer entry", consumer.id))?;
            need(
                entry.parameters.len() == 2
                    && consumer.receiver.is_none()
                    && consumer.borrow_return.is_none()
                    && entry.parameters.iter().zip([key, val]).all(|(&p, c)| {
                        consumer.entity(p).map(|e| e.ty)
                            == graph.function.entity(EntityId::Loan(c)).map(|e| e.ty)
                    }),
                "conditional identity call parameters match actual K/V types",
                consumer,
            )?;
            uses.permit(EntityId::Loan(key), call);
            uses.permit(EntityId::Loan(val), call);
            if index == 2 {
                let initial = value(call)?;
                let ty = graph
                    .function
                    .entity(EntityId::Value(initial))
                    .ok_or_else(|| failure("conditional identity Return result entity", initial))?
                    .ty;
                need(
                    matches!(ty, EntityType::Value(t)
                if module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 32, signed: true })
                    && consumer.return_types.as_slice() == [t] && graph.function.return_types.as_slice() == [t]),
                    "conditional identity real operand Int result/signatures",
                    ty,
                )?;
                uses.protect(EntityId::Value(initial));
                let mut cursor = initial;
                let mut prefix = vec![call.id];
                for i in ops
                    .iter()
                    .skip(graph.position(call)? + 1)
                    .take(drop_position - graph.position(call)? - 1)
                {
                    need(
                        i.operation == Operation::Copy { source: cursor },
                        "conditional identity Return only actual forward Copy chain",
                        i,
                    )?;
                    uses.permit(EntityId::Value(cursor), i);
                    cursor = value(i)?;
                    uses.protect(EntityId::Value(cursor));
                    prefix.push(i.id);
                }
                need(
                    ops[..drop_position]
                        .iter()
                        .map(|i| i.id)
                        .collect::<Vec<_>>()
                        == prefix
                        && graph.term(block)?.origin == Origin::Source(transfer)
                        && matches!(&graph.term(block)?.kind, TerminatorKind::Return { values } if values.as_slice() == [cursor]),
                    "conditional identity actual operand and copies precede cleanup, same value returned",
                    graph.term(block)?,
                )?;
                output.returned = Some(cursor);
            } else {
                need(
                    call.results.is_empty() && consumer.return_types.is_empty(),
                    "conditional identity read has actual Unit result",
                    call,
                )?;
            }
        }
        Ok(output)
    }

    pub(super) fn exhausted(
        &self,
        block: BlockId,
        roles: Roles,
        uses: &mut Uses,
    ) -> Result<(), Failure> {
        let (graph, a) = (self.graph, self.a);
        need(
            roles.body.is_none(),
            "conditional identity false path never acquired K/V/Guard",
            roles,
        )?;
        let expected = self.end_source(block, roles, a.span, uses)?;
        need(
            graph.ops(block)?.iter().map(|i| i.id).collect::<Vec<_>>() == expected,
            "conditional identity exhaustion only source cleanup, no payload/Next",
            graph.ops(block)?,
        )
    }

    pub(super) fn fallback(&self, block: BlockId) -> Result<(), Failure> {
        let (graph, f, a) = (self.graph, self.f, self.a);
        let scan = one(
            a.functions.iter().filter(|v| v.name == "scan"),
            "conditional identity fallback scan",
        )?;
        let body = f
            .parsed
            .ast()
            .statements()
            .get(scan.body)
            .map_err(|e| failure("conditional identity scan body", e))?;
        let Statement::Block { elements } = body.payload() else {
            return Err(failure("conditional identity scan block", body));
        };
        let statement = elements
            .last()
            .ok_or_else(|| failure("conditional identity fallback statement", elements))?;
        let statement = f
            .parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|e| failure("conditional identity fallback AST", e))?;
        let Statement::Expression { expression } = statement.payload() else {
            return Err(failure(
                "conditional identity fallback expression",
                statement,
            ));
        };
        let (returned, span) = f.expression(*expression)?;
        let Expression::Return {
            value: Some(operand),
            ..
        } = returned
        else {
            return Err(failure(
                "conditional identity actual fallback Return",
                returned,
            ));
        };
        let (_, operand_span) = f.expression(*operand)?;
        let TerminatorKind::Return { values } = &graph.term(block)?.kind else {
            return Err(failure(
                "conditional identity after-loop Return",
                graph.term(block)?,
            ));
        };
        let returned = one(
            values.iter().copied(),
            "conditional identity fallback sole result",
        )?;
        let constant = graph.definition(EntityId::Value(returned))?;
        need(
            graph.term(block)?.origin == Origin::Source(span)
                && constant.block == block
                && constant.origin == Origin::Source(operand_span)
                && constant.operation == Operation::Constant(ScalarConstant::Integer(0)),
            "conditional identity source fallback exact zero after source cleanup",
            constant,
        )?;
        need(
            !graph.ops(block)?.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::MapIterationFirst { .. }
                        | Operation::MapIterationHasNext { .. }
                        | Operation::MapIterationKeyLoan { .. }
                        | Operation::MapIterationValueLoan { .. }
                        | Operation::MapIterationNext { .. }
                        | Operation::HeapAllocate { .. }
                        | Operation::DirectCall { .. }
                )
            }),
            "conditional identity after-loop cannot read provider payload or execute body",
            graph.ops(block)?,
        )
    }
}

pub(in crate::ssa) mod unit {
//! Same-analysis unit facts; every Unit ID comes from real products or wraps an actual AST node.
use super::super::{
    super::super::Report,
    ast::{Anchors, Input},
};
use crate::test_support::diagnostics::diagnostic;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        SymbolKind, UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, IterationSource, LoanKind, MapIterationComponent,
        OwnershipBindingKind, UnitCallArgumentOwnershipKind, UnitDropTarget,
        UnitIterationSourceAccess, UnitLoanTarget, UnitValueDeliveryKind,
    },
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor, MapIterationKind,
        ParameterMode, UnitCallTarget, UnitConstructionTarget, UnitExpressionId, UnitStatementId,
        UnitTypeKind,
    },
};

fn reference(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
) -> Option<UnitSymbolId> {
    let (_, span) = f.expression(expression).ok()?;
    let refs = names
        .names()
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    match refs.as_slice() {
        [r] => match r.target() {
            UnitReferenceTarget::Symbol(id) => Some(*id),
            _ => None,
        },
        _ => None,
    }
}

fn has_span(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    span: Span,
    kind: Option<SymbolKind>,
) -> bool {
    names
        .names()
        .source_units()
        .iter()
        .filter(|s| s.source_unit() == id.source_unit())
        .any(|s| {
            s.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol()
                    && s.span() == span
                    && !s.is_synthetic()
                    && kind.is_none_or(|k| k == s.kind())
            })
        })
}

pub(in crate::ssa) fn observe(
    f: &Input,
    anchors: Option<&Anchors>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    diagnostic!(
        "SPEC0331 CONDITIONAL UNIT REAL maps={:#?} drops={:#?} contracts={:#?} loans={:#?} deliveries={:#?}",
        owned.map_iterations(),
        owned.drops(),
        owned.call_argument_contracts(),
        owned.loans(),
        owned.value_deliveries()
    );
    let Some(a) = anchors else { return };
    report.check(
        "conditional actual For source location",
        a.span.source_id() == f.parsed.source_id()
            && f.sources.source_name(a.span.source_id()) == Ok("provider.ko"),
        (a.statement, a.body, a.span),
    );
    let [source_names] = names.names().source_units() else {
        report.check(
            "unit conditional single real source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let unit = source_names.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let statement = UnitStatementId::new(unit, a.statement);
    let (Some(d), Some(p)) = (
        typed.map_iteration(statement),
        owned.map_iteration(statement),
    ) else {
        report.check(
            "unit conditional real descriptor/plan",
            false,
            (typed.map_iterations(), owned.map_iterations()),
        );
        return;
    };
    let constructor = if f.map {
        IntrinsicTypeConstructor::Map
    } else {
        IntrinsicTypeConstructor::MutableMap
    };
    let kind = if f.map {
        MapIterationKind::Map
    } else {
        MapIterationKind::MutableMap
    };
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
    let source = reference(f, names, a.source);
    let source_symbol = source.and_then(|id| {
        source_names
            .resolution()
            .symbols()
            .iter()
            .find(|s| s.id() == id.symbol())
    });
    report.check("unit conditional For/source/K/V identity", source_names.resolution().source_id() == f.parsed.source_id()
        && typed.map_iterations().len() == 1 && owned.map_iterations().len() == 1 && d.statement() == statement
        && d.source() == expression(a.source) && p.descriptor() == d && typed.expression_type(expression(a.source)) == Some(d.source_type())
        && typed.sequential_iteration(statement).is_none() && owned.iteration(statement).is_none()
        && matches!(typed.types().get(d.source_type()), Some(UnitTypeKind::Intrinsic { constructor: actual, arguments }) if *actual == constructor && d.kind() == kind && arguments.as_slice() == [d.key_type(), d.value_type()])
        && [d.key_type(), d.value_type()].iter().all(|t| typed.types().get(*t) == Some(&UnitTypeKind::Builtin(BuiltinType::String))), (unit, d, p));
    report.check("unit conditional direct parameter source capability", source_symbol.is_some_and(|s| s.kind() == SymbolKind::ValueParameter && !s.is_synthetic() && s.name() == "source")
        && source.is_some_and(|id| id.source_unit() == unit && owned.binding_kind(id) == Some(capability))
        && matches!(p.source(), Some(UnitLoanTarget::Place(path)) if path.is_root() && Some(path.root()) == source)
        && matches!(p.source_kind(), IterationSource::BorrowedSource(_))
        && p.source_access() == if f.map { UnitIterationSourceAccess::Owned } else { UnitIterationSourceAccess::Shared }, (source_symbol, p.source_kind(), p.source_access()));
    for function in &a.functions {
        let declarations = names
            .names()
            .index()
            .declarations()
            .iter()
            .filter(|d| {
                d.source_unit() == unit
                    && d.root() == function.item
                    && d.name_span() == function.name_span
                    && d.kind() == SymbolKind::Function
            })
            .collect::<Vec<_>>();
        let callables = typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|s| s.callable())
            .filter(|c| c.name_span() == function.name_span)
            .collect::<Vec<_>>();
        report.check(
            "unit conditional actual function DeclarationId and signature",
            matches!(declarations.as_slice(), [_]) && matches!(callables.as_slice(), [_]),
            (function, &declarations, &callables),
        );
        if let [callable] = callables.as_slice() {
            if function.name == "scan" {
                report.check("unit conditional actual scan parameter slot/mode", callable.receiver().is_none()
                    && matches!(callable.parameters(), [parameter] if parameter.symbol() == source && parameter.mode() == mode && parameter.ty() == d.source_type()), callable);
            } else if ["read", "operand"].contains(&function.name) {
                report.check(
                    "unit conditional synchronous dual String Borrow parameters",
                    callable.receiver().is_none()
                        && callable.parameters().len() == 2
                        && callable.parameters().iter().all(|p| {
                            p.mode() == ParameterMode::Borrow
                                && typed.types().get(p.ty())
                                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                                && p.symbol().is_some_and(|id| {
                                    id.source_unit() == unit
                                        && owned.binding_kind(id)
                                            == Some(OwnershipBindingKind::Shared)
                                })
                        }),
                    callable,
                );
            }
        }
    }
    report.check(
        "unit conditional distinct component bindings/loans",
        d.key_binding() != d.value_binding() && p.loans()[0] != p.loans()[1],
        p.loans(),
    );
    for (i, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = d.bindings()[i];
        let loan = p.loan(component);
        report.check(
            "unit conditional real String Shared component lineage",
            binding.source_unit() == unit
                && has_span(names, binding, a.bindings[i], Some(SymbolKind::ForBinding))
                && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][i])
                && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && p.bindings()[i].symbol() == binding
                && p.bindings()[i].kind() == OwnershipBindingKind::Shared
                && loan.statement() == statement
                && loan.source() == expression(a.source)
                && loan.binding() == binding
                && loan.component() == component
                && loan.kind() == LoanKind::Shared
                && Some(loan.origin()) == p.source(),
            loan,
        );
    }
    for branch in &a.branches {
        report.check(
            "unit conditional If uses actual key and typed String equality",
            reference(f, names, branch.key) == Some(d.key_binding())
                && typed.expression_type(expression(branch.key)) == Some(d.key_type())
                && typed.expression_type(expression(branch.literal)) == Some(d.key_type())
                && typed
                    .expression_type(expression(branch.condition))
                    .and_then(|t| typed.types().get(t))
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
                && typed
                    .expression_type(expression(branch.expression))
                    .and_then(|t| typed.types().get(t))
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                && typed
                    .expression_type(expression(branch.transfer))
                    .and_then(|t| typed.types().get(t))
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Nothing)),
            (branch, branch.body),
        );
    }
    let locals = owned
        .formed_owned_locals()
        .iter()
        .copied()
        .filter(|id| id.source_unit() == unit && has_span(names, *id, a.local, None))
        .collect::<Vec<_>>();
    let local = match locals.as_slice() {
        [id] => Some(*id),
        _ => None,
    };
    let construction = typed.construction(expression(a.guard));
    report.check("unit conditional actual Guard construction and binding origin", local.is_some()
        && construction.is_some_and(|c| c.expression() == expression(a.guard) && c.arguments().is_empty()
            && typed.expression_type(expression(a.guard)) == Some(c.result_type())
            && local.is_some_and(|id| typed.symbol_type(id) == Some(c.result_type()))
            && typed.is_resource_type(c.result_type()) == Some(true)
            && matches!(c.target(), UnitConstructionTarget::Nominal(id) if names.names().index().declarations().iter().any(|d| d.id() == id && d.source_unit() == unit && d.name_span() == a.guard_name))), (a.local, f.expression(a.guard), construction));
    diagnostic!(
        "SPEC0331 CONDITIONAL UNIT GUARD binding={local:?} binding_origin={:?} construction={:#?} drops={:#?}",
        a.local,
        f.expression(a.guard),
        owned
            .drops()
            .iter()
            .filter(|drop| matches!(drop.target(), UnitDropTarget::Named(id) if Some(id) == local))
            .collect::<Vec<_>>()
    );
    for (name, id, arguments) in &a.component_calls {
        let call = typed.call(expression(*id));
        let target = a
            .functions
            .iter()
            .find(|v| v.name == *name)
            .and_then(|v| {
                names.names().index().declarations().iter().find(|d| {
                    d.source_unit() == unit && d.root() == v.item && d.name_span() == v.name_span
                })
            })
            .map(|d| d.id());
        report.check(
            "unit conditional actual component consumer and Borrow arguments",
            call.is_some_and(|c| {
                matches!(c.target(), UnitCallTarget::Declaration(id) if Some(id) == target)
                    && c.receiver().is_none()
                    && c.arguments().len() == 2
                    && c.arguments().iter().enumerate().all(|(i, arg)| {
                        arg.argument_index() == i
                            && arg.parameter_index() == i
                            && arg.mode() == ParameterMode::Borrow
                            && arg.parameter_type() == [d.key_type(), d.value_type()][i]
                            && reference(f, names, arguments[i]) == Some(d.bindings()[i])
                    })
            }),
            (name, expression(*id), call),
        );
        report.check(
            "unit conditional actual consumer result type",
            typed
                .expression_type(expression(*id))
                .and_then(|t| typed.types().get(t))
                == Some(&UnitTypeKind::Builtin(if *name == "operand" {
                    BuiltinType::Int
                } else {
                    BuiltinType::Unit
                })),
            call,
        );
    }
    let operand = a.functions.iter().find(|v| v.name == "operand");
    let read = a
        .functions
        .iter()
        .find(|v| v.name == "read")
        .and_then(|v| {
            names.names().index().declarations().iter().find(|d| {
                d.source_unit() == unit && d.root() == v.item && d.name_span() == v.name_span
            })
        })
        .map(|d| d.id());
    let signature = typed
        .signatures()
        .declarations()
        .iter()
        .filter_map(|d| d.callable())
        .find(|c| operand.is_some_and(|v| c.name_span() == v.name_span));
    let call = typed.call(expression(a.operand_read.0));
    report.check(
        "unit conditional operand forwards its own two Borrow parameters",
        signature.is_some_and(|signature| {
            signature.parameters().len() == 2
                && call.is_some_and(|call| {
                    matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == read)
                        && call.receiver().is_none()
                        && call.arguments().len() == 2
                        && call.arguments().iter().enumerate().all(|(i, argument)| {
                            argument.argument_index() == i
                                && argument.parameter_index() == i
                                && argument.mode() == ParameterMode::Borrow
                                && argument.parameter_type() == signature.parameters()[i].ty()
                                && signature.parameters()[i].symbol().is_some()
                                && reference(f, names, a.operand_read.1[i])
                                    == signature.parameters()[i].symbol()
                        })
                })
        }),
        (a.operand_read, signature, call),
    );
    let scan = a
        .functions
        .iter()
        .find(|v| v.name == "scan")
        .and_then(|v| {
            names.names().index().declarations().iter().find(|d| {
                d.source_unit() == unit && d.root() == v.item && d.name_span() == v.name_span
            })
        })
        .map(|d| d.id());
    for entry in &a.entry_calls {
        let call_id = expression(entry.call);
        let argument_id = expression(entry.argument);
        let call = typed.call(call_id);
        let argument_source = reference(f, names, entry.argument);
        let result = reference(f, names, entry.result_reference);
        let int = Some(&UnitTypeKind::Builtin(BuiltinType::Int));
        report.check("unit conditional entry call real scan/slot/mode/source and result reference", call.is_some_and(|c| matches!(c.target(), UnitCallTarget::Declaration(id) if Some(id) == scan)
            && c.receiver().is_none() && matches!(c.arguments(), [arg] if arg.argument_index() == 0 && arg.parameter_index() == 0 && arg.mode() == mode && arg.parameter_type() == d.source_type()))
            && argument_source.is_some_and(|id| has_span(names, id, entry.source_binding, None))
            && result.is_some_and(|id| has_span(names, id, entry.result_binding, None))
            && [entry.call, entry.result_reference, entry.result_literal].iter().all(|id| typed.expression_type(expression(*id)).and_then(|t| typed.types().get(t)) == int)
            && typed.expression_type(expression(entry.comparison)).and_then(|t| typed.types().get(t)) == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
            && typed.expression_type(expression(entry.source_initializer)) == Some(d.source_type()), (entry, call, argument_source, result));
        let contracts = owned
            .call_argument_contracts_for(call_id)
            .collect::<Vec<_>>();
        report.check("unit conditional entry actual ownership contract", matches!(contracts.as_slice(), [contract] if contract.argument() == argument_id && contract.parameter_index() == 0
            && contract.kind() == if f.map { UnitCallArgumentOwnershipKind::Value } else { UnitCallArgumentOwnershipKind::SharedLoan }), contracts);
        let loans = owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == call_id)
            .collect::<Vec<_>>();
        report.check(
            "unit conditional entry actual Value or Shared loan identity",
            if f.map {
                loans.is_empty()
            } else {
                matches!(loans.as_slice(), [loan] if loan.argument() == argument_id
                    && loan.kind() == LoanKind::Shared
                    && matches!(loan.target(), UnitLoanTarget::Place(path)
                        if path.is_root() && Some(path.root()) == argument_source))
            },
            loans,
        );
        let deliveries = owned
            .value_deliveries()
            .iter()
            .filter(|delivery| delivery.call() == call_id)
            .collect::<Vec<_>>();
        report.check("unit conditional entry actual Move or Borrow delivery", if f.map {
            matches!(deliveries.as_slice(), [delivery] if delivery.argument() == argument_id && delivery.kind() == UnitValueDeliveryKind::Move && delivery.place().is_some_and(|path| path.is_root() && Some(path.root()) == argument_source))
        } else { deliveries.is_empty() }, deliveries);
    }
    for exit in p.exits() {
        diagnostic!(
            "SPEC0331 CONDITIONAL UNIT EXIT actual={exit:#?} complete={:#?}",
            owned.iteration_cleanup_at(exit.point())
        );
    }
    // No unit owner/selector/phi table is invented: the public unit plan has no such accessor.
}
}
