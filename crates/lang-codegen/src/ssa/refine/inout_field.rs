//! Admission and construction share the validated identity of a direct owned heap field.
use super::RefineInputs;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{LoanKind, UnitLoanTarget},
    parser::Expression,
    source::Span,
    type_checking::{
        Copyability, NominalKind, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

pub(super) struct InoutField {
    pub(super) owner: UnitSymbolId,
    pub(super) owner_ty: UnitTypeId,
    pub(super) field: UnitSymbolId,
    pub(super) target: UnitTypeId,
    pub(super) begin: Span,
}

impl RefineInputs<'_> {
    pub(super) fn inout_field(
        &self,
        call: UnitExpressionId,
        argument: ExpressionId,
        parameter: UnitTypeId,
    ) -> Option<InoutField> {
        let nodes = self.parsed.ast().expressions();
        let ungrouped = |mut id| {
            while let Expression::Group { expression } = nodes.get(id).ok()?.payload() {
                id = *expression;
            }
            Some(id)
        };
        let argument = UnitExpressionId::new(self.source_unit, argument);
        let projection_id =
            UnitExpressionId::new(self.source_unit, ungrouped(argument.expression())?);
        let projection = self.typed.aggregate_projection(projection_id)?;
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return None;
        };
        if call.source_unit() != self.source_unit
            || receiver.source_unit() != self.source_unit
            || projection.kind() != UnitAggregateProjectionKind::Field
            || self.typed.non_null_use(argument).is_some()
        {
            return None;
        }
        let target = self.concrete(projection.ty())?;
        if target != self.concrete(parameter)?
            || self.concrete(self.typed.expression_type(argument)?)? != target
            || !matches!(
                self.typed.copyability(target),
                Copyability::Copyable | Copyability::MoveOnly
            )
        {
            return None;
        }
        let receiver = nodes.get(ungrouped(receiver.expression())?).ok()?;
        if !matches!(receiver.payload(), Expression::Name) {
            return None;
        }
        let span = receiver.span();
        let owner = *self.references.get(&(span.start(), span.end()))?;
        let owner_ty = self.concrete(self.typed.symbol_type(owner)?)?;
        let UnitTypeKind::Nominal {
            declaration,
            arguments,
        } = self.typed.types().get(owner_ty)?
        else {
            return None;
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)?
            .nominal()?;
        if nominal.kind() != NominalKind::Class || !arguments.is_empty() {
            return None;
        }
        let field = projection.field();
        let mut loans = self
            .owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == call && loan.argument() == argument);
        let loan = loans.next()?;
        if loans.next().is_some()
            || loan.kind() != LoanKind::Exclusive
            || loan.end_span() != nodes.get(call.expression()).ok()?.span()
            || !matches!(loan.target(), UnitLoanTarget::Place(place)
                if place.root() == owner && place.fields() == [field] && place.element().is_none())
        {
            return None;
        }
        Some(InoutField {
            owner,
            owner_ty,
            field,
            target,
            begin: loan.begin_span(),
        })
    }
}
