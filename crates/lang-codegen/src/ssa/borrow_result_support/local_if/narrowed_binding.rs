//! Join a lexical borrow to the typed proof of its narrowed source.
use super::{contains, ordinary_local, ungroup};
use crate::ssa::lowering_support::{LoweringError, LoweringErrorKind, error};
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowBindingFact, BorrowBindingStorage, LoanKind, UnitLoanTarget, UnitPreflightFacts,
    },
    parser::{Expression, ParsedFile},
    type_checking::{
        BorrowReturnOrigin, BuiltinType, CompilationUnitTypes, IntrinsicTypeConstructor,
        NullableWhenSubjectCategory, ParameterMode, UnitExpressionId, UnitNullableWhenUseWitness,
        UnitTypeId, UnitTypeKind,
    },
};

type Binding = BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>;

/// Identify the source expression of a direct or returned lexical borrow.
fn narrowed_binding_argument(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    fact: &Binding,
) -> Option<(UnitExpressionId, Option<UnitTypeId>)> {
    let unit = fact.binding().source_unit();
    let initializer = fact.initializer();
    if fact.storage() != BorrowBindingStorage::BorrowedStorage || initializer.source_unit() != unit
    {
        return None;
    }
    let ungroup = |id: UnitExpressionId| {
        let expression = ungroup(file, id.expression(), fact.marker_span()).ok()?;
        Some(UnitExpressionId::new(unit, expression))
    };
    let initializer = ungroup(initializer)?;
    match fact.source_loan() {
        None => Some((initializer, None)),
        Some(source) => {
            if source.call() != initializer || source.argument().source_unit() != unit {
                return None;
            }
            let call = typed.call(initializer)?;
            let BorrowReturnOrigin::Parameter(index) = call.borrow_return()?.origin() else {
                return None;
            };
            let mapping = call
                .arguments()
                .iter()
                .find(|mapping| mapping.parameter_index() == index)?;
            let node = file
                .ast()
                .expressions()
                .get(initializer.expression())
                .ok()?;
            let Expression::Call { arguments, .. } = node.payload() else {
                return None;
            };
            if mapping.mode() != ParameterMode::Borrow
                || arguments.get(mapping.argument_index())?.value != source.argument().expression()
                || Some(call.return_type()) != typed.symbol_type(fact.binding())
            {
                return None;
            }
            Some((ungroup(source.argument())?, Some(mapping.parameter_type())))
        }
    }
}

// A narrowed String binding is built as a child of the active payload view.
pub(super) fn narrowed_string_binding(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    fact: &Binding,
) -> Option<ordinary_local::Policy> {
    let UnitLoanTarget::Place(place) = fact.origin() else {
        return None;
    };
    let unit = fact.binding().source_unit();
    if !place.is_root() || place.root().source_unit() != unit {
        return None;
    }
    let (argument, parameter) = narrowed_binding_argument(file, typed, fact)?;
    let proof = typed.non_null_use(argument)?;
    let inner = proof.narrowed_type();
    let source = match fact.parent() {
        Some(parent) if parent.source_unit() == unit => {
            let bindings = owned.borrow_results().bindings();
            let index = bindings
                .binary_search_by_key(&parent, |binding| binding.binding())
                .ok()?;
            let parent_fact = &bindings[index];
            if parent_fact.storage() != BorrowBindingStorage::BorrowedStorage
                || parent_fact.origin() != fact.origin()
            {
                return None;
            }
            parent
        }
        Some(_) => return None,
        None => place.root(),
    };
    if proof.symbol() != source
        || typed.symbol_type(source) != Some(proof.declared_type())
        || typed.types().get(proof.declared_type()) != Some(&UnitTypeKind::Nullable(inner))
        || typed.types().get(inner) != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || !matches!(
            file.ast()
                .expressions()
                .get(argument.expression())
                .ok()?
                .payload(),
            Expression::Name
        )
    {
        return None;
    }
    let result = typed.symbol_type(fact.binding())?;
    if result == inner {
        return Some(ordinary_local::Policy::NarrowedString);
    }
    let call_source = fact.source_loan()?;
    // A returned nullable loan must come from the original nullable storage.
    (parameter == Some(proof.declared_type())
        && result == proof.declared_type()
        && typed.nullable_when_use(argument).is_none()
        && owned.loans().iter().any(|loan| {
            loan.call() == call_source.call()
                && loan.argument() == call_source.argument()
                && loan.kind() == LoanKind::Shared
                && loan.target() == fact.origin()
        }))
    .then_some(ordinary_local::Policy::NarrowedNullableString)
}

/// Join a typed non-null When use to the formed lexical child and its source lease.
pub(in crate::ssa) fn narrowed_when_binding(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    fact: &Binding,
) -> Option<UnitNullableWhenUseWitness> {
    let UnitLoanTarget::Place(origin) = fact.origin() else {
        return None;
    };
    let (argument, _) = narrowed_binding_argument(file, typed, fact)?;
    let witness = typed.nullable_when_use(argument)?;
    let descriptor = typed.nullable_when(witness.control())?;
    if witness.reference() != argument
        || !matches!(
            (
                descriptor.source_category(),
                typed.types().get(witness.inner_type())
            ),
            (
                NullableWhenSubjectCategory::BorrowRoot
                    | NullableWhenSubjectCategory::OwnedRoot
                    | NullableWhenSubjectCategory::InoutRoot,
                Some(
                    UnitTypeKind::Builtin(BuiltinType::String)
                        | UnitTypeKind::Intrinsic {
                            constructor: IntrinsicTypeConstructor::Rc,
                            ..
                        }
                )
            )
        )
        || descriptor.nullable_type() != witness.nullable_type()
        || descriptor.inner_type() != witness.inner_type()
        || witness.entry_index() != 1
        || descriptor.stable_symbol() != Some(witness.symbol())
        || descriptor.source_generation() != Some(witness.source_generation())
        || descriptor.subject() != witness.subject()
        || descriptor.entries().get(1)?.body() != witness.body()
        || fact.parent().unwrap_or(origin.root()) != witness.symbol()
        || (fact.parent().is_none() && !origin.is_root())
        || typed.symbol_type(fact.binding()) != Some(witness.inner_type())
        || fact.source_loan().is_some_and(|source| {
            !owned.loans().iter().any(|loan| {
                loan.call() == source.call()
                    && loan.argument() == source.argument()
                    && loan.kind() == LoanKind::Shared
                    && loan.target() == fact.origin()
            })
        })
    {
        return None;
    }
    Some(witness)
}

/// The proof exception covers the source operand, while argument control must
/// still carry the ordinary prefix-end evidence.
pub(in crate::ssa) fn validate_narrowed_when_initializer(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &dyn UnitPreflightFacts,
    fact: &Binding,
) -> Result<(), LoweringError> {
    let Some(source) = fact.source_loan() else {
        return Ok(());
    };
    let call = file
        .ast()
        .expressions()
        .get(source.call().expression())
        .map_err(|_| error(LoweringErrorKind::MissingFact, fact.marker_span()))?;
    let allowed = ordinary_local::argument_returns(file, typed, owned, fact, true);
    for (expression, node) in file.ast().expressions().iter() {
        if contains(call.span(), node.span())
            && matches!(
                node.payload(),
                Expression::If { .. }
                    | Expression::When { .. }
                    | Expression::Return { .. }
                    | Expression::Binary { .. }
            )
            && !allowed
                .as_ref()
                .is_some_and(|allowed| allowed.contains(&expression.index()))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    Ok(())
}
