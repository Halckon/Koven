//! A normal Unit If replaces a named owner; its result is Unit, not a Map expression birth.
use super::*;
use lang_frontend::parser::{AssignmentOperator, VariableKind};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in super::super) struct Proof {
    pub root: UnitSymbolId,
    pub initializer: UnitExpressionId,
    pub control: UnitExpressionId,
    pub condition: UnitExpressionId,
    pub arm: StatementId,
    pub replacement: UnitExpressionId,
    pub marker: Span,
    pub before: bool,
    replaced: usize,
    final_drop: usize,
}

impl Proof {
    /// Indices refer to the same immutable ownership product used to qualify.
    /// Return its complete facts so consumers still compare whole identities.
    pub(in super::super) fn drops(
        self,
        owned: &CompilationUnitOwnership,
        span: Span,
    ) -> Result<[UnitDropFact; 2], LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let replaced = *owned.drops().get(self.replaced).ok_or_else(bad)?;
        let final_drop = *owned.drops().get(self.final_drop).ok_or_else(bad)?;
        if self.replaced == self.final_drop
            || [replaced, final_drop].iter().any(|fact| {
                fact.target() != UnitDropTarget::Named(self.root)
                    || fact.value_origin() != self.marker
            })
            || replaced.point() != UnitDropPoint::AfterExpression(self.replacement)
            || !matches!(final_drop.point(), UnitDropPoint::CallReturn(call)
                if call.source_unit() == self.root.source_unit())
        {
            return Err(bad());
        }
        Ok([replaced, final_drop])
    }
}

pub(super) fn qualify(
    frontend: (
        &ParsedFile,
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
    ),
    plan: &UnitMapIterationOwnershipPlan,
    local: (StatementId, UnitSymbolId),
    span: Span,
) -> Result<Option<QualifiedMap>, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let (body, root) = local;
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let source = root.source_unit();
    let reference = |id: UnitExpressionId| {
        let at = parsed
            .ast()
            .expressions()
            .get(id.expression())
            .map_err(|_| bad())?
            .span();
        let matches = names
            .names()
            .references()
            .iter()
            .filter(|r| r.source_unit() == source && r.span() == at)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [r] => match r.target() {
                UnitReferenceTarget::Symbol(s) => Ok(*s),
                _ => Err(bad()),
            },
            _ => Err(bad()),
        }
    };
    let assignments = typed
        .assignments()
        .iter()
        .filter(|d| {
            d.expression().source_unit() == source && reference(d.target()).ok() == Some(root)
        })
        .collect::<Vec<_>>();
    let [assignment] = assignments.as_slice() else {
        return Ok(None);
    };
    let at = parsed
        .ast()
        .expressions()
        .get(assignment.expression().expression())
        .map_err(|_| bad())?
        .span();
    let controls = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::If {
                condition,
                then_branch,
                else_branch: None,
                ..
            } = node.payload()
            else {
                return None;
            };
            let arm = parsed.ast().statements().get(*then_branch).ok()?;
            contains(arm.span(), at).then_some((id, *condition, *then_branch, node.span()))
        })
        .collect::<Vec<_>>();
    let [(control, condition, arm, control_span)] = controls.as_slice() else {
        return Ok(None);
    };
    let before = control_span.end() < span.start();
    if !before && control_span.start() <= span.end() {
        return Err(unsupported());
    }
    let storage = plan.descriptor().source_type();
    if assignment.operator() != AssignmentOperator::Assign
        || !assignment.falls_through()
        || assignment.target_type() != storage
        || reference(assignment.target())? != root
        || typed.copyability(storage) != Copyability::MoveOnly
        || typed
            .expression_type(UnitExpressionId::new(source, *control))
            .and_then(|ty| builtin_type(typed, ty))
            != Some(BuiltinType::Unit)
        || typed
            .expression_type(UnitExpressionId::new(source, *condition))
            .and_then(|ty| builtin_type(typed, ty))
            != Some(BuiltinType::Boolean)
        || !owned.formed_owned_locals().contains(&root)
    {
        return Err(bad());
    }
    let symbol = names
        .names()
        .source_units()
        .iter()
        .find(|u| u.source_unit() == source)
        .and_then(|u| u.resolution().symbols().get(root.symbol().index()))
        .ok_or_else(bad)?;
    if symbol.kind() != SymbolKind::Variable
        || symbol.is_synthetic()
        || typed.symbol_type(root) != Some(storage)
        || owned.captures().iter().any(|capture| {
            capture.source()
                == lang_frontend::ownership_checking::UnitClosureCaptureSource::Symbol(root)
        })
    {
        return Err(unsupported());
    }
    let declarations = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Variable {
                kind: VariableKind::Var,
                name: NameMarker::Present(marker),
                initializer,
                ..
            } if *marker == symbol.span() => Some((*marker, *initializer)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(marker, initializer)] = declarations.as_slice() else {
        return Err(bad());
    };
    let initializer = UnitExpressionId::new(source, *initializer);
    let initial_span = parsed
        .ast()
        .expressions()
        .get(initializer.expression())
        .map_err(|_| bad())?
        .span();
    if initial_span.end() >= control_span.start() || initial_span.end() >= span.start() {
        return Err(bad());
    }
    local::factory(parsed, names, typed, owned, initializer, storage, span)?;
    local::factory(
        parsed,
        names,
        typed,
        owned,
        assignment.value(),
        storage,
        span,
    )?;
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|d| d.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let replaced = drops
        .iter()
        .copied()
        .filter(|d| d.point() == UnitDropPoint::AfterExpression(assignment.value()))
        .collect::<Vec<_>>();
    let [replaced] = replaced.as_slice() else {
        return Err(bad());
    };
    let final_drops = drops
        .iter()
        .copied()
        .filter(|d| d != replaced)
        .collect::<Vec<_>>();
    let [final_drop] = final_drops.as_slice() else {
        return Err(bad());
    };
    let UnitDropPoint::CallReturn(call) = final_drop.point() else {
        return Err(unsupported());
    };
    let final_span = parsed
        .ast()
        .expressions()
        .get(call.expression())
        .map_err(|_| bad())?
        .span();
    let final_call = typed.call(call).ok_or_else(bad)?;
    let contracts = owned.call_argument_contracts_for(call).filter(|c| {
        c.kind() == lang_frontend::ownership_checking::UnitCallArgumentOwnershipKind::SharedLoan
            && matches!(owned.loans().iter().find(|l| l.call() == call && l.argument() == c.argument()).map(|l| l.target()),
                Some(UnitLoanTarget::Place(place)) if place.is_root() && place.root() == root)
    }).collect::<Vec<_>>();
    let [contract] = contracts.as_slice() else {
        return Err(bad());
    };
    let [argument] = final_call.arguments() else {
        return Err(unsupported());
    };
    if final_call.receiver().is_some()
        || !matches!(
            final_call.target(),
            UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
        )
        || builtin_type(typed, final_call.return_type()) != Some(BuiltinType::Unit)
        || final_call.result_source() != CallableResultSource::Owned
        || argument.mode() != ParameterMode::Borrow
        || argument.parameter_index() != 0
        || argument.parameter_type() != storage
        || contract.parameter_index() != 0
        || contract.parameter_type() != storage
        || contract.crosses_thread()
        || reference(contract.argument())? != root
    {
        return Err(unsupported());
    }
    if drops.len() != 2
        || drops.iter().any(|d| d.value_origin() != *marker)
        || contracts.len() != 1
        || final_span.start() <= control_span.end()
        || !crate::ssa::map_source::future_reference(
            parsed,
            span,
            final_span,
            names.names().references().iter().filter_map(|r| {
                (r.target() == &UnitReferenceTarget::Symbol(root)).then_some(r.span())
            }),
        )
    {
        return Err(bad());
    }
    let body_exit = qualify_body(parsed, names, plan, plan.descriptor(), body, span)?;
    let terminal = [*final_drop];
    let (source_drop, transfer_drop) = validate_cleanup(
        plan,
        owned,
        (body, body_exit),
        (root, Some(&terminal)),
        *marker,
        Some(*final_drop),
        span,
    )?;
    let proof = Proof {
        root,
        initializer,
        control: UnitExpressionId::new(source, *control),
        condition: UnitExpressionId::new(source, *condition),
        arm: *arm,
        replacement: assignment.value(),
        marker: *marker,
        before,
        replaced: owned
            .drops()
            .iter()
            .position(|fact| fact == replaced)
            .ok_or_else(bad)?,
        final_drop: owned
            .drops()
            .iter()
            .position(|fact| fact == final_drop)
            .ok_or_else(bad)?,
    };
    Ok(Some(QualifiedMap {
        statement: plan.descriptor().statement(),
        source: SourceIdentity::BindingChoice(proof),
        source_drop,
        transfer_drop,
        late_drop: Some(*final_drop),
        future_transfer: None,
        body_exit,
        body_point: body_point(plan.descriptor().statement(), body, body_exit),
        conditional: None,
        nested: None,
    }))
}
