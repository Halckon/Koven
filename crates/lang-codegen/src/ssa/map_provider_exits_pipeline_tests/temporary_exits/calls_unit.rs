//! source-qualified factory/scanner/consumer 与真实 Return caller 链；不手造 local ID。
use super::{
    super::Report,
    ast::{Anchors, Call, Input},
    unit::{binding_at, declaration, reference, symbol},
};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        DeclarationId, Namespace, SourceUnitId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CompilationUnitOwnership, LoanKind, OwnershipBindingKind, UnitCallArgumentOwnershipKind,
        UnitLoanTarget,
    },
    parser::Expression,
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, ParameterMode, UnitCallTarget,
        UnitCallableTarget, UnitExpressionId, UnitTypeKind,
    },
};

pub(super) fn callee(
    f: &Input,
    names: &ValidatedCompilationUnitNames,
    expression: ExpressionId,
    selected: DeclarationId,
    namespace: Namespace,
) -> bool {
    let [source] = names.names().source_units() else {
        return false;
    };
    let Ok((Expression::Call { callee, .. }, _)) = f.expression(expression) else {
        return false;
    };
    let Ok((Expression::Name, span)) = f.expression(*callee) else {
        return false;
    };
    let references = names
        .names()
        .references()
        .iter()
        .filter(|r| r.span() == span)
        .collect::<Vec<_>>();
    matches!(references.as_slice(), [reference] if reference.source_unit() == source.source_unit()
    && reference.namespace() == Some(namespace) && match reference.target() {
        UnitReferenceTarget::Declaration(id) => *id == selected,
        UnitReferenceTarget::OverloadSet(ids) => ids.contains(&selected),
        _ => false,
    })
}

pub(super) fn observe(
    f: &Input,
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    let [source] = names.names().source_units() else {
        report.check(
            "unit temporary exits callable unique source",
            false,
            names.names().source_units(),
        );
        return;
    };
    let unit = source.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    signatures(a, names, typed, owned, unit, report);
    for site in &a.sites {
        let factory = a
            .function(site.factory)
            .and_then(|function| declaration(names, function, unit));
        let call = typed.call(expression(site.source));
        report.check("unit temporary exits real qualified factory Call, selected callee and Owned result",
            factory.is_some_and(|id| callee(f, names, site.source, id, Namespace::Value))
                && call.is_some_and(|call| call.expression() == expression(site.source)
                    && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == factory)
                    && call.receiver().is_none() && call.arguments().is_empty()
                    && call.result_source() == CallableResultSource::Owned
                    && Some(call.return_type()) == typed.expression_type(expression(site.source))), (site, factory, call));
        let consumer = Call {
            expression: site.read,
            span: site.read_span,
            target: site.consumer,
            arguments: site.arguments.to_vec(),
        };
        let roots = site
            .bindings
            .iter()
            .map(|span| symbol(names, unit, *span))
            .collect::<Option<Vec<_>>>();
        report.check(
            "unit temporary exits real qualified consumer component roots",
            roots.is_some(),
            (&consumer, &roots),
        );
        if let Some(roots) = roots {
            borrow_call(f, a, &consumer, names, (typed, owned), &roots, report);
        }
    }
    if let Some(read) = &a.operand {
        let roots = read
            .binding_markers
            .iter()
            .map(|span| symbol(names, unit, *span))
            .collect::<Option<Vec<_>>>();
        report.check(
            "unit temporary exits operand to read qualified parameter roots",
            roots.is_some(),
            (read, &roots),
        );
        if let Some(roots) = roots {
            borrow_call(f, a, &read.call, names, (typed, owned), &roots, report);
        }
    }
    for scan in &a.scans {
        let target = a
            .function(scan.target)
            .and_then(|function| declaration(names, function, unit));
        let call = typed.call(expression(scan.expression));
        report.check("unit temporary exits real qualified zero-argument scanner and Int result", target.is_some_and(|id| callee(f, names, scan.expression, id, Namespace::Value))
            && f.expression(scan.expression).is_ok_and(|(_, span)| span == scan.span) && scan.arguments.is_empty() && call.is_some_and(|call| call.expression() == expression(scan.expression)
                && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == target)
                && call.receiver().is_none() && call.arguments().is_empty()
                && call.result_source() == CallableResultSource::Owned
                && typed.types().get(call.return_type()) == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                && typed.expression_type(expression(scan.expression)) == Some(call.return_type()))
            && !owned.loans().iter().any(|loan| loan.call() == expression(scan.expression))
            && owned.call_argument_contracts_for(expression(scan.expression)).next().is_none()
            && !owned.value_deliveries().iter().any(|v| v.call() == expression(scan.expression)), (scan, target, call));
    }
    for (index, check) in a.return_checks.iter().enumerate() {
        let root = symbol(names, unit, check.marker);
        let initializer = UnitExpressionId::new(unit, check.initializer);
        let reference_id = UnitExpressionId::new(unit, check.reference);
        report.check(
            "unit temporary exits caller exact 7/0 result reference to actual scan initializer",
            root.is_some_and(|id| {
                id.source_unit() == unit
                    && binding_at(names, id, check.marker, SymbolKind::Variable)
            }) && check.expected == if index == 0 { "7" } else { "0" }
                && reference(f, names, check.reference) == root
                && a.scans
                    .iter()
                    .any(|call| call.expression == check.initializer)
                && typed.call(initializer).is_some_and(|call| {
                    call.expression() == initializer
                        && typed.types().get(call.return_type())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                })
                && root
                    .is_some_and(|id| typed.symbol_type(id) == typed.expression_type(initializer))
                && typed.expression_type(reference_id) == typed.expression_type(initializer),
            (check, root, typed.call(initializer)),
        );
    }
}

fn borrow_call(
    f: &Input,
    a: &Anchors,
    site: &Call,
    names: &ValidatedCompilationUnitNames,
    products: (&CompilationUnitTypes, &CompilationUnitOwnership),
    roots: &[UnitSymbolId],
    report: &mut Report,
) {
    let (typed, owned) = products;
    let Some(first) = roots.first() else {
        report.check("unit temporary exits nonempty Borrow roots", false, roots);
        return;
    };
    let unit = first.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let call_id = expression(site.expression);
    let target = a
        .function(site.target)
        .and_then(|function| declaration(names, function, unit));
    let call = typed.call(call_id);
    report.check(
        "unit temporary exits actual read/scan declaration and ordered Borrow references",
        target.is_some()
            && f.expression(site.expression)
                .is_ok_and(|(_, span)| span == site.span)
            && site.arguments.len() == roots.len() && call.is_some_and(|call| {
            call.expression() == call_id
                && matches!(call.target(), UnitCallTarget::Declaration(id) if Some(id) == target)
                && target.is_some_and(|id| callee(f, names, site.expression, id, Namespace::Value))
                && call.receiver().is_none()
                && call.result_source() == CallableResultSource::Owned
                && typed.expression_type(call_id) == Some(call.return_type())
                && call.arguments().len() == roots.len()
                && typed.types().get(call.return_type())
                    == Some(&UnitTypeKind::Builtin(expected_result(site.target)))
                && call.arguments().iter().enumerate().all(|(i, arg)| {
                    arg.argument_index() == i
                        && arg.parameter_index() == i
                        && arg.mode() == ParameterMode::Borrow
                        && roots[i].source_unit() == unit
                        && typed.expression_type(expression(site.arguments[i]))
                            == Some(arg.parameter_type())
                        && typed.symbol_type(roots[i]) == Some(arg.parameter_type())
                        && reference(f, names, site.arguments[i]) == Some(roots[i])
                })
        }),
        (site, roots, target, call),
    );
    let contracts = owned
        .call_argument_contracts_for(call_id)
        .collect::<Vec<_>>();
    report.check(
        "unit temporary exits read/scan exact qualified SharedLoan contracts",
        contracts.len() == roots.len()
            && site
                .arguments
                .iter()
                .zip(roots)
                .enumerate()
                .all(|(i, (argument, root))| {
                    contracts
                        .iter()
                        .filter(|c| {
                            c.call() == call_id
                                && c.argument() == expression(*argument)
                                && c.parameter_index() == i
                                && typed.symbol_type(*root) == Some(c.parameter_type())
                                && c.kind() == UnitCallArgumentOwnershipKind::SharedLoan
                        })
                        .count()
                        == 1
                })
            && !owned
                .value_deliveries()
                .iter()
                .any(|delivery| delivery.call() == call_id),
        (site, &contracts),
    );
    let loans = owned
        .loans()
        .iter()
        .filter(|loan| loan.call() == call_id)
        .collect::<Vec<_>>();
    report.check("unit temporary exits read/scan loans retain qualified root and argument identity", loans.len() == roots.len()
        && site.arguments.iter().zip(roots).all(|(argument, root)| loans.iter().filter(|loan|
            loan.argument() == expression(*argument) && loan.kind() == LoanKind::Shared
                && matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == *root)).count() == 1), (site, roots, loans));
}

fn signatures(
    a: &Anchors,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    unit: SourceUnitId,
    report: &mut Report,
) {
    for function in &a.functions {
        let id = declaration(names, function, unit);
        let signatures = typed
            .signatures()
            .declarations()
            .iter()
            .filter(|s| Some(s.declaration()) == id)
            .filter_map(|s| s.callable())
            .collect::<Vec<_>>();
        report.check("unit temporary exits actual function declaration/callable", id.is_some()
            && matches!(signatures.as_slice(), [signature] if signature.name_span() == function.name_span
                && matches!(signature.target(), UnitCallableTarget::Declaration(target) if Some(target) == id)), (function, id, &signatures));
        let [signature] = signatures.as_slice() else {
            continue;
        };
        let source_type = typed.expression_type(UnitExpressionId::new(unit, a.sites[0].source));
        report.check(
            "unit temporary exits exact unmarked Borrow signatures",
            signature.receiver().is_none()
                && signature.type_parameters().is_empty()
                && signature.has_body()
                && signature.result_source() == CallableResultSource::Owned
                && signature.parameters().len() == function.parameters.len()
                && signature
                    .parameters()
                    .iter()
                    .enumerate()
                    .all(|(i, parameter)| {
                        parameter.mode() == ParameterMode::Borrow
                            && parameter.symbol() == symbol(names, unit, function.parameters[i])
                            && parameter.symbol().is_some_and(|id| {
                                owned.binding_kind(id) == Some(OwnershipBindingKind::Shared)
                            })
                            && typed.types().get(parameter.ty())
                                == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    })
                && if a.sites.iter().any(|site| site.factory == function.name) {
                    Some(signature.return_type()) == source_type
                } else {
                    typed.types().get(signature.return_type())
                        == Some(&UnitTypeKind::Builtin(expected_result(function.name)))
                },
            (function, signature),
        );
    }
}

fn expected_result(name: &str) -> BuiltinType {
    if matches!(name, "operand" | "scanNonempty" | "scanEmpty") {
        BuiltinType::Int
    } else {
        BuiltinType::Unit
    }
}

pub(in crate::ssa) mod query_body {
    //! Actual K/V Borrow lineage, Resource allocation and ordered transfer cleanup.
    use super::super::{
        super::{
            fixture::{Failure, failure, need},
            query_graph::{Graph, Uses, function, loan, result, value},
            query_inputs::one,
        },
        ast::Exit,
        query_inputs::SiteInput,
        query_ssa::{Roles, ty},
    };
    use crate::ssa::model::*;
    use lang_frontend::source::Span;
    use std::collections::{BTreeMap, BTreeSet};

    pub(in crate::ssa) struct Body<'a, 'ir> {
        pub graph: &'a Graph<'ir>,
        pub module: &'ir Module,
        pub site: &'a SiteInput,
        pub block: BlockId,
        pub roles: Roles,
        pub exit: Exit,
    }

    #[derive(Debug)]
    pub(in crate::ssa) struct End {
        pub next: Option<ValueId>,
        pub returned: Option<ValueId>,
        pub call: InstructionId,
        pub guard: ValueId,
        pub providers: Vec<InstructionId>,
    }

    impl Body<'_, '_> {
        pub fn check(
            &self,
            uses: &mut Uses,
            protected: &mut BTreeSet<EntityId>,
        ) -> Result<End, Failure> {
            let (g, site, roles) = (self.graph, self.site, self.roles);
            let ops = g.ops(self.block)?;
            let key = one(
                ops.iter()
                    .copied()
                    .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
                "temporary exits SSA sole actual KeyLoan",
            )?;
            let val = one(
                ops.iter()
                    .copied()
                    .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
                "temporary exits SSA sole actual ValueLoan",
            )?;
            let (k, v) = (loan(key)?, loan(val)?);
            need(key.operation == Operation::MapIterationKeyLoan { source: roles.source, cursor: roles.cursor }
            && val.operation == Operation::MapIterationValueLoan { source: roles.source, cursor: roles.cursor }
            && [key, val].iter().all(|i| i.origin == Origin::Source(site.span))
            && g.position(key)? == 0 && g.position(val)? == 1 && k != v
            && [k, v].iter().all(|&l| matches!(ty(g, EntityId::Loan(l)), Ok(EntityType::Loan { kind: LoanKind::Shared, target }) if self.module.type_kind(target) == Some(&SsaTypeKind::StringOwner))),
            "temporary exits SSA ordered current source/cursor Shared String components", (roles, key, val))?;
            for i in [key, val] {
                uses.permit(EntityId::Loan(roles.source), i);
                uses.permit(EntityId::Value(roles.cursor), i);
            }
            let allocation = one(
                ops.iter()
                    .copied()
                    .filter(|i| matches!(i.operation, Operation::HeapAllocate { .. })),
                "temporary exits SSA actual body Guard allocation",
            )?;
            let Operation::HeapAllocate { owner, payload } = allocation.operation else {
                return Err(failure("temporary exits SSA Guard allocation", allocation));
            };
            let construction = g.definition(EntityId::Value(payload))?;
            let deinit = function(self.module, site.guard.deinit)?;
            let guard = value(allocation)?;
            need(
                matches!(&construction.operation, Operation::AggregateConstruct { aggregate, fields }
                if fields.is_empty() && matches!(self.module.type_kind(*aggregate), Some(SsaTypeKind::Aggregate { fields, .. }) if fields.is_empty())
                && matches!(self.module.type_kind(owner), Some(SsaTypeKind::HeapOwner { payload: Some(actual), .. }) if actual == aggregate)
                && ty(g, result(construction)?).ok() == Some(EntityType::Value(*aggregate)))
                    && construction.block == self.block
                    && construction.origin == Origin::Source(site.guard.construction)
                    && allocation.origin == construction.origin
                    && g.position(construction)? == 2
                    && g.position(allocation)? == 3
                    && ty(g, EntityId::Value(guard))? == EntityType::Value(owner)
                    && self.module.deinits.get(&owner) == Some(&deinit.id)
                    && deinit.receiver
                        == Some(EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: owner,
                        })
                    && deinit.return_types.is_empty()
                    && deinit.borrow_return.is_none()
                    && deinit.carrier_return.is_none()
                    && site.guard.marker != site.guard.construction
                    && site.guard.class_marker != site.guard.marker,
                "temporary exits SSA actual empty Resource payload/allocation/deinit and construction origin",
                (construction, allocation, deinit),
            )?;
            for entity in [
                EntityId::Value(payload),
                EntityId::Value(guard),
                EntityId::Loan(k),
                EntityId::Loan(v),
            ] {
                uses.protect(entity);
                protected.insert(entity);
            }
            uses.permit(EntityId::Value(payload), allocation);
            let consumer = function(self.module, site.consumer)?;
            let entry = consumer
                .entry_block()
                .and_then(|b| consumer.block(b))
                .ok_or_else(|| failure("temporary exits SSA consumer actual entry", consumer.id))?;
            need(
                consumer.receiver.is_none()
                    && consumer.borrow_return.is_none()
                    && consumer.carrier_return.is_none()
                    && entry.parameters.len() == 2
                    && entry.parameters.iter().zip([k, v]).all(|(&p, l)| {
                        consumer.entity(p).map(|e| e.ty)
                            == g.function.entity(EntityId::Loan(l)).map(|e| e.ty)
                    })
                    && if self.exit == Exit::Return {
                        matches!(consumer.return_types.as_slice(), [t] if self.module.type_kind(*t) == Some(&SsaTypeKind::Integer { bits: 32, signed: true }))
                    } else {
                        consumer.return_types.is_empty()
                    },
                "temporary exits SSA actual two-Borrow consumer signature",
                consumer,
            )?;
            let call = one(
                ops.iter()
                    .copied()
                    .filter(|i| matches!(i.operation, Operation::DirectCall { .. })),
                "temporary exits SSA sole actual body consumer Call",
            )?;
            let mut roots = BTreeMap::from([(k, (0, None)), (v, (1, None))]);
            let mut live = BTreeSet::from([k, v]);
            let mut consumed = false;
            let mut at = 4;
            while let Some(i) = ops.get(at).copied() {
                match &i.operation {
                    Operation::SharedReborrow { source } => {
                        let (index, _) = *roots.get(source).ok_or_else(|| {
                            failure("temporary exits SSA derived Borrow parent", i)
                        })?;
                        let l = loan(i)?;
                        need(
                            !consumed
                                && live.contains(source)
                                && ty(g, EntityId::Loan(l))? == ty(g, EntityId::Loan(*source))?
                                && i.origin == Origin::Source(site.arguments[index])
                                && roots.insert(l, (index, Some(*source))).is_none()
                                && live.insert(l),
                            "temporary exits SSA actual live component reborrow",
                            i,
                        )?;
                        uses.permit(EntityId::Loan(*source), i);
                        uses.protect(EntityId::Loan(l));
                        protected.insert(EntityId::Loan(l));
                    }
                    Operation::DirectCall {
                        callee,
                        receiver,
                        arguments,
                    } => {
                        need(
                            !consumed
                                && i.id == call.id
                                && *callee == consumer.id
                                && receiver.is_none()
                                && arguments.len() == 2
                                && i.origin == Origin::Source(site.read_call),
                            "temporary exits SSA actual selected consumer Call",
                            i,
                        )?;
                        for (index, &argument) in arguments.iter().enumerate() {
                            let EntityId::Loan(l) = argument else {
                                return Err(failure(
                                    "temporary exits SSA Borrow argument",
                                    argument,
                                ));
                            };
                            need(
                                live.contains(&l)
                                    && roots.get(&l).is_some_and(|(root, _)| *root == index),
                                "temporary exits SSA consumer ordered actual K/V lineage",
                                (i, index, &roots),
                            )?;
                            uses.permit(argument, i);
                        }
                        need(
                            if self.exit == Exit::Return {
                                ty(g, result(i)?)? == EntityType::Value(consumer.return_types[0])
                            } else {
                                i.results.is_empty()
                            },
                            "temporary exits SSA actual consumer result type",
                            i,
                        )?;
                        consumed = true;
                    }
                    Operation::BorrowEnd { loan }
                        if roots.get(loan).is_some_and(|(_, parent)| parent.is_some()) =>
                    {
                        need(
                            consumed
                                && live.remove(loan)
                                && !roots.iter().any(|(child, (_, parent))| {
                                    *parent == Some(*loan) && live.contains(child)
                                })
                                && i.origin == Origin::Source(site.read_call)
                                && i.results.is_empty(),
                            "temporary exits SSA derived call Borrow ends before parent cleanup",
                            i,
                        )?;
                        uses.permit(EntityId::Loan(*loan), i);
                    }
                    Operation::Copy { .. } if consumed && self.exit == Exit::Return => {
                        same_value(
                            g,
                            value(i)?,
                            value(call)?,
                            site.read_call,
                            uses,
                            protected,
                            &mut BTreeSet::new(),
                        )?;
                    }
                    _ => break,
                }
                at += 1;
            }
            need(
                consumed && live == BTreeSet::from([k, v]),
                "temporary exits SSA actual consumer and all derived call loans complete",
                (&live, &roots),
            )?;
            let end_origin = site.span;
            let mut sequence = vec![
                (Operation::Drop { owner: guard }, site.guard.marker),
                (Operation::BorrowEnd { loan: v }, end_origin),
                (Operation::BorrowEnd { loan: k }, end_origin),
            ];
            if self.exit != Exit::Continue {
                sequence.extend([
                    (Operation::BorrowEnd { loan: roles.source }, end_origin),
                    (Operation::Drop { owner: roles.owner }, site.drop),
                ]);
            }
            for (operation, origin) in sequence {
                let i = *ops.get(at).ok_or_else(|| {
                    failure(
                        "temporary exits SSA missing ordered transfer cleanup",
                        (at, &ops),
                    )
                })?;
                need(
                    i.operation == operation
                        && i.origin == Origin::Source(origin)
                        && i.results.is_empty(),
                    "temporary exits SSA actual Guard/V/K/source/owner cleanup sequence",
                    i,
                )?;
                for entity in operation.entities() {
                    uses.permit(entity, i);
                }
                at += 1;
            }
            let mut providers = vec![key.id, val.id];
            let next = if self.exit == Exit::Continue {
                let i = *ops
                    .get(at)
                    .ok_or_else(|| failure("temporary exits SSA missing Continue Next", &ops))?;
                need(
                    i.operation
                        == Operation::MapIterationNext {
                            source: roles.source,
                            cursor: roles.cursor,
                        }
                        && i.origin == Origin::Source(site.transfer)
                        && ty(g, result(i)?)? == ty(g, EntityId::Value(roles.cursor))?,
                    "temporary exits SSA Continue preserves current source and advances own cursor",
                    i,
                )?;
                uses.permit(EntityId::Loan(roles.source), i);
                uses.permit(EntityId::Value(roles.cursor), i);
                uses.protect(result(i)?);
                protected.insert(result(i)?);
                providers.push(i.id);
                at += 1;
                Some(value(i)?)
            } else {
                None
            };
            let returned = if self.exit == Exit::Return {
                let root = value(call)?;
                uses.protect(EntityId::Value(root));
                protected.insert(EntityId::Value(root));
                let TerminatorKind::Return { values } = &g.term(self.block)?.kind else {
                    return Err(failure(
                        "temporary exits SSA actual Return transfer",
                        g.term(self.block)?,
                    ));
                };
                let actual = one(
                    values.iter().copied(),
                    "temporary exits SSA sole returned Int",
                )?;
                for i in &ops[at..] {
                    need(
                        matches!(i.operation, Operation::Copy { .. }),
                        "temporary exits SSA only proven result copies after cleanup",
                        i,
                    )?;
                    same_value(
                        g,
                        value(i)?,
                        root,
                        site.read_call,
                        uses,
                        protected,
                        &mut BTreeSet::new(),
                    )?;
                }
                same_value(
                    g,
                    actual,
                    root,
                    site.read_call,
                    uses,
                    protected,
                    &mut BTreeSet::new(),
                )?;
                need(
                    g.term(self.block)?.origin == Origin::Source(site.transfer),
                    "temporary exits SSA actual Return source",
                    g.term(self.block)?,
                )?;
                at = ops.len();
                Some(actual)
            } else {
                None
            };
            need(
                at == ops.len(),
                "temporary exits SSA whole body instruction closure",
                (&ops, at),
            )?;
            need(
                g.edges.iter().all(|(_, e)| {
                    e.arguments.iter().all(|a| {
                        !matches!(a, EntityId::Loan(l) if roots.contains_key(l))
                            && *a != EntityId::Value(guard)
                            && *a != EntityId::Value(payload)
                    })
                }),
                "temporary exits SSA no Guard/KV/derived/result edge escape",
                &g.edges,
            )?;
            Ok(End {
                next,
                returned,
                call: call.id,
                guard,
                providers,
            })
        }
    }

    /// A scalar identity accepts only its real birth, typed Copy, or every actual incoming.
    pub(in crate::ssa) fn same_value(
        g: &Graph<'_>,
        current: ValueId,
        birth: ValueId,
        copy_origin: Span,
        uses: &mut Uses,
        protected: &mut BTreeSet<EntityId>,
        visiting: &mut BTreeSet<ValueId>,
    ) -> Result<(), Failure> {
        need(
            ty(g, EntityId::Value(current))? == ty(g, EntityId::Value(birth))?,
            "temporary exits SSA same scalar type",
            (current, birth),
        )?;
        uses.protect(EntityId::Value(current));
        protected.insert(EntityId::Value(current));
        if current == birth {
            return Ok(());
        }
        need(
            visiting.insert(current),
            "temporary exits SSA scalar lineage cannot self-justify a cycle",
            current,
        )?;
        let data = g
            .function
            .entity(EntityId::Value(current))
            .ok_or_else(|| failure("temporary exits SSA scalar definition", current))?;
        match data.definition {
            Definition::InstructionResult { .. } => {
                let i = g.definition(EntityId::Value(current))?;
                let Operation::Copy { source } = i.operation else {
                    return Err(failure(
                        "temporary exits SSA actual scalar Copy or birth required",
                        i,
                    ));
                };
                let original = g.definition(EntityId::Value(birth))?;
                let source_definition = g.definition(EntityId::Value(source))?;
                need(
                    i.origin == Origin::Source(copy_origin)
                        && i.block == original.block
                        && source_definition.block == i.block
                        && g.position(original)? < g.position(i)?
                        && g.position(source_definition)? < g.position(i)?,
                    "temporary exits SSA Copy actual source expression, birth block and evaluation order",
                    i,
                )?;
                same_value(g, source, birth, copy_origin, uses, protected, visiting)?;
                uses.permit(EntityId::Value(source), i);
            }
            Definition::BlockParameter { block, .. } => {
                let incoming = g.incoming(block);
                need(
                    !incoming.is_empty(),
                    "temporary exits SSA scalar parameter needs actual incoming",
                    block,
                )?;
                for (_, edge) in incoming {
                    let EntityId::Value(v) = g.argument(edge, EntityId::Value(current))? else {
                        return Err(failure("temporary exits SSA scalar incoming", edge));
                    };
                    same_value(g, v, birth, copy_origin, uses, protected, visiting)?;
                }
            }
        }
        visiting.remove(&current);
        Ok(())
    }
}
