//! Typed direct swap operands and their exact ownership receipts.
use super::{RefineInputs, inout_field::InoutField, receiver_field_replace::ReceiverField};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{LoanKind, UnitLoanTarget},
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{OwnershipPrimitiveKind, ParameterMode, UnitExpressionId, UnitTypeId},
};

pub(super) struct ElementOperand {
    pub(super) owner: UnitSymbolId,
    pub(super) receiver: ExpressionId,
    pub(super) container: UnitTypeId,
    pub(super) index: ExpressionId,
    /// An index that exits before producing a place has no loan.
    pub(super) begin: Option<Span>,
}

pub(super) enum SwapOperand {
    Root {
        symbol: UnitSymbolId,
        expression: ExpressionId,
        begin: Span,
    },
    Element(ElementOperand),
    Field(InoutField),
    ReceiverField(ReceiverField),
}

pub(super) enum BorrowedReplace {
    Root { symbol: UnitSymbolId, begin: Span },
    Field(InoutField),
}

impl RefineInputs<'_> {
    fn argument_root(&self, mut expression: ExpressionId) -> Option<UnitSymbolId> {
        let nodes = self.parsed.ast().expressions();
        loop {
            let node = nodes.get(expression).ok()?;
            match node.payload() {
                Expression::Name => {
                    return self
                        .references
                        .get(&(node.span().start(), node.span().end()))
                        .copied();
                }
                Expression::Group { expression: child } => expression = *child,
                _ => return None,
            }
        }
    }

    fn is_inout_parameter(&self, symbol: UnitSymbolId) -> bool {
        self.parameters
            .iter()
            .zip(&self.modes)
            .any(|(parameter, mode)| *parameter == symbol && *mode == ParameterMode::Inout)
    }

    pub(super) fn borrowed_replace(
        &self,
        call: UnitExpressionId,
        arguments: &[CallArgument],
    ) -> Option<BorrowedReplace> {
        let [place, replacement] = arguments else {
            return None;
        };
        let id = |expression| UnitExpressionId::new(self.source_unit, expression);
        let primitive = self.typed.ownership_primitive(call)?;
        if call.source_unit() != self.source_unit
            || primitive.kind() != OwnershipPrimitiveKind::Replace
            || primitive.operands() != [id(place.value), id(replacement.value)]
        {
            return None;
        }
        let mut deliveries = self.owned.value_deliveries().iter().filter(|delivery| {
            delivery.call() == call && delivery.argument() == id(replacement.value)
        });
        let completes = self.typed.expression_falls_through(id(replacement.value)) != Some(false);
        if deliveries.next().is_some() != completes || deliveries.next().is_some() {
            return None;
        }
        if let Some(symbol) = self
            .argument_root(place.value)
            .filter(|symbol| self.is_inout_parameter(*symbol))
        {
            let value_type = self.concrete(primitive.value_type())?;
            let argument = id(place.value);
            if self.concrete(self.typed.expression_type(argument)?)? != value_type
                || self.concrete(self.typed.symbol_type(symbol)?)? != value_type
                || self.typed.non_null_use(argument).is_some()
            {
                return None;
            }
            let span = self
                .parsed
                .ast()
                .expressions()
                .get(call.expression())
                .ok()?
                .span();
            let mut loans = self
                .owned
                .loans()
                .iter()
                .filter(|loan| loan.call() == call && loan.argument() == argument);
            let loan = loans.next()?;
            if loans.next().is_some()
                || loan.kind() != LoanKind::Exclusive
                || loan.end_span() != span
                || !matches!(loan.target(), UnitLoanTarget::Place(place)
                    if place.is_root() && place.root() == symbol)
            {
                return None;
            }
            return Some(BorrowedReplace::Root {
                symbol,
                begin: loan.begin_span(),
            });
        }
        self.inout_field(call, place.value, primitive.value_type())
            .filter(|field| self.is_inout_parameter(field.owner))
            .map(BorrowedReplace::Field)
    }

    pub(super) fn projected_swap(
        &self,
        call: UnitExpressionId,
        arguments: &[CallArgument],
    ) -> Option<Vec<SwapOperand>> {
        let [left, right] = arguments else {
            return None;
        };
        let id = |expr| UnitExpressionId::new(self.source_unit, expr);
        let primitive = self.typed.ownership_primitive(call)?;
        if call.source_unit() != self.source_unit
            || primitive.kind() != OwnershipPrimitiveKind::Swap
            || primitive.operands() != [id(left.value), id(right.value)]
        {
            return None;
        }
        let nodes = self.parsed.ast().expressions();
        let span = nodes.get(call.expression()).ok()?.span();
        let ungroup = |mut expression| {
            while let Expression::Group { expression: child } =
                nodes.get(expression).ok()?.payload()
            {
                expression = *child;
            }
            Some(expression)
        };
        if ![left, right].iter().any(|argument| {
            self.argument_root(argument.value)
                .is_some_and(|root| self.is_inout_parameter(root))
                || ungroup(argument.value).is_some_and(|expr| {
                    self.typed.element_place(id(expr)).is_some()
                        || self.typed.aggregate_projection(id(expr)).is_some()
                })
        }) {
            return None;
        }

        let mut operands = Vec::with_capacity(2);
        for arg in [left, right] {
            let expression = id(ungroup(arg.value)?);
            if self.typed.aggregate_projection(expression).is_some() {
                let operand = if let Some(field) =
                    self.receiver_field_projection(call, arg.value, primitive.value_type())
                {
                    SwapOperand::ReceiverField(field)
                } else {
                    SwapOperand::Field(self.inout_field(call, arg.value, primitive.value_type())?)
                };
                operands.push(operand);
                continue;
            }
            let mut facts = self
                .owned
                .loans()
                .iter()
                .filter(|fact| fact.call() == call && fact.argument() == id(arg.value));
            let Some(element) = self.typed.element_place(expression) else {
                let name = nodes.get(expression.expression()).ok()?;
                if !matches!(name.payload(), Expression::Name)
                    || self.typed.expression_type(id(arg.value))? != primitive.value_type()
                    || self.typed.non_null_use(id(arg.value)).is_some()
                {
                    return None;
                }
                let symbol = *self
                    .references
                    .get(&(name.span().start(), name.span().end()))?;
                let fact = facts.next()?;
                if facts.next().is_some()
                    || fact.kind() != LoanKind::Exclusive
                    || fact.end_span() != span
                    || !matches!(fact.target(), UnitLoanTarget::Place(place)
                        if place.root() == symbol && place.is_root())
                {
                    return None;
                }
                operands.push(SwapOperand::Root {
                    symbol,
                    expression: arg.value,
                    begin: fact.begin_span(),
                });
                continue;
            };
            let receiver = element.receiver();
            let index = element.index();
            if element.expression() != expression
                || !element.is_mutable()
                || element.element_type() != primitive.value_type()
                || receiver.source_unit() != self.source_unit
                || index.source_unit() != self.source_unit
            {
                return None;
            }
            let name = nodes.get(ungroup(receiver.expression())?).ok()?;
            if !matches!(name.payload(), Expression::Name) {
                return None;
            }
            let owner = *self
                .references
                .get(&(name.span().start(), name.span().end()))?;
            let container = self.concrete(self.typed.expression_type(receiver)?)?;
            let begin = if self.typed.expression_falls_through(index) == Some(false) {
                if facts.next().is_some() {
                    return None;
                }
                None
            } else {
                let fact = facts.next()?;
                if facts.next().is_some()
                    || fact.kind() != LoanKind::Exclusive
                    || fact.end_span() != span
                    || !matches!(fact.target(), UnitLoanTarget::Place(place)
                        if place.root() == owner && place.fields().is_empty() && place.element().is_some())
                {
                    return None;
                }
                Some(fact.begin_span())
            };
            operands.push(SwapOperand::Element(ElementOperand {
                owner,
                receiver: receiver.expression(),
                container,
                index: index.expression(),
                begin,
            }));
            if begin.is_none() {
                break;
            }
        }
        Some(operands)
    }
}
