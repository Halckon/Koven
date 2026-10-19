//! One original analysis, public identities and unmodified cleanup facts.
mod deep;
mod provider;
mod unit;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{
    Case,
    ast::{self, Input},
};
pub(super) use deep::Proof;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::{NameResolution, ReferenceTarget, SymbolId},
    ownership_checking::{
        DropTarget, LoanEndPoint, LoanTarget, MapIterationSourceAccess, OwnershipBindingKind,
        OwnershipCheckedFile,
    },
    parser::ParsedFile,
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, CallableTarget, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, TypeKind, TypedFile,
    },
};
pub(super) use unit::check as unit;
fn symbol(n: &NameResolution, marker: Span) -> Result<SymbolId, Failure> {
    one(
        n.symbols()
            .iter()
            .filter(|s| s.span() == marker && !s.is_synthetic())
            .map(|s| s.id()),
        "D original symbol marker",
    )
}
fn reference(p: &ParsedFile, n: &NameResolution, e: ExpressionId) -> Result<SymbolId, Failure> {
    one(
        n.references()
            .iter()
            .filter(|r| r.span() == ast::span(p, e))
            .filter_map(|r| match r.target() {
                ReferenceTarget::Symbol(s) => Some(*s),
                _ => None,
            }),
        "D exact original Name reference",
    )
}
pub(super) fn single(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &NameResolution,
    t: &TypedFile,
    o: &OwnershipCheckedFile,
) -> Result<Proof, Failure> {
    let root = symbol(n, i.source)?;
    let ty = t
        .symbol_type(root)
        .ok_or_else(|| failure("D actual root type", root))?;
    println!(
        "SCOPE D FUTURE FE label={} unit=false root={root:?} type={ty:?} formed={:#?} drops={:#?} conditions={:#?} calls={:#?} loans={:#?} ends={:#?} plans={:#?} puts={:#?} strings={:#?}",
        c.label,
        o.formed_owned_locals(),
        o.drops(),
        o.cleanup_conditions(),
        t.calls(),
        o.loans(),
        o.loan_ends(),
        o.map_iterations(),
        t.map_puts(),
        o.string_effects()
    );
    for d in o.drops() {
        println!(
            "SCOPE D FUTURE DROP label={} unit=false target={:?} point={:?} origin={:?} owner={:?} condition={:?} value={:?} node={:?}",
            c.label,
            d.target(),
            d.point(),
            d.value_origin(),
            d.owner(),
            d.condition(),
            d.owner()
                .and_then(|v| o.cleanup_conditions().owner_value(v)),
            d.condition().and_then(|v| o.cleanup_conditions().get(v))
        );
    }
    need(
        matches!(t.types().get(ty),Some(TypeKind::Intrinsic{constructor,arguments})if *constructor==if c.code==21{IntrinsicTypeConstructor::MutableMap}else{IntrinsicTypeConstructor::Map}&&arguments.len()==2&&arguments.iter().all(|a|t.types().get(*a)==Some(&TypeKind::Builtin(BuiltinType::String)))),
        "D concrete original String Map storage",
        ty,
    )?;
    if c.code == 21 {
        need(
            o.formed_owned_locals().contains(&root),
            "D actual formed MutableMap local",
            root,
        )?;
    } else {
        need(
            t.parameter_mode(root) == Some(ParameterMode::Value)
                && o.binding_kind(root) == Some(OwnershipBindingKind::Owned),
            "D actual own Value source parameter",
            root,
        )?;
    }
    let factory = symbol(n, i.factory_name)?;
    let scan = symbol(n, i.scan_name)?;
    let observer = symbol(n, i.observer_name)?;
    for birth in [i.birth, i.entry_birth].into_iter().flatten() {
        let call = t
            .call(birth)
            .ok_or_else(|| failure("D actual factory Call", birth))?;
        need(
            call.target() == CallableTarget::Source(factory)
                && call.arguments().is_empty()
                && call.receiver().is_none()
                && call.result_source() == CallableResultSource::Owned
                && t.expression_type(birth) == Some(ty)
                && t.expression_category(birth) == Some(ExpressionCategory::Temporary),
            "D actual selected owned factory result",
            call,
        )?;
    }
    let call = t
        .call(i.scan_call)
        .ok_or_else(|| failure("D actual entry scan Call", i.scan_call))?;
    need(
        call.target() == CallableTarget::Source(scan)
            && call.receiver().is_none()
            && call.arguments().len() == usize::from(c.code != 21)
            && call.result_source() == CallableResultSource::Owned
            && if c.code == 19 {
                call.return_type() == ty
            } else {
                t.types().get(call.return_type()) == Some(&TypeKind::Builtin(BuiltinType::Unit))
            },
        "D actual scanner selected target/ABI",
        call,
    )?;
    if let Some(arg) = i.scan_argument {
        need(
            matches!(call.arguments(),[a]if a.mode()==ParameterMode::Value&&a.parameter_index()==0&&a.argument_index()==0&&a.parameter_type()==ty)
                && t.expression_type(arg) == Some(ty)
                && !o.loans().iter().any(|l| l.call() == i.scan_call),
            "D actual owned argument delivery has no Borrow substitute",
            call,
        )?;
    }
    if let Some((ret, value)) = i.returned {
        need(
            reference(p, n, value)? == root
                && t.expression_type(value) == Some(ty)
                && !o
                    .drops()
                    .iter()
                    .any(|d| d.target() == DropTarget::Named(root)),
            "D actual Return consumes own source without callee Drop",
            (ret, value),
        )?;
    }
    if let Some((marker, initializer)) = i.successor {
        let successor = symbol(n, marker)?;
        need(
            reference(p, n, initializer)? == root
                && t.expression_type(initializer) == Some(ty)
                && o.formed_owned_locals().contains(&successor)
                && !o
                    .drops()
                    .iter()
                    .any(|d| d.target() == DropTarget::Named(root)),
            "D successor MoveName transfers exact original own parameter",
            (successor, initializer),
        )?;
        one(
            o.drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Named(successor)),
            "D actual successor sole Drop",
        )?;
    }
    if c.code == 21 {
        one(
            o.drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Named(root)),
            "D actual updated MutableMap sole named Drop",
        )?;
    }
    let mut observes = i
        .observes
        .iter()
        .enumerate()
        .map(|(index, &(a, b))| {
            let expected = if c.code == 20 && (c.provider || index == 1) {
                symbol(
                    n,
                    i.successor.ok_or_else(|| failure("D successor AST", i))?.0,
                )?
            } else {
                root
            };
            need(
                reference(p, n, b)? == expected,
                "D actual future observe Name identity",
                b,
            )?;
            Ok((a, b, expected))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    if let Some((a, b)) = i.entry_observe {
        let marker = i.result.ok_or_else(|| failure("D result marker", i))?;
        let result = symbol(n, marker)?;
        need(
            reference(p, n, b)? == result && o.formed_owned_locals().contains(&result),
            "D caller binds actual returned Map",
            result,
        )?;
        one(
            o.drops()
                .iter()
                .filter(|d| d.target() == DropTarget::Named(result)),
            "D caller result sole named Drop",
        )?;
        observes.push((a, b, result));
    }
    for (call, arg, actualroot) in observes {
        let d = t
            .call(call)
            .ok_or_else(|| failure("D actual observe Call", call))?;
        let loan = one(
            o.loans()
                .iter()
                .filter(|l| l.call() == call && l.argument() == arg),
            "D original observe loan",
        )?;
        let end = one(
            o.loan_ends()
                .iter()
                .filter(|l| l.call() == call && l.argument() == arg),
            "D original observe endpoint",
        )?;
        need(
            d.target() == CallableTarget::Source(observer)
                && matches!(d.arguments(),[a]if a.mode()==ParameterMode::Borrow&&a.parameter_index()==0&&a.argument_index()==0&&a.parameter_type()==ty)
                && matches!(loan.target(),LoanTarget::Place(path)if path.is_root()&&path.root()==actualroot)
                && loan.begin_span() == ast::span(p, arg)
                && loan.end_span() == ast::span(p, call)
                && end.point() == LoanEndPoint::CallReturn(call),
            "D actual current owner observe exact loan/CallReturn",
            (d, loan, end),
        )?;
    }
    if let Some(put) = i.put {
        let d = t
            .map_put(put)
            .ok_or_else(|| failure("D actual future put", put))?;
        need(
            reference(p, n, d.receiver())? == root
                && [d.key(), d.value()]
                    .iter()
                    .all(|e| o.string_effect(*e).is_some()),
            "D actual put current root and independent clone owner effects",
            d,
        )?;
    }
    need(
        t.map_iterations().len() == usize::from(c.provider)
            && o.map_iterations().len() == usize::from(c.provider),
        "D exact acquired original plan count",
        o.map_iterations(),
    )?;
    if let Some((statement, source)) = i.site {
        let plan = o
            .map_iteration(statement)
            .ok_or_else(|| failure("D actual provider plan", statement))?;
        need(
            t.map_iteration(statement) == Some(plan.descriptor())
                && plan.descriptor().source() == source
                && plan.descriptor().source_type() == ty
                && reference(p, n, source)? == root
                && matches!(plan.source(),Some(LoanTarget::Place(path))if path.is_root()&&path.root()==root)
                && plan.source_access() == MapIterationSourceAccess::Owned
                && plan.loans().len() == 2
                && !plan.exits().is_empty(),
            "D original complete owned source provider",
            plan,
        )?;
    }
    if c.provider {
        provider::single(p, i, n, t, o)?;
    }
    deep::single(c, p, i, n, o)
}
