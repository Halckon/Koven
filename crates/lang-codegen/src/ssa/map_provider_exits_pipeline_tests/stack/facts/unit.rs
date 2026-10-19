//! Qualified FE identities copied only from the callback's original compilation products.
use super::*;
use lang_frontend::{
    name_resolution::{SymbolKind, UnitReferenceTarget, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, LoanKind, MapIterationComponent, OwnershipBindingKind,
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction as A, UnitIterationExitKind as E,
        UnitIterationSourceAccess, UnitLoanTarget,
    },
    type_checking::{
        CompilationUnitTypes, ExpressionCategory, IntrinsicTypeConstructor, MapIterationKind,
        ParameterMode, UnitCallTarget, UnitExpressionId, UnitStatementId, UnitTypeKind,
    },
};

pub(in crate::ssa::map_provider_exits_pipeline_tests::stack) fn collect(
    _sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Inputs, Failure> {
    let anchors = ast(parsed)?;
    let source_names = one(
        names.names().source_units().iter(),
        "stack original sole qualified source",
    )?;
    let unit = source_names.source_unit();
    need(
        source_names.resolution().source_id() == parsed.source_id()
            && typed.map_iterations().len() == anchors.len()
            && owned.map_iterations().len() == anchors.len(),
        "stack same qualified source and complete For plans",
        (&anchors, typed.map_iterations(), owned.map_iterations()),
    )?;
    let mut sites = Vec::new();
    let mut cleanups = Vec::new();
    for (index, s) in anchors.iter().enumerate() {
        let statement = UnitStatementId::new(unit, s.statement);
        let source_id = UnitExpressionId::new(unit, s.source);
        let d = typed
            .map_iteration(statement)
            .ok_or_else(|| failure("stack qualified typed For", statement))?;
        let p = owned
            .map_iteration(statement)
            .ok_or_else(|| failure("stack qualified owned For", statement))?;
        need(
            d.statement() == statement
                && d.source() == source_id
                && p.descriptor() == d
                && typed.expression_type(source_id) == Some(d.source_type())
                && matches!(typed.types().get(d.source_type()),Some(UnitTypeKind::Intrinsic{constructor,arguments})
                if *constructor==if d.kind()==MapIterationKind::Map {IntrinsicTypeConstructor::Map}else{IntrinsicTypeConstructor::MutableMap}
                && arguments.as_slice()==[d.key_type(),d.value_type()]),
            "stack qualified exact source/K/V intrinsic descriptor",
            (s, d, p),
        )?;
        need(
            typed.types().get(d.key_type())
                == Some(&UnitTypeKind::Builtin(
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
                        .names()
                        .references()
                        .iter()
                        .filter(|r| r.span() == node.span()),
                    "stack qualified source reference",
                )?;
                let UnitReferenceTarget::Symbol(root) = reference.target() else {
                    return Err(failure("stack qualified actual root", reference));
                };
                let symbol = one(
                    source_names
                        .resolution()
                        .symbols()
                        .iter()
                        .filter(|v| v.id() == root.symbol()),
                    "stack qualified original symbol",
                )?;
                let slot = parameter(parsed, s.function, symbol.span())?;
                let callable = one(
                    typed
                        .signatures()
                        .declarations()
                        .iter()
                        .filter_map(|v| v.callable())
                        .filter(|c| {
                            c.parameters()
                                .get(slot)
                                .is_some_and(|p| p.symbol() == Some(*root))
                        }),
                    "stack qualified exact parameter signature",
                )?;
                let actual = &callable.parameters()[slot];
                let borrow = actual.mode() == ParameterMode::Borrow;
                need(
                    root.source_unit() == unit
                        && symbol.kind() == SymbolKind::ValueParameter
                        && actual.ty() == d.source_type()
                        && typed.symbol_type(*root) == Some(d.source_type())
                        && typed.expression_category(source_id) == Some(ExpressionCategory::Place)
                        && matches!(actual.mode(), ParameterMode::Borrow | ParameterMode::Value)
                        && owned.binding_kind(*root)
                            == Some(if borrow {
                                OwnershipBindingKind::Shared
                            } else {
                                OwnershipBindingKind::Owned
                            })
                        && p.source_access()
                            == if borrow {
                                UnitIterationSourceAccess::Shared
                            } else {
                                UnitIterationSourceAccess::Owned
                            }
                        && matches!(p.source(),Some(UnitLoanTarget::Place(path)) if path.is_root() && path.root()==*root),
                    "stack qualified source parameter/root/mode/slot",
                    (s, actual, p),
                )?;
                if borrow {
                    need(
                        !owned
                            .drops()
                            .iter()
                            .any(|drop| drop.target() == UnitDropTarget::Named(*root)),
                        "stack qualified Borrow root has no owner Drop",
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
                    .call(source_id)
                    .ok_or_else(|| failure("stack qualified temporary Call", source_id))?;
                let UnitCallTarget::Declaration(target) = call.target() else {
                    return Err(failure("stack qualified actual selected callee", call));
                };
                let declaration = one(
                    names
                        .names()
                        .index()
                        .declarations()
                        .iter()
                        .filter(|v| v.id() == target && v.source_unit() == unit),
                    "stack qualified source callable",
                )?;
                let callee = parsed.ast().items().get(declaration.root()).unwrap().span();
                need(
                    call.receiver().is_none()
                        && call.arguments().len() == arguments.len()
                        && call.return_type() == d.source_type()
                        && p.source_access() == UnitIterationSourceAccess::Temporary
                        && p.source() == Some(&UnitLoanTarget::Temporary(source_id)),
                    "stack qualified one original source evaluation",
                    (call, p),
                )?;
                Source::Call {
                    span: node.span(),
                    callee,
                    arity: arguments.len(),
                }
            }
            _ => return Err(failure("stack qualified original source shape", node)),
        };
        for (component, role) in [MapIterationComponent::Key, MapIterationComponent::Value]
            .into_iter()
            .zip(0..2)
        {
            let binding = d.bindings()[role];
            let loan = p.loan(component);
            let symbol = one(
                source_names
                    .resolution()
                    .symbols()
                    .iter()
                    .filter(|v| v.id() == binding.symbol()),
                "stack qualified actual component symbol",
            )?;
            need(
                binding.source_unit() == unit
                    && symbol.span() == s.names[role]
                    && symbol.kind() == SymbolKind::ForBinding
                    && typed.symbol_type(binding) == Some([d.key_type(), d.value_type()][role])
                    && owned.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && p.bindings()[role].symbol() == binding
                    && p.bindings()[role].kind() == OwnershipBindingKind::Shared
                    && loan == &p.loans()[role]
                    && loan.statement() == statement
                    && loan.source() == source_id
                    && loan.binding() == binding
                    && loan.component() == component
                    && loan.kind() == LoanKind::Shared
                    && Some(loan.origin()) == p.source(),
                "stack qualified independent actual K/V loans",
                (s, symbol, loan),
            )?;
        }
        let expected = exits(parsed, &anchors, index);
        let mut owner_markers = Vec::new();
        need(
            p.exits().len() == expected.len(),
            "stack qualified complete per-layer exits",
            p.exits(),
        )?;
        for exit in p.exits() {
            let (kind, point) = match exit.kind() {
                E::Fallthrough => (0, s.body_span),
                E::Exhaustion => (1, s.span),
                E::Return(id) => (
                    2,
                    parsed
                        .ast()
                        .expressions()
                        .get(id.expression())
                        .unwrap()
                        .span(),
                ),
                E::Break(id) => (
                    3,
                    parsed
                        .ast()
                        .expressions()
                        .get(id.expression())
                        .unwrap()
                        .span(),
                ),
                _ => return Err(failure("stack qualified original exit kind", exit)),
            };
            need(
                expected.contains(&(kind, point)) && point_span(parsed, exit.point())? == point,
                "stack qualified precise original exit point",
                exit,
            )?;
            let complete = owned
                .iteration_cleanup_at(exit.point())
                .ok_or_else(|| failure("stack qualified full cleanup table", exit))?;
            need(
                complete == exit.actions(),
                "stack qualified complete same-point action identity",
                (exit, complete),
            )?;
            let mut actions = Vec::new();
            for action in complete {
                actions.push(match action {
                    A::EndBinding { statement, symbol } => {
                        let i = layer(&anchors, statement.statement())?;
                        let dd = typed.map_iteration(*statement).unwrap();
                        if *symbol == dd.value_binding() {
                            Action::Value(i)
                        } else {
                            need(
                                *symbol == dd.key_binding(),
                                "stack qualified exact EndBinding K/V",
                                action,
                            )?;
                            Action::Key(i)
                        }
                    }
                    A::FinishProvider(statement) => {
                        Action::Finish(layer(&anchors, statement.statement())?)
                    }
                    A::EndSource(statement) => Action::End(layer(&anchors, statement.statement())?),
                    A::Drop(drop) => {
                        need(
                            owned.drops().contains(drop) && drop.point() == exit.point(),
                            "stack qualified same original owner Drop",
                            drop,
                        )?;
                        if let UnitDropTarget::Named(root) = drop.target() {
                            let marker = one(
                                source_names
                                    .resolution()
                                    .symbols()
                                    .iter()
                                    .filter(|n| n.id() == root.symbol()),
                                "stack qualified cleanup owner original marker",
                            )?
                            .span();
                            if !owner_markers.contains(&marker) {
                                owner_markers.push(marker);
                            }
                        }
                        Action::Drop(drop.value_origin())
                    }
                    _ => return Err(failure("stack qualified unexpected cleanup action", action)),
                });
            }
            validate_order(&actions)?;
            need(
                actions.contains(&if kind == 1 {
                    Action::End(index)
                } else {
                    Action::Value(index)
                }),
                "stack qualified own cleanup is present",
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
            representation: representation::unit(parsed, names, typed, d.value_type())?,
        });
    }
    let source_drops = source_drops::unit(parsed, names, owned, &sites)?;
    let calls = calls(parsed, names, typed, owned)?;
    let guards = representation::unit_guards(parsed, names, typed, owned, &anchors)?;
    let name_inputs = names_snapshot(parsed, names)?;
    Ok(Inputs {
        sites,
        cleanups,
        calls,
        guards,
        names: name_inputs,
        source_drops,
    })
}

fn point_span(parsed: &ParsedFile, point: UnitDropPoint) -> Result<Span, Failure> {
    match point {
        UnitDropPoint::AfterStatement(s) | UnitDropPoint::LoopExit(s) => {
            Ok(parsed.ast().statements().get(s.statement()).unwrap().span())
        }
        UnitDropPoint::ControlTransfer(e) => Ok(parsed
            .ast()
            .expressions()
            .get(e.expression())
            .unwrap()
            .span()),
        _ => Err(failure("stack qualified actual cleanup point", point)),
    }
}

fn calls(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<Vec<Call>, Failure> {
    use lang_frontend::ownership_checking::UnitCallArgumentOwnershipKind as Kind;
    let source = one(
        names.names().source_units().iter(),
        "stack Call actual qualified unit",
    )?;
    let mut result = Vec::new();
    for (id, node) in parsed.ast().expressions().iter() {
        let Expression::Call { arguments, .. } = node.payload() else {
            continue;
        };
        let id = UnitExpressionId::new(source.source_unit(), id);
        let Some(call) = typed.call(id) else {
            continue;
        };
        let UnitCallTarget::Declaration(target) = call.target() else {
            continue;
        };
        let declaration = one(
            names
                .names()
                .index()
                .declarations()
                .iter()
                .filter(|d| d.id() == target),
            "stack Call actual qualified declaration",
        )?;
        need(
            call.receiver().is_none() && call.arguments().len() == arguments.len(),
            "stack qualified Call original arity",
            call,
        )?;
        let mut args = Vec::new();
        let mut loans = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            let n = parsed.ast().expressions().get(argument.value).unwrap();
            let actual = &call.arguments()[index];
            need(
                actual.argument_index() == index
                    && actual.parameter_index() == index
                    && typed.expression_type(UnitExpressionId::new(
                        source.source_unit(),
                        argument.value,
                    )) == Some(actual.parameter_type()),
                "stack qualified original Call slot",
                (call, argument),
            )?;
            let roots = names
                .names()
                .references()
                .iter()
                .filter(|r| r.span() == n.span())
                .filter_map(|r| match r.target() {
                    UnitReferenceTarget::Symbol(s) => Some(*s),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let root = if matches!(n.payload(), Expression::Name) {
                Some(one(roots.into_iter(), "stack qualified Call Name root")?)
            } else {
                None
            };
            let marker = root
                .map(|root| {
                    one(
                        source
                            .resolution()
                            .symbols()
                            .iter()
                            .filter(|s| s.id() == root.symbol()),
                        "stack qualified Call root marker",
                    )
                    .map(|s| s.span())
                })
                .transpose()?;
            let contract = one(
                owned.call_argument_contracts_for(id).filter(|c| {
                    c.argument() == UnitExpressionId::new(source.source_unit(), argument.value)
                }),
                "stack qualified original argument ownership",
            )?;
            need(
                contract.parameter_index() == index
                    && contract.parameter_type() == actual.parameter_type()
                    && contract.argument_span() == n.span()
                    && contract.call_span() == node.span()
                    && contract.kind()
                        == if actual.mode() == ParameterMode::Borrow {
                            Kind::SharedLoan
                        } else {
                            Kind::Value
                        },
                "stack qualified precise original Borrow/Value argument contract",
                contract,
            )?;
            let endpoint = if actual.mode() == ParameterMode::Borrow {
                need(
                    contract.loan_begin_span() == n.span(),
                    "stack qualified real argument Borrow start",
                    contract,
                )?;
                let actual_loans = owned
                    .loans()
                    .iter()
                    .filter(|loan| loan.call() == id && loan.argument() == contract.argument())
                    .collect::<Vec<_>>();
                need(
                    actual_loans.len() <= 1,
                    "stack qualified original argument has at most one published loan",
                    &actual_loans,
                )?;
                if let Some(loan) = actual_loans.first() {
                    need(loan.kind()==LoanKind::Shared&&loan.begin_span()==contract.loan_begin_span()&&loan.end_span()==contract.call_span()
                        &&root.is_none_or(|root|matches!(loan.target(),UnitLoanTarget::Place(p) if p.is_root()&&p.root()==root)),
                        "stack qualified published loan agrees with original synchronous contract",loan)?;
                }
                Some(CallLoan {
                    begin: contract.loan_begin_span(),
                    end: contract.call_span(),
                })
            } else {
                None
            };
            loans.push(endpoint);
            args.push((n.span(), marker, actual.mode() == ParameterMode::Borrow));
        }
        result.push(Call {
            span: node.span(),
            callee: parsed.ast().items().get(declaration.root()).unwrap().span(),
            arguments: args,
            loans,
        });
    }
    Ok(result)
}

fn names_snapshot(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
) -> Result<Vec<NameInput>, Failure> {
    let source = one(
        names.names().source_units().iter(),
        "stack qualified Name source",
    )?;
    let mut result = Vec::new();
    for (_, n) in parsed.ast().expressions().iter() {
        if !matches!(n.payload(), Expression::Name) {
            continue;
        }
        let roots = names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == n.span())
            .filter_map(|r| match r.target() {
                UnitReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            })
            .collect::<Vec<_>>();
        if roots.is_empty() {
            continue;
        }
        let root = one(
            roots.into_iter(),
            "stack qualified original Name selected symbol",
        )?;
        need(
            root.source_unit() == source.source_unit(),
            "stack qualified Name original unit",
            root,
        )?;
        let symbol = one(
            source
                .resolution()
                .symbols()
                .iter()
                .filter(|s| s.id() == root.symbol()),
            "stack qualified Name declaration marker",
        )?;
        result.push(NameInput {
            span: n.span(),
            marker: symbol.span(),
        });
    }
    Ok(result)
}
