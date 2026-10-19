//! Original concrete Int facts and exact current owner/drop facts from one analysis.
mod unit;
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
    scope_lambda_body::facts::owner,
};
use super::{Case, ast};
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
#[derive(Debug)]
pub(super) struct Proof {
    pub drop: Span,
}
type Input<'a> = (&'a Case, &'a SourceMap, &'a ParsedFile, &'a ast::Input);
fn root(n: &NameResolution, span: Span) -> Result<SymbolId, Failure> {
    one(
        n.symbols()
            .iter()
            .filter(|s| s.span() == span && !s.is_synthetic()),
        "Int actual original symbol",
    )
    .map(|s| s.id())
}
fn receiver(
    p: &ParsedFile,
    id: lang_frontend::ast::ExpressionId,
) -> lang_frontend::ast::ExpressionId {
    let Expression::Call { callee, .. } = p.ast().expressions().get(id).unwrap().payload() else {
        unreachable!()
    };
    let Expression::Member { receiver, .. } = p.ast().expressions().get(*callee).unwrap().payload()
    else {
        unreachable!()
    };
    *receiver
}
pub(super) fn single(
    input: Input<'_>,
    n: &NameResolution,
    t: &TypedFile,
    o: &OwnershipCheckedFile,
) -> Result<Proof, Failure> {
    let (c, _, p, i) = input;
    let initial = root(n, i.initial)?;
    let source = root(n, i.source)?;
    let constructed = t
        .map_construction(i.construction)
        .ok_or_else(|| failure("Int actual typed mutableMapOf", i.construction))?;
    let put = t
        .map_put(i.put)
        .ok_or_else(|| failure("Int actual typed put", i.put))?;
    let consumed = t
        .map_consume(i.consume)
        .ok_or_else(|| failure("Int actual typed consume", i.consume))?;
    let int = constructed.key_type();
    let mutable = constructed.map_type();
    let map = consumed.result_type();
    need(
        constructed.expression() == i.construction
            && constructed.callable() == IntrinsicCallable::MutableMapOf
            && constructed.value_type() == int
            && t.types().get(int) == Some(&TypeKind::Builtin(BuiltinType::Int))
            && t.copyability(int) == Some(Copyability::Copyable)
            && t.symbol_type(initial) == Some(mutable)
            && o.formed_owned_locals().contains(&initial)
            && put.expression() == i.put
            && put.receiver() == receiver(p, i.put)
            && put.key() == i.payload[0]
            && put.value() == i.payload[1]
            && put.key_type() == int
            && put.value_type() == int
            && i.payload
                .iter()
                .all(|id| t.expression_type(*id) == Some(int))
            && consumed.expression() == i.consume
            && consumed.receiver() == receiver(p, i.consume)
            && consumed.receiver_type() == mutable
            && consumed.source() == Some((initial, mutable))
            && consumed.key_type() == int
            && consumed.value_type() == int
            && t.expression_type(i.consume) == Some(map)
            && t.types().get(map)
                == Some(&TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Map,
                    arguments: vec![int, int],
                }),
        "Int original typed construction/put scalar payloads/consume concrete Copy storage",
        (constructed, put, consumed),
    )?;
    for id in [put.receiver(), consumed.receiver()] {
        let r = one(
            n.references()
                .iter()
                .filter(|r| r.span() == ast::span(p, id)),
            "Int actual factory receiver root",
        )?;
        need(
            r.target() == &ReferenceTarget::Symbol(initial),
            "Int put/consume actual same m owner root",
            r,
        )?;
    }
    need(
        !o.drops().iter().any(|d| {
            d.target() == DropTarget::Named(initial)
                || d.target() == DropTarget::Temporary(i.consume)
        }),
        "Int original consumed owner and Return have no duplicate Drop",
        o.drops(),
    )?;
    let call = t
        .call(i.birth)
        .ok_or_else(|| failure("Int actual makeInt Call", i.birth))?;
    need(
        call.target() == CallableTarget::Source(root(n, i.make_name)?)
            && call.arguments().is_empty()
            && call.return_type() == map
            && call.result_source() == CallableResultSource::Owned
            && call.borrow_return().is_none()
            && t.symbol_type(source) == Some(map)
            && o.formed_owned_locals().contains(&source)
            && !o
                .drops()
                .iter()
                .any(|d| d.target() == DropTarget::Temporary(i.birth)),
        "Int actual named source owns exact concrete makeInt return",
        call,
    )?;
    let reader = one(
        t.callables()
            .iter()
            .filter(|s| s.symbol() == root(n, i.reader_name).unwrap()),
        "Int actual reader signature",
    )?;
    need(
        reader.parameters().len() == 2
            && reader
                .parameters()
                .iter()
                .all(|a| a.mode == ParameterMode::Borrow && a.ty == int)
            && reader.parameter_symbols()
                == [
                    Some(root(n, i.reader_params[0])?),
                    Some(root(n, i.reader_params[1])?),
                ],
        "Int reader true declared ABI and Copy target types",
        reader,
    )?;
    for index in 0..2 {
        let r = one(
            n.references()
                .iter()
                .filter(|r| r.span() == ast::span(p, i.compared[index])),
            "Int actual compared parameter",
        )?;
        need(
            r.target() == &ReferenceTarget::Symbol(root(n, i.reader_params[index])?)
                && t.expression_type(i.compared[index]) == Some(int)
                && t.expression_type(i.literals[index]) == Some(int)
                && t.expression_type(i.compares[index]).is_some_and(|ty| {
                    t.types().get(ty) == Some(&TypeKind::Builtin(BuiltinType::Boolean))
                }),
            "Int original key==1/value==2 actual typed comparison",
            r,
        )?;
    }
    let scanner = t
        .call(i.scanner_call)
        .ok_or_else(|| failure("Int original scanner entry", i.scanner_call))?;
    need(
        scanner.target() == CallableTarget::Source(root(n, i.scan_name)?)
            && scanner.arguments().is_empty(),
        "Int actual zero-argument scanner call",
        scanner,
    )?;
    let drop = one(
        o.drops()
            .iter()
            .filter(|d| d.target() == DropTarget::Named(source)),
        "Int unique current source Drop",
    )?;
    need(
        drop.value_origin() == i.source
            && drop.point()
                == if let Some(site) = &i.site {
                    DropPoint::LoopExit(site.statement)
                } else {
                    DropPoint::CallReturn(i.size.unwrap())
                },
        "Int exact original source cleanup point",
        drop,
    )?;
    need(
        t.map_iterations().len() == usize::from(c.provider)
            && o.map_iterations().len() == usize::from(c.provider),
        "Int exact acquired provider set",
        o.map_iterations(),
    )?;
    if let Some(site) = &i.site {
        let plan = o
            .map_iteration(site.statement)
            .ok_or_else(|| failure("Int actual For plan", site))?;
        let d = plan.descriptor();
        let r = one(
            n.references()
                .iter()
                .filter(|r| r.span() == ast::span(p, site.source)),
            "Int original For source Name",
        )?;
        need(
            t.map_iteration(site.statement) == Some(d)
                && d.source() == site.source
                && d.source_type() == map
                && d.key_type() == int
                && d.value_type() == int
                && r.target() == &ReferenceTarget::Symbol(source)
                && plan.source_access() == MapIterationSourceAccess::Owned
                && matches!(plan.source(),Some(LoanTarget::Place(path))if path.is_root()&&path.root()==source),
            "Int actual owned current source and ordered K/V Int descriptor",
            plan,
        )?;
        owner::single(
            &owner::Input {
                site: Some(site),
                birth: i.birth,
                birth_span: ast::span(p, i.birth),
            },
            source,
            plan,
            o,
            drop,
        )?;
        let read = t
            .call(site.read)
            .ok_or_else(|| failure("Int actual reader Call", site.read))?;
        need(
            read.target() == CallableTarget::Source(root(n, i.reader_name)?)
                && read.arguments().len() == 2,
            "Int selected actual reader",
            read,
        )?;
        for index in 0..2 {
            let binding = root(n, site.bindings[index])?;
            let component = &plan.loans()[index];
            let loan = one(
                o.loans()
                    .iter()
                    .filter(|l| l.call() == site.read && l.argument() == site.arguments[index]),
                "Int actual component call loan",
            )?;
            let end = one(
                o.loan_ends()
                    .iter()
                    .filter(|l| l.call() == site.read && l.argument() == site.arguments[index]),
                "Int actual component loan endpoint",
            )?;
            need(
                d.bindings()[index] == binding
                    && o.binding_kind(binding) == Some(OwnershipBindingKind::Shared)
                    && component.binding() == binding
                    && component.statement() == site.statement
                    && component.source() == site.source
                    && Some(component.origin()) == plan.source()
                    && component.kind() == LoanKind::Shared
                    && read.arguments()[index].argument_index() == index
                    && read.arguments()[index].parameter_index() == index
                    && read.arguments()[index].mode() == ParameterMode::Borrow
                    && read.arguments()[index].parameter_type() == int
                    && matches!(loan.target(),LoanTarget::Place(path)if path.is_root()&&path.root()==binding)
                    && loan.kind() == LoanKind::Shared
                    && end.point() == LoanEndPoint::CallReturn(site.read),
                "Int exact Shared projection and declared reader delivery/end",
                (component, loan, end),
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
                o.drops()
                    .iter()
                    .filter(|d| d.point() == point)
                    .copied()
                    .map(IterationCleanupAction::Drop),
            );
            let exit = one(
                plan.exits().iter().filter(|e| e.kind() == kind),
                "Int actual complete normal exit",
            )?;
            need(
                plan.exits().len() == 2
                    && exit.point() == point
                    && exit.actions() == actions
                    && o.cleanup_conditions().get(exit.condition())
                        == Some(&CleanupCondition::Always)
                    && o.cleanup_steps()
                        .iter()
                        .filter_map(|(at, a)| (*at == point).then_some(*a))
                        .collect::<Vec<_>>()
                        == actions,
                "Int complete actual provider cleanup order",
                exit,
            )?;
        }
    } else {
        let size = t
            .map_size(i.size.unwrap())
            .ok_or_else(|| failure("Int actual no-For MapSize", i.size))?;
        need(
            size.map_type() == map
                && t.expression_type(size.receiver()) == Some(map)
                && matches!(drop.owner().and_then(|id|o.cleanup_conditions().owner_value(id)),Some(CleanupOwnerValue::Expression{expression,origin})if *expression==i.birth&&*origin==ast::span(p,i.birth))
                && drop.condition().is_none(),
            "Int no-For actual MapSize and exact source birth cleanup",
            size,
        )?;
    }
    Ok(Proof {
        drop: drop.value_origin(),
    })
}
pub(super) use unit::check as unit;
