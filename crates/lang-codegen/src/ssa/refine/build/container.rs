//! Sequential containers (`Array<T>`, `List<T>`, `MutableList<T>`) in the refined CFG:
//! construction, `.size`, indexing `xs[i]`, borrowed element arguments `f(xs[i])`,
//! element assignment `xs[i] = v`, and `MutableList<T>` mutating operations.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    parser::Expression,
    source::Span,
    type_checking::{ContainerConstructionKind, UnitTypeId},
};

use super::{Builder, Lowered, callable::LentArgument};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::ROp,
};

impl Builder<'_, '_> {
    pub(super) fn container_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<Lowered>, LoweringError> {
        let id = self.unit(expression);
        let typed = self.inputs.typed;
        if typed.container_construction(id).is_some() {
            return self.container_construction(expression, span).map(Some);
        }
        if typed.container_size(id).is_some() {
            return self.container_size(expression, span).map(Some);
        }
        if typed.element_place(id).is_some() {
            return self.container_index(expression, span).map(Some);
        }
        if typed.container_append(id).is_some() {
            return self.container_append(expression, span).map(Some);
        }
        if typed.container_clear(id).is_some() {
            return self.container_clear(expression, span).map(Some);
        }
        if typed.container_remove_at(id).is_some() {
            return self.container_remove_at(expression, span).map(Some);
        }
        if typed.container_remove_first(id).is_some() {
            return self.container_remove_first(expression, span).map(Some);
        }
        if typed.container_remove_last(id).is_some() {
            return self.container_remove_last(expression, span).map(Some);
        }
        if typed.container_insert_at(id).is_some() {
            return self.container_insert_at(expression, span).map(Some);
        }
        Ok(None)
    }

    fn container_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let (Expression::Call { arguments, .. }, _) = self.node(expression)? else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let container_ty = self.c(descriptor.container_type());
        let element_ty = self.c(descriptor.element_type());
        let container = self.ssa_type(container_ty, span)?;
        let mut elements = Vec::with_capacity(arguments.len());
        match descriptor.kind() {
            ContainerConstructionKind::EmptyMutableList => {
                if !arguments.is_empty() {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
            ContainerConstructionKind::ListForm => {
                for argument in arguments {
                    let Some(value) = self.required(argument.value)? else {
                        return Ok(Lowered::Diverged);
                    };
                    let value = self.adapt(value, argument.value, element_ty)?;
                    self.deliver(argument.value);
                    elements.push(value);
                }
            }
            ContainerConstructionKind::RuntimeLength => {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
        let slot = self.value(
            ROp::ContainerConstruct {
                container,
                elements,
            },
            EntityType::Value(container),
            span,
        )?;
        Ok(Lowered::Value(slot))
    }

    fn container_size(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_size(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let receiver_span = self.span(receiver);
        let symbol = self.named_container_symbol(receiver, span)?;
        let container_ty = self.c(descriptor.container_type());
        let target = self.ssa_type(container_ty, receiver_span)?;
        let (loan, created) = if let Some(&loan) = self.borrowed.get(&symbol) {
            (loan, None)
        } else {
            let owner = *self
                .bindings
                .get(&symbol)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, receiver_span))?;
            let place = self.value(
                ROp::RootPlace { owner },
                EntityType::Place(target),
                receiver_span,
            )?;
            let loan = self.value(
                ROp::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
                receiver_span,
            )?;
            (loan, Some(loan))
        };
        let result_ty = self.ssa_type(self.c(descriptor.result_type()), span)?;
        let result = self.value(
            ROp::ContainerLength { owner: loan },
            EntityType::Value(result_ty),
            span,
        )?;
        if let Some(loan) = created {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        self.drops(UnitDropPoint::CallReturn(id))?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.receiver()))?;
        Ok(Lowered::Value(result))
    }

    fn container_index(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .element_place(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let receiver = descriptor.receiver().expression();
        let symbol = self.named_container_symbol(receiver, span)?;
        let owner = self
            .borrowed
            .get(&symbol)
            .or_else(|| self.bindings.get(&symbol))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let Some(index) = self.required(descriptor.index().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let element = self.ssa_type(self.c(descriptor.element_type()), span)?;
        let place = self.value(
            ROp::ContainerElementPlace { owner, index },
            EntityType::Place(element),
            span,
        )?;
        let value = self.value(ROp::ReadPlace { place }, EntityType::Value(element), span)?;
        Ok(Lowered::Value(value))
    }

    /// Borrows `xs[i]` as a shared loan for a `Borrow` call argument.
    pub(super) fn borrowed_container_element_loan(
        &mut self,
        expression: ExpressionId,
        parameter: UnitTypeId,
        span: Span,
    ) -> Result<Option<LentArgument>, LoweringError> {
        let mut root = expression;
        while let (Expression::Group { expression: inner }, _) = self.node(root)? {
            root = inner;
        }
        let id = self.unit(root);
        let Some(descriptor) = self.inputs.typed.element_place(id) else {
            return Ok(None);
        };
        if self.c(descriptor.element_type()) != parameter {
            return Ok(None);
        }
        let receiver = descriptor.receiver().expression();
        let symbol = self.named_container_symbol(receiver, span)?;
        let owner = self
            .borrowed
            .get(&symbol)
            .or_else(|| self.bindings.get(&symbol))
            .copied()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let index = self
            .required(descriptor.index().expression())?
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element = self.ssa_type(parameter, span)?;
        let arg_span = self.span(expression);
        let place = self.value(
            ROp::ContainerElementPlace { owner, index },
            EntityType::Place(element),
            arg_span,
        )?;
        let loan = self.value(
            ROp::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: element,
            },
            arg_span,
        )?;
        Ok(Some((loan, vec![loan], Some(expression))))
    }

    pub(super) fn is_element_target(&self, target: ExpressionId) -> bool {
        let mut root = target;
        while let Ok((Expression::Group { expression: inner }, _)) = self.node(root) {
            root = inner;
        }
        self.inputs.typed.element_place(self.unit(root)).is_some()
    }

    pub(super) fn element_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        value: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let mut root_target = target;
        while let (Expression::Group { expression: inner }, _) = self.node(root_target)? {
            root_target = inner;
        }
        let descriptor = self
            .inputs
            .typed
            .element_place(self.unit(root_target))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let Some(index) = self.required(descriptor.index().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let Some(replacement) = self.required(value)? else {
            return Ok(Lowered::Diverged);
        };
        let element_ty = self.c(descriptor.element_type());
        let replacement = self.adapt(replacement, value, element_ty)?;
        self.deliver(value);
        let owner = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(
            ROp::ContainerReplace {
                owner,
                index,
                value: replacement,
            },
            Vec::new(),
            span,
        )?;
        let replaced = self.collect_drops(UnitDropPoint::AfterReplacement(self.unit(assignment)));
        self.finish_drops(&replaced);
        Ok(Lowered::Unit)
    }

    fn container_append(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_append(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let element_expr = descriptor.element().expression();
        let Some(element) = self.required(element_expr)? else {
            return Ok(Lowered::Diverged);
        };
        let element = self.adapt(element, element_expr, self.c(descriptor.element_type()))?;
        self.deliver(element_expr);
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(
            ROp::ContainerAppend {
                owner: binding,
                element,
            },
            vec![binding],
            span,
        )?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.element()))?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Unit)
    }

    fn container_clear(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_clear(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(ROp::ContainerClear { owner: binding }, vec![binding], span)?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Unit)
    }

    fn container_remove_at(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_remove_at(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let Some(index) = self.required(descriptor.index().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element_ty = self.ssa_type(self.c(descriptor.element_type()), span)?;
        let removed = self.body.slot(EntityType::Value(element_ty));
        self.push(
            ROp::ContainerRemoveAt {
                owner: binding,
                index,
            },
            vec![removed, binding],
            span,
        )?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.index()))?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(removed))
    }

    fn container_remove_first(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_remove_first(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element_ty = self.ssa_type(self.c(descriptor.element_type()), span)?;
        let removed = self.body.slot(EntityType::Value(element_ty));
        self.push(
            ROp::ContainerRemoveFirst { owner: binding },
            vec![removed, binding],
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(removed))
    }

    fn container_remove_last(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_remove_last(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let element_ty = self.ssa_type(self.c(descriptor.element_type()), span)?;
        let removed = self.body.slot(EntityType::Value(element_ty));
        self.push(
            ROp::ContainerRemoveLast { owner: binding },
            vec![removed, binding],
            span,
        )?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Value(removed))
    }

    fn container_insert_at(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let id = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .container_insert_at(id)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = self.named_container_symbol(descriptor.receiver().expression(), span)?;
        let Some(index) = self.required(descriptor.index().expression())? else {
            return Ok(Lowered::Diverged);
        };
        let element_expr = descriptor.element().expression();
        let Some(element) = self.required(element_expr)? else {
            return Ok(Lowered::Diverged);
        };
        let element = self.adapt(element, element_expr, self.c(descriptor.element_type()))?;
        self.deliver(element_expr);
        let binding = *self
            .bindings
            .get(&symbol)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        self.push(
            ROp::ContainerInsertAt {
                owner: binding,
                index,
                element,
            },
            vec![binding],
            span,
        )?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.index()))?;
        self.drops(UnitDropPoint::AfterExpression(descriptor.element()))?;
        self.drops(UnitDropPoint::CallReturn(id))?;
        Ok(Lowered::Unit)
    }

    fn named_container_symbol(
        &self,
        mut expression: ExpressionId,
        span: Span,
    ) -> Result<UnitSymbolId, LoweringError> {
        loop {
            let (payload, node_span) = self.node(expression)?;
            match payload {
                Expression::Name => return self.reference(node_span),
                Expression::Group { expression: inner } => expression = inner,
                _ => return Err(error(LoweringErrorKind::UnsupportedNode, span)),
            }
        }
    }
}
