//! Original AST/FE anchors plus independent SSA incoming and lexical drop identities.
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use crate::ssa::model::*;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{
        NameResolution, ReferenceTarget, SymbolKind, UnitReferenceTarget,
        ValidatedCompilationUnitNames,
    },
    ownership_checking::{
        CleanupCondition, CleanupOwnerValue, CleanupSelection, CleanupSelectorSource,
        CompilationUnitOwnership, DropPoint, DropTarget, IterationCleanupAction as A,
        IterationExitKind as E, IterationPhiBoundary as B, IterationPhiIncomingKind as K,
        LoanKind as FLoanKind, LoanTarget, MapIterationSourceAccess, OwnershipBindingKind,
        OwnershipCheckedFile, UnitCallArgumentOwnershipKind as UK, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as UA, UnitIterationExitKind as UE, UnitIterationSourceAccess,
        UnitLoanTarget, UnitValueDeliveryKind,
    },
    parser::{Expression, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
    type_checking::{
        CallableTarget, CompilationUnitTypes, ParameterMode, TypedFile, UnitCallTarget,
        UnitExpressionId, UnitStatementId,
    },
};

#[derive(Debug)]
pub(super) struct Input {
    pub owned: bool,
    pub scan: Span,
    pub parameter: Span,
    pub read: Span,
    pub marker: Option<Span>,
    pub statement: StatementId,
    pub source: ExpressionId,
    pub source_span: Span,
    pub scope: StatementId,
    pub body: StatementId,
    pub for_span: Span,
    pub continued: Option<ExpressionId>,
}

pub(super) fn ast(sources: &SourceMap, parsed: &ParsedFile, owned: bool) -> Result<Input, Failure> {
    let find = |wanted: &str| {
        one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                match node.payload() {
                    Item::Function {
                        name: NameMarker::Present(name),
                        form,
                        parameters,
                        ..
                    } if sources.slice(*name) == Ok(wanted) => {
                        Some((node.span(), form, parameters))
                    }
                    _ => None,
                }
            }),
            "resource original function root",
        )
    };
    let (scan, form, parameters) = find("scan")?;
    let [parameter] = parameters.as_slice() else {
        return Err(failure("resource original parameter arity", parameters));
    };
    let NameMarker::Present(parameter) = parameter.name else {
        return Err(failure("resource original parameter name", parameter));
    };
    let read = find("read")?.0;
    let scope = match form {
        FunctionForm::ImplicitUnitBlock(b)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(b),
            ..
        } => *b,
        _ => return Err(failure("resource actual callable block", form)),
    };
    let Statement::Block { elements } = parsed
        .ast()
        .statements()
        .get(scope)
        .map_err(|e| failure("resource scope", e))?
        .payload()
    else {
        return Err(failure("resource scope block", scope));
    };
    let statement = one(
        elements.iter().copied().filter(|id| {
            parsed
                .ast()
                .statements()
                .get(*id)
                .is_ok_and(|node| matches!(node.payload(), Statement::For { .. }))
        }),
        "one actual scanner For",
    )?;
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|e| failure("resource For", e))?;
    let Statement::For { source, body, .. } = node.payload() else {
        return Err(failure("resource actual For", node));
    };
    let marker = elements
        .iter()
        .skip_while(|id| **id != statement)
        .skip(1)
        .find_map(|id| {
            let node = parsed.ast().statements().get(*id).ok()?;
            let Statement::Expression { expression } = node.payload() else {
                return None;
            };
            parsed
                .ast()
                .expressions()
                .get(*expression)
                .ok()
                .map(|node| node.span())
        });
    let source_span = parsed
        .ast()
        .expressions()
        .get(*source)
        .map_err(|e| failure("resource source", e))?
        .span();
    need(
        sources.slice(source_span) == Ok("source"),
        "resource original source text",
        source_span,
    )?;
    let body_span = parsed.ast().statements().get(*body).unwrap().span();
    let continued = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            (matches!(node.payload(), Expression::Continue { .. })
                && node.span().source_id() == body_span.source_id()
                && node.span().start() > body_span.start()
                && node.span().end() < body_span.end())
            .then_some(id)
        })
        .collect::<Vec<_>>();
    need(continued.len() <= 1, "actual optional Continue", &continued)?;
    Ok(Input {
        owned,
        scan,
        parameter,
        read,
        marker,
        statement,
        source: *source,
        source_span,
        scope,
        body: *body,
        for_span: node.span(),
        continued: continued.first().copied(),
    })
}

pub(super) fn single(
    input: &Input,
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
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
                && p.loans()[i].kind() == FLoanKind::Shared,
            "resource single separate Shared components",
            p.loans(),
        )?;
    }
    let fallthrough = one(
        p.exits()
            .iter()
            .filter(|e| e.kind() == input.continued.map_or(E::Fallthrough, E::Continue)),
        "resource single fallthrough",
    )?;
    let exhausted = one(
        p.exits().iter().filter(|e| e.kind() == E::Exhaustion),
        "resource single exhaustion",
    )?;
    need(
        p.exits().len() == 2
            && fallthrough.point()
                == input.continued.map_or(
                    DropPoint::AfterStatement(input.body),
                    DropPoint::ControlTransfer,
                )
            && fallthrough.actions().ends_with(&[
                A::EndBinding {
                    statement: input.statement,
                    symbol: d.value_binding(),
                },
                A::EndBinding {
                    statement: input.statement,
                    symbol: d.key_binding(),
                },
            ])
            && exhausted.point() == DropPoint::LoopExit(input.statement)
            && exhausted.actions()
                == [
                    A::FinishProvider(input.statement),
                    A::EndSource(input.statement),
                ],
        "resource single exact cleanup, without component/source Drop",
        p.exits(),
    )?;
    let body_drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|f| f.point() == fallthrough.point())
        .map(A::Drop)
        .collect::<Vec<_>>();
    need(
        fallthrough.actions()[..fallthrough.actions().len() - 2] == body_drops,
        "single full body-local Drop prefix",
        fallthrough,
    )?;
    let drops = owned
        .drops()
        .iter()
        .filter(|f| f.target() == DropTarget::Named(root))
        .collect::<Vec<_>>();
    if input.owned {
        let [drop] = drops.as_slice() else {
            return Err(failure("resource single unique scope drop", drops));
        };
        let conditions = owned.cleanup_conditions();
        let h = one(
            p.closure_phis()
                .iter()
                .filter(|phi| phi.symbol() == root && phi.boundary() == B::Header),
            "resource single Header",
        )?;
        let x = one(
            p.closure_phis()
                .iter()
                .filter(|phi| phi.symbol() == root && phi.boundary() == B::Exit),
            "resource single Exit",
        )?;
        need(
            p.closure_phis().len() == 2
                && p.closure_phi_incomings().len() == 3
                && h.owner() != x.owner()
                && drop.point() == DropPoint::AfterStatement(input.scope)
                && drop.value_origin() == symbol.span()
                && drop.owner() == Some(x.owner())
                && drop.condition() == Some(x.availability_condition())
                && drop.capture_slot().is_none()
                && drop.instance_address().is_none(),
            "resource single real Exit owner lexical obligation",
            (h, x, drop),
        )?;
        for phi in [h, x] {
            let selector = conditions
                .selector(phi.availability_selector())
                .ok_or_else(|| failure("resource original selector", phi))?;
            need(
                matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi { statement, symbol, boundary, .. }) if *statement == input.statement && *symbol == root && *boundary == phi.boundary())
                    && matches!(selector.source(), CleanupSelectorSource::IterationPhi { statement, boundary, .. } if statement == input.statement && boundary == phi.boundary())
                    && selector.selection() == CleanupSelection::IterationPresence
                    && selector.branch_count() == 2
                    && matches!(conditions.get(phi.availability_condition()), Some(CleanupCondition::Choice { selector, branches }) if *selector == phi.availability_selector() && matches!(branches.as_slice(), [no, yes] if conditions.get(*no) == Some(&CleanupCondition::Never) && conditions.get(*yes) == Some(&CleanupCondition::Always))),
                "resource single public owner and complete Choice",
                (phi, selector),
            )?;
        }
        for (kind, phi, point) in [
            (K::Entry, h, DropPoint::AfterExpression(input.source)),
            (
                input.continued.map_or(K::Fallthrough, K::Continue),
                h,
                fallthrough.point(),
            ),
            (K::Exhaustion, x, exhausted.point()),
        ] {
            let edge = one(
                p.closure_phi_incomings()
                    .iter()
                    .filter(|e| e.kind() == kind),
                "resource single all incoming kinds",
            )?;
            let [binding] = edge.bindings() else {
                return Err(failure("resource single incoming binding", edge));
            };
            let [value] = binding.values() else {
                return Err(failure("resource single incoming value", binding));
            };
            let first = kind == K::Entry;
            need(
                edge.point() == point
                    && edge.boundary() == phi.boundary()
                    && conditions.get(edge.condition()) == Some(&CleanupCondition::Always)
                    && binding.target() == phi.owner()
                    && binding.availability_selector() == phi.availability_selector()
                    && value.condition() == binding.available_when()
                    && if first {
                        conditions.get(binding.available_when()) == Some(&CleanupCondition::Always)
                            && matches!(conditions.owner_value(value.source()), Some(CleanupOwnerValue::Parameter { symbol: s, origin }) if *s == root && *origin == symbol.span())
                    } else {
                        value.source() == h.owner()
                            && binding.available_when() == h.availability_condition()
                    },
                "resource single each complete incoming provenance",
                edge,
            )?;
        }
    } else {
        need(
            drops.is_empty(),
            "resource single Borrow source has no owner Drop",
            drops,
        )?;
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
            matches!(call.arguments(), [a] if a.mode() == mode && a.argument_index() == 0 && a.parameter_index() == 0 && Some(a.parameter_type()) == typed.expression_type(arg.value))
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
    let fallthrough = one(
        p.exits().iter().filter(|e| {
            e.kind()
                == input.continued.map_or(UE::Fallthrough, |e| {
                    UE::Continue(UnitExpressionId::new(root.source_unit(), e))
                })
        }),
        "resource unit fallthrough",
    )?;
    let exhausted = one(
        p.exits().iter().filter(|e| e.kind() == UE::Exhaustion),
        "resource unit exhaustion",
    )?;
    need(
        p.exits().len() == 2
            && fallthrough.point()
                == input.continued.map_or(
                    UnitDropPoint::AfterStatement(UnitStatementId::new(
                        root.source_unit(),
                        input.body,
                    )),
                    |e| {
                        UnitDropPoint::ControlTransfer(UnitExpressionId::new(root.source_unit(), e))
                    },
                )
            && fallthrough.actions().ends_with(&[
                UA::EndBinding {
                    statement,
                    symbol: d.value_binding(),
                },
                UA::EndBinding {
                    statement,
                    symbol: d.key_binding(),
                },
            ])
            && exhausted.point() == UnitDropPoint::LoopExit(statement)
            && exhausted.actions() == [UA::FinishProvider(statement), UA::EndSource(statement)],
        "resource unit exact cleanup without source/component Drop",
        p.exits(),
    )?;
    let body_drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|f| f.point() == fallthrough.point())
        .map(UA::Drop)
        .collect::<Vec<_>>();
    need(
        fallthrough.actions()[..fallthrough.actions().len() - 2] == body_drops,
        "unit full body-local Drop prefix",
        fallthrough,
    )?;
    let drops = owned
        .drops()
        .iter()
        .filter(|f| f.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    need(
        if input.owned {
            matches!(drops.as_slice(), [f] if f.point() == UnitDropPoint::AfterStatement(UnitStatementId::new(root.source_unit(), input.scope)) && f.value_origin() == input.parameter)
        } else {
            drops.is_empty()
        },
        "resource unit unique lexical drop or Borrow no drop",
        drops,
    )?;
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
            matches!(call.arguments(), [a] if a.mode() == mode && a.parameter_index() == 0 && a.argument_index() == 0 && Some(a.parameter_type()) == typed.expression_type(arg_id))
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

pub(super) fn ssa(program: &Program, input: &Input, unit: bool) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "resource SSA one module")?;
    let scan = function(module, input.scan)?;
    let g = Graph::new(scan)?;
    let op = |predicate: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| predicate(&i.operation)),
            "resource SSA unique operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let guard = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
    let Operation::MapIterationFirst { source: lease } = first.operation else {
        return Err(failure("resource first", first));
    };
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = guard.operation
    else {
        return Err(failure("resource guard", guard));
    };
    let Operation::MapIterationKeyLoan {
        source: bs,
        cursor: bc,
    } = key.operation
    else {
        return Err(failure("resource key", key));
    };
    let pre = first.block;
    let header = guard.block;
    let body = key.block;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(header)?.kind
    else {
        return Err(failure("resource guard edge", g.term(header)?));
    };
    need(
        *condition == value(guard)?
            && when_true.target == body
            && val.block == body
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                },
        "resource SSA actual guard and components",
        (guard, key, val),
    )?;
    let exhausted = when_false.target;
    need(
        pre == scan
            .entry_block()
            .ok_or_else(|| failure("resource scan entry", scan.id))?
            && g.blocks.len() == 4,
        "resource exact reachable normal CFG",
        &g.blocks,
    )?;
    g.incoming_from(pre, &[])?;
    g.incoming_from(header, &[pre, body])?;
    g.incoming_from(body, &[header])?;
    g.incoming_from(exhausted, &[header])?;
    let [seed] = g.block(pre)?.parameters.as_slice() else {
        return Err(failure("resource scanner actual parameter", g.block(pre)?));
    };
    need(
        matches!(
            (input.owned, seed),
            (true, EntityId::Value(_)) | (false, EntityId::Loan(_))
        ),
        "resource SSA caller storage role",
        seed,
    )?;
    let birth = g.definition(EntityId::Loan(lease))?;
    if input.owned {
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("resource owned lease", birth));
        };
        need(
            g.definition(EntityId::Place(place))?.operation
                == Operation::RootPlace {
                    owner: match seed {
                        EntityId::Value(v) => *v,
                        _ => return Err(failure("resource Value seed", seed)),
                    },
                },
            "resource lease root is original parameter",
            birth,
        )?;
    } else {
        need(
            matches!(birth.operation, Operation::SharedReborrow { source } if EntityId::Loan(source) == *seed),
            "resource caller reborrow protection",
            birth,
        )?;
    }
    let entry = g.branch(pre)?;
    let back = g.branch(body)?;
    let carries_seed = input.owned || unit;
    let (hseed, bseed, xseed) = if carries_seed {
        let h = g.transported(entry, *seed)?;
        (
            h,
            g.transported(when_true, h)?,
            g.transported(when_false, h)?,
        )
    } else {
        need(
            !g.edges
                .iter()
                .any(|(_, edge)| edge.arguments.contains(seed)),
            "single Borrow caller stays original dominating parameter",
            seed,
        )?;
        (*seed, *seed, *seed)
    };
    need(
        g.transported(entry, EntityId::Loan(lease))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc)
            && back.target == header
            && (!carries_seed || g.argument(back, hseed)? == bseed)
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && next.operation
                == Operation::MapIterationNext {
                    source: bs,
                    cursor: bc,
                }
            && g.argument(back, EntityId::Value(hc))? == result(next)?,
        "resource every incoming seed/lease/cursor column",
        (entry, when_true, when_false, back),
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let EntityId::Loan(xs) = xs else {
        return Err(failure("resource false lease", xs));
    };
    let source_end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: xs }),
        "resource exact EndSource",
    )?;
    let marker = one(
        g.instructions.iter().copied().filter(|i| {
            input
                .marker
                .is_some_and(|marker| i.origin == Origin::Source(marker))
                && matches!(i.operation, Operation::PrintString { .. })
        }),
        "resource real post-provider marker call",
    )?;
    need(
        source_end.block == exhausted
            && marker.block == exhausted
            && g.position(source_end)? < g.position(marker)?,
        "resource EndSource before real marker",
        (source_end, marker),
    )?;
    let roles = [*seed, hseed, bseed, xseed];
    let drops = g.instructions.iter().copied().filter(|i| matches!(i.operation, Operation::Drop { owner } if roles.contains(&EntityId::Value(owner)))).collect::<Vec<_>>();
    if input.owned {
        need(
            matches!(drops.as_slice(), [drop] if drop.block == exhausted && drop.origin == Origin::Source(input.parameter) && drop.operation == Operation::Drop { owner: match xseed { EntityId::Value(v) => v, _ => return Err(failure("resource false owner", xseed)) } } && g.position(drop)? > g.position(marker)?),
            "resource unique current exhaustion owner dropped after marker",
            &drops,
        )?;
    } else {
        need(drops.is_empty() && !g.instructions.iter().any(|i| matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == *seed)), "resource scanner protects caller loan", &drops)?;
    }
    let reader = function(module, input.read)?;
    let read = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == reader.id),
        ),
        "resource actual reader call",
    )?;
    need(
        matches!(&read.operation, Operation::DirectCall { arguments, .. } if arguments.as_slice() == [EntityId::Loan(loan(key)?), EntityId::Loan(loan(val)?)]),
        "resource K/V borrowed operand identities",
        read,
    )?;
    let cursor_exit = g.transported(when_false, EntityId::Value(hc))?;
    let cursors = [
        result(first)?,
        EntityId::Value(hc),
        EntityId::Value(bc),
        cursor_exit,
    ];
    for cursor in cursors {
        need(scan.entity(cursor).is_some_and(|e| matches!(e.ty, EntityType::Value(t) if module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 64, signed: false }))), "resource actual cursor storage", cursor)?;
    }
    let seed_type = scan
        .entity(*seed)
        .ok_or_else(|| failure("resource seed type", seed))?
        .ty
        .semantic_type();
    let (container_kind, kt, vt) = module
        .map_container(seed_type)
        .ok_or_else(|| failure("resource actual map storage", seed_type))?;
    need(
        container_kind
            == if input.owned {
                MapContainerKind::Map
            } else {
                MapContainerKind::MutableMap
            }
            && module.type_kind(kt) == Some(&SsaTypeKind::StringOwner)
            && module.deinits.contains_key(&vt)
            && scan.entity(result(key)?).is_some_and(|e| {
                e.ty == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: kt,
                }
            })
            && scan.entity(result(val)?).is_some_and(|e| {
                e.ty == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: vt,
                }
            })
            && loan(key)? != loan(val)?,
        "resource Map and two exact Shared storage types",
        (seed_type, kt, vt),
    )?;
    let ends = [key, val].map(|component| one(g.instructions.iter().copied().filter(|i| matches!(i.operation, Operation::BorrowEnd { loan: l } if component.results.as_slice() == [EntityId::Loan(l)])), "resource actual component end"));
    let [kend, vend] = ends;
    let kend = kend?;
    let vend = vend?;
    need(
        kend.block == body
            && vend.block == body
            && g.position(read)? < g.position(vend)?
            && g.position(vend)? < g.position(kend)?
            && g.position(kend)? < g.position(next)?,
        "resource read then reverse component end before Next",
        (read, vend, kend, next),
    )?;
    let mut uses = Uses::default();
    for role in roles
        .into_iter()
        .chain(cursors)
        .chain([result(key)?, result(val)?])
    {
        uses.protect(role);
    }
    uses.permit(EntityId::Value(hc), guard);
    for i in [key, val, next] {
        uses.permit(EntityId::Value(bc), i);
    }
    for component in [key, val] {
        uses.permit(result(component)?, read);
    }
    uses.permit(result(key)?, kend);
    uses.permit(result(val)?, vend);
    uses.permit(EntityId::Loan(lease), first);
    uses.permit(EntityId::Loan(hs), guard);
    uses.permit(EntityId::Loan(bs), key);
    uses.permit(EntityId::Loan(bs), val);
    uses.permit(EntityId::Loan(bs), next);
    uses.permit(EntityId::Loan(xs), source_end);
    // seed is used exactly by root/reborrow birth and the final owned drop.
    if input.owned {
        if let Operation::BorrowBegin { place, .. } = birth.operation {
            uses.permit(*seed, g.definition(EntityId::Place(place))?);
        }
    } else {
        uses.permit(*seed, birth);
    }
    for drop in drops {
        uses.permit(xseed, drop);
    }
    uses.check(&g, None)?;
    need(
        [first, guard, key, val, next]
            .iter()
            .all(|i| i.origin == Origin::Source(input.for_span)),
        "resource exact provider source origins",
        input.for_span,
    )?;
    Ok(())
}
