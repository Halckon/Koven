//! Shared admission/build lookup for a direct field of the current Inout receiver.
use super::RefineInputs;
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{LoanKind, UnitLoanTarget},
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        NominalKind, OwnershipPrimitiveKind, ParameterMode, UnitAggregateProjectionKind,
        UnitAggregateProjectionReceiver, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

pub(super) struct ReceiverField {
    pub(super) owner_type: UnitTypeId,
    pub(super) field: UnitSymbolId,
    pub(super) begin: Span,
    projection: UnitExpressionId,
}

impl RefineInputs<'_> {
    pub(super) fn receiver_field_replace(
        &self,
        call: UnitExpressionId,
        arguments: &[CallArgument],
    ) -> Option<ReceiverField> {
        let [field_arg, replacement] = arguments else {
            return None;
        };
        if call.source_unit() != self.source_unit {
            return None;
        }
        let id = |expression| UnitExpressionId::new(self.source_unit, expression);
        let primitive = self.typed.ownership_primitive(call)?;
        if primitive.kind() != OwnershipPrimitiveKind::Replace
            || primitive.operands() != [id(field_arg.value), id(replacement.value)]
        {
            return None;
        }
        let field =
            self.receiver_field_projection(call, field_arg.value, primitive.value_type())?;
        if let Some(plan) = self.owned.field_replacement(call) {
            if plan.descriptor() != &primitive
                || self.concrete(plan.owner_type())? != field.owner_type
                || plan.inout_receiver().map(|(projection, _)| projection) != Some(field.projection)
                || !plan.place().is_root()
                || plan.place().root() != field.field
            {
                return None;
            }
        } else if self.typed.expression_falls_through(id(replacement.value)) != Some(false) {
            // A terminating replacement retains the argument loan without a commit plan.
            return None;
        }
        Some(field)
    }

    pub(super) fn receiver_field_projection(
        &self,
        call: UnitExpressionId,
        argument: ExpressionId,
        value_type: UnitTypeId,
    ) -> Option<ReceiverField> {
        if call.source_unit() != self.source_unit {
            return None;
        }
        let id = |expression| UnitExpressionId::new(self.source_unit, expression);
        let nodes = self.parsed.ast().expressions();
        let ungrouped = |mut expression| {
            while let Expression::Group { expression: child } =
                nodes.get(expression).ok()?.payload()
            {
                expression = *child;
            }
            Some(expression)
        };
        let projection = self.typed.aggregate_projection(id(ungrouped(argument)?))?;
        let current = self
            .receiver
            .filter(|receiver| receiver.mode == ParameterMode::Inout)?;
        let owner_type = self.concrete(current.ty)?;
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.concrete(projection.ty())? != self.concrete(value_type)?
        {
            return None;
        }
        match projection.receiver() {
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.source_unit
                    || self.concrete(self.typed.expression_type(receiver)?)? != owner_type
                {
                    return None;
                }
                let node = nodes.get(ungrouped(receiver.expression())?).ok()?;
                let span = node.span();
                if !matches!(node.payload(), Expression::This)
                    || self.references.contains_key(&(span.start(), span.end()))
                {
                    return None;
                }
            }
            UnitAggregateProjectionReceiver::This(declaration) if declaration == current.owner => {}
            _ => return None,
        }
        let UnitTypeKind::Nominal {
            declaration,
            arguments,
        } = self.typed.types().get(owner_type)?
        else {
            return None;
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)?
            .nominal()?;
        let field = projection.field();
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || *declaration != current.owner
            || !nominal
                .fields()
                .iter()
                .any(|member| member.symbol() == field && member.ty() == projection.ty())
        {
            return None;
        }
        let mut loans = self
            .owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == call && loan.argument() == id(argument));
        let loan = loans.next()?;
        if loans.next().is_some()
            || loan.kind() != LoanKind::Exclusive
            || loan.end_span() != nodes.get(call.expression()).ok()?.span()
            || !matches!(loan.target(), UnitLoanTarget::Place(place)
                if place.is_root() && place.root() == field)
        {
            return None;
        }
        Some(ReceiverField {
            owner_type,
            field,
            begin: loan.begin_span(),
            projection: projection.expression(),
        })
    }
}
