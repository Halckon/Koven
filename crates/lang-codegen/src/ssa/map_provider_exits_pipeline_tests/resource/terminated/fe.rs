//! Same-analysis terminal FE identity and complete cleanup, independent of lowering qualification.
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::super::identity::Input;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolKind, UnitReferenceTarget,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupCondition, CleanupOwnerValue, CleanupSelection, CleanupSelectorSource,
        CompilationUnitOwnership, DropPoint, DropTarget, IterationCleanupAction as A,
        IterationExitKind as E, IterationPhiBoundary as B, IterationPhiIncomingKind as K,
        LoanKind as FLoanKind, LoanTarget, MapIterationComponent, MapIterationSourceAccess,
        OwnershipBindingKind, OwnershipCheckedFile, UnitCallArgumentOwnershipKind as UK,
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as UA,
        UnitIterationExitKind as UE, UnitIterationSourceAccess, UnitLoanTarget,
        UnitValueDeliveryKind,
    },
    parser::{Expression, Item, NameMarker, ParsedFile},
    source::Span,
    type_checking::{
        BuiltinType, CallableTarget, CompilationUnitTypes, ParameterMode, TypeKind, TypedFile,
        UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

fn transfer(input: &Input, parsed: &ParsedFile) -> Result<(ExpressionId, bool), Failure> {
    need(
        input.continued.is_none(),
        "terminal body has no Continue",
        input,
    )?;
    let body = parsed
        .ast()
        .statements()
        .get(input.body)
        .map_err(|e| failure("terminal original body", e))?
        .span();
    one(
        parsed.ast().expressions().iter().filter_map(|(id, node)| {
            let returning = match node.payload() {
                Expression::Break { .. } => false,
                Expression::Return { .. } => true,
                _ => return None,
            };
            (node.span().source_id() == body.source_id()
                && node.span().start() > body.start()
                && node.span().end() < body.end())
            .then_some((id, returning))
        }),
        "terminal original unique body Break/Return",
    )
}

fn loop_locals(input: &Input, parsed: &ParsedFile) -> Vec<(Span, ExpressionId)> {
    parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| {
            let Item::Variable {
                name: NameMarker::Present(name),
                initializer,
                ..
            } = node.payload()
            else {
                return None;
            };
            (node.span().source_id() == input.for_span.source_id()
                && node.span().start() > input.for_span.start()
                && node.span().end() < input.for_span.end())
            .then_some((*name, *initializer))
        })
        .collect()
}

fn normal_point(input: &Input, parsed: &ParsedFile) -> Result<DropPoint, Failure> {
    let scope = parsed
        .ast()
        .statements()
        .get(input.scope)
        .map_err(|e| failure("terminal original scope", e))?
        .span();
    let returns = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (matches!(node.payload(), Expression::Return { .. })
                && node.span().source_id() == scope.source_id()
                && node.span().start() >= input.for_span.end()
                && node.span().end() < scope.end())
            .then_some(id)
        })
        .collect::<Vec<_>>();
    match returns.as_slice() {
        [] => Ok(DropPoint::AfterStatement(input.scope)),
        [returned] => Ok(DropPoint::ControlTransfer(*returned)),
        _ => Err(failure(
            "terminal original unique exhaustion continuation Return",
            returns,
        )),
    }
}

fn scan_name(input: &Input, parsed: &ParsedFile) -> Result<Span, Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            match node.payload() {
                Item::Function {
                    name: NameMarker::Present(name),
                    ..
                } if node.span() == input.scan => Some(*name),
                _ => None,
            }
        }),
        "terminal original scan AST name",
    )
}

/// Both explicit returns belong to the original scan; implicit Unit exhaustion has no operand.
fn return_values(
    transfer: ExpressionId,
    normal: DropPoint,
    parsed: &ParsedFile,
) -> Result<Vec<Option<ExpressionId>>, Failure> {
    let mut sites = vec![transfer];
    if let DropPoint::ControlTransfer(expression) = normal {
        sites.push(expression);
    }
    sites
        .into_iter()
        .map(|id| {
            let node = parsed
                .ast()
                .expressions()
                .get(id)
                .map_err(|e| failure("terminal original Return", e))?;
            let Expression::Return { value, .. } = node.payload() else {
                return Err(failure("terminal actual Return AST", node));
            };
            Ok(*value)
        })
        .collect()
}

pub(super) fn single(
    input: &Input,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let (transfer, returning) = transfer(input, parsed)?;
    let normal = normal_point(input, parsed)?;
    let locals = loop_locals(input, parsed);
    let root = one(
        names
            .references()
            .iter()
            .filter(|r| r.span() == input.source_span)
            .filter_map(|r| match r.target() {
                ReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "resource single original source root",
    )?;
    let symbol = one(
        names.symbols().iter().filter(|s| {
            s.id() == root
                && s.kind() == SymbolKind::ValueParameter
                && !s.is_synthetic()
                && s.span() == input.parameter
        }),
        "resource single actual parameter",
    )?;
    let signature = one(
        typed
            .callables()
            .iter()
            .filter(|c| c.parameter_symbols() == [Some(root)]),
        "resource single actual scan signature",
    )?;
    let scan_name = scan_name(input, parsed)?;
    need(
        names
            .symbols()
            .iter()
            .any(|s| s.id() == signature.symbol() && s.span() == scan_name),
        "terminal single original scan root",
        signature,
    )?;
    if returning {
        for operand in return_values(transfer, normal, parsed)? {
            need(
                match operand {
                    Some(value) => typed.expression_type(value) == Some(signature.return_type()),
                    None => {
                        typed.types().get(signature.return_type())
                            == Some(&TypeKind::Builtin(BuiltinType::Unit))
                    }
                },
                "terminal single original Return operand matches scan result",
                (operand, signature),
            )?;
        }
    }
    let mode = if input.owned {
        ParameterMode::Value
    } else {
        ParameterMode::Borrow
    };
    let p = owned
        .map_iteration(input.statement)
        .ok_or_else(|| failure("resource single plan", input.statement))?;
    let d = p.descriptor();
    need(
        typed.map_iterations().len() == 1
            && owned.map_iterations().len() == 1
            && typed.map_iteration(input.statement) == Some(d)
            && d.source() == input.source
            && typed.is_resource_type(d.value_type()) == Some(true)
            && typed.is_resource_type(d.source_type()) == Some(true)
            && matches!(signature.parameters(), [s] if s.mode == mode && s.ty == d.source_type())
            && matches!(p.source(), Some(LoanTarget::Place(place)) if place.root() == root && place.is_root())
            && p.source_access()
                == if input.owned {
                    MapIterationSourceAccess::Owned
                } else {
                    MapIterationSourceAccess::Shared
                },
        "resource single original type/source capability",
        (d, p.source(), signature),
    )?;
    for (i, binding) in d.bindings().into_iter().enumerate() {
        need(
            owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                && p.loans()[i].binding() == binding
                && p.bindings()[i].symbol() == binding
                && p.bindings()[i].kind() == OwnershipBindingKind::Shared
                && p.loans()[i].component()
                    == [MapIterationComponent::Key, MapIterationComponent::Value][i]
                && p.loans()[i].kind() == FLoanKind::Shared
                && p.loans()[i].statement() == input.statement
                && p.loans()[i].source() == input.source
                && Some(p.loans()[i].origin()) == p.source(),
            "resource single separate Shared components",
            p.loans(),
        )?;
    }
    let drops = owned
        .drops()
        .iter()
        .filter(|f| f.target() == DropTarget::Named(root))
        .collect::<Vec<_>>();
    let transfer_point = DropPoint::ControlTransfer(transfer);
    let exhausted_point = DropPoint::LoopExit(input.statement);
    need(
        if input.owned {
            drops.len() == if returning { 2 } else { 1 }
                && drops.iter().all(|d| {
                    d.value_origin() == symbol.span()
                        && d.capture_slot().is_none()
                        && d.instance_address().is_none()
                })
                && drops.iter().filter(|d| d.point() == normal).count() == 1
                && (!returning || drops.iter().filter(|d| d.point() == transfer_point).count() == 1)
        } else {
            drops.is_empty()
        },
        "terminal single exact source obligations",
        &drops,
    )?;
    need(
        p.exits().len() == 2,
        "terminal single complete exit set",
        p.exits(),
    )?;
    for (kind, point, body) in [
        (
            if returning {
                E::Return(transfer)
            } else {
                E::Break(transfer)
            },
            transfer_point,
            true,
        ),
        (E::Exhaustion, exhausted_point, false),
    ] {
        let exit = one(
            p.exits()
                .iter()
                .filter(|e| e.kind() == kind && e.point() == point),
            "terminal single actual exit",
        )?;
        let mut expected = owned
            .drops()
            .iter()
            .copied()
            .filter(|d| d.point() == point && d.target() != DropTarget::Named(root))
            .map(A::Drop)
            .collect::<Vec<_>>();
        need(
            expected.len() == if body { locals.len() } else { 0 },
            "terminal single every actual loop-local cleanup",
            (&locals, &expected),
        )?;
        for action in &expected {
            let A::Drop(drop) = action else {
                unreachable!()
            };
            need(body && drop.condition().is_none()
                && drop.value_origin().source_id() == input.for_span.source_id()
                && drop.value_origin().start() > input.for_span.start()
                && drop.value_origin().end() < input.for_span.end()
                && matches!(drop.target(), DropTarget::Named(local) if names.symbols().iter().any(|s| s.id()==local && s.span()==drop.value_origin()))
                && drop.owner().is_some_and(|owner| matches!(owned.cleanup_conditions().owner_value(owner), Some(CleanupOwnerValue::Expression { expression, origin }) if locals.contains(&(drop.value_origin(),*expression)) && parsed.ast().expressions().get(*expression).is_ok_and(|node| node.span()==*origin && origin.start()>input.for_span.start() && origin.end()<input.for_span.end()))),
                "terminal single real loop-local construction/drop", drop)?;
        }
        if body {
            expected.extend([
                A::EndBinding {
                    statement: input.statement,
                    symbol: d.value_binding(),
                },
                A::EndBinding {
                    statement: input.statement,
                    symbol: d.key_binding(),
                },
            ]);
        }
        expected.extend([
            A::FinishProvider(input.statement),
            A::EndSource(input.statement),
        ]);
        expected.extend(
            drops
                .iter()
                .filter(|d| d.point() == point)
                .map(|d| A::Drop(**d)),
        );
        let full = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(at, action)| (*at == point).then_some(*action))
            .collect::<Vec<_>>();
        need(
            exit.actions() == expected && full == expected,
            "terminal single full ordered cleanup, source after EndSource",
            (exit, full, expected),
        )?;
    }
    let conditions = owned.cleanup_conditions();
    let incoming_kinds = if returning {
        vec![K::Entry, K::Exhaustion]
    } else {
        vec![K::Entry, K::Break(transfer), K::Exhaustion]
    };
    need(
        p.closure_phi_incomings().len() == incoming_kinds.len(),
        "terminal single complete incoming set",
        p,
    )?;
    if input.owned {
        let h = one(
            p.closure_phis()
                .iter()
                .filter(|p| p.symbol() == root && p.boundary() == B::Header),
            "terminal single Header",
        )?;
        let x = one(
            p.closure_phis()
                .iter()
                .filter(|p| p.symbol() == root && p.boundary() == B::Exit),
            "terminal single Exit",
        )?;
        need(
            p.closure_phis().len() == 2
                && h.owner() != x.owner()
                && h.availability_selector() != x.availability_selector(),
            "terminal single distinct boundary owner/selector",
            p,
        )?;
        for phi in [h, x] {
            let selector = conditions
                .selector(phi.availability_selector())
                .ok_or_else(|| failure("terminal single actual selector", phi))?;
            need(
                matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi { statement, symbol, boundary, origin }) if *statement==input.statement && *symbol==root && *boundary==phi.boundary() && *origin==input.for_span)
                    && matches!(selector.source(), CleanupSelectorSource::IterationPhi { statement, boundary, .. } if statement==input.statement && boundary==phi.boundary())
                    && selector.origin() == input.for_span
                    && selector.selection() == CleanupSelection::IterationPresence
                    && selector.branch_count() == 2
                    && matches!(conditions.get(phi.availability_condition()), Some(CleanupCondition::Choice { selector, branches }) if *selector==phi.availability_selector() && matches!(branches.as_slice(), [no,yes] if conditions.get(*no)==Some(&CleanupCondition::Never) && conditions.get(*yes)==Some(&CleanupCondition::Always))),
                "terminal single complete original owner and Choice",
                (phi, selector),
            )?;
        }
        for drop in &drops {
            let phi = if drop.point() == transfer_point { h } else { x };
            need(
                drop.owner() == Some(phi.owner())
                    && drop.condition() == Some(phi.availability_condition()),
                "terminal single Drop reads exact current boundary owner",
                drop,
            )?;
        }
        for kind in incoming_kinds {
            let edge = one(
                p.closure_phi_incomings()
                    .iter()
                    .filter(|e| e.kind() == kind),
                "terminal single unique incoming kind",
            )?;
            let entry = kind == K::Entry;
            let phi = if entry { h } else { x };
            let point = if entry {
                DropPoint::AfterExpression(input.source)
            } else if kind == K::Exhaustion {
                exhausted_point
            } else {
                transfer_point
            };
            let binding = one(
                edge.bindings().iter(),
                "terminal single exact incoming binding",
            )?;
            let value = one(
                binding.values().iter(),
                "terminal single live incoming owner",
            )?;
            need(
                edge.point() == point
                    && edge.boundary() == phi.boundary()
                    && conditions.get(edge.condition()) == Some(&CleanupCondition::Always)
                    && binding.target() == phi.owner()
                    && binding.availability_selector() == phi.availability_selector()
                    && value.condition() == binding.available_when()
                    && if entry {
                        conditions.get(binding.available_when()) == Some(&CleanupCondition::Always)
                            && matches!(conditions.owner_value(value.source()), Some(CleanupOwnerValue::Parameter { symbol, origin }) if *symbol==root && *origin==input.parameter)
                    } else {
                        value.source() == h.owner()
                            && binding.available_when() == h.availability_condition()
                    },
                "terminal single every incoming original provenance",
                edge,
            )?;
        }
    } else {
        need(
            p.closure_phis().is_empty(),
            "terminal single Borrow has no owner phi",
            p,
        )?;
        for kind in incoming_kinds {
            let edge = one(
                p.closure_phi_incomings()
                    .iter()
                    .filter(|e| e.kind() == kind),
                "terminal single Borrow incoming",
            )?;
            let entry = kind == K::Entry;
            let point = if entry {
                DropPoint::AfterExpression(input.source)
            } else if kind == K::Exhaustion {
                exhausted_point
            } else {
                transfer_point
            };
            need(
                edge.bindings().is_empty()
                    && edge.point() == point
                    && edge.boundary() == if entry { B::Header } else { B::Exit }
                    && conditions.get(edge.condition()) == Some(&CleanupCondition::Always),
                "terminal single Borrow has only exact empty incoming bindings",
                edge,
            )?;
        }
    }
    let mut calls = 0;
    for (id, node) in parsed.ast().expressions().iter() {
        let Some(call) = typed
            .call(id)
            .filter(|c| c.target() == CallableTarget::Source(signature.symbol()))
        else {
            continue;
        };
        calls += 1;
        let Expression::Call { arguments, .. } = node.payload() else {
            return Err(failure("resource caller AST", node));
        };
        let [arg] = arguments.as_slice() else {
            return Err(failure("resource caller arity", arguments));
        };
        let arg_span = parsed
            .ast()
            .expressions()
            .get(arg.value)
            .map_err(|e| failure("resource caller argument", e))?
            .span();
        let arg_root = one(
            names
                .references()
                .iter()
                .filter(|r| r.span() == arg_span)
                .filter_map(|r| match r.target() {
                    ReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                }),
            "resource original caller root",
        )?;
        let loans = owned
            .loans()
            .iter()
            .filter(|l| l.call() == id)
            .collect::<Vec<_>>();
        need(
            call.return_type() == signature.return_type()
                && typed.expression_type(id) == Some(signature.return_type())
                && matches!(call.arguments(), [a] if a.mode() == mode && a.argument_index() == 0 && a.parameter_index() == 0 && Some(a.parameter_type()) == typed.expression_type(arg.value))
                && if input.owned {
                    loans.is_empty()
                } else {
                    matches!(loans.as_slice(), [l] if l.argument() == arg.value && l.kind() == FLoanKind::Shared && matches!(l.target(), LoanTarget::Place(p) if p.is_root() && p.root() == arg_root))
                },
            "resource single caller Value/Borrow contract and original root",
            (call, arg_root, loans),
        )?;
    }
    need(
        calls == 2 && symbol.span() == input.parameter,
        "resource single two actual caller sites and original parameter span",
        (calls, symbol),
    )?;
    Ok(())
}

pub(super) fn unit(
    input: &Input,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let (transfer, returning) = transfer(input, parsed)?;
    let normal = normal_point(input, parsed)?;
    let locals = loop_locals(input, parsed);
    let root = one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == input.source_span)
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "resource unit original source root",
    )?;
    let statement = UnitStatementId::new(root.source_unit(), input.statement);
    let p = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("resource unit plan", statement))?;
    let d = p.descriptor();
    let declaration = one(
        typed.signatures().declarations().iter().filter(|s| {
            s.callable()
                .is_some_and(|c| matches!(c.parameters(), [v] if v.symbol() == Some(root)))
        }),
        "resource unit original scan declaration",
    )?;
    let signature = declaration
        .callable()
        .ok_or_else(|| failure("resource callable signature", declaration))?;
    need(
        signature.name_span() == scan_name(input, parsed)?,
        "terminal unit original scan root",
        signature,
    )?;
    if returning {
        for operand in return_values(transfer, normal, parsed)? {
            need(
                match operand {
                    Some(value) => {
                        typed.expression_type(UnitExpressionId::new(root.source_unit(), value))
                            == Some(signature.return_type())
                    }
                    None => {
                        typed.types().get(signature.return_type())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                    }
                },
                "terminal unit original Return operand matches qualified scan result",
                (operand, signature),
            )?;
        }
    }
    let mode = if input.owned {
        ParameterMode::Value
    } else {
        ParameterMode::Borrow
    };
    need(
        typed.map_iterations().len() == 1
            && owned.map_iterations().len() == 1
            && typed.map_iteration(statement) == Some(d)
            && d.source() == UnitExpressionId::new(root.source_unit(), input.source)
            && typed.is_resource_type(d.value_type()) == Some(true)
            && typed.is_resource_type(d.source_type()) == Some(true)
            && matches!(signature.parameters(), [v] if v.mode() == mode && v.ty() == d.source_type())
            && matches!(p.source(), Some(UnitLoanTarget::Place(place)) if place.is_root() && place.root() == root)
            && p.source_access()
                == if input.owned {
                    UnitIterationSourceAccess::Owned
                } else {
                    UnitIterationSourceAccess::Shared
                },
        "resource unit real source type and capability",
        (signature, p),
    )?;
    for (index, binding) in d.bindings().into_iter().enumerate() {
        need(
            p.bindings()[index].symbol() == binding
                && p.bindings()[index].kind() == OwnershipBindingKind::Shared
                && p.loans()[index].binding() == binding
                && p.loans()[index].component()
                    == [MapIterationComponent::Key, MapIterationComponent::Value][index]
                && p.loans()[index].statement() == statement
                && p.loans()[index].source() == d.source()
                && p.loans()[index].kind() == FLoanKind::Shared
                && Some(p.loans()[index].origin()) == p.source(),
            "terminal unit exact source-qualified Shared components",
            p,
        )?;
    }
    let transfer = UnitExpressionId::new(root.source_unit(), transfer);
    let transfer_point = UnitDropPoint::ControlTransfer(transfer);
    let normal = match normal {
        DropPoint::AfterStatement(s) => {
            UnitDropPoint::AfterStatement(UnitStatementId::new(root.source_unit(), s))
        }
        DropPoint::ControlTransfer(e) => {
            UnitDropPoint::ControlTransfer(UnitExpressionId::new(root.source_unit(), e))
        }
        _ => return Err(failure("terminal unit original normal point", normal)),
    };
    let drops = owned
        .drops()
        .iter()
        .filter(|f| f.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    need(
        if input.owned {
            drops.len() == if returning { 2 } else { 1 }
                && drops.iter().all(|d| d.value_origin() == input.parameter)
                && drops.iter().filter(|d| d.point() == normal).count() == 1
                && (!returning || drops.iter().filter(|d| d.point() == transfer_point).count() == 1)
        } else {
            drops.is_empty()
        },
        "terminal unit exact source obligations",
        &drops,
    )?;
    need(p.exits().len() == 2, "terminal unit complete exits", p)?;
    for (kind, point, body) in [
        (
            if returning {
                UE::Return(transfer)
            } else {
                UE::Break(transfer)
            },
            transfer_point,
            true,
        ),
        (UE::Exhaustion, UnitDropPoint::LoopExit(statement), false),
    ] {
        let exit = one(
            p.exits()
                .iter()
                .filter(|e| e.kind() == kind && e.point() == point),
            "terminal unit actual exit",
        )?;
        let mut expected = owned
            .drops()
            .iter()
            .copied()
            .filter(|d| d.point() == point && d.target() != UnitDropTarget::Named(root))
            .map(UA::Drop)
            .collect::<Vec<_>>();
        need(
            expected.len() == if body { locals.len() } else { 0 },
            "terminal unit every actual loop-local cleanup",
            (&locals, &expected),
        )?;
        for action in &expected {
            let UA::Drop(drop) = action else {
                unreachable!()
            };
            need(
                body && drop.value_origin().source_id() == input.for_span.source_id()
                    && locals.iter().any(|(name, _)| *name == drop.value_origin())
                    && drop.value_origin().start() > input.for_span.start()
                    && drop.value_origin().end() < input.for_span.end()
                    && matches!(drop.target(), UnitDropTarget::Named(local) if local.source_unit()==root.source_unit()
                    && names.names().source_units().iter().any(|u| u.source_unit()==local.source_unit() && u.resolution().symbols().iter().any(|s| s.id()==local.symbol() && s.span()==drop.value_origin()))),
                "terminal unit actual loop-local named Drop",
                drop,
            )?;
        }
        if body {
            expected.extend([
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
        expected.extend([UA::FinishProvider(statement), UA::EndSource(statement)]);
        expected.extend(
            drops
                .iter()
                .filter(|d| d.point() == point)
                .map(|d| UA::Drop(**d)),
        );
        need(
            exit.actions() == expected
                && owned.iteration_cleanup_at(point) == Some(expected.as_slice()),
            "terminal unit complete cleanup with source after EndSource",
            (exit, expected),
        )?;
    }
    // Call ownership contracts retain the original source-qualified argument and mode.
    let calls = typed
        .calls()
        .iter()
        .filter(|c| c.target() == UnitCallTarget::Declaration(declaration.declaration()))
        .collect::<Vec<_>>();
    need(
        calls.len() == 2,
        "resource unit both real caller sites",
        &calls,
    )?;
    for call in calls {
        let node = parsed
            .ast()
            .expressions()
            .get(call.expression().expression())
            .map_err(|e| failure("resource unit caller AST", e))?;
        let Expression::Call { arguments, .. } = node.payload() else {
            return Err(failure("resource unit actual caller", node));
        };
        let [argument] = arguments.as_slice() else {
            return Err(failure("resource unit original arity", arguments));
        };
        let arg_id = UnitExpressionId::new(root.source_unit(), argument.value);
        let span = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .map_err(|e| failure("resource unit argument AST", e))?
            .span();
        let arg_root = one(
            names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == span)
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                }),
            "resource unit original caller root",
        )?;
        let contracts = owned
            .call_argument_contracts_for(call.expression())
            .collect::<Vec<_>>();
        let deliveries = owned
            .value_deliveries()
            .iter()
            .filter(|d| d.call() == call.expression())
            .collect::<Vec<_>>();
        let loans = owned
            .loans()
            .iter()
            .filter(|l| l.call() == call.expression())
            .collect::<Vec<_>>();
        need(
            call.return_type() == signature.return_type()
                && typed.expression_type(call.expression()) == Some(signature.return_type())
                && matches!(call.arguments(), [a] if a.mode() == mode && a.parameter_index() == 0 && a.argument_index() == 0 && Some(a.parameter_type()) == typed.expression_type(arg_id))
                && matches!(contracts.as_slice(), [c] if c.argument() == arg_id && c.parameter_index() == 0 && c.kind() == if input.owned { UK::Value } else { UK::SharedLoan })
                && if input.owned {
                    loans.is_empty()
                        && matches!(deliveries.as_slice(), [d] if d.argument() == arg_id && d.kind() == UnitValueDeliveryKind::Move && d.place().is_some_and(|p| p.is_root() && p.root() == arg_root))
                } else {
                    deliveries.is_empty()
                        && matches!(loans.as_slice(), [l] if l.argument() == arg_id && l.kind() == FLoanKind::Shared && matches!(l.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == arg_root))
                },
            "resource unit exact original caller move/reborrow identity",
            (call, arg_root, contracts, deliveries, loans),
        )?;
    }
    Ok(())
}
