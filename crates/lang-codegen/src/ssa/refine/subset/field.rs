//! Subset rules for local variables, assignments, field replacement (SPEC-0358), and drop facts.
use std::collections::BTreeSet;

use lang_frontend::{
    ast::{ExpressionId, ItemId},
    name_resolution::SymbolKind,
    ownership_checking::{UnitDropPoint, UnitDropTarget},
    parser::{AssignmentOperator, Expression, Item, NameMarker, VariableKind},
    type_checking::{
        CallableResultSource, Copyability, NominalKind, ParameterMode,
        UnitAggregateProjectionKind, UnitAggregateProjectionReceiver, UnitCallTarget,
        UnitConstructionTarget, UnitExpressionId, UnitNominalSignature, UnitTypeId, UnitTypeKind,
    },
};

use super::{Checker, receiver::Owner};
use crate::ssa::{refine::RefineBody, unit_plan::resolve_nominal_runtime_field_types};

impl Checker<'_, '_> {
    pub(super) fn local_variable(&mut self, mut declaration: ItemId) -> bool {
        while let Ok(node) = self.inputs.parsed.ast().items().get(declaration) {
            if let Item::Modified {
                declaration: inner, ..
            } = node.payload()
            {
                declaration = *inner;
            } else {
                break;
            }
        }
        let Ok(node) = self.inputs.parsed.ast().items().get(declaration) else {
            return false;
        };
        let Item::Variable {
            name: NameMarker::Present(name_span),
            initializer,
            kind,
            ..
        } = node.payload().clone()
        else {
            return false;
        };
        let initializer_id = self.id(initializer);
        if matches!(kind, VariableKind::BorrowVal(_))
            && self.inputs.typed.expression_falls_through(initializer_id) == Some(false)
        {
            // The initializer never establishes the binding or its result loan.
            return self.expression(initializer);
        }
        if let Some(fact) = self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|fact| fact.initializer() == initializer_id)
        {
            return self.local_borrow_binding(fact.binding(), initializer);
        }
        if self
            .inputs
            .sources
            .slice(name_span)
            .is_ok_and(|name| name == "_")
        {
            return false;
        }
        let symbol = self
            .inputs
            .names
            .names()
            .source_units()
            .get(self.inputs.source_unit.index())
            .and_then(|source| {
                source.resolution().symbols().iter().find(|symbol| {
                    symbol.span() == name_span && symbol.kind() == SymbolKind::Variable
                })
            })
            .and_then(|local| {
                self.inputs
                    .typed
                    .body_symbol_types()
                    .keys()
                    .copied()
                    .find(|symbol| {
                        symbol.source_unit() == self.inputs.source_unit
                            && symbol.symbol() == local.id()
                    })
            });
        let Some(symbol) = symbol else {
            return false;
        };
        let Some(declared) = self.symbol_type(symbol) else {
            return false;
        };
        self.note_nullable(declared);
        if self.view_or_function(declared) {
            return self.callable_local(symbol, initializer);
        }
        self.migrated |= self.class_has_deinit(declared) || self.range_type(declared);
        let supported = (self.unit(declared)
            || self.storage_type(declared)
            || self.nothing(declared)
            || self
                .expression_type(initializer)
                .is_some_and(|ty| self.nothing(ty)))
            && self.expression(initializer);
        self.known.insert(symbol);
        supported
    }

    pub(super) fn assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        value: ExpressionId,
    ) -> bool {
        let Ok(target_node) = self.inputs.parsed.ast().expressions().get(target) else {
            return false;
        };
        if !matches!(target_node.payload(), Expression::Name) {
            return false;
        }
        let Some(symbol) = self.reference(target) else {
            return false;
        };
        let (assignment_id, target_id, value_id) =
            (self.id(assignment), self.id(target), self.id(value));
        let Some(descriptor) = self.inputs.typed.assignment(assignment_id) else {
            return false;
        };
        let target_ty = self.c(descriptor.target_type());
        if descriptor.expression() != assignment_id
            || descriptor.target() != target_id
            || descriptor.value() != value_id
            || descriptor.operator() != AssignmentOperator::Assign
            || !self.known.contains(&symbol)
            || self.callables.contains(&symbol)
            || self.symbol_type(symbol) != Some(target_ty)
            || !self.storage_type(target_ty)
            || !self.adaptable(value, target_ty)
            || (self.loops > 0
                && self.view_symbols.contains(&symbol)
                && !self.scope_requires_refine())
        {
            return false;
        }
        let mode_ok = match self.parameter_mode(symbol) {
            Some(ParameterMode::Inout) => {
                self.migrated = true;
                self.copyability_of(target_ty) == Copyability::Copyable
            }
            None => {
                self.migrated |= self.copyability_of(target_ty) == Copyability::MoveOnly;
                true
            }
            Some(_) => false,
        };
        let ok = mode_ok && self.expression(value);
        self.view_symbols.remove(&symbol);
        ok
    }

    pub(super) fn compound_assignment(
        &mut self,
        target: ExpressionId,
        _operator: AssignmentOperator,
        value: ExpressionId,
    ) -> bool {
        let Ok(target_node) = self.inputs.parsed.ast().expressions().get(target) else {
            return false;
        };
        if !matches!(target_node.payload(), Expression::Name) {
            return false;
        }
        let Some(symbol) = self.reference(target) else {
            return false;
        };
        let Some(target_ty) = self.expression_type(target) else {
            return false;
        };
        if !self.known.contains(&symbol)
            || self.callables.contains(&symbol)
            || self.for_borrowed.contains(&symbol)
            || self.parameter_mode(symbol).is_some()
            || self.symbol_type(symbol) != Some(target_ty)
            || !self.integer_type(target_ty)
            || self.expression_type(value) != Some(target_ty)
            || (self.loops > 0
                && self.view_symbols.contains(&symbol)
                && !self.scope_requires_refine())
        {
            return false;
        }
        let ok = self.expression(value);
        self.view_symbols.remove(&symbol);
        ok
    }

    /// Whether the assignment target is a field projection (not a binding).
    pub(super) fn is_field_target(&self, target: ExpressionId) -> bool {
        self.inputs
            .typed
            .aggregate_projection(self.id(target))
            .is_some()
    }

    /// `owner.f = v`: `f` is a scalar or String field of the owner's plain class; the RHS is
    /// in the subset.
    pub(super) fn field_assignment(
        &mut self,
        assignment: ExpressionId,
        target: ExpressionId,
        value: ExpressionId,
    ) -> bool {
        self.migrated = true;
        let typed = self.inputs.typed;
        let (assignment_id, target_id) = (self.id(assignment), self.id(target));
        let (Some(descriptor), Some(projection)) = (
            typed.assignment(assignment_id),
            typed.aggregate_projection(target_id),
        ) else {
            return false;
        };
        let field_type = self.c(projection.ty());
        if descriptor.target() != target_id
            || descriptor.value() != self.id(value)
            || descriptor.operator() != AssignmentOperator::Assign
            || self.c(descriptor.target_type()) != field_type
            || projection.kind() != UnitAggregateProjectionKind::Field
            || !self.storage_type(field_type)
        {
            return false;
        }
        let (owner, is_this) = match projection.receiver() {
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return false;
                }
                let is_this = self
                    .inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(self.ungrouped(receiver.expression()))
                    .is_ok_and(|node| matches!(node.payload(), Expression::This));
                (Owner::Expression(receiver.expression()), is_this)
            }
            UnitAggregateProjectionReceiver::This(declaration) => (Owner::This(declaration), true),
        };
        let owner_ok = self
            .owner_type(owner, ParameterMode::Inout)
            .is_some_and(|owner| {
                self.class_field(owner, projection.field())
                    || (is_this
                        && (self.value_class(owner) || self.move_value_class(owner))
                        && self.value_signature(owner).is_some_and(|(nominal, args)| {
                            args.is_empty()
                                && nominal
                                    .fields()
                                    .iter()
                                    .any(|field| field.symbol() == projection.field())
                        }))
            });
        if !owner_ok || !self.expression(value) {
            return false;
        }
        self.replacements.insert(assignment_id);
        true
    }

    /// Drop facts must name plain bindings or temporaries at points the builder emits.
    pub(super) fn drops_supported(&self) -> bool {
        self.inputs.owned.drops().iter().all(|fact| {
            let point_ok = match fact.point() {
                // This cleanup must run before the selected arm starts.
                UnitDropPoint::BranchEntry { .. } => false,
                UnitDropPoint::AfterExpression(expression)
                | UnitDropPoint::AfterBinaryOperands(expression) => {
                    !self.branch_form.contains(&expression)
                }
                UnitDropPoint::BeforeReplacement(assignment) => {
                    self.replacements.contains(&assignment)
                        && matches!(fact.target(), UnitDropTarget::ReplacedField { .. })
                }
                UnitDropPoint::LambdaEntry(lambda) => {
                    matches!(&self.inputs.body, RefineBody::Lambda(thunk) if thunk.expression == lambda)
                }
                UnitDropPoint::AfterReplacement(assignment) => {
                    self.element_replacements.contains(&assignment)
                        && fact.target() == UnitDropTarget::ReplacedElement(assignment)
                }
                // A jump out of a call argument drops the earlier MoveOnly arguments it
                // abandons; temporaries holding loans stay outside.
                UnitDropPoint::ControlTransfer(_) => match fact.target() {
                    UnitDropTarget::Temporary(expression) => self.pending_owner(expression),
                    _ => true,
                },
                _ => true,
            };
            let target_ok = match fact.target() {
                UnitDropTarget::Named(_)
                | UnitDropTarget::Temporary(_)
                | UnitDropTarget::ReplacedField { .. } => true,
                UnitDropTarget::ReplacedCapture { .. } => false,
                UnitDropTarget::This(owner) => self.inputs.receiver.is_some_and(|receiver| {
                    receiver.owner == owner && receiver.mode == ParameterMode::Value
                }),
                UnitDropTarget::ReplacedElement(assignment) => {
                    self.element_replacements.contains(&assignment)
                }
                // Released by the drop glue of a closure formed in this function.
                UnitDropTarget::Captured { closure, .. } => {
                    self.inputs.callables.contains_key(&closure)
                        || matches!(
                            &self.inputs.body,
                            RefineBody::Lambda(thunk) if thunk.expression == closure
                        )
                }
            };
            !self.fact_in_function(fact.value_origin())
                || self.inside_lambda(fact.value_origin())
                || (point_ok && target_ok)
        }) && self
            .inputs
            .owned
            .conditional_receiver_drops()
            .iter()
            .all(|fact| {
                !self.fact_in_function(fact.value_origin())
                    || self.inside_lambda(fact.value_origin())
                    || (!match fact.point() {
                        UnitDropPoint::AfterExpression(expression)
                        | UnitDropPoint::AfterBinaryOperands(expression) => {
                            self.branch_form.contains(&expression)
                        }
                        _ => false,
                    } && self.inputs.receiver.is_some_and(|receiver| {
                        receiver.owner == fact.owner()
                            && receiver.mode == ParameterMode::Value
                            && receiver.ty == self.c(fact.receiver_type())
                    }))
            })
    }

    /// A pending argument a jump may abandon: a MoveOnly named owner or non-borrowed
    /// temporary moved into a Value argument. A borrowed temporary stays outside unless the
    /// legacy path can no longer lower its call: a function-value call (captured closures
    /// lower only here), a `RangeView` carrier call, or a borrow-result call proven not to
    /// dispatch. The ordinary exit ends those loans before dropping the temporary.
    fn pending_owner(&self, expression: UnitExpressionId) -> bool {
        if self.temporary_field_parents.contains(&expression) {
            return true;
        }
        if expression.source_unit() != self.inputs.source_unit {
            return false;
        }
        let Ok(node) = self
            .inputs
            .parsed
            .ast()
            .expressions()
            .get(expression.expression())
        else {
            return false;
        };
        let span = node.span();
        // The innermost call lending the temporary decides.
        let expressions = self.inputs.parsed.ast().expressions();
        let lender = self
            .inputs
            .owned
            .loans()
            .iter()
            .filter_map(|loan| {
                (loan.argument().source_unit() == self.inputs.source_unit)
                    .then(|| expressions.get(loan.argument().expression()).ok())
                    .flatten()
                    .filter(|arg| {
                        arg.span().start() <= span.start() && span.end() <= arg.span().end()
                    })
                    .map(|arg| (arg.span().end() - arg.span().start(), loan.call()))
            })
            .min_by_key(|(width, _)| *width);
        let ty = self.expression_type(expression.expression());
        let borrowed = lender.is_some_and(|(_, call)| {
            let abandoned_callback = ty.is_some_and(|ty| self.view_function(ty))
                && self.inputs.owned.call_may_dispatch(call) == Some(false);
            !abandoned_callback
                && !self.abandoned_borrow_result_call(call)
                && !self.inputs.typed.call(call).is_some_and(|call| {
                    call.target() == UnitCallTarget::FunctionValue
                        || matches!(call.result_source(), CallableResultSource::Carrier(_))
                })
        });
        !borrowed && ty.is_some_and(|ty| self.copyability_of(ty) == Copyability::MoveOnly)
    }

    fn fact_in_function(&self, origin: lang_frontend::source::Span) -> bool {
        self.inputs.scope_span().is_some_and(|item| {
            origin.source_id() == item.source_id()
                && item.start() <= origin.start()
                && origin.end() <= item.end()
                && (!matches!(self.inputs.body, RefineBody::Lambda(_)) || origin != item)
        })
    }

    /// The expression inside any number of grouping parentheses.
    pub(super) fn ungrouped(&self, mut expression: ExpressionId) -> ExpressionId {
        while let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) {
            let Expression::Group { expression: inner } = node.payload() else {
                break;
            };
            expression = *inner;
        }
        expression
    }

    pub(super) fn value_signature(
        &self,
        ty: UnitTypeId,
    ) -> Option<(&UnitNominalSignature, Vec<UnitTypeId>)> {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.kind(ty)
        else {
            return None;
        };
        let nominal = self
            .inputs
            .typed
            .signatures()
            .declaration(*declaration)?
            .nominal()?;
        (nominal.kind() == NominalKind::ValueClass).then(|| (nominal, arguments.clone()))
    }

    /// Concrete field types of a value class, in declaration order.
    pub(super) fn value_fields(&self, ty: UnitTypeId) -> Option<Vec<UnitTypeId>> {
        let (nominal, arguments) = self.value_signature(ty)?;
        resolve_nominal_runtime_field_types(self.inputs.typed, ty, nominal, &arguments).ok()
    }

    /// A concrete Copyable value class whose fields are supported storage types.
    pub(super) fn value_class(&self, ty: UnitTypeId) -> bool {
        self.value_class_seen(ty, &mut BTreeSet::new())
    }

    pub(super) fn value_class_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> bool {
        let Some(fields) = self
            .value_fields(ty)
            .filter(|_| self.inputs.typed.copyability(ty) == Copyability::Copyable)
        else {
            return false;
        };
        if !active.insert(ty) {
            return false;
        }
        let ok = fields
            .iter()
            .all(|field| self.storage_type_seen(*field, active));
        active.remove(&ty);
        ok
    }

    /// A concrete MoveOnly value class whose fields are supported storage types.
    pub(super) fn move_value_class(&self, ty: UnitTypeId) -> bool {
        self.move_value_class_seen(ty, &mut BTreeSet::new())
    }

    pub(super) fn move_value_class_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> bool {
        let Some(fields) = self
            .value_fields(ty)
            .filter(|_| self.inputs.typed.copyability(ty) == Copyability::MoveOnly)
        else {
            return false;
        };
        if !active.insert(ty) {
            return false;
        }
        let ok = fields
            .iter()
            .all(|field| self.storage_type_seen(*field, active));
        active.remove(&ty);
        ok
    }

    /// A stateless `object` singleton (`NominalKind::Object` with no fields or type arguments).
    pub(super) fn object_type(&self, ty: UnitTypeId) -> bool {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.kind(ty)
        else {
            return false;
        };
        arguments.is_empty()
            && self
                .inputs
                .typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .is_some_and(|nominal| {
                    nominal.kind() == NominalKind::Object && nominal.fields().is_empty()
                })
    }

    pub(super) fn object_name(&self, expression: ExpressionId) -> bool {
        use lang_frontend::name_resolution::{Namespace, UnitReferenceTarget};
        let root = self.ungrouped(expression);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        if !matches!(node.payload(), Expression::Name) {
            return false;
        }
        let Some(ty) = self
            .expression_type(root)
            .filter(|ty| self.object_type(*ty))
        else {
            return false;
        };
        let Some(UnitTypeKind::Nominal {
            declaration: type_decl,
            ..
        }) = self.kind(ty)
        else {
            return false;
        };
        let declarations = self.inputs.names.names().index().declarations();
        let Some(type_record) = declarations.get(type_decl.index()) else {
            return false;
        };
        self.inputs
            .names
            .names()
            .references()
            .iter()
            .find(|reference| {
                reference.source_unit() == self.inputs.source_unit
                    && reference.namespace() == Some(Namespace::Value)
                    && reference.span() == node.span()
            })
            .and_then(|reference| match reference.target() {
                UnitReferenceTarget::Declaration(decl) => declarations.get(decl.index()),
                _ => None,
            })
            .is_some_and(|value_record| {
                value_record.kind() == SymbolKind::ObjectValue
                    && value_record.source_unit() == type_record.source_unit()
                    && value_record.root() == type_record.root()
            })
    }

    /// `V(fields...)`: every field delivered in evaluation order.
    pub(super) fn value_construction(&mut self, expression: ExpressionId) -> bool {
        self.migrated = true;
        let id = self.id(expression);
        let Some(descriptor) = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
        else {
            return false;
        };
        let result = self.c(descriptor.result_type());
        let UnitConstructionTarget::Nominal(_) = descriptor.target() else {
            return false;
        };
        let (Some((nominal, _)), Some(fields)) =
            (self.value_signature(result), self.value_fields(result))
        else {
            return false;
        };
        let symbols: Vec<_> = nominal
            .fields()
            .iter()
            .map(|field| field.symbol())
            .collect();
        let mut plans = self
            .inputs
            .owned
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == id);
        let (Some(plan), None) = (plans.next(), plans.next()) else {
            return false;
        };
        let root_ok = if self.value_class(result) {
            plan.root_obligation().is_none()
        } else if self.move_value_class(result) {
            plan.root_obligation().is_some_and(|root| {
                root.construction() == id
                    && self.c(root.result_type()) == result
                    && root.kind()
                        == lang_frontend::ownership_checking::ConstructionRootKind::Inline
            })
        } else {
            false
        };
        let identity = root_ok
            && self.expression_type(expression) == Some(result)
            && descriptor.arguments().len() == fields.len()
            && plan.target() == descriptor.target()
            && plan.terminating_operand().is_none()
            && plan.deliveries().len() == fields.len();
        if !identity {
            return false;
        }
        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        arguments
            .into_iter()
            .zip(plan.deliveries().to_vec())
            .enumerate()
            .all(|(index, (argument, delivery))| {
                let value = argument.argument();
                let parameter = self.c(argument.parameter_type());
                argument.evaluation_index() == index
                    && value.source_unit() == self.inputs.source_unit
                    && fields.get(argument.parameter_index()) == Some(&parameter)
                    && symbols.get(argument.parameter_index()).copied()
                        == argument.parameter_symbol()
                    && delivery.construction() == id
                    && delivery.argument() == value
                    && delivery.parameter_index() == argument.parameter_index()
                    && delivery.evaluation_index() == index
                    && self.adaptable(value.expression(), parameter)
                    && self.expression(value.expression())
            })
    }

    pub(super) fn local_destructuring(
        &mut self,
        statement: lang_frontend::ast::StatementId,
        initializer: ExpressionId,
    ) -> bool {
        let statement_id =
            lang_frontend::type_checking::UnitStatementId::new(self.inputs.source_unit, statement);
        let Some(descriptor) = self.inputs.typed.destructuring(statement_id).cloned() else {
            return false;
        };
        let Some(init_ty) = self.expression_type(initializer) else {
            return false;
        };
        let mode_ok = match descriptor.mode() {
            lang_frontend::type_checking::DestructuringMode::Copy => self.value_class(init_ty),
            lang_frontend::type_checking::DestructuringMode::Consume => {
                self.move_value_class(init_ty)
            }
        };
        if descriptor.statement() != statement_id || !mode_ok || !self.expression(initializer) {
            return false;
        }
        let ok = descriptor.components().iter().all(|component| {
            let ty = self.c(component.ty());
            self.note_nullable(ty);
            self.known.insert(component.symbol());
            self.symbol_type(component.symbol()) == Some(ty) && self.storage_type(ty)
        });
        self.has_local_destructuring |=
            ok && (self.loop_route == super::LoopRoute::Existing || descriptor.is_reachable());
        self.migrated |= ok;
        ok
    }

    /// The value-class owner of a projection, read by copy: a known name or the current
    /// receiver (explicit or implicit `this`).
    pub(super) fn value_projection(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        let Some(projection) = typed.aggregate_projection(id) else {
            return false;
        };
        let field_type = self.c(projection.ty());
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.expression_type_of(id) != Some(field_type)
            || !(self.scalar(field_type)
                || self.range_type(field_type)
                || self.value_class(field_type))
        {
            return false;
        }
        let (owner, is_this) = match projection.receiver() {
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return false;
                }
                let receiver_expr = receiver.expression();
                let inner = self.ungrouped(receiver_expr);
                if typed.aggregate_projection(self.id(inner)).is_some() {
                    let Some(owner_type) = self.expression_type(inner) else {
                        return false;
                    };
                    if !self.value_class(owner_type) || !self.value_projection(inner) {
                        return false;
                    }
                    self.migrated = true;
                    return self
                        .value_signature(owner_type)
                        .is_some_and(|(nominal, _)| {
                            nominal
                                .fields()
                                .iter()
                                .any(|field| field.symbol() == projection.field())
                        });
                }
                if !self.plain(receiver_expr) {
                    return false;
                }
                let Ok(node) = self.inputs.parsed.ast().expressions().get(inner) else {
                    return false;
                };
                match node.payload() {
                    Expression::Name => {
                        let Some(symbol) = self.reference(inner) else {
                            return false;
                        };
                        if !self.known.contains(&symbol)
                            || self.callables.contains(&symbol)
                            || typed.non_null_use(self.id(inner)).is_some_and(|fact| {
                                fact.symbol() != symbol
                                    || self.copyability_of(self.c(fact.narrowed_type()))
                                        != Copyability::Copyable
                            })
                        {
                            return false;
                        }
                        (Owner::Expression(inner), false)
                    }
                    Expression::This => (Owner::Expression(inner), true),
                    _ => return false,
                }
            }
            UnitAggregateProjectionReceiver::This(declaration) => (Owner::This(declaration), true),
        };
        let Some(owner_type) =
            self.owner_type(owner, lang_frontend::type_checking::ParameterMode::Borrow)
        else {
            return false;
        };
        self.migrated = true;
        (self.value_class(owner_type) || self.move_value_class(owner_type))
            && self
                .value_signature(owner_type)
                .is_some_and(|(nominal, args)| {
                    (!self.inputs.receiver.is_some_and(|r| {
                        r.mode == lang_frontend::type_checking::ParameterMode::Inout && is_this
                    }) || args.is_empty())
                        && nominal
                            .fields()
                            .iter()
                            .any(|field| field.symbol() == projection.field())
                })
    }
}
