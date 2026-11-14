//! Read-only fixture evidence from the exact validated unit used by ordinary lowering.
//! Not admission: BoundLoop still rejects nesting; the caller binds a separate BoundDualNestedLoop.
use super::*;
use lang_frontend::{
    ownership_checking::{UnitDropPoint, UnitDropTarget, UnitLoanTarget},
    parser::{Expression, Item, NameMarker, Statement},
    type_checking::{BuiltinType, UnitExpressionId, UnitStatementId, UnitTypeKind},
};

// Exact identities are discovered below independently of the sealed observation.
struct Identities {
    callable: lang_frontend::type_checking::UnitItemId,
    controls: [UnitStatementId; 2],
    roots: [lang_frontend::name_resolution::UnitSymbolId; 3],
    conditions: [UnitExpressionId; 2],
    steps: [UnitExpressionId; 2],
    sites: [UnitExpressionId; 4], // a, b, parent incoming, child incoming
    calls: [UnitExpressionId; 3],
    drops: [lang_frontend::ownership_checking::UnitDropFact; 4],
    loans: [lang_frontend::ownership_checking::UnitLoanFact; 3],
    retained: lang_frontend::name_resolution::UnitSymbolId,
    final_use: Option<(
        UnitExpressionId,
        lang_frontend::ownership_checking::UnitLoanFact,
    )>,
}
pub(super) struct Facts {
    identities: Identities,
    pub(super) callable: Origin,
    pub(super) final_root_use: Option<(Origin, Origin)>,
    pub(super) independent_counters: bool,
    pub(super) counter_parameters: [lang_frontend::name_resolution::UnitSymbolId; 2],
    pub(super) counter_initializers: [UnitExpressionId; 2],
    pub(super) controls: [Origin; 2],
    pub(super) conditions: [Origin; 2],
    pub(super) steps: [Origin; 2],
    pub(super) reads: [Origin; 2],
    pub(super) literals: [(Origin, Vec<u8>); 4],
    pub(super) drops: [Origin; 4], // parent old, child old, current root, retained
    pub(super) borrow: [Origin; 3], // parent Replace, child Replace, suffix
    pub(super) calls: [Origin; 3],
}
pub(super) fn facts(unit: &OwnedCompilationUnitView<'_, '_>) -> Option<Facts> {
    let [input] = unit.inputs() else {
        return None;
    };
    let ast = input.parsed().ast();
    let postuse = match (
        ast.items().iter().count(),
        ast.statements().iter().count(),
        ast.expressions().iter().count(),
    ) {
        (7, 14, 27) => false,
        (7, 15, 30) => true,
        _ => return None,
    };
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
    let (parent_old, parent_old_span) = symbol("parentOld")?;
    let (old, old_span) = symbol("old")?;
    for name in ["a", "b", "parentOld", "old", "outer", "inner", "count"] {
        let (s, _) = symbol(name)?;
        let expected = if ["a", "b", "parentOld", "old"].contains(&name) {
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
    let [parent_replace, replace] = unit.ownership().ownership_primitives() else {
        return None;
    };
    for plan in [parent_replace, replace] {
        if plan.descriptor().kind() != lang_frontend::type_checking::OwnershipPrimitiveKind::Replace
            || plan.places().len() != 1
            || !plan.places()[0].is_root()
            || plan.places()[0].root() != a
        {
            return None;
        }
    }
    let parent_call = parent_replace.descriptor().expression();
    let call = replace.descriptor().expression();
    if parent_call == call {
        return None;
    }
    let suffix = expression("println(b)")?;
    let typed_suffix = unit.types().call(suffix.0)?;
    if !typed_suffix.prints_line() || typed_suffix.aborts() || typed_suffix.receiver().is_some() {
        return None;
    }
    let [parent_old_drop, old_drop, root_drop, kept_drop] = unit.ownership().drops() else {
        return None;
    };
    for (drop, name, symbol, span, call) in [
        (
            parent_old_drop,
            "parentOld",
            parent_old,
            parent_old_span,
            parent_call,
        ),
        (old_drop, "old", old, old_span, call),
    ] {
        let UnitDropPoint::AfterStatement(stmt) = drop.point() else {
            return None;
        };
        let Statement::LocalVariable { declaration } =
            ast.statements().get(stmt.statement()).ok()?.payload()
        else {
            return None;
        };
        let var = variable(name)?;
        if *declaration != var.0
            || var.1 != call.expression()
            || drop.target() != UnitDropTarget::Named(symbol)
            || drop.value_origin() != span
        {
            return None;
        }
    }
    let final_call = if postuse {
        Some(expression("println(a)")?)
    } else {
        None
    };
    if let Some((call, _)) = final_call {
        let descriptor = unit.types().call(call)?;
        if !descriptor.prints_line() || descriptor.aborts() || descriptor.receiver().is_some() {
            return None;
        }
    }
    let root_point = final_call
        .as_ref()
        .map_or(UnitDropPoint::LoopExit(parent.0), |c| {
            UnitDropPoint::CallReturn(c.0)
        });
    if root_drop.target() != UnitDropTarget::Named(a)
        || root_drop.point() != root_point
        || root_drop.value_origin() != a_span
        || kept_drop.target() != UnitDropTarget::Named(b)
        || kept_drop.point() != UnitDropPoint::CallReturn(suffix.0)
        || kept_drop.value_origin() != b_span
    {
        return None;
    }
    let (parent_loan, replace_loan, final_loan, suffix_loan) =
        match (postuse, unit.ownership().loans()) {
            (false, [parent, child, kept]) => (parent, child, None, kept),
            (true, [parent, child, final_root, kept]) => (parent, child, Some(final_root), kept),
            _ => return None,
        };
    let final_root_use = if let (Some(loan), Some((call, call_origin))) =
        (final_loan, final_call.clone())
    {
        if loan.call() != call
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            || !matches!(loan.target(), UnitLoanTarget::Place(p) if p.is_root() && p.root() == a)
            || loan.end_span() != ast.expressions().get(call.expression()).ok()?.span()
        {
            return None;
        }
        Some((Origin::Source(loan.begin_span()), call_origin))
    } else {
        None
    };
    for (loan, expected_call, root, kind) in [
        (
            parent_loan,
            parent_call,
            a,
            lang_frontend::ownership_checking::LoanKind::Exclusive,
        ),
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
    let child_parameter = symbol("childCount").map(|s| s.0).unwrap_or(parameter);
    let independent_counters = child_parameter != parameter;
    for (id, parameter) in reads.into_iter().zip([parameter, child_parameter]) {
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
        parent_replace.descriptor().operands()[1].expression(),
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
        identities: Identities {
            callable: lang_frontend::type_checking::UnitItemId::new(source, *function),
            controls: [parent.0, child.0],
            roots: [a, symbol("outer")?.0, symbol("inner")?.0],
            conditions: [outer.0, inner.0],
            steps: [expression("outer -= 1")?.0, expression("inner -= 1")?.0],
            sites: sites.map(|id| UnitExpressionId::new(source, id)),
            calls: [parent_call, call, suffix.0],
            drops: [*parent_old_drop, *old_drop, *root_drop, *kept_drop],
            loans: [
                parent_loan.clone(),
                replace_loan.clone(),
                suffix_loan.clone(),
            ],
            retained: b,
            final_use: final_call
                .zip(final_loan)
                .map(|((call, _), loan)| (call, loan.clone())),
        },
        final_root_use,
        independent_counters,
        counter_parameters: [parameter, child_parameter],
        counter_initializers: reads.map(|id| UnitExpressionId::new(source, id)),
        callable: Origin::Source(ast.items().get(*function).ok()?.span()),
        controls: [parent.1, child.1],
        conditions: [outer.1, inner.1],
        steps: [expression("outer -= 1")?.1, expression("inner -= 1")?.1],
        reads: reads.map(|id| Origin::Source(ast.expressions().get(id).unwrap().span())),
        literals: literals.try_into().ok()?,
        drops: [
            Origin::Source(parent_old_span),
            Origin::Source(old_span),
            Origin::Source(a_span),
            Origin::Source(b_span),
        ],
        borrow: [
            Origin::Source(parent_loan.begin_span()),
            Origin::Source(replace_loan.begin_span()),
            Origin::Source(suffix_loan.begin_span()),
        ],
        calls: [
            Origin::Source(ast.expressions().get(parent_call.expression()).ok()?.span()),
            Origin::Source(ast.expressions().get(call.expression()).ok()?.span()),
            suffix.1,
        ],
    })
}

/// Reconcile independently discovered source/fact identities with the same-run sealed sidecar.
/// Neither fixture discovery nor this equality check is a new source-admission capability.
impl Facts {
    pub(super) fn matches_bound(
        &self,
        unit: &OwnedCompilationUnitView<'_, '_>,
        bound: &lang_frontend::ownership_checking::d10_catalog_experiment::BoundDualNestedLoop,
    ) -> Option<()> {
        if !bound.matches(unit) {
            return None;
        }
        let discovered = self;
        let ids = &discovered.identities;
        if bound.controls() != (ids.callable, ids.controls[0], ids.controls[1])
            || bound.roots()
                != (
                    ids.roots[0],
                    ids.roots[1],
                    ids.roots[2],
                    discovered.counter_parameters[0],
                )
            || bound.counter_parameters() != discovered.counter_parameters
            || bound.counter_initializers() != discovered.counter_initializers
            || bound.counter_expressions()
                != [
                    (ids.conditions[0], ids.steps[0]),
                    (ids.conditions[1], ids.steps[1]),
                ]
            || bound.producer_sites() != [ids.sites[0], ids.sites[2], ids.sites[3]]
            || bound.replacements() != [ids.calls[0], ids.calls[1]]
            || bound.cleanup() != &ids.drops[..3]
            || bound.loans() != &ids.loans[..2]
        {
            return None;
        }
        let kept = bound.retained();
        if kept.source() != (ids.retained, ids.sites[1])
            || kept.call() != ids.calls[2]
            || kept.loan() != &ids.loans[2]
            || kept.cleanup() != ids.drops[3]
        {
            return None;
        }
        match (bound.final_root_use(), &ids.final_use) {
            (None, None) => {}
            (Some(actual), Some((call, loan)))
                if actual.source() == (ids.roots[0], ids.sites[0])
                    && actual.call() == *call
                    && actual.loan() == loan
                    && actual.cleanup() == ids.drops[2] => {}
            _ => return None,
        }
        Some(())
    }
}
