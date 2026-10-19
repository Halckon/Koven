//! Same-analysis intrinsic descriptors, represented types and named/temporary owner obligations.
mod callable;
mod owner;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Kind, Shape, ast};
use lang_frontend::{
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolId, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::*,
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::*,
};
pub(super) type Input<'a> = (&'a Case, &'a SourceMap, &'a ParsedFile, &'a ast::Input);
#[derive(Debug, Default)]
pub(super) struct Proof {
    pub callable_drop: Option<UnitDropFact>,
}

fn root(names: &NameResolution, marker: Span) -> Result<SymbolId, Failure> {
    one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == marker && !s.is_synthetic()),
        "birth original declaration symbol",
    )
    .map(|s| s.id())
}
fn unit_root(names: &ValidatedCompilationUnitNames, marker: Span) -> Result<UnitSymbolId, Failure> {
    one(
        names.names().source_units().iter().flat_map(|u| {
            u.resolution()
                .symbols()
                .iter()
                .filter(move |s| s.span() == marker && !s.is_synthetic())
                .map(move |s| (u.source_unit(), s.id()))
        }),
        "birth original qualified declaration",
    )
    .and_then(|(unit, symbol)| {
        one(
            names
                .names()
                .references()
                .iter()
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(id)
                        if id.source_unit() == unit && id.symbol() == symbol =>
                    {
                        Some(*id)
                    }
                    _ => None,
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter(),
            "birth actual qualified declaration identity",
        )
    })
}
pub(super) fn single(
    input: Input<'_>,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Proof, Failure> {
    let (c, _, parsed, i) = input;
    need(
        c.kind != Kind::FunctionValue,
        "birth Single FunctionValue remains separate 0336 prerequisite",
        c.label,
    )?;
    let ty = typed
        .expression_type(i.birth)
        .ok_or_else(|| failure("birth original result type", i.birth))?;
    need(
        typed.expression_category(i.birth) == Some(ExpressionCategory::Temporary)
            && typed.is_resource_type(ty) == Some(false)
            && matches!(typed.types().get(ty),Some(TypeKind::Intrinsic{constructor,arguments})if *constructor==if c.mutable{IntrinsicTypeConstructor::MutableMap}else{IntrinsicTypeConstructor::Map}&&arguments.len()==2&&arguments.iter().all(|t|typed.types().get(*t)==Some(&TypeKind::Builtin(BuiltinType::String)))),
        "birth represented exact String/String Map result",
        ty,
    )?;
    let construct = i
        .construction
        .ok_or_else(|| failure("birth actual construction AST", i))?;
    let d = typed
        .map_construction(construct)
        .ok_or_else(|| failure("birth actual typed construction descriptor", construct))?;
    let wanted = if c.kind == Kind::Consume || c.mutable {
        IntrinsicCallable::MutableMapOf
    } else {
        IntrinsicCallable::MapOf
    };
    need(
        d.expression() == construct
            && d.callable() == wanted
            && typed.expression_type(construct) == Some(d.map_type())
            && typed.expression_category(construct) == Some(ExpressionCategory::Temporary)
            && typed.types().get(d.key_type()) == Some(&TypeKind::Builtin(BuiltinType::String))
            && typed.types().get(d.value_type()) == Some(&TypeKind::Builtin(BuiltinType::String)),
        "birth actual typed MapNew operation/kind/KV",
        d,
    )?;
    if c.kind == Kind::Consume {
        let consumed = typed
            .map_consume(i.birth)
            .ok_or_else(|| failure("birth original typed consume", i.birth))?;
        let initial = root(names, i.initial.unwrap())?;
        let Expression::Call { callee, .. } =
            parsed.ast().expressions().get(i.birth).unwrap().payload()
        else {
            unreachable!()
        };
        let Expression::Member { receiver, .. } =
            parsed.ast().expressions().get(*callee).unwrap().payload()
        else {
            return Err(failure("birth original consume receiver", callee));
        };
        let reference = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, *receiver)),
            "birth consume original named receiver",
        )?;
        need(
            consumed.expression() == i.birth
                && consumed.receiver() == *receiver
                && consumed.source() == Some((initial, d.map_type()))
                && consumed.receiver_type() == d.map_type()
                && consumed.result_type() == ty
                && [consumed.key_type(), consumed.value_type()] == [d.key_type(), d.value_type()]
                && reference.target() == &ReferenceTarget::Symbol(initial)
                && owned.formed_owned_locals().contains(&initial)
                && !owned
                    .drops()
                    .iter()
                    .any(|x| x.target() == DropTarget::Named(initial)),
            "birth consume owns actual current initial root without duplicate Drop",
            consumed,
        )?;
    } else {
        need(
            d.map_type() == ty,
            "birth actual constructed output type",
            d,
        )?;
    }
    if let Some(put) = i.put {
        let p = typed
            .map_put(put)
            .ok_or_else(|| failure("birth original typed put", put))?;
        need(
            p.expression() == put
                && p.key_type() == d.key_type()
                && p.value_type() == d.value_type()
                && typed.expression_type(p.receiver()) == Some(d.map_type()),
            "birth original put retains actual mutable map representation",
            p,
        )?;
        let initial = root(names, i.initial.unwrap())?;
        let reference = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == ast::span(parsed, p.receiver())),
            "birth put actual root reference",
        )?;
        need(
            reference.target() == &ReferenceTarget::Symbol(initial),
            "birth put updates exact construction root",
            reference,
        )?;
    }
    provider_single(input, names, typed, owned, ty)?;
    Ok(Proof::default())
}
fn provider_single(
    input: Input<'_>,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    ty: TypeId,
) -> Result<(), Failure> {
    let (c, _, parsed, i) = input;
    let target = if let Some(marker) = i.root {
        let root = root(names, marker)?;
        need(
            owned.formed_owned_locals().contains(&root)
                && typed.symbol_type(root) == Some(ty)
                && !owned
                    .drops()
                    .iter()
                    .any(|x| x.target() == DropTarget::Temporary(i.birth)),
            "birth actual named result formation/no hidden duplicate",
            root,
        )?;
        DropTarget::Named(root)
    } else {
        DropTarget::Temporary(i.birth)
    };
    let drop = *one(
        owned.drops().iter().filter(|d| d.target() == target),
        "birth output unique actual Drop",
    )?;
    need(
        drop.value_origin() == i.root.unwrap_or(ast::span(parsed, i.birth))
            && drop.point()
                == if let Some(site) = &i.site {
                    DropPoint::LoopExit(site.statement)
                } else {
                    DropPoint::CallReturn(i.size.unwrap())
                },
        "birth output exact source-origin and last-use cleanup",
        drop,
    )?;
    need(
        typed.map_iterations().len() == usize::from(i.site.is_some())
            && owned.map_iterations().len() == usize::from(i.site.is_some()),
        "birth original provider/control plan count",
        owned.map_iterations(),
    )?;
    if let Some(site) = &i.site {
        let p = owned
            .map_iteration(site.statement)
            .ok_or_else(|| failure("birth actual provider ownership", site))?;
        let d = p.descriptor();
        need(
            typed.map_iteration(site.statement) == Some(d)
                && d.source() == site.source
                && d.source_type() == ty,
            "birth exact original For descriptor",
            d,
        )?;
        if let DropTarget::Named(root) = target {
            let reference = one(
                names
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, site.source)),
                "birth original For named root",
            )?;
            need(
                reference.target() == &ReferenceTarget::Symbol(root)
                    && p.source_access() == MapIterationSourceAccess::Owned
                    && matches!(p.source(),Some(LoanTarget::Place(path))if path.is_root()&&path.root()==root),
                "birth named For uses the original owner root",
                p,
            )?;
            owner::single(i, root, p, owned, &drop)?;
        } else {
            need(
                c.shape == Shape::Direct
                    && p.source_access() == MapIterationSourceAccess::Temporary
                    && p.source() == Some(&LoanTarget::Temporary(i.birth))
                    && p.closure_phis().is_empty()
                    && p.closure_phi_incomings().len() == 3,
                "birth direct original temporary owner",
                p,
            )?;
        }
        if c.shape == Shape::Direct {
            super::super::query_temporary_cleanup::single(
                site.statement,
                site.source,
                site.body,
                ast::span(parsed, i.birth),
                owned,
            )?;
        }
        let read = typed
            .call(site.read)
            .ok_or_else(|| failure("birth original reader Call", site.read))?;
        for n in 0..2 {
            let binding = root(names, site.bindings[n])?;
            let l = &p.loans()[n];
            let reference = one(
                names
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, site.arguments[n])),
                "birth actual component argument reference",
            )?;
            need(
                d.bindings()[n] == binding
                    && reference.target() == &ReferenceTarget::Symbol(binding)
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && l.statement() == site.statement
                    && l.source() == site.source
                    && l.binding() == binding
                    && Some(l.origin()) == p.source()
                    && l.kind() == LoanKind::Shared
                    && read.arguments()[n].argument_index() == n
                    && read.arguments()[n].parameter_index() == n
                    && read.arguments()[n].mode() == ParameterMode::Borrow,
                "birth complete original K/V Shared and reader Borrow lineage",
                l,
            )?;
        }
        for (kind, point, mut actions) in [
            (
                IterationExitKind::Fallthrough,
                DropPoint::AfterStatement(site.body),
                vec![
                    IterationCleanupAction::EndBinding {
                        statement: site.statement,
                        symbol: d.value_binding(),
                    },
                    IterationCleanupAction::EndBinding {
                        statement: site.statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                IterationExitKind::Exhaustion,
                DropPoint::LoopExit(site.statement),
                vec![
                    IterationCleanupAction::FinishProvider(site.statement),
                    IterationCleanupAction::EndSource(site.statement),
                ],
            ),
        ] {
            actions.extend(
                owned
                    .drops()
                    .iter()
                    .filter(|d| d.point() == point)
                    .copied()
                    .map(IterationCleanupAction::Drop),
            );
            let exit = one(
                p.exits().iter().filter(|e| e.kind() == kind),
                "birth actual complete normal exit",
            )?;
            need(
                p.exits().len() == 2
                    && exit.point() == point
                    && exit.actions() == actions
                    && owned
                        .cleanup_steps()
                        .iter()
                        .filter_map(|(at, a)| (*at == point).then_some(*a))
                        .collect::<Vec<_>>()
                        == actions,
                "birth complete ordered original cleanup",
                exit,
            )?;
        }
    } else {
        need(
            drop.condition().is_none()
                && matches!(drop.owner().and_then(|owner|owned.cleanup_conditions().owner_value(owner)),Some(CleanupOwnerValue::Expression{expression,origin})if *expression==i.birth&&*origin==ast::span(parsed,i.birth)),
            "birth no-For Drop consumes exact actual result birth",
            drop,
        )?;
        let size = typed
            .map_size(i.size.unwrap())
            .ok_or_else(|| failure("birth no-For actual size descriptor", i.size))?;
        need(
            size.map_type() == ty && typed.expression_type(size.receiver()) == Some(ty),
            "birth actual no-For map size",
            size,
        )?;
    }
    Ok(())
}
pub(super) fn unit(
    input: Input<'_>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let (c, _, parsed, i) = input;
    let unit = one(names.names().source_units().iter(), "birth original unit")?.source_unit();
    let e = |id| UnitExpressionId::new(unit, id);
    let ty = typed
        .expression_type(e(i.birth))
        .ok_or_else(|| failure("birth qualified result type", i.birth))?;
    need(
        typed.expression_category(e(i.birth)) == Some(ExpressionCategory::Temporary)
            && typed.is_resource_type(ty) == Some(false)
            && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor,arguments})if *constructor==if c.mutable{IntrinsicTypeConstructor::MutableMap}else{IntrinsicTypeConstructor::Map}&&arguments.len()==2&&arguments.iter().all(|t|typed.types().get(*t)==Some(&UnitTypeKind::Builtin(BuiltinType::String)))),
        "birth qualified exact represented result",
        ty,
    )?;
    let mut proof = Proof::default();
    if c.kind == Kind::FunctionValue {
        proof.callable_drop = Some(callable::unit(input, names, typed, owned)?);
    } else {
        let construct = i.construction.unwrap();
        let d = typed
            .map_construction(e(construct))
            .ok_or_else(|| failure("birth qualified typed MapNew", construct))?;
        need(
            d.expression() == e(construct)
                && d.callable()
                    == if c.kind == Kind::Consume || c.mutable {
                        IntrinsicCallable::MutableMapOf
                    } else {
                        IntrinsicCallable::MapOf
                    }
                && typed.expression_type(e(construct)) == Some(d.map_type())
                && typed.types().get(d.key_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                && typed.types().get(d.value_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String)),
            "birth qualified exact construction/KV identity",
            d,
        )?;
        if c.kind == Kind::Consume {
            let consume = typed
                .map_consume(e(i.birth))
                .ok_or_else(|| failure("birth actual qualified consume", i.birth))?;
            let initial = unit_root(names, i.initial.unwrap())?;
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, consume.receiver().expression())),
                "birth actual qualified consume root",
            )?;
            need(
                consume.expression() == e(i.birth)
                    && consume.source() == Some((initial, d.map_type()))
                    && consume.receiver_type() == d.map_type()
                    && consume.result_type() == ty
                    && [consume.key_type(), consume.value_type()] == [d.key_type(), d.value_type()]
                    && reference.target() == &UnitReferenceTarget::Symbol(initial)
                    && owned.formed_owned_locals().contains(&initial)
                    && !owned
                        .drops()
                        .iter()
                        .any(|d| d.target() == UnitDropTarget::Named(initial)),
                "birth qualified consume current root and no duplicate old Drop",
                consume,
            )?;
        } else {
            need(
                d.map_type() == ty,
                "birth qualified construction output type",
                d,
            )?;
        }
        if let Some(put) = i.put {
            let p = typed
                .map_put(e(put))
                .ok_or_else(|| failure("birth qualified actual put", put))?;
            let initial = unit_root(names, i.initial.unwrap())?;
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, p.receiver().expression())),
                "birth qualified actual put root",
            )?;
            need(
                p.expression() == e(put)
                    && p.key_type() == d.key_type()
                    && p.value_type() == d.value_type()
                    && typed.expression_type(p.receiver()) == Some(d.map_type())
                    && reference.target() == &UnitReferenceTarget::Symbol(initial),
                "birth qualified put current initial root",
                p,
            )?;
        }
    }
    let target = if let Some(marker) = i.root {
        let root = unit_root(names, marker)?;
        need(
            owned.formed_owned_locals().contains(&root)
                && typed.symbol_type(root) == Some(ty)
                && !owned
                    .drops()
                    .iter()
                    .any(|x| x.target() == UnitDropTarget::Temporary(e(i.birth))),
            "birth qualified actual named result formation",
            root,
        )?;
        UnitDropTarget::Named(root)
    } else {
        UnitDropTarget::Temporary(e(i.birth))
    };
    let drop = *one(
        owned.drops().iter().filter(|d| d.target() == target),
        "birth qualified unique output Drop",
    )?;
    need(
        drop.value_origin() == i.root.unwrap_or(ast::span(parsed, i.birth))
            && drop.point()
                == if let Some(site) = &i.site {
                    UnitDropPoint::LoopExit(UnitStatementId::new(unit, site.statement))
                } else {
                    UnitDropPoint::CallReturn(e(i.size.unwrap()))
                },
        "birth qualified exact output cleanup boundary",
        drop,
    )?;
    need(
        typed.map_iterations().len() == usize::from(i.site.is_some())
            && owned.map_iterations().len() == usize::from(i.site.is_some()),
        "birth qualified exact provider/control set",
        owned.map_iterations(),
    )?;
    if let Some(site) = &i.site {
        let statement = UnitStatementId::new(unit, site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("birth qualified actual provider", site))?;
        let d = p.descriptor();
        need(
            typed.map_iteration(statement) == Some(d)
                && d.source() == e(site.source)
                && d.source_type() == ty,
            "birth qualified same original For source/type",
            d,
        )?;
        if let UnitDropTarget::Named(root) = target {
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, site.source)),
                "birth qualified actual For Name",
            )?;
            need(
                reference.target() == &UnitReferenceTarget::Symbol(root)
                    && p.source_access() == UnitIterationSourceAccess::Owned
                    && matches!(p.source(),Some(UnitLoanTarget::Place(path))if path.is_root()&&path.root()==root),
                "birth qualified For uses actual original formed root",
                p,
            )?;
        } else {
            need(
                p.source_access() == UnitIterationSourceAccess::Temporary
                    && p.source() == Some(&UnitLoanTarget::Temporary(e(i.birth))),
                "birth qualified direct original temporary",
                p,
            )?;
        }
        if c.shape == Shape::Direct {
            super::super::query_temporary_cleanup::unit(
                UnitStatementId::new(unit, site.body),
                ast::span(parsed, i.birth),
                statement,
                e(i.birth),
                owned,
            )?;
        }
        let read = typed
            .call(e(site.read))
            .ok_or_else(|| failure("birth qualified actual reader", site.read))?;
        for n in 0..2 {
            let binding = unit_root(names, site.bindings[n])?;
            let l = &p.loans()[n];
            let reference = one(
                names
                    .names()
                    .references()
                    .iter()
                    .filter(|r| r.span() == ast::span(parsed, site.arguments[n])),
                "birth qualified component reference",
            )?;
            need(
                d.bindings()[n] == binding
                    && reference.target() == &UnitReferenceTarget::Symbol(binding)
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && l.statement() == statement
                    && l.source() == e(site.source)
                    && l.binding() == binding
                    && Some(l.origin()) == p.source()
                    && l.kind() == LoanKind::Shared
                    && read.arguments()[n].argument_index() == n
                    && read.arguments()[n].parameter_index() == n
                    && read.arguments()[n].mode() == ParameterMode::Borrow,
                "birth qualified ordered K/V Borrow lineage",
                l,
            )?;
        }
        for (kind, point, mut actions) in [
            (
                UnitIterationExitKind::Fallthrough,
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, site.body)),
                vec![
                    UnitIterationCleanupAction::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UnitIterationCleanupAction::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ],
            ),
            (
                UnitIterationExitKind::Exhaustion,
                UnitDropPoint::LoopExit(statement),
                vec![
                    UnitIterationCleanupAction::FinishProvider(statement),
                    UnitIterationCleanupAction::EndSource(statement),
                ],
            ),
        ] {
            actions.extend(
                owned
                    .drops()
                    .iter()
                    .filter(|d| d.point() == point)
                    .copied()
                    .map(UnitIterationCleanupAction::Drop),
            );
            let exit = one(
                p.exits().iter().filter(|e| e.kind() == kind),
                "birth qualified complete exit",
            )?;
            need(
                p.exits().len() == 2
                    && exit.point() == point
                    && exit.actions() == actions
                    && owned.iteration_cleanup_at(point) == Some(actions.as_slice()),
                "birth qualified full ordered exit cleanup",
                exit,
            )?;
        }
    } else {
        let size = typed
            .map_size(e(i.size.unwrap()))
            .ok_or_else(|| failure("birth qualified actual control size", i.size))?;
        need(
            size.map_type() == ty && typed.expression_type(size.receiver()) == Some(ty),
            "birth qualified no-For actual result size",
            size,
        )?;
    }
    Ok(proof)
}
