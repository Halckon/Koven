//! 同次分析的公开 FE→SSA 身份桥；普通 query，不调用生产资格证明。
#[path = "query_cleanup.rs"]
pub(in crate::ssa) mod query_cleanup;
mod query_guards {
    //! 两个真实 Resource 构造/析构身份；形成历史与当前 owner 运输分开验证。
    use super::super::ast::{Anchors, Input};
    use super::{Failure, failure, need, one, unit_bridge};
    use lang_frontend::{
        name_resolution::{
            Namespace, SourceUnitId, SymbolKind, UnitSymbolId, ValidatedCompilationUnitNames,
        },
        ownership_checking::CompilationUnitOwnership,
        type_checking::{
            CompilationUnitTypes, ExpressionCategory, NominalKind, ParameterMode,
            UnitConstructionTarget, UnitExpressionId, UnitItemId, UnitStatementId, UnitTypeKind,
        },
    };

    pub(super) fn unit(
        f: &Input,
        a: &Anchors,
        names: &ValidatedCompilationUnitNames,
        typed: &CompilationUnitTypes,
        owned: &CompilationUnitOwnership,
        unit: SourceUnitId,
    ) -> Result<[UnitSymbolId; 2], Failure> {
        let mut locals = Vec::new();
        let mut classes = Vec::new();
        let mut types = Vec::new();
        for guard in &a.guards {
            // 普通 Guard 无 Name 读取；只从真实 formed 表取 qualified ID，再回核 names marker。
            let local = one(
                owned.formed_owned_locals().iter().copied().filter(|id| {
                    id.source_unit() == unit
                        && unit_bridge::binding_at(names, *id, guard.marker, SymbolKind::Variable)
                            .is_ok()
                }),
                "nested exits FE unique actual qualified Guard formation",
            )?;
            unit_bridge::binding_at(names, local, guard.marker, SymbolKind::Variable)?;
            let class = one(
                names.names().index().declarations().iter().filter(|d| {
                    d.source_unit() == unit
                        && d.root() == guard.class_item
                        && d.name_span() == guard.class_marker
                        && d.kind() == SymbolKind::Classifier
                        && d.namespace() == Namespace::Type
                }),
                "nested exits FE unique actual qualified Guard declaration",
            )?
            .id();
            let nominal = one(
                typed
                    .signatures()
                    .declarations()
                    .iter()
                    .filter(|d| d.declaration() == class)
                    .filter_map(|d| d.nominal()),
                "nested exits FE unique qualified Guard nominal",
            )?;
            let expression = UnitExpressionId::new(unit, guard.initializer);
            let construction = typed.construction(expression).ok_or_else(|| {
                failure("nested exits FE actual qualified Guard construction", guard)
            })?;
            let deinit = nominal
                .deinit()
                .ok_or_else(|| failure("nested exits FE actual qualified Guard deinit", nominal))?;
            need(
                nominal.declaration() == class
                    && nominal.kind() == NominalKind::Class
                    && nominal.fields().is_empty()
                    && nominal.type_parameters().is_empty()
                    && deinit.owner() == class
                    && deinit.item() == UnitItemId::new(unit, guard.deinit_item)
                    && deinit.body() == UnitStatementId::new(unit, guard.deinit_body)
                    && deinit.receiver_mode() == ParameterMode::Borrow
                    && deinit.receiver_type() == construction.result_type()
                    && construction.expression() == expression
                    && construction.arguments().is_empty()
                    && construction.instance().type_arguments().is_empty()
                    && construction.target() == UnitConstructionTarget::Nominal(class)
                    && matches!(typed.types().get(construction.result_type()),
                    Some(UnitTypeKind::Nominal { declaration, arguments }) if *declaration == class && arguments.is_empty())
                    && typed.expression_type(expression) == Some(construction.result_type())
                    && typed.expression_category(expression) == Some(ExpressionCategory::Temporary)
                    && typed.symbol_type(local) == Some(construction.result_type())
                    && typed.is_resource_type(construction.result_type()) == Some(true),
                "nested exits FE qualified Guard Resource formation, nominal and real deinit",
                (guard, local, nominal, construction, deinit),
            )?;
            unit_bridge::callee(f, names, guard.initializer, class, Namespace::Type)?;
            locals.push(local);
            classes.push(class);
            types.push(construction.result_type());
        }
        need(
            locals.len() == 2
                && locals[0] != locals[1]
                && classes[0] != classes[1]
                && types[0] != types[1],
            "nested exits FE qualified Guard identities stay distinct",
            (&locals, classes, types),
        )?;
        locals
            .try_into()
            .map_err(|v| failure("nested exits FE two actual qualified Guard locals", v))
    }
}
#[path = "query_inputs_unit.rs"]
mod unit_bridge;

use super::{
    super::{
        fixture::{Failure, failure, need},
        query_inputs::one,
    },
    ast::{Anchors, Exit, Function, Input, Read, Site},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{SourceUnitId, ValidatedCompilationUnitNames},
    ownership_checking::CompilationUnitOwnership,
    parser::Expression,
    source::Span,
    type_checking::{BuiltinType, CompilationUnitTypes},
};

#[derive(Clone, Copy, Debug)]
pub(super) struct SiteInput {
    pub span: Span,
    pub source: Span,
    pub body: Span,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ReadInput {
    pub span: Span,
    pub arguments: [Span; 2],
    pub site: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CallerInput {
    pub initializer: Span,
    pub call: Span,
    pub argument: Span,
    pub local: Span,
    pub size: Span,
    pub drop: Span,
    pub result_reference: Option<Span>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::ssa) struct GuardInput {
    pub marker: Span,
    pub construction: Span,
    pub deinit: Span,
    pub class_marker: Span,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct OperandInput {
    pub function: Span,
    pub call: Span,
    pub arguments: [Span; 4],
}

#[derive(Debug)]
pub(in crate::ssa) struct Inputs {
    pub unit: SourceUnitId,
    pub entry: Span,
    pub scan: Span,
    pub read: Span,
    pub factory: Span,
    pub parameter: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests::nested_exits) sites: [SiteInput; 2],
    pub(in crate::ssa::map_provider_exits_pipeline_tests::nested_exits) reads: Vec<ReadInput>,
    pub(in crate::ssa::map_provider_exits_pipeline_tests::nested_exits) callers: [CallerInput; 2],
    pub sizes: Vec<Span>,
    pub guards: [GuardInput; 2],
    pub transfer: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests::nested_exits) operand:
        Option<OperandInput>,
}

pub(super) fn unit(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    unit_bridge::collect(f, a, names, typed, owned)
}

fn anchor<'a>(a: &'a Anchors, name: &str) -> Result<&'a Function, Failure> {
    one(
        a.functions.iter().filter(|v| v.name == name),
        "nested exits FE unique actual function AST",
    )
}

fn expected_result(f: &Input, name: &str) -> BuiltinType {
    if name == "operand" || (name == "scan" && f.exit == Exit::Return) {
        BuiltinType::Int
    } else {
        BuiltinType::Unit
    }
}

fn callee_span(f: &Input, expression: ExpressionId) -> Result<Span, Failure> {
    let (value, _) = f.expression(expression)?;
    let Expression::Call { callee, .. } = value else {
        return Err(failure("nested exits FE actual Call expression", value));
    };
    let (callee, span) = f.expression(*callee)?;
    need(
        matches!(callee, Expression::Name),
        "nested exits FE actual direct Name callee",
        callee,
    )?;
    Ok(span)
}

fn site_span(f: &Input, site: &Site) -> Result<SiteInput, Failure> {
    let body = f
        .parsed
        .ast()
        .statements()
        .get(site.body)
        .map_err(|e| failure("nested exits FE actual body span", e))?
        .span();
    Ok(SiteInput {
        span: site.span,
        source: site.source_span,
        body,
    })
}

fn read_span(f: &Input, a: &Anchors, read: &Read, index: usize) -> Result<ReadInput, Failure> {
    let [key, value] = read.call.arguments.as_slice() else {
        return Err(failure("nested exits FE actual read argument pair", read));
    };
    let sites = if f.exit == Exit::Return {
        &[0, 0, 1][..]
    } else {
        &[0, 0, 1, 0][..]
    };
    let site = *sites
        .get(index)
        .ok_or_else(|| failure("nested exits FE read count", index))?;
    let expected = if f.exit == Exit::Return && index > 0 {
        let parameters = &anchor(a, "operand")?.parameters;
        parameters
            .get(site * 2..site * 2 + 2)
            .ok_or_else(|| failure("nested exits FE operand parameter pair", parameters))?
    } else {
        &a.sites[site].bindings
    };
    need(
        read.binding_markers == expected,
        "nested exits FE read keeps real site or operand parameter marker pair",
        read,
    )?;
    Ok(ReadInput {
        span: read.call.span,
        arguments: [f.expression(*key)?.1, f.expression(*value)?.1],
        site,
    })
}

fn caller_span(f: &Input, a: &Anchors, index: usize, drop: Span) -> Result<CallerInput, Failure> {
    let call = &a.scans[index];
    let [argument] = call.arguments.as_slice() else {
        return Err(failure("nested exits FE actual scan argument", call));
    };
    let (local, initializer) = a.locals[index];
    let size = one(
        a.sizes.iter().filter(|s| s.root_marker == local),
        "nested exits FE actual caller size anchor",
    )?;
    need(
        drop == local,
        "nested exits FE caller Drop is actual local marker",
        (local, drop),
    )?;
    let result_reference = a
        .return_checks
        .get(index)
        .map(|check| f.expression(check.reference).map(|(_, span)| span))
        .transpose()?;
    Ok(CallerInput {
        initializer: f.expression(initializer)?.1,
        call: call.span,
        argument: f.expression(*argument)?.1,
        local,
        size: size.span,
        drop,
        result_reference,
    })
}

fn spans(f: &Input, a: &Anchors, drops: [Span; 2], unit: SourceUnitId) -> Result<Inputs, Failure> {
    let parameter = a
        .parameter()
        .ok_or_else(|| failure("nested exits FE sole scan parameter", a))?;
    let mut sizes = a
        .sizes
        .iter()
        .filter(|s| s.root_marker == parameter)
        .map(|s| s.span)
        .collect::<Vec<_>>();
    sizes.sort_by_key(|s| s.start());
    need(
        sizes.len() == if f.exit == Exit::Return { 0 } else { 2 }
            && sizes
                .windows(2)
                .all(|pair| pair[0].end() <= pair[1].start())
            && a.reads.len() == if f.exit == Exit::Return { 3 } else { 4 }
            && a.return_checks.len() == if f.exit == Exit::Return { 2 } else { 0 },
        "nested exits FE exact scan size/read/result-reference coverage",
        (&sizes, a),
    )?;
    let operand = match &a.operand {
        Some(call) => {
            let [ok, ov, ik, iv] = call.arguments.as_slice() else {
                return Err(failure(
                    "nested exits FE exact four operand arguments",
                    call,
                ));
            };
            need(
                f.exit == Exit::Return,
                "nested exits FE operand only for Return",
                call,
            )?;
            Some(OperandInput {
                function: anchor(a, "operand")?.span,
                call: call.span,
                arguments: [
                    f.expression(*ok)?.1,
                    f.expression(*ov)?.1,
                    f.expression(*ik)?.1,
                    f.expression(*iv)?.1,
                ],
            })
        }
        None => {
            need(
                f.exit != Exit::Return,
                "nested exits FE Return requires operand",
                f.exit,
            )?;
            None
        }
    };
    Ok(Inputs {
        unit,
        entry: anchor(a, "entry")?.span,
        scan: anchor(a, "scan")?.span,
        read: anchor(a, "read")?.span,
        factory: anchor(a, a.factory.target)?.span,
        parameter,
        sites: [site_span(f, &a.sites[0])?, site_span(f, &a.sites[1])?],
        reads: a
            .reads
            .iter()
            .enumerate()
            .map(|(i, read)| read_span(f, a, read, i))
            .collect::<Result<Vec<_>, _>>()?,
        callers: [
            caller_span(f, a, 0, drops[0])?,
            caller_span(f, a, 1, drops[1])?,
        ],
        sizes,
        guards: a.guards.each_ref().map(|g| GuardInput {
            marker: g.marker,
            construction: g.span,
            deinit: g.deinit_span,
            class_marker: g.class_marker,
        }),
        transfer: f.expression(a.transfer)?.1,
        operand,
    })
}

pub(in crate::ssa) mod query_resources {
    //! Guard allocation identity and exact emitted cleanup, separate from FE cleanup actions.
    use super::super::super::{
        fixture::{Failure, failure, need},
        query_graph::{function, result, value},
        query_inputs::one,
    };
    use super::super::{
        query_body::{Context, Layer},
        query_graph::{Role, Roles, Witness, ty},
        query_inputs::{GuardInput, Inputs},
    };
    use crate::ssa::model::*;
    use lang_frontend::source::Span;

    pub(in crate::ssa) struct CleanupSite<'a> {
        pub layer: Layer,
        pub guard: Role,
        pub guard_input: &'a GuardInput,
        pub origin: Span,
        pub advance: Option<Span>,
    }

    impl<'ir> Context<'_, 'ir> {
        pub fn guard(
            &self,
            block: BlockId,
            input: &GuardInput,
            role: Role,
            roles: &mut Roles,
            w: &mut Witness,
        ) -> Result<InstructionId, Failure> {
            let g = self.graph;
            let allocation = one(
                g.ops(block)?.into_iter().filter(|i| {
                    i.origin == Origin::Source(input.construction)
                        && matches!(i.operation, Operation::HeapAllocate { .. })
                }),
                "nested exits SSA exact source Guard allocation",
            )?;
            let Operation::HeapAllocate { owner, payload } = allocation.operation else {
                return Err(failure(
                    "nested exits SSA actual Guard allocation",
                    allocation,
                ));
            };
            let deinit = function(self.module, input.deinit)?;
            let payload_op = g.definition(EntityId::Value(payload))?;
            let Operation::AggregateConstruct {
                aggregate,
                ref fields,
            } = payload_op.operation
            else {
                return Err(failure(
                    "nested exits SSA actual Guard aggregate",
                    payload_op,
                ));
            };
            need(
                payload_op.block == block
                    && payload_op.origin == Origin::Source(input.construction)
                    && fields.is_empty()
                    && matches!(self.module.type_kind(owner), Some(SsaTypeKind::HeapOwner { payload: Some(actual), .. }) if *actual == aggregate)
                    && matches!(self.module.type_kind(aggregate), Some(SsaTypeKind::Aggregate { fields, .. }) if fields.is_empty())
                    && self.module.deinits.get(&owner) == Some(&deinit.id)
                    && ty(g, result(allocation)?)? == EntityType::Value(owner)
                    && ty(g, result(payload_op)?)? == EntityType::Value(aggregate)
                    && g.position(payload_op)? == 2
                    && g.position(allocation)? == 3
                    && input.class_marker != input.marker
                    && deinit.return_types.is_empty()
                    && deinit.borrow_return.is_none()
                    && deinit.carrier_return.is_none()
                    && deinit.receiver
                        == Some(EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: owner,
                        }),
                "nested exits SSA Guard actual empty payload/type/deinit and source-order construction",
                (input, payload_op, allocation, deinit),
            )?;
            w.uses.protect(EntityId::Value(payload));
            w.uses.permit(EntityId::Value(payload), allocation);
            roles.insert(role, result(allocation)?);
            w.protect(roles)?;
            Ok(allocation.id)
        }

        /// Returns the exact expected sequence, without treating FinishProvider as a fake SSA op.
        pub fn cleanup(
            &self,
            block: BlockId,
            cleanup: CleanupSite<'_>,
            roles: &Roles,
            w: &mut Witness,
        ) -> Result<(Roles, Vec<InstructionId>), Failure> {
            let CleanupSite {
                layer,
                guard,
                guard_input,
                origin,
                advance,
            } = cleanup;
            let g = self.graph;
            let owner = roles.value(guard)?;
            let drop = one(
                g.ops(block)?
                    .into_iter()
                    .filter(|i| i.operation == Operation::Drop { owner }),
                "nested exits SSA exact current Guard Drop",
            )?;
            need(
                drop.origin == Origin::Source(guard_input.marker) && drop.results.is_empty(),
                "nested exits SSA current Guard Drop retains actual FE local origin",
                drop,
            )?;
            w.uses.permit(EntityId::Value(owner), drop);
            let mut sequence = vec![drop.id];
            for role in [layer.val, layer.key] {
                let loan = roles.loan(role)?;
                let end = one(
                    g.ops(block)?
                        .into_iter()
                        .filter(|i| i.operation == Operation::BorrowEnd { loan }),
                    "nested exits SSA unique current V/K EndBinding",
                )?;
                need(
                    end.origin == Origin::Source(origin) && end.results.is_empty(),
                    "nested exits SSA V/K cleanup exact source point",
                    end,
                )?;
                w.uses.permit(EntityId::Loan(loan), end);
                sequence.push(end.id);
            }
            let mut after = roles.clone();
            for role in [guard, layer.val, layer.key] {
                after.0.remove(&role);
            }
            let source = roles.loan(layer.source)?;
            if let Some(advance_origin) = advance {
                let cursor = roles.value(layer.cursor)?;
                let next = one(
                    g.ops(block)?
                        .into_iter()
                        .filter(|i| i.operation == Operation::MapIterationNext { source, cursor }),
                    "nested exits SSA exact current layer Next",
                )?;
                need(
                    next.origin == Origin::Source(advance_origin)
                        && ty(g, result(next)?)? == ty(g, roles.get(layer.cursor)?)?,
                    "nested exits SSA Next origin and same cursor type",
                    next,
                )?;
                w.uses.permit(EntityId::Loan(source), next);
                w.uses.permit(EntityId::Value(cursor), next);
                w.providers.insert(next.id);
                sequence.push(next.id);
                after.insert(layer.cursor, result(next)?);
            } else {
                let end = one(
                    g.ops(block)?
                        .into_iter()
                        .filter(|i| i.operation == Operation::BorrowEnd { loan: source }),
                    "nested exits SSA exact current layer EndSource",
                )?;
                need(
                    end.origin == Origin::Source(origin) && end.results.is_empty(),
                    "nested exits SSA terminal EndSource origin",
                    end,
                )?;
                w.uses.permit(EntityId::Loan(source), end);
                sequence.push(end.id);
                after.0.remove(&layer.source);
                after.0.remove(&layer.cursor);
            }
            w.protect(&after)?;
            Ok((after, sequence))
        }

        pub fn suffix(&self, block: BlockId, expected: &[InstructionId]) -> Result<(), Failure> {
            let first = expected
                .first()
                .copied()
                .ok_or_else(|| failure("nested exits SSA nonempty cleanup suffix", expected))?;
            let instruction = self
                .graph
                .function
                .instruction(first)
                .ok_or_else(|| failure("nested exits SSA cleanup head", first))?;
            need(
                self.graph.ops(block)?[self.graph.position(instruction)?..]
                    .iter()
                    .map(|i| i.id)
                    .collect::<Vec<_>>()
                    == expected,
                "nested exits SSA full cleanup suffix in exact order",
                (expected, self.graph.ops(block)?),
            )
        }

        pub fn operand(
            &self,
            block: BlockId,
            input: &Inputs,
            roles: &Roles,
            w: &mut Witness,
        ) -> Result<(&'ir Instruction, ValueId), Failure> {
            let g = self.graph;
            let anchor = input
                .operand
                .as_ref()
                .ok_or_else(|| failure("nested exits SSA actual operand bridge", input))?;
            let callee = function(self.module, anchor.function)?;
            let args = [
                Role::OuterKey,
                Role::OuterValue,
                Role::InnerKey,
                Role::InnerValue,
            ]
            .into_iter()
            .map(|role| roles.get(role))
            .collect::<Result<Vec<_>, _>>()?;
            let call = one(
                g.ops(block)?.into_iter().filter(|i| {
                    i.origin == Origin::Source(anchor.call)
                        && matches!(i.operation, Operation::DirectCall { .. })
                }),
                "nested exits SSA actual Return operand call",
            )?;
            let entry = callee
                .entry_block()
                .and_then(|id| callee.block(id))
                .ok_or_else(|| failure("nested exits SSA operand callee entry", callee.id))?;
            let initial = value(call)?;
            let int = ty(g, EntityId::Value(initial))?.semantic_type();
            need(
                matches!(&call.operation, Operation::DirectCall { callee: target, receiver: None, arguments }
                if *target == callee.id && arguments == &args)
                    && module_int(self.module, int)
                    && callee.return_types.as_slice() == [int]
                    && g.function.return_types.as_slice() == [int]
                    && entry.parameters.len() == 4
                    && entry.parameters.iter().zip(&args).all(|(&p, &a)| {
                        callee.entity(p).map(|d| d.ty) == g.function.entity(a).map(|d| d.ty)
                    })
                    && anchor
                        .arguments
                        .iter()
                        .all(|&span| super::super::super::fixture::contains(anchor.call, span)),
                "nested exits SSA actual operand declaration and four ordered current Borrow slots",
                (call, callee, &args),
            )?;
            for arg in args {
                w.uses.permit(arg, call);
            }
            let mut current = initial;
            w.uses.protect(EntityId::Value(current));
            for copy in g
                .ops(block)?
                .into_iter()
                .skip(g.position(call)? + 1)
                .take_while(|i| matches!(i.operation, Operation::Copy { .. }))
            {
                need(
                    copy.operation == Operation::Copy { source: current }
                        && ty(g, result(copy)?)? == EntityType::Value(int)
                        && copy.origin == Origin::Source(input.transfer),
                    "nested exits SSA same Int operand forward Copy chain",
                    copy,
                )?;
                w.uses.permit(EntityId::Value(current), copy);
                current = value(copy)?;
                w.uses.protect(EntityId::Value(current));
            }
            need(
                g.term(block)?.origin == Origin::Source(input.transfer)
                    && matches!(&g.term(block)?.kind, TerminatorKind::Return { values } if values.as_slice() == [current]),
                "nested exits SSA exact evaluated Int reaches actual Return",
                g.term(block)?,
            )?;
            w.returned = Some((block, current));
            w.returns.insert(block, vec![current]);
            Ok((call, current))
        }
    }

    pub(in crate::ssa) fn module_int(module: &Module, ty: SsaTypeId) -> bool {
        module.type_kind(ty)
            == Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
    }
}
