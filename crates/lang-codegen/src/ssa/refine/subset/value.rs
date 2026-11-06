//! Subset rules for Copyable `value class` (SPEC-0361): inline construction, field reads by
//! copy and value-class receivers. They mirror `build/value.rs`.
use std::collections::BTreeSet;

use lang_frontend::{
    ast::ExpressionId,
    parser::Expression,
    type_checking::{
        Copyability, NominalKind, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitConstructionTarget, UnitNominalSignature, UnitTypeId, UnitTypeKind,
    },
};

use super::{Checker, receiver::Owner};
use crate::ssa::unit_plan::resolve_nominal_runtime_field_types;

impl Checker<'_, '_> {
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
        use lang_frontend::name_resolution::{Namespace, SymbolKind, UnitReferenceTarget};
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
        self.migrated = true;
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
        descriptor.components().iter().all(|component| {
            let ty = self.c(component.ty());
            self.note_nullable(ty);
            self.known.insert(component.symbol());
            self.symbol_type(component.symbol()) == Some(ty) && self.storage_type(ty)
        })
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
