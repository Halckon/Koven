use std::collections::BTreeSet;

use lang_frontend::{
    ast::{ExpressionId, TypeRefId},
    ownership_checking::ConstructionRootKind,
    parser::Expression,
    type_checking::{
        Copyability, IntrinsicTypeConstructor, NominalKind, UnitAggregateProjectionKind,
        UnitAggregateProjectionReceiver, UnitConstructionTarget, UnitEnumCaseSignature,
        UnitNominalSignature, UnitTypeId, UnitTypeKind, UnitTypeRefId,
    },
};

use super::Checker;
use crate::ssa::unit_plan::resolve_enum_runtime_payload_types;

impl Checker<'_, '_> {
    /// A concrete Copyable `enum class` whose payloads are all scalars.
    pub(super) fn enum_type(&self, ty: UnitTypeId) -> bool {
        self.enum_signature(ty).is_some()
            && self.copyability_of(ty) == Copyability::Copyable
            && self
                .enum_payload_types(ty)
                .is_some_and(|cases| cases.iter().flatten().all(|payload| self.scalar(*payload)))
    }

    /// A concrete MoveOnly `enum class` carrying at least one `Box<T>` payload and only
    /// supported storage-type payloads (including recursive boxed enum cases).
    pub(super) fn box_enum_type(&self, ty: UnitTypeId) -> bool {
        self.box_enum_type_seen(ty, &mut BTreeSet::new())
    }

    pub(super) fn box_enum_type_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> bool {
        let Some(cases) = self
            .enum_signature(ty)
            .filter(|_| self.copyability_of(ty) == Copyability::MoveOnly)
            .and_then(|_| self.enum_payload_types(ty))
        else {
            return false;
        };
        if !active.insert(ty) {
            return true;
        }
        let has_box = cases.iter().flatten().any(|payload| {
            matches!(
                self.kind(*payload),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Box,
                    ..
                })
            )
        });
        let ok = has_box
            && cases
                .iter()
                .flatten()
                .all(|payload| self.storage_type_seen(*payload, active));
        active.remove(&ty);
        ok
    }

    pub(super) fn enum_signature(
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
        (nominal.kind() == NominalKind::EnumClass
            && arguments.len() == nominal.type_parameters().len())
        .then(|| (nominal, arguments.clone()))
    }

    pub(super) fn enum_payload_types(&self, ty: UnitTypeId) -> Option<Vec<Vec<UnitTypeId>>> {
        let span = self.inputs.span;
        resolve_enum_runtime_payload_types(self.inputs.typed, ty, span).ok()
    }

    /// The case of `root` whose type-test symbol is the case type `case_type`.
    fn case_of_type(
        &self,
        root: UnitTypeId,
        case_type: UnitTypeId,
    ) -> Option<&UnitEnumCaseSignature> {
        let Some(UnitTypeKind::EnumCase { case, root: own }) = self.kind(case_type) else {
            return None;
        };
        let (nominal, _) = (self.c(*own) == root)
            .then(|| self.enum_signature(root))
            .flatten()?;
        nominal
            .enum_cases()
            .iter()
            .find(|candidate| candidate.type_symbol() == *case)
    }

    /// `this` of an enum method whose receiver is in the subset.
    pub(super) fn this_expression(&self, expression: ExpressionId) -> bool {
        self.inputs.receiver.is_some_and(|receiver| {
            self.expression_type(expression) == Some(receiver.ty) && self.enum_type(receiver.ty)
        })
    }

    /// `E.C(...)` / `C`: fields delivered in source order, no root obligation (Copyable)
    /// or `Inline` root obligation (MoveOnly boxed enum).
    pub(super) fn enum_construction(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        let Some(descriptor) = typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
        else {
            return false;
        };
        let result = self.c(descriptor.result_type());
        let UnitConstructionTarget::EnumCase(target) = descriptor.target() else {
            return false;
        };
        let Some((nominal, type_arguments)) = self.enum_signature(result) else {
            return false;
        };
        let Some((index, case)) = nominal
            .enum_cases()
            .iter()
            .enumerate()
            .find(|(_, case)| case.value_symbol() == target)
        else {
            return false;
        };
        let payload_symbols: Vec<_> = case.payloads().iter().map(|field| field.symbol()).collect();
        let is_box_enum = self.box_enum_type(result);
        if !type_arguments.is_empty() || is_box_enum {
            self.migrated = true;
        }
        let Some(payloads) = self
            .enum_payload_types(result)
            .and_then(|cases| cases.get(index).cloned())
        else {
            return false;
        };
        let mut plans = self
            .inputs
            .owned
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == id);
        let (Some(plan), None) = (plans.next(), plans.next()) else {
            return false;
        };
        let instance_arguments: Vec<_> = descriptor
            .instance()
            .type_arguments()
            .iter()
            .map(|ty| self.c(*ty))
            .collect();
        let root_ok = if self.enum_type(result) {
            plan.root_obligation().is_none()
        } else if is_box_enum {
            plan.root_obligation().is_some_and(|root| {
                root.construction() == id
                    && self.c(root.result_type()) == result
                    && root.kind() == ConstructionRootKind::Inline
            })
        } else {
            false
        };
        let identity = root_ok
            && self.expression_type(expression) == Some(result)
            && instance_arguments == type_arguments
            && descriptor.arguments().len() == payloads.len()
            && plan.target() == descriptor.target()
            && plan.terminating_operand().is_none()
            && plan.deliveries().len() == payloads.len();
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
                    && payloads.get(argument.parameter_index()) == Some(&parameter)
                    && payload_symbols.get(argument.parameter_index()).copied()
                        == argument.parameter_symbol()
                    && delivery.construction() == id
                    && delivery.argument() == value
                    && delivery.parameter_index() == argument.parameter_index()
                    && delivery.evaluation_index() == index
                    && self.adaptable(value.expression(), parameter)
                    && self.expression(value.expression())
            })
    }

    /// A payloadless case value compared with an enum subject: identity only, never built.
    pub(super) fn case_value_condition(
        &self,
        expression: ExpressionId,
        subject: UnitTypeId,
    ) -> bool {
        let id = self.id(expression);
        self.inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .is_some_and(|descriptor| {
                matches!(descriptor.target(), UnitConstructionTarget::EnumCase(_))
                    && descriptor.arguments().is_empty()
                    && self.c(descriptor.result_type()) == subject
                    && self.expression_type(expression) == Some(subject)
            })
    }

    /// `is C` / `!is C` against a case of the subject's own enum root.
    pub(super) fn type_test_target(&self, type_ref: TypeRefId, subject: UnitTypeId) -> bool {
        self.inputs
            .typed
            .type_ref_type(UnitTypeRefId::new(self.inputs.source_unit, type_ref))
            .is_some_and(|target| self.case_of_type(subject, target).is_some())
    }

    /// `operand is C` as a Boolean value: a stable enum place (`this` or a known name).
    pub(super) fn type_test_expression(
        &mut self,
        operand: ExpressionId,
        type_ref: TypeRefId,
    ) -> bool {
        self.migrated = true;
        let Some(operand_type) = self.expression_type(operand) else {
            return false;
        };
        self.enum_type(operand_type)
            && self.type_test_target(type_ref, operand_type)
            && self.enum_place(operand)
    }

    /// `this` or a known local/parameter name, read without a drop of its own.
    fn enum_place(&mut self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::This => self.this_expression(expression),
            Expression::Name => self.expression(expression),
            _ => false,
        }
    }

    /// A scalar payload read from a receiver the flow facts narrowed to one case:
    /// `x.f` on a narrowed name or `this`, or bare `f` (implicit `this`).
    pub(super) fn enum_projection(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        let Some(projection) = typed.aggregate_projection(id) else {
            return false;
        };
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.expression_type_of(id) != Some(self.c(projection.ty()))
            || !self.scalar(self.c(projection.ty()))
        {
            return false;
        }
        let root = match projection.receiver() {
            UnitAggregateProjectionReceiver::This(owner) => {
                let Some(receiver) = self.inputs.receiver else {
                    return false;
                };
                if receiver.owner != owner || !self.enum_type(receiver.ty) {
                    return false;
                }
                receiver.ty
            }
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return false;
                }
                let Some(UnitTypeKind::EnumCase { root, .. }) = self
                    .inputs
                    .typed
                    .expression_type(receiver)
                    .and_then(|ty| self.kind(ty))
                else {
                    return false;
                };
                let root = self.c(*root);
                if !self.enum_type(root) || !self.plain(receiver.expression()) {
                    return false;
                }
                let Ok(node) = self
                    .inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(receiver.expression())
                else {
                    return false;
                };
                let place = match node.payload() {
                    Expression::This => self
                        .inputs
                        .receiver
                        .is_some_and(|this| this.ty == root && self.enum_type(root)),
                    Expression::Name => {
                        self.reference(receiver.expression()).is_some_and(|symbol| {
                            self.known.contains(&symbol) && self.symbol_type(symbol) == Some(root)
                        })
                    }
                    _ => false,
                };
                if !place {
                    return false;
                }
                root
            }
        };
        if self
            .enum_signature(root)
            .is_some_and(|(_, arguments)| !arguments.is_empty())
        {
            self.migrated = true;
        }
        self.payload_case(root, projection.field()).is_some()
    }

    /// The unique case of `root` declaring payload `field`, with the field's index.
    pub(super) fn payload_case(
        &self,
        root: UnitTypeId,
        field: lang_frontend::name_resolution::UnitSymbolId,
    ) -> Option<(usize, usize)> {
        let (nominal, _) = self.enum_signature(root)?;
        let mut found = nominal
            .enum_cases()
            .iter()
            .enumerate()
            .filter_map(|(case, signature)| {
                signature
                    .payloads()
                    .iter()
                    .position(|payload| payload.symbol() == field)
                    .map(|index| (case, index))
            });
        match (found.next(), found.next()) {
            (Some(found), None) => Some(found),
            _ => None,
        }
    }
}
