//! Unit temporary qualification uses only source-qualified descriptor, plan and Drop facts.
use super::*;

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<QualifiedMap, LoweringError> {
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let statement = descriptor.statement();
    let source = UnitExpressionId::new(
        statement.source_unit(),
        crate::ssa::map_source::transparent_expression(
            parsed,
            descriptor.source().expression(),
            |id| {
                let id = UnitExpressionId::new(statement.source_unit(), id);
                typed.expression_type(id).zip(typed.expression_category(id))
            },
        )?,
    );
    let source_node = parsed
        .ast()
        .expressions()
        .get(source.expression())
        .map_err(|_| missing())?;
    // Formation follows the same ordinary selected Call as a bound local;
    // source-qualified temporary lifetime and cleanup remain separate below.
    if matches!(source_node.payload(), Expression::If { .. }) {
        expression_birth::qualify(
            parsed,
            names,
            typed,
            owned,
            source,
            descriptor.source_type(),
            span,
        )?;
    } else {
        local::factory(
            parsed,
            names,
            typed,
            owned,
            source,
            descriptor.source_type(),
            span,
        )?;
    }
    if typed.copyability(descriptor.source_type()) != Copyability::MoveOnly {
        return Err(missing());
    }
    // Unlike named parameters, a factory result has no parameter-origin or unit phi table.
    let plan = owned.map_iteration(statement).ok_or_else(missing)?;
    if plan.descriptor() != descriptor
        || plan.source_access() != UnitIterationSourceAccess::Temporary
        || plan.source_kind() != &IterationSource::BorrowedSource(UnitLoanTarget::Temporary(source))
        || plan.source() != Some(&UnitLoanTarget::Temporary(source))
    {
        return Err(missing());
    }
    let bindings = descriptor.bindings();
    if bindings[0] == bindings[1]
        || bindings
            .iter()
            .any(|symbol| symbol.source_unit() != statement.source_unit())
        || typed.symbol_type(bindings[0]) != Some(descriptor.key_type())
        || typed.symbol_type(bindings[1]) != Some(descriptor.value_type())
    {
        return Err(missing());
    }
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = plan.bindings()[index];
        let loan = &plan.loans()[index];
        if binding.symbol() != bindings[index]
            || binding.kind() != OwnershipBindingKind::Shared
            || owned.binding_kind(bindings[index]) != Some(OwnershipBindingKind::Shared)
            || loan.statement() != statement
            || loan.source() != descriptor.source()
            || loan.component() != component
            || loan.binding() != bindings[index]
            || loan.origin() != &UnitLoanTarget::Temporary(source)
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
        {
            return Err(missing());
        }
    }
    reject_nested(parsed, statement.statement(), span)?;
    if let Some(qualified) = conditional::qualify(
        parsed,
        names,
        typed,
        owned,
        plan,
        (body, SourceIdentity::Temporary(source)),
        span,
    )? {
        return Ok(qualified);
    }
    Err(missing())
}
