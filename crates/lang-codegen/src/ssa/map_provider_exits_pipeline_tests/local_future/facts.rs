//! Public local formation, multi-For live owner identity and actual Pure last-use facts.
mod ast;
mod loans;
mod owner;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Report};
use ast::{Loop, Scanner};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolId, SymbolKind, UnitReferenceTarget, UnitSymbolId,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupCondition as C, CleanupConditionId, CleanupOwnerValue as O, CleanupOwnerValueId,
        CleanupSelection, CleanupSelectorSource, CompilationUnitOwnership, DropFact, DropPoint,
        DropTarget, IterationCleanupAction as A, IterationExitKind as E, IterationPhiBoundary as B,
        IterationPhiIncomingKind as K, IterationPhiPresenceSource, IterationSource, LoanEndPoint,
        LoanKind, LoanTarget, MapIterationComponent, MapIterationOwnershipPlan,
        MapIterationSourceAccess, OwnershipBindingKind, OwnershipCheckedFile,
        UnitCallArgumentOwnershipKind, UnitDropFact, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as UA, UnitIterationExitKind as UE, UnitIterationSourceAccess,
        UnitLoanTarget,
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
        "local future actual Name symbol",
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
        "local future actual qualified Name symbol",
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
pub(super) fn single(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    report: &mut Report,
) {
    report.check(
        "local future complete single provider set",
        typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 }
            && owned.map_iterations().len() == typed.map_iterations().len(),
        (typed.map_iterations(), owned.map_iterations()),
    );
    match ast::scanners(case, sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "local future original single formation/transport/lifetime",
                    &single_site((case, sources, parsed), site, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("local future original AST", &Err(e)),
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
            "local future actual declaration and formation history",
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
        "local future actual alias Move has no duplicate obligation",
        (root, original),
    )?;
    let ty = typed
        .symbol_type(root)
        .ok_or_else(|| failure("local future original source type", root))?;
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.symbol_type(original) == Some(ty)
            && typed.is_resource_type(ty) == Some(false)
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
            && matches!(typed.types().get(ty),Some(TypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && matches!(arguments.as_slice(),[k,v] if k==v && typed.types().get(*k)==Some(&TypeKind::Builtin(BuiltinType::String)))),
        "local future exact Pure Map birth/type/category",
        ty,
    )?;
    let factory = one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == site.factory && s.kind() == SymbolKind::Function),
        "local future selected original factory",
    )?;
    let call = typed
        .call(site.birth.initializer)
        .ok_or_else(|| failure("local future actual factory Call", site))?;
    need(
        call.target() == CallableTarget::Source(factory.id())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == ty
            && call.result_source() == CallableResultSource::Owned,
        "local future original zero-argument Call delivers exact owned Map",
        call,
    )?;
    let scanner = one(
        names
            .symbols()
            .iter()
            .filter(|s| s.span() == site.name && s.kind() == SymbolKind::Function),
        "local future original scanner declaration",
    )?;
    need(
        typed.callables().iter().any(|c| {
            c.symbol() == scanner.id()
                && c.parameters().is_empty()
                && typed.types().get(c.return_type()) == Some(&TypeKind::Builtin(BuiltinType::Unit))
        }),
        "local future actual zero-parameter Unit scanner",
        scanner,
    )?;
    let drop = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(root)),
        "local future unique actual Pure source Drop",
    )?;
    need(
        drop.value_origin() == site.root.marker
            && drop.capture_slot().is_none()
            && drop.instance_address().is_none(),
        "local future original current root Drop marker",
        drop,
    )?;
    let mut previous = None;
    for loop_site in &site.loops {
        let p = owned
            .map_iteration(loop_site.statement)
            .ok_or_else(|| failure("local future actual Map plan", loop_site))?;
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
                && p.source_access() == MapIterationSourceAccess::Owned
                && matches!(p.source_kind(),IterationSource::BorrowedSource(LoanTarget::Place(path)) if path.is_root() && path.root()==root),
            "local future actual original For source and Owned capability",
            p,
        )?;
        for (index, binding) in d.bindings().into_iter().enumerate() {
            need(
                names.symbols().iter().any(|s| {
                    s.id() == binding
                        && s.span() == loop_site.bindings[index]
                        && s.kind() == SymbolKind::ForBinding
                }) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && typed.types().get([d.key_type(), d.value_type()][index])
                        == Some(&TypeKind::Builtin(BuiltinType::String))
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
                "local future actual original K/V Shared lineage",
                p.loans(),
            )?;
        }
        let guard = *one(
            owned
                .drops()
                .iter()
                .filter(|d| d.value_origin() == loop_site.guard.marker),
            "local future actual Guard Drop",
        )?;
        need(guard.point()==loop_site.point && guard.condition().is_none()
            && matches!(guard.target(),DropTarget::Named(id) if owned.formed_owned_locals().contains(&id) && names.symbols().iter().any(|s|s.id()==id && s.span()==loop_site.guard.marker))
            && guard.owner().is_some_and(|o|matches!(owned.cleanup_conditions().owner_value(o),Some(O::Expression{expression,origin}) if *expression==loop_site.guard.initializer && *origin==loop_site.guard_span)),"local future original Guard construction/current exit",guard)?;
        need(
            p.exits().len() == 2,
            "local future complete actual provider exits",
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
                "local future actual exact exit",
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
                "local future full cleanup preserves source until actual future read",
                (exit, full, expected),
            )?;
        }
        previous = Some(owner::check(site, loop_site, root, previous, p, owned)?);
    }
    let (owner, condition) =
        previous.ok_or_else(|| failure("local future actual final Exit", site))?;
    need(
        drop.owner() == Some(owner) && drop.condition() == Some(condition),
        "local future final ASAP Drop reads exact last Exit owner/availability",
        drop,
    )?;
    loans::single(
        (case, sources, parsed, site),
        names,
        typed,
        owned,
        (root, ty),
        &drop,
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
    report.check(
        "local future complete qualified provider set",
        typed.map_iterations().len() == if case.form == 2 { 4 } else { 2 }
            && owned.map_iterations().len() == typed.map_iterations().len(),
        (typed.map_iterations(), owned.map_iterations()),
    );
    match ast::scanners(case, sources, parsed) {
        Ok(sites) => {
            for site in &sites {
                report.query(
                    "local future original qualified formation/cleanup/loans",
                    &unit_site((case, sources, parsed), site, names, typed, owned),
                );
            }
        }
        Err(e) => report.query::<()>("local future original AST", &Err(e)),
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
            id.source_unit() == unit
                && marker(names, id, local.marker, SymbolKind::Variable)
                && owned.formed_owned_locals().contains(&id),
            "local future qualified actual declaration/formation",
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
        "local future qualified alias original has no duplicate Drop",
        (root, original),
    )?;
    let ty = typed
        .symbol_type(root)
        .ok_or_else(|| failure("local future qualified source type", root))?;
    let constructor = if case.mutable {
        IntrinsicTypeConstructor::MutableMap
    } else {
        IntrinsicTypeConstructor::Map
    };
    need(
        typed.symbol_type(original) == Some(ty)
            && typed.is_resource_type(ty) == Some(false)
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
            && matches!(typed.types().get(ty),Some(UnitTypeKind::Intrinsic{constructor:c,arguments}) if *c==constructor && matches!(arguments.as_slice(),[k,v] if k==v && typed.types().get(*k)==Some(&UnitTypeKind::Builtin(BuiltinType::String)))),
        "local future qualified exact Pure Map birth/type/category",
        ty,
    )?;
    let factory = one(
        typed.signatures().declarations().iter().filter(|d| {
            d.callable()
                .is_some_and(|c| c.name_span() == site.factory && c.parameters().is_empty())
        }),
        "local future qualified original selected factory",
    )?;
    let call = typed
        .call(expression(site.birth.initializer))
        .ok_or_else(|| failure("local future qualified actual birth Call", site))?;
    need(
        call.target() == UnitCallTarget::Declaration(factory.declaration())
            && call.arguments().is_empty()
            && call.receiver().is_none()
            && call.return_type() == ty
            && call.result_source() == CallableResultSource::Owned,
        "local future qualified actual Call formation",
        call,
    )?;
    let scanner = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .filter(|c| c.name_span() == site.name),
        "local future qualified original scanner",
    )?;
    need(
        scanner.parameters().is_empty()
            && typed.types().get(scanner.return_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Unit)),
        "local future qualified zero-argument Unit scanner",
        scanner,
    )?;
    let drop = *one(
        owned
            .drops()
            .iter()
            .filter(|d| d.target() == UnitDropTarget::Named(root)),
        "local future unique qualified Pure Drop",
    )?;
    need(
        drop.value_origin() == site.root.marker,
        "local future qualified original root Drop marker",
        drop,
    )?;
    for loop_site in &site.loops {
        let statement = UnitStatementId::new(unit, loop_site.statement);
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("local future actual qualified Map plan", loop_site))?;
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
                && p.source_access() == UnitIterationSourceAccess::Owned
                && matches!(p.source_kind(),IterationSource::BorrowedSource(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==root),
            "local future original qualified For descriptor/Owned lineage",
            p,
        )?;
        for (index, binding) in d.bindings().into_iter().enumerate() {
            need(
                marker(
                    names,
                    binding,
                    loop_site.bindings[index],
                    SymbolKind::ForBinding,
                ) && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][index])
                    && typed.types().get([d.key_type(), d.value_type()][index])
                        == Some(&UnitTypeKind::Builtin(BuiltinType::String))
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
                "local future exact qualified original K/V identities",
                p.loans(),
            )?;
        }
        let point = match loop_site.point {
            DropPoint::AfterStatement(s) => {
                UnitDropPoint::AfterStatement(UnitStatementId::new(unit, s))
            }
            DropPoint::ControlTransfer(e) => UnitDropPoint::ControlTransfer(expression(e)),
            other => return Err(failure("local future qualified actual point", other)),
        };
        let kind = match loop_site.kind {
            E::Fallthrough => UE::Fallthrough,
            E::Continue(e) => UE::Continue(expression(e)),
            E::Break(e) => UE::Break(expression(e)),
            other => return Err(failure("local future actual qualified kind", other)),
        };
        let guard = *one(
            owned
                .drops()
                .iter()
                .filter(|d| d.value_origin() == loop_site.guard.marker),
            "local future original qualified Guard Drop",
        )?;
        need(
            guard.point() == point
                && matches!(guard.target(),UnitDropTarget::Named(id) if marker(names,id,loop_site.guard.marker,SymbolKind::Variable) && owned.formed_owned_locals().contains(&id)),
            "local future actual qualified Guard target/point",
            guard,
        )?;
        need(
            p.exits().len() == 2,
            "local future complete qualified exit set",
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
                "local future original qualified exit",
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
                "local future qualified full cleanup never drops still-needed source",
                (exit, expected),
            )?;
        }
    }
    loans::unit(
        (case, sources, parsed, site),
        names,
        typed,
        owned,
        (root, ty),
        &drop,
    )
}
