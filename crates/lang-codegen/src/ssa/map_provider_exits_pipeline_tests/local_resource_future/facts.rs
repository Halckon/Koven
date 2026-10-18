//! Original Resource formation and live cross-For owners; never infer Pure ASAP cleanup.
use super::super::local_future::facts::ast;
mod borrows;
use super::super::local_future::facts::owner;
use super::super::local_resources::facts::nominal;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Report};
use ast::Scanner;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupOwnerValue as O, CompilationUnitOwnership, DropPoint, DropTarget,
        IterationCleanupAction as A, IterationExitKind as E, IterationSource, LoanEndPoint,
        LoanKind, LoanTarget, MapIterationComponent, MapIterationSourceAccess,
        OwnershipBindingKind, OwnershipCheckedFile, UnitCallArgumentOwnershipKind, UnitDropPoint,
        UnitDropTarget, UnitIterationCleanupAction as UA, UnitIterationExitKind as UE,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, CompilationUnitTypes,
        ExpressionCategory, IntrinsicTypeConstructor, MapIterationKind, ParameterMode, TypeKind,
        TypedFile, UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};
fn reference(
    parsed: &ParsedFile,
    names: &NameResolution,
    id: ExpressionId,
) -> Result<SymbolId, Failure> {
    let span = ast::span(parsed, id)?;
    one(
        names
            .references()
            .iter()
            .filter(|r| r.span() == span)
            .filter_map(|r| match r.target() {
                ReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "Resource future original Name root",
    )
}
fn qualified(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    id: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    let span = ast::span(parsed, id)?;
    one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span)
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "Resource future original qualified Name root",
    )
}
fn marker(
    names: &ValidatedCompilationUnitNames,
    id: UnitSymbolId,
    span: Span,
    kind: SymbolKind,
) -> bool {
    names.names().source_units().iter().any(|u| {
        u.source_unit() == id.source_unit()
            && u.resolution().symbols().iter().any(|s| {
                s.id() == id.symbol() && s.span() == span && s.kind() == kind && !s.is_synthetic()
            })
    })
}
fn lexical(site: &Scanner, parsed: &ParsedFile) -> Result<ExpressionId, Failure> {
    one(
        parsed.ast().expressions().iter().filter_map(|(id, n)| {
            (ast::within(site.span, n.span()) && matches!(n.payload(), Expression::Return { .. }))
                .then_some(id)
        }),
        "Resource future real scanner lexical Return",
    )
}
pub(super) fn single(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    match ast::scanners((case.binding, case.exit, case.form), sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "Resource future original single formation/owners/loans",
                    &single_site((case, sources, parsed), site, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("Resource future actual AST", &Err(e)),
    }
}
fn single_site(
    input: (&Case, &SourceMap, &ParsedFile),
    site: &Scanner,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let (case, sources, parsed) = input;
    let root = reference(parsed, names, site.loops[0].source)?;
    let original = if site.root.initializer == site.birth.initializer {
        root
    } else {
        reference(parsed, names, site.root.initializer)?
    };
    for (id, local) in [(root, &site.root), (original, &site.birth)] {
        need(
            names.symbols().iter().any(|s| {
                s.id() == id
                    && s.span() == local.marker
                    && s.kind() == SymbolKind::Variable
                    && !s.is_synthetic()
            }) && owned.formed_owned_locals().contains(&id),
            "Resource future actual formation history",
            (id, local),
        )?;
    }
    need(
        (root == original) == (case.binding != 2)
            && (root == original
                || !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == DropTarget::Named(original)))
            && !owned
                .drops()
                .iter()
                .any(|d| d.target() == DropTarget::Temporary(site.birth.initializer)),
        "Resource future Move carries original obligation once",
        (root, original),
    )?;
    let ty = typed
        .symbol_type(root)
        .ok_or_else(|| failure("Resource future source type", root))?;
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.symbol_type(original) == Some(ty)
            && typed.is_resource_type(ty) == Some(true)
            && typed.expression_type(site.birth.initializer) == Some(ty)
            && typed.expression_category(site.birth.initializer)
                == Some(ExpressionCategory::Temporary)
            && typed.expression_type(site.root.initializer) == Some(ty)
            && typed.expression_category(site.root.initializer)
                == Some(if root == original {
                    ExpressionCategory::Temporary
                } else {
                    ExpressionCategory::Place
                })
            && matches!(typed.types().get(ty),Some(TypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && arguments.len()==2 && typed.types().get(arguments[0])==Some(&TypeKind::Builtin(BuiltinType::String))),
        "Resource future exact Resource Call birth/type/category",
        ty,
    )?;
    let factory = one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == site.factory && s.kind() == SymbolKind::Function),
        "Resource future selected factory",
    )?;
    let call = typed
        .call(site.birth.initializer)
        .ok_or_else(|| failure("Resource future actual factory Call", site))?;
    need(
        call.target() == CallableTarget::Source(factory.id())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == ty
            && call.result_source() == CallableResultSource::Owned,
        "Resource future exact original Call birth",
        call,
    )?;
    let mut previous = None;
    for loop_site in &site.loops {
        let p = owned
            .map_iteration(loop_site.statement)
            .ok_or_else(|| failure("Resource future original Map plan", loop_site))?;
        let d = p.descriptor();
        need(
            typed.map_iteration(loop_site.statement) == Some(d)
                && d.source() == loop_site.source
                && d.source_type() == ty
                && d.kind()
                    == if case.mutable {
                        MapIterationKind::MutableMap
                    } else {
                        MapIterationKind::Map
                    }
                && reference(parsed, names, loop_site.source)? == root
                && typed.expression_category(loop_site.source) == Some(ExpressionCategory::Place)
                && matches!(typed.types().get(ty),Some(TypeKind::Intrinsic{arguments,..}) if arguments.as_slice()==[d.key_type(),d.value_type()])
                && p.source_access() == MapIterationSourceAccess::Owned
                && matches!(p.source_kind(),IterationSource::BorrowedSource(LoanTarget::Place(path)) if path.is_root() && path.root()==root),
            "Resource future actual original For source/Owned lineage",
            p,
        )?;
        nominal::single(case.nullable, sources, parsed, names, typed, d.value_type())?;
        for (index, binding) in d.bindings().into_iter().enumerate() {
            need(
                names.symbols().iter().any(|s| {
                    s.id() == binding
                        && s.span() == loop_site.bindings[index]
                        && s.kind() == SymbolKind::ForBinding
                }) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[index].symbol() == binding
                    && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && p.loans()[index].statement() == loop_site.statement
                    && p.loans()[index].source() == loop_site.source
                    && p.loans()[index].binding() == binding
                    && p.loans()[index].component()
                        == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                    && p.loans()[index].kind() == LoanKind::Shared
                    && Some(p.loans()[index].origin()) == p.source(),
                "Resource future exact nullable/direct Shared K/V",
                p.loans(),
            )?;
        }
        let guard = *one(
            owned
                .drops()
                .iter()
                .filter(|d| d.value_origin() == loop_site.guard.marker),
            "Resource future actual Guard Drop",
        )?;
        need(guard.point()==loop_site.point && guard.condition().is_none() && matches!(guard.target(),DropTarget::Named(id) if owned.formed_owned_locals().contains(&id) && names.symbols().iter().any(|s|s.id()==id && s.span()==loop_site.guard.marker))
            && guard.owner().is_some_and(|o|matches!(owned.cleanup_conditions().owner_value(o),Some(O::Expression{expression,origin}) if *expression==loop_site.guard.initializer && *origin==loop_site.guard_span)),"Resource future original Guard construction/drop",guard)?;
        need(
            p.exits().len() == 2,
            "Resource future complete For exits",
            p.exits(),
        )?;
        for (kind, point, body) in [
            (loop_site.kind, loop_site.point, true),
            (
                E::Exhaustion,
                DropPoint::LoopExit(loop_site.statement),
                false,
            ),
        ] {
            let exit = one(
                p.exits()
                    .iter()
                    .filter(|e| e.kind() == kind && e.point() == point),
                "Resource future actual exact exit",
            )?;
            let mut expected = if body {
                vec![
                    A::Drop(guard),
                    A::EndBinding {
                        statement: loop_site.statement,
                        symbol: d.value_binding(),
                    },
                    A::EndBinding {
                        statement: loop_site.statement,
                        symbol: d.key_binding(),
                    },
                ]
            } else {
                vec![]
            };
            if !body || matches!(kind, E::Break(_)) {
                expected.extend([
                    A::FinishProvider(loop_site.statement),
                    A::EndSource(loop_site.statement),
                ]);
            }
            let full = owned
                .cleanup_steps()
                .iter()
                .filter_map(|(at, a)| (*at == point).then_some(*a))
                .collect::<Vec<_>>();
            need(
                exit.actions() == expected && full == expected,
                "Resource future full For cleanup keeps live lexical source",
                (exit, full, expected),
            )?;
        }
        previous = Some(owner::check(site, loop_site, root, previous, p, owned)?);
    }
    let (current, condition) =
        previous.ok_or_else(|| failure("Resource future last Exit", site))?;
    let returned = lexical(site, parsed)?;
    let drop = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(root)),
        "Resource future unique lexical source Drop",
    )?;
    need(
        drop.point() == DropPoint::ControlTransfer(returned)
            && drop.owner() == Some(current)
            && drop.condition() == Some(condition)
            && drop.value_origin() == site.root.marker
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none(),
        "Resource future exact final Exit owner/availability at lexical Return",
        drop,
    )?;
    let steps = owned
        .cleanup_steps()
        .iter()
        .filter_map(|(at, a)| (*at == drop.point()).then_some(*a))
        .collect::<Vec<_>>();
    need(
        steps == [A::Drop(drop)],
        "Resource future full lexical cleanup",
        steps,
    )?;
    borrows::single(
        (case, sources, parsed, site),
        names,
        typed,
        owned,
        (root, ty),
    )
}
pub(super) fn unit(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    report: &mut Report,
) {
    match ast::scanners((case.binding, case.exit, case.form), sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "Resource future original qualified formation/cleanup/loans",
                    &unit_site((case, sources, parsed), site, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("Resource future actual AST", &Err(e)),
    }
}
fn unit_site(
    input: (&Case, &SourceMap, &ParsedFile),
    site: &Scanner,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let (case, sources, parsed) = input;
    let root = qualified(parsed, names, site.loops[0].source)?;
    let unit = root.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    let original = if site.root.initializer == site.birth.initializer {
        root
    } else {
        qualified(parsed, names, site.root.initializer)?
    };
    for (id, local) in [(root, &site.root), (original, &site.birth)] {
        need(
            marker(names, id, local.marker, SymbolKind::Variable)
                && id.source_unit() == unit
                && owned.formed_owned_locals().contains(&id),
            "Resource future qualified actual formation history",
            (id, local),
        )?;
    }
    need(
        (root == original) == (case.binding != 2)
            && (root == original
                || !owned
                    .drops()
                    .iter()
                    .any(|d| d.target() == UnitDropTarget::Named(original)))
            && !owned.drops().iter().any(|d| {
                d.target() == UnitDropTarget::Temporary(expression(site.birth.initializer))
            }),
        "Resource future qualified Move obligation remains unique",
        (root, original),
    )?;
    let ty = typed
        .symbol_type(root)
        .ok_or_else(|| failure("Resource future qualified source type", root))?;
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.symbol_type(original) == Some(ty)
            && typed.is_resource_type(ty) == Some(true)
            && typed.expression_type(expression(site.birth.initializer)) == Some(ty)
            && typed.expression_category(expression(site.birth.initializer))
                == Some(ExpressionCategory::Temporary)
            && typed.expression_type(expression(site.root.initializer)) == Some(ty)
            && typed.expression_category(expression(site.root.initializer))
                == Some(if root == original {
                    ExpressionCategory::Temporary
                } else {
                    ExpressionCategory::Place
                })
            && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && arguments.len()==2 && typed.types().get(arguments[0])==Some(&UnitTypeKind::Builtin(BuiltinType::String))),
        "Resource future qualified exact Resource birth/type/category",
        ty,
    )?;
    let factory = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.factory && c.parameters().is_empty())
        }),
        "Resource future qualified original factory",
    )?;
    let call = typed
        .call(expression(site.birth.initializer))
        .ok_or_else(|| failure("Resource future qualified actual Call", site))?;
    need(
        call.target() == UnitCallTarget::Declaration(factory.declaration())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == ty
            && call.result_source() == CallableResultSource::Owned,
        "Resource future qualified selected factory Call",
        call,
    )?;
    for loop_site in &site.loops {
        let statement = UnitStatementId::new(unit, loop_site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("Resource future original qualified For plan", loop_site))?;
        let d = p.descriptor();
        need(
            typed.map_iteration(statement) == Some(d)
                && d.source() == expression(loop_site.source)
                && d.source_type() == ty
                && d.kind()
                    == if case.mutable {
                        MapIterationKind::MutableMap
                    } else {
                        MapIterationKind::Map
                    }
                && qualified(parsed, names, loop_site.source)? == root
                && typed.expression_category(expression(loop_site.source))
                    == Some(ExpressionCategory::Place)
                && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{arguments,..}) if arguments.as_slice()==[d.key_type(),d.value_type()])
                && p.source_access() == UnitIterationSourceAccess::Owned
                && matches!(p.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root),
            "Resource future original qualified For/source/Owned identity",
            p,
        )?;
        nominal::unit(case.nullable, sources, parsed, names, typed, d.value_type())?;
        for (index, binding) in d.bindings().into_iter().enumerate() {
            need(
                marker(
                    names,
                    binding,
                    loop_site.bindings[index],
                    SymbolKind::ForBinding,
                ) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[index].symbol() == binding
                    && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                    && p.loans()[index].statement() == statement
                    && p.loans()[index].source() == d.source()
                    && p.loans()[index].binding() == binding
                    && p.loans()[index].component()
                        == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                    && p.loans()[index].kind() == LoanKind::Shared
                    && Some(p.loans()[index].origin()) == p.source(),
                "Resource future qualified nullable/direct original K/V",
                p.loans(),
            )?;
        }
        let point = match loop_site.point {
            DropPoint::AfterStatement(s) => {
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
            }
            DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
            other => return Err(failure("Resource future qualified actual point", other)),
        };
        let kind = match loop_site.kind {
            E::Fallthrough => UE::Fallthrough,
            E::Continue(e) => UE::Continue(expression(e)),
            E::Break(e) => UE::Break(expression(e)),
            other => return Err(failure("Resource future qualified actual kind", other)),
        };
        let guard = *one(
            owned
                .drops()
                .iter()
                .filter(|d| d.value_origin() == loop_site.guard.marker),
            "Resource future qualified actual Guard Drop",
        )?;
        need(
            guard.point() == point
                && matches!(guard.target(),UnitDropTarget::Named(id) if marker(names,id,loop_site.guard.marker,SymbolKind::Variable) && owned.formed_owned_locals().contains(&id)),
            "Resource future qualified original Guard point/target",
            guard,
        )?;
        need(
            p.exits().len() == 2,
            "Resource future qualified full exit set",
            p.exits(),
        )?;
        for (kind, point, body) in [
            (kind, point, true),
            (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
        ] {
            let exit = one(
                p.exits()
                    .iter()
                    .filter(|e| e.kind() == kind && e.point() == point),
                "Resource future qualified actual exit",
            )?;
            let mut expected = if body {
                vec![
                    UA::Drop(guard),
                    UA::EndBinding {
                        statement,
                        symbol: d.value_binding(),
                    },
                    UA::EndBinding {
                        statement,
                        symbol: d.key_binding(),
                    },
                ]
            } else {
                vec![]
            };
            if !body || matches!(kind, UE::Break(_)) {
                expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
            }
            need(
                exit.actions() == expected
                    && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
                "Resource future qualified complete For cleanup keeps source",
                (exit, expected),
            )?;
        }
    }
    let drop = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root)),
        "Resource future qualified unique lexical Drop",
    )?;
    need(
        drop.point() == UnitDropPoint::ControlTransfer(expression(lexical(site, parsed)?))
            && drop.value_origin() == site.root.marker,
        "Resource future qualified true lexical Drop",
        drop,
    )?;
    borrows::unit(
        (case, sources, parsed, site),
        names,
        typed,
        owned,
        (root, ty),
    )
}
