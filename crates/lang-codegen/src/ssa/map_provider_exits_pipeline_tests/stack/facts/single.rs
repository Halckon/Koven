//! Borrow the original single-file products; never re-run or replace an analysis.
use super::*;
use lang_frontend::{
    name_resolution::{NameResolution, ReferenceTarget, SymbolKind},
    ownership_checking::{
        CleanupOwnerValue, DropPoint, DropTarget, IterationCleanupAction as A,
        IterationExitKind as E, LoanKind, LoanTarget, MapIterationComponent,
        MapIterationSourceAccess, OwnershipBindingKind, OwnershipCheckedFile,
    },
    type_checking::{
        CallableTarget, ExpressionCategory, IntrinsicTypeConstructor, MapIterationKind,
        ParameterMode, TypeKind, TypedFile,
    },
};

pub(in crate::ssa::map_provider_exits_pipeline_tests::stack) fn collect(
    _sources: &SourceMap,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Inputs, Failure> {
    let anchors = ast(parsed)?;
    need(
        names.source_id() == parsed.source_id()
            && typed.source_id() == parsed.source_id()
            && owned.source_id() == parsed.source_id()
            && owned.is_compatible_with(names, typed)
            && typed.map_iterations().len() == anchors.len()
            && owned.map_iterations().len() == anchors.len(),
        "stack same single analysis and complete descriptor/plan coverage",
        (&anchors, typed.map_iterations(), owned.map_iterations()),
    )?;
    let mut sites = Vec::new();
    let mut cleanups = Vec::new();
    for (index, s) in anchors.iter().enumerate() {
        let d = typed
            .map_iteration(s.statement)
            .ok_or_else(|| failure("stack original typed For", s))?;
        let p = owned
            .map_iteration(s.statement)
            .ok_or_else(|| failure("stack original owned For", s))?;
        need(
            d.statement() == s.statement
                && d.source() == s.source
                && p.descriptor() == d
                && typed.expression_type(s.source) == Some(d.source_type())
                && matches!(typed.types().get(d.source_type()),Some(TypeKind::Intrinsic{constructor,arguments})
                if *constructor==if d.kind()==MapIterationKind::Map {IntrinsicTypeConstructor::Map}else{IntrinsicTypeConstructor::MutableMap}
                && arguments.as_slice()==[d.key_type(),d.value_type()]),
            "stack exact intrinsic source/K/V descriptor",
            (s, d, p),
        )?;
        need(
            typed.types().get(d.key_type())
                == Some(&TypeKind::Builtin(
                    lang_frontend::type_checking::BuiltinType::String,
                )),
            "stack original FE key is actual String",
            d,
        )?;
        let node = parsed.ast().expressions().get(s.source).unwrap();
        let source = match node.payload() {
            Expression::Name => {
                let reference = one(
                    names
                        .references()
                        .iter()
                        .filter(|r| r.span() == node.span()),
                    "stack actual source reference",
                )?;
                let ReferenceTarget::Symbol(root) = reference.target() else {
                    return Err(failure("stack actual source symbol", reference));
                };
                let symbol = one(
                    names.symbols().iter().filter(|v| v.id() == *root),
                    "stack exact source symbol",
                )?;
                let slot = parameter(parsed, s.function, symbol.span())?;
                let callable = one(
                    typed
                        .callables()
                        .iter()
                        .filter(|c| c.parameter_symbols().get(slot) == Some(&Some(*root))),
                    "stack actual source parameter signature",
                )?;
                let actual = &callable.parameters()[slot];
                let borrow = actual.mode == ParameterMode::Borrow;
                need(
                    symbol.kind() == SymbolKind::ValueParameter
                        && actual.ty == d.source_type()
                        && typed.symbol_type(*root) == Some(d.source_type())
                        && typed.expression_category(s.source) == Some(ExpressionCategory::Place)
                        && matches!(actual.mode, ParameterMode::Borrow | ParameterMode::Value)
                        && owned.binding_kind(*root)
                            == Some(if borrow {
                                OwnershipBindingKind::Shared
                            } else {
                                OwnershipBindingKind::Owned
                            })
                        && p.source_access()
                            == if borrow {
                                MapIterationSourceAccess::Shared
                            } else {
                                MapIterationSourceAccess::Owned
                            }
                        && matches!(p.source(),Some(LoanTarget::Place(path)) if path.is_root() && path.root()==*root),
                    "stack original source root and parameter slot/mode",
                    (s, root, actual, p),
                )?;
                if borrow {
                    need(
                        !owned
                            .drops()
                            .iter()
                            .any(|drop| drop.target() == DropTarget::Named(*root)),
                        "stack Borrow root has no owner Drop",
                        root,
                    )?;
                }
                Source::Parameter {
                    slot,
                    marker: symbol.span(),
                    borrow,
                }
            }
            Expression::Call { arguments, .. } => {
                let call = typed
                    .call(s.source)
                    .ok_or_else(|| failure("stack original temporary Call", s))?;
                let CallableTarget::Source(target) = call.target() else {
                    return Err(failure("stack temporary selected source callable", call));
                };
                let symbol = one(
                    names.symbols().iter().filter(|v| v.id() == target),
                    "stack temporary actual callee",
                )?;
                need(
                    call.receiver().is_none()
                        && call.arguments().len() == arguments.len()
                        && call.return_type() == d.source_type()
                        && p.source_access() == MapIterationSourceAccess::Temporary
                        && p.source() == Some(&LoanTarget::Temporary(s.source)),
                    "stack one original temporary source evaluation",
                    (call, p),
                )?;
                Source::Call {
                    span: node.span(),
                    callee: callable(parsed, symbol.span())?,
                    arity: arguments.len(),
                }
            }
            _ => return Err(failure("stack original source shape", node)),
        };
        for (component, role) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .zip(0..2)
        {
            let symbol = one(
                names
                    .symbols()
                    .iter()
                    .filter(|v| v.span() == s.names[role] && v.kind() == SymbolKind::ForBinding),
                "stack exact component marker",
            )?;
            let binding = d.bindings()[role];
            let loan = p.loan(component);
            need(
                binding == symbol.id()
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][role])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[role].symbol() == binding
                    && p.bindings()[role].kind() == OwnershipBindingKind::Shared
                    && loan == &p.loans()[role]
                    && loan.statement() == s.statement
                    && loan.source() == s.source
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && Some(loan.origin()) == p.source(),
                "stack exact per-layer K/V loan identity",
                (s, symbol, loan),
            )?;
        }
        incoming::check(parsed, s, p, owned, names)?;
        let mut owner_markers = Vec::new();
        for phi in p.closure_phis() {
            let value = owned.cleanup_conditions().owner_value(phi.owner());
            need(
                matches!(value,Some(CleanupOwnerValue::IterationPhi{statement,boundary,symbol,..})
                if *statement==s.statement && *boundary==phi.boundary() && *symbol==phi.symbol()),
                "stack actual current owner phi and source symbol",
                (phi, value),
            )?;
            let marker = one(
                names.symbols().iter().filter(|n| n.id() == phi.symbol()),
                "stack owner phi original marker",
            )?
            .span();
            if !owner_markers.contains(&marker) {
                owner_markers.push(marker);
            }
        }
        let expected = exits(parsed, &anchors, index);
        need(
            p.exits().len() == expected.len(),
            "stack exact FE per-layer exit coverage",
            p.exits(),
        )?;
        for exit in p.exits() {
            let (kind, point) = match exit.kind() {
                E::Fallthrough => (0, parsed.ast().statements().get(s.body).unwrap().span()),
                E::Exhaustion => (1, s.span),
                E::Return(id) => (2, parsed.ast().expressions().get(id).unwrap().span()),
                E::Break(id) => (3, parsed.ast().expressions().get(id).unwrap().span()),
                _ => return Err(failure("stack actual finite exit kind", exit)),
            };
            need(
                expected.contains(&(kind, point))
                    && point_span(parsed, exit.point())? == point
                    && owned.cleanup_conditions().get(exit.condition()).is_some(),
                "stack exact original exit point and real condition",
                exit,
            )?;
            let complete = owned
                .cleanup_steps()
                .iter()
                .filter_map(|(at, a)| (*at == exit.point()).then_some(*a))
                .collect::<Vec<_>>();
            need(
                complete == exit.actions(),
                "stack full same-point ordered cleanup table",
                (exit, &complete),
            )?;
            let mut actions = Vec::new();
            for action in &complete {
                actions.push(match *action {
                    A::EndBinding { statement, symbol } => {
                        let i = layer(&anchors, statement)?;
                        let dd = typed.map_iteration(statement).unwrap();
                        if symbol == dd.value_binding() {
                            Action::Value(i)
                        } else {
                            need(
                                symbol == dd.key_binding(),
                                "stack EndBinding exact K/V",
                                action,
                            )?;
                            Action::Key(i)
                        }
                    }
                    A::FinishProvider(statement) => Action::Finish(layer(&anchors, statement)?),
                    A::EndSource(statement) => Action::End(layer(&anchors, statement)?),
                    A::Drop(drop) => {
                        need(
                            owned.drops().contains(&drop)
                                && drop.point() == exit.point()
                                && drop.capture_slot().is_none()
                                && drop.instance_address().is_none()
                                && drop
                                    .condition()
                                    .is_none_or(|c| owned.cleanup_conditions().get(c).is_some()),
                            "stack actual owner Drop and condition",
                            drop,
                        )?;
                        let owner = drop
                            .owner()
                            .ok_or_else(|| failure("stack Drop current owner identity", drop))?;
                        let fact = owned
                            .cleanup_conditions()
                            .owner_value(owner)
                            .ok_or_else(|| failure("stack Drop owner definition", drop))?;
                        need(
                            matches!(
                                fact,
                                CleanupOwnerValue::Parameter { .. }
                                    | CleanupOwnerValue::Expression { .. }
                                    | CleanupOwnerValue::IterationPhi { .. }
                            ),
                            "stack ordinary current owner definition",
                            fact,
                        )?;
                        Action::Drop(drop.value_origin())
                    }
                    _ => return Err(failure("stack unexpected cleanup action", action)),
                });
            }
            validate_order(&actions)?;
            need(
                actions.contains(&if kind == 1 {
                    Action::End(index)
                } else {
                    Action::Value(index)
                }),
                "stack own binding/source cleanup is present",
                &actions,
            )?;
            save(&mut cleanups, point, kind, actions)?;
        }
        sites.push(Site {
            ast: s.clone(),
            source,
            mutable: d.kind() == MapIterationKind::MutableMap,
            key_type: format!("{:?}", typed.types().get(d.key_type())),
            value_type: format!("{:?}", typed.types().get(d.value_type())),
            owner_markers,
            representation: representation::single(parsed, names, typed, d.value_type())?,
        });
    }
    let calls = calls(parsed, names, typed, owned)?;
    let guards = representation::single_guards(parsed, names, typed, owned, &anchors)?;
    let name_inputs = names_snapshot(parsed, names)?;
    Ok(Inputs {
        sites,
        cleanups,
        calls,
        guards,
        names: name_inputs,
    })
}

fn point_span(parsed: &ParsedFile, point: DropPoint) -> Result<Span, Failure> {
    match point {
        DropPoint::AfterStatement(s) | DropPoint::LoopExit(s) => {
            Ok(parsed.ast().statements().get(s).unwrap().span())
        }
        DropPoint::ControlTransfer(e) => Ok(parsed.ast().expressions().get(e).unwrap().span()),
        _ => Err(failure("stack actual exit cleanup point", point)),
    }
}

fn calls(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<Vec<Call>, Failure> {
    let mut result = Vec::new();
    for (id, node) in parsed.ast().expressions().iter() {
        let Expression::Call { arguments, .. } = node.payload() else {
            continue;
        };
        let Some(call) = typed.call(id) else {
            continue;
        };
        let CallableTarget::Source(target) = call.target() else {
            continue;
        };
        let callee = one(
            names.symbols().iter().filter(|s| s.id() == target),
            "stack Call real selected symbol",
        )?;
        need(
            call.receiver().is_none() && call.arguments().len() == arguments.len(),
            "stack source Call actual arity",
            call,
        )?;
        let mut args = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            let n = parsed.ast().expressions().get(argument.value).unwrap();
            let actual = &call.arguments()[index];
            need(
                actual.argument_index() == index
                    && actual.parameter_index() == index
                    && typed.expression_type(argument.value) == Some(actual.parameter_type()),
                "stack Call original argument/parameter slot",
                (call, argument),
            )?;
            let roots = names
                .references()
                .iter()
                .filter(|r| r.span() == n.span())
                .filter_map(|r| match r.target() {
                    ReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let root = if matches!(n.payload(), Expression::Name) {
                Some(one(roots.into_iter(), "stack Call actual Name root")?)
            } else {
                None
            };
            let marker = root
                .map(|root| {
                    one(
                        names.symbols().iter().filter(|s| s.id() == root),
                        "stack Call root marker",
                    )
                    .map(|s| s.span())
                })
                .transpose()?;
            if actual.mode() == ParameterMode::Borrow
                && let Some(root) = root
            {
                let loan = one(
                    owned
                        .loans()
                        .iter()
                        .filter(|l| l.call() == id && l.argument() == argument.value),
                    "stack Call real Shared loan",
                )?;
                need(
                    loan.kind() == LoanKind::Shared
                        && matches!(loan.target(),LoanTarget::Place(p) if p.is_root() && p.root()==root)
                        && loan.begin_span() == n.span()
                        && loan.end_span() == node.span(),
                    "stack original synchronous argument loan",
                    loan,
                )?;
            }
            args.push((n.span(), marker, actual.mode() == ParameterMode::Borrow));
        }
        result.push(Call {
            span: node.span(),
            callee: callable(parsed, callee.span())?,
            arguments: args,
        });
    }
    Ok(result)
}

fn names_snapshot(parsed: &ParsedFile, names: &NameResolution) -> Result<Vec<NameInput>, Failure> {
    let mut result = Vec::new();
    for (_, n) in parsed.ast().expressions().iter() {
        if !matches!(n.payload(), Expression::Name) {
            continue;
        }
        let roots = names
            .references()
            .iter()
            .filter(|r| r.span() == n.span())
            .filter_map(|r| match r.target() {
                ReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            })
            .collect::<Vec<_>>();
        if roots.is_empty() {
            continue;
        }
        let root = one(roots.into_iter(), "stack original Name selected symbol")?;
        let symbol = one(
            names.symbols().iter().filter(|s| s.id() == root),
            "stack Name actual declaration marker",
        )?;
        result.push(NameInput {
            span: n.span(),
            marker: symbol.span(),
        });
    }
    Ok(result)
}
