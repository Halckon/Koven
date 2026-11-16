//! Read-only fixture evidence from the exact validated unit used by ordinary lowering.
//! Not admission: BoundLoop still rejects nesting; the caller binds a separate BoundNestedLoop.
use super::*;
use lang_frontend::{
    ownership_checking::{UnitDropPoint, UnitDropTarget, UnitLoanTarget},
    parser::{Expression, Item, NameMarker, Statement},
    type_checking::{BuiltinType, UnitExpressionId, UnitStatementId, UnitTypeKind},
};

pub(super) struct Facts {
    pub(super) callable: Origin,
    pub(super) controls: [Origin; 2],
    pub(super) conditions: [Origin; 2],
    pub(super) steps: [Origin; 2],
    pub(super) reads: [Origin; 2],
    pub(super) literals: [(Origin, Vec<u8>); 3],
    pub(super) drops: [Origin; 3],  // old, replaced, retained
    pub(super) borrow: [Origin; 2], // replace, suffix
    pub(super) calls: [Origin; 2],
}
pub(super) fn facts(unit: &OwnedCompilationUnitView<'_, '_>) -> Option<Facts> {
    let [input] = unit.inputs() else {
        return None;
    };
    let ast = input.parsed().ast();
    if ast.items().iter().count() != 6
        || ast.statements().iter().count() != 13
        || ast.expressions().iter().count() != 23
    {
        return None;
    }
    let [names] = unit.names().names().source_units() else {
        return None;
    };
    let source = names.source_unit();
    let expression = |text: &str| {
        let matches = ast
            .expressions()
            .iter()
            .filter(|(_, n)| unit.sources().slice(n.span()).ok() == Some(text))
            .collect::<Vec<_>>();
        let [(id, node)] = matches.as_slice() else {
            return None;
        };
        Some((
            UnitExpressionId::new(source, *id),
            Origin::Source(node.span()),
        ))
    };
    let symbol =
        |name: &str| {
            let matches = names
                .resolution()
                .symbols()
                .iter()
                .filter(|s| s.name() == name)
                .collect::<Vec<_>>();
            let [s] = matches.as_slice() else {
                return None;
            };
            let actual =
                unit.types()
                    .body_symbol_types()
                    .keys()
                    .copied()
                    .chain(unit.names().names().references().iter().filter_map(
                        |r| match r.target() {
                            lang_frontend::name_resolution::UnitReferenceTarget::Symbol(id) => {
                                Some(*id)
                            }
                            _ => None,
                        },
                    ))
                    .find(|id| id.source_unit() == source && id.symbol() == s.id())?;
            Some((actual, s.span()))
        };
    let variable = |name: &str| {
        let matches = ast
            .items()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Item::Variable {
                    name: NameMarker::Present(span),
                    initializer,
                    ..
                } if unit.sources().slice(*span).ok() == Some(name) => {
                    Some((id, *initializer, *span))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let [v] = matches.as_slice() else {
            return None;
        };
        Some(*v)
    };
    let (a, a_span) = symbol("a")?;
    let (b, b_span) = symbol("b")?;
    let (old, old_span) = symbol("old")?;
    for name in ["a", "b", "old", "outer", "inner", "count"] {
        let (s, _) = symbol(name)?;
        let expected = if ["a", "b", "old"].contains(&name) {
            BuiltinType::String
        } else {
            BuiltinType::Int
        };
        if unit.types().types().get(unit.types().symbol_type(s)?)
            != Some(&UnitTypeKind::Builtin(expected))
        {
            return None;
        }
    }
    let outer = expression("outer > 0")?;
    let inner = expression("inner > 0")?;
    let control = |condition: UnitExpressionId| {
        let candidates=ast.statements().iter().filter(|(_,n)|matches!(n.payload(),Statement::While {condition:c,..} if *c==condition.expression())).collect::<Vec<_>>();
        let [(id, n)] = candidates.as_slice() else {
            return None;
        };
        Some((UnitStatementId::new(source, *id), Origin::Source(n.span())))
    };
    let parent = control(outer.0)?;
    let child = control(inner.0)?;
    if parent.0 == child.0 {
        return None;
    }
    let call = unit.ownership().ownership_primitives();
    let [replace] = call else {
        return None;
    };
    let call = replace.descriptor().expression();
    if replace.descriptor().kind() != lang_frontend::type_checking::OwnershipPrimitiveKind::Replace
        || replace.places().len() != 1
        || !replace.places()[0].is_root()
        || replace.places()[0].root() != a
    {
        return None;
    }
    let suffix = expression("println(b)")?;
    let typed_suffix = unit.types().call(suffix.0)?;
    if !typed_suffix.prints_line() || typed_suffix.aborts() || typed_suffix.receiver().is_some() {
        return None;
    }
    let [old_drop, root_drop, kept_drop] = unit.ownership().drops() else {
        return None;
    };
    let UnitDropPoint::AfterStatement(old_stmt) = old_drop.point() else {
        return None;
    };
    let Statement::LocalVariable { declaration } =
        ast.statements().get(old_stmt.statement()).ok()?.payload()
    else {
        return None;
    };
    let old_var = variable("old")?;
    if *declaration != old_var.0
        || old_var.1 != call.expression()
        || old_drop.target() != UnitDropTarget::Named(old)
        || old_drop.value_origin() != old_span
        || root_drop.target() != UnitDropTarget::Named(a)
        || root_drop.point() != UnitDropPoint::LoopExit(parent.0)
        || root_drop.value_origin() != a_span
        || kept_drop.target() != UnitDropTarget::Named(b)
        || kept_drop.point() != UnitDropPoint::CallReturn(suffix.0)
        || kept_drop.value_origin() != b_span
    {
        return None;
    }
    let [replace_loan, suffix_loan] = unit.ownership().loans() else {
        return None;
    };
    for (loan, expected_call, root, kind) in [
        (
            replace_loan,
            call,
            a,
            lang_frontend::ownership_checking::LoanKind::Exclusive,
        ),
        (
            suffix_loan,
            suffix.0,
            b,
            lang_frontend::ownership_checking::LoanKind::Shared,
        ),
    ] {
        if loan.call() != expected_call
            || loan.kind() != kind
            || !matches!(loan.target(),UnitLoanTarget::Place(p) if p.is_root() && p.root()==root)
            || loan.end_span()
                != ast
                    .expressions()
                    .get(expected_call.expression())
                    .ok()?
                    .span()
        {
            return None;
        }
    }
    let reads = [variable("outer")?.1, variable("inner")?.1];
    let (parameter, _) = symbol("count")?;
    for id in reads {
        let node = ast.expressions().get(id).ok()?;
        if !matches!(node.payload(), Expression::Name) {
            return None;
        }
        let refs = names
            .resolution()
            .references()
            .iter()
            .filter(|r| r.span() == node.span())
            .collect::<Vec<_>>();
        let [r] = refs.as_slice() else {
            return None;
        };
        if !matches!(r.target(),lang_frontend::name_resolution::ReferenceTarget::Symbol(s) if *s==parameter.symbol())
        {
            return None;
        }
    }
    let sites = [
        variable("a")?.1,
        variable("b")?.1,
        replace.descriptor().operands()[1].expression(),
    ];
    let mut literals = Vec::new();
    for site in sites {
        literals.push((
            Origin::Source(ast.expressions().get(site).ok()?.span()),
            super::super::lowering_support::string_literal::decode_plain(
                input.parsed(),
                unit.sources().source_text(input.source_id()).ok()?,
                site,
            )
            .ok()??,
        ));
    }
    let [function] = input.parsed().roots() else {
        return None;
    };
    Some(Facts {
        callable: Origin::Source(ast.items().get(*function).ok()?.span()),
        controls: [parent.1, child.1],
        conditions: [outer.1, inner.1],
        steps: [expression("outer -= 1")?.1, expression("inner -= 1")?.1],
        reads: reads.map(|id| Origin::Source(ast.expressions().get(id).unwrap().span())),
        literals: literals.try_into().ok()?,
        drops: [
            Origin::Source(old_span),
            Origin::Source(a_span),
            Origin::Source(b_span),
        ],
        borrow: [
            Origin::Source(replace_loan.begin_span()),
            Origin::Source(suffix_loan.begin_span()),
        ],
        calls: [
            Origin::Source(ast.expressions().get(call.expression()).ok()?.span()),
            suffix.1,
        ],
    })
}
