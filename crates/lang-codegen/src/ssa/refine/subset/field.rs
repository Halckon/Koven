//! Subset rules for field replacement (SPEC-0358): `owner.f = v` on a plain class owner held
//! by a known local, an Inout parameter or the Inout receiver. They mirror `build/field.rs`.
use lang_frontend::{
    ast::{ExpressionId, ItemId},
    name_resolution::SymbolKind,
    parser::{AssignmentOperator, Expression, Item, NameMarker},
    type_checking::{
        Copyability, ParameterMode, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
    },
};

use super::{Checker, receiver::Owner};

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
            ..
        } = node.payload().clone()
        else {
            return false;
        };
        let initializer_id = self.id(initializer);
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
        let supported =
            (self.unit(declared) || self.storage_type(declared) || self.nothing(declared))
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
}
