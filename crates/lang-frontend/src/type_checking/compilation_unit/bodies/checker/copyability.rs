//! SPEC-0197 compilation-unit body 的封闭 `Copyable` 判定。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    name_resolution::{DeclarationId, UnitSymbolId},
    type_checking::{
        BuiltinType, Capability, CompilationUnitSignatures, Copyability, DeferredReason,
        NominalKind, UnitTypeId, UnitTypeKind, UnitTypeParameterBound,
    },
};

use super::BodyChecker;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnitTransferability {
    Transferable,
    NotTransferable,
    Unknown,
    Error,
}

struct CapabilityQuery<'a> {
    signatures: &'a CompilationUnitSignatures,
}

/// Evidence only: this never changes the public capability classification.
#[derive(Default)]
struct CopyabilityFailureTrace {
    source_error: bool,
    unexplained: bool,
}

impl CapabilityQuery<'_> {
    pub(super) fn copyability_of(&self, ty: UnitTypeId) -> Copyability {
        self.copyability_with(ty, &BTreeMap::new(), &mut BTreeSet::new(), None)
    }

    fn copyability_with(
        &self,
        ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
        mut trace: Option<&mut CopyabilityFailureTrace>,
    ) -> Copyability {
        if let Some(trace) = trace.as_deref_mut() {
            trace.unexplained |=
                !self.complete_type_inputs(ty, substitutions, &mut BTreeSet::new());
        }
        match self.signatures.types().get(ty) {
            Some(
                UnitTypeKind::Builtin(
                    BuiltinType::Byte
                    | BuiltinType::Short
                    | BuiltinType::Int
                    | BuiltinType::Long
                    | BuiltinType::UByte
                    | BuiltinType::UShort
                    | BuiltinType::UInt
                    | BuiltinType::ULong
                    | BuiltinType::Float
                    | BuiltinType::Double
                    | BuiltinType::Boolean
                    | BuiltinType::Char
                    | BuiltinType::Unit
                    | BuiltinType::Nothing
                    | BuiltinType::IntRange
                    | BuiltinType::LongRange,
                )
                | UnitTypeKind::IntegerLiteral(_),
            ) => Copyability::Copyable,
            Some(
                UnitTypeKind::Builtin(BuiltinType::String | BuiltinType::Any)
                | UnitTypeKind::Function { .. }
                | UnitTypeKind::Intrinsic { .. },
            ) => Copyability::MoveOnly,
            Some(UnitTypeKind::Nullable(inner)) => {
                self.copyability_with(*inner, substitutions, active, trace)
            }
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => self.nominal_copyability(*declaration, arguments, substitutions, active, trace),
            Some(UnitTypeKind::EnumCase { root, .. }) => {
                self.copyability_with(*root, substitutions, active, trace)
            }
            Some(UnitTypeKind::TypeParameter(symbol)) => {
                if let Some(actual) = substitutions.get(symbol).copied()
                    && actual != ty
                {
                    return self.copyability_with(actual, substitutions, active, trace);
                }
                match self
                    .signatures
                    .type_parameter(*symbol)
                    .map(|item| item.bound())
                {
                    Some(UnitTypeParameterBound::Capability(Capability::Copyable)) => {
                        Copyability::Copyable
                    }
                    Some(UnitTypeParameterBound::Error) => {
                        if let Some(trace) = trace {
                            trace.unexplained = true;
                        }
                        Copyability::Error
                    }
                    _ => Copyability::MoveOnly,
                }
            }
            Some(UnitTypeKind::Deferred(DeferredReason::AnyValueRepresentation)) => {
                Copyability::MoveOnly
            }
            Some(UnitTypeKind::Deferred(_)) => Copyability::Unknown,
            Some(UnitTypeKind::Error) => {
                if let Some(trace) = trace {
                    trace.source_error = true;
                }
                Copyability::Error
            }
            Some(UnitTypeKind::StaticSelf(_) | UnitTypeKind::Capability(_)) | None => {
                if let Some(trace) = trace {
                    trace.unexplained = true;
                }
                Copyability::Error
            }
        }
    }

    fn nominal_copyability(
        &self,
        declaration: DeclarationId,
        arguments: &[UnitTypeId],
        outer: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
        mut trace: Option<&mut CopyabilityFailureTrace>,
    ) -> Copyability {
        if self
            .signatures
            .invalid_inline_nominals()
            .contains(&declaration)
            || !active.insert(declaration)
        {
            if let Some(trace) = trace {
                trace.unexplained = true;
            }
            return Copyability::Error;
        }
        let Some(nominal) = self
            .signatures
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
        else {
            active.remove(&declaration);
            if let Some(trace) = trace {
                trace.unexplained = true;
            }
            return Copyability::Error;
        };
        let result = match nominal.kind() {
            NominalKind::Class | NominalKind::Object => Copyability::MoveOnly,
            NominalKind::Interface => {
                if let Some(trace) = trace.as_deref_mut() {
                    trace.unexplained = true;
                }
                Copyability::Error
            }
            NominalKind::ValueClass | NominalKind::EnumClass => {
                if let Some(trace) = trace.as_deref_mut() {
                    // Never let zip silently certify missing/extra arguments or replace an
                    // unrelated outer mapping. These checks affect evidence, not the result.
                    trace.unexplained |= nominal.type_parameters().len() != arguments.len()
                        || nominal
                            .type_parameters()
                            .iter()
                            .any(|p| outer.contains_key(p));
                    let fields = nominal
                        .fields()
                        .iter()
                        .chain(nominal.enum_cases().iter().flat_map(|case| case.payloads()));
                    trace.unexplained |= fields.into_iter().any(|field| {
                        self.signatures.symbol_type(field.symbol()) != Some(field.ty())
                    });
                }
                let mut substitutions = outer.clone();
                substitutions.extend(
                    nominal
                        .type_parameters()
                        .iter()
                        .copied()
                        .zip(arguments.iter().copied()),
                );
                let components = if nominal.kind() == NominalKind::ValueClass {
                    nominal
                        .fields()
                        .iter()
                        .map(|field| field.ty())
                        .collect::<Vec<_>>()
                } else {
                    nominal
                        .enum_cases()
                        .iter()
                        .flat_map(|case| case.payloads().iter().map(|payload| payload.ty()))
                        .collect::<Vec<_>>()
                };
                components
                    .into_iter()
                    .fold(Copyability::Copyable, |state, component| {
                        combine(
                            state,
                            self.copyability_with(
                                component,
                                &substitutions,
                                active,
                                trace.as_deref_mut(),
                            ),
                        )
                    })
            }
        };
        active.remove(&declaration);
        result
    }

    /// Structural completeness is a veto only, never evidence of a source Error.
    /// Do not substitute/intern: missing IDs must not turn into fallback Error nodes.
    /// Nominal layouts are visited by the capability query above, not a second algorithm.
    fn complete_type_inputs(
        &self,
        ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> bool {
        if !active.insert(ty) {
            return false;
        }
        let complete = match self.signatures.types().get(ty) {
            Some(
                UnitTypeKind::Builtin(_) | UnitTypeKind::IntegerLiteral(_) | UnitTypeKind::Error,
            ) => true,
            Some(UnitTypeKind::Nullable(inner)) => {
                self.complete_type_inputs(*inner, substitutions, active)
            }
            Some(UnitTypeKind::Function {
                parameters,
                return_type,
                ..
            }) => {
                parameters
                    .iter()
                    .all(|p| self.complete_type_inputs(p.ty(), substitutions, active))
                    && self.complete_type_inputs(*return_type, substitutions, active)
            }
            Some(UnitTypeKind::Intrinsic { arguments, .. }) => arguments
                .iter()
                .all(|argument| self.complete_type_inputs(*argument, substitutions, active)),
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => {
                self.complete_nominal_identity(*declaration, arguments.len())
                    && arguments
                        .iter()
                        .all(|argument| self.complete_type_inputs(*argument, substitutions, active))
            }
            Some(UnitTypeKind::TypeParameter(symbol)) => {
                let identity = self
                    .signatures
                    .type_parameter(*symbol)
                    .is_some_and(|parameter| {
                        parameter.symbol() == *symbol
                            && !matches!(
                                parameter.bound(),
                                UnitTypeParameterBound::Error
                                    | UnitTypeParameterBound::Interface(_)
                            )
                    })
                    && self.signatures.symbol_type(*symbol) == Some(ty);
                identity
                    && match substitutions.get(symbol).copied() {
                        Some(actual) if actual != ty => {
                            self.complete_type_inputs(actual, substitutions, active)
                        }
                        _ => true,
                    }
            }
            // Refinement identity and static/interface capabilities are not part of
            // this finite nominal-field/payload source-error proof.
            Some(
                UnitTypeKind::EnumCase { .. }
                | UnitTypeKind::StaticSelf(_)
                | UnitTypeKind::Capability(_)
                | UnitTypeKind::Deferred(_),
            )
            | None => false,
        };
        active.remove(&ty);
        complete
    }

    fn complete_nominal_identity(&self, declaration: DeclarationId, arity: usize) -> bool {
        let Some(signature) = self.signatures.declaration(declaration) else {
            return false;
        };
        let Some(nominal) = signature.nominal() else {
            return false;
        };
        if signature.declaration() != declaration
            || nominal.declaration() != declaration
            || self
                .signatures
                .invalid_inline_nominals()
                .contains(&declaration)
            || signature.symbol() != nominal.symbol()
            || signature.ty() != nominal.ty()
            || self.signatures.symbol_type(nominal.symbol()) != Some(nominal.ty())
            || nominal.type_parameters().len() != arity
        {
            return false;
        }
        let Some(UnitTypeKind::Nominal {
            declaration: own,
            arguments,
        }) = self.signatures.types().get(nominal.ty())
        else {
            return false;
        };
        let mut parameters = BTreeSet::new();
        *own == declaration
            && arguments.len() == arity
            && nominal
                .type_parameters()
                .iter()
                .zip(arguments)
                .all(|(symbol, ty)| {
                    parameters.insert(*symbol)
                        && self.signatures.symbol_type(*symbol) == Some(*ty)
                        && matches!(
                            self.signatures.types().get(*ty),
                            Some(UnitTypeKind::TypeParameter(actual)) if actual == symbol
                        )
                        && self
                            .signatures
                            .type_parameter(*symbol)
                            .is_some_and(|parameter| {
                                parameter.symbol() == *symbol
                                    && !matches!(
                                        parameter.bound(),
                                        UnitTypeParameterBound::Error
                                            | UnitTypeParameterBound::Interface(_)
                                    )
                            })
                })
    }

    pub(super) fn transferability_of(&self, ty: UnitTypeId) -> UnitTransferability {
        self.transferability_with(ty, &BTreeMap::new(), &mut BTreeSet::new())
    }

    fn transferability_with(
        &self,
        ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
    ) -> UnitTransferability {
        match self.signatures.types().get(ty) {
            Some(
                UnitTypeKind::Builtin(
                    BuiltinType::Byte
                    | BuiltinType::Short
                    | BuiltinType::Int
                    | BuiltinType::Long
                    | BuiltinType::UByte
                    | BuiltinType::UShort
                    | BuiltinType::UInt
                    | BuiltinType::ULong
                    | BuiltinType::Float
                    | BuiltinType::Double
                    | BuiltinType::Boolean
                    | BuiltinType::Char
                    | BuiltinType::String
                    | BuiltinType::Unit
                    | BuiltinType::Nothing
                    | BuiltinType::IntRange
                    | BuiltinType::LongRange,
                )
                | UnitTypeKind::IntegerLiteral(_),
            ) => UnitTransferability::Transferable,
            Some(UnitTypeKind::Builtin(BuiltinType::Any) | UnitTypeKind::Function { .. }) => {
                UnitTransferability::NotTransferable
            }
            Some(UnitTypeKind::Nullable(inner)) => {
                self.transferability_with(*inner, substitutions, active)
            }
            Some(UnitTypeKind::Intrinsic {
                constructor:
                    crate::type_checking::IntrinsicTypeConstructor::Rc
                    | crate::type_checking::IntrinsicTypeConstructor::View,
                ..
            }) => UnitTransferability::NotTransferable,
            Some(UnitTypeKind::Intrinsic { arguments, .. }) => {
                arguments
                    .iter()
                    .fold(UnitTransferability::Transferable, |state, argument| {
                        combine_transferability(
                            state,
                            self.transferability_with(*argument, substitutions, active),
                        )
                    })
            }
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => self.nominal_transferability(*declaration, arguments, substitutions, active),
            Some(UnitTypeKind::EnumCase { root, .. }) => {
                self.transferability_with(*root, substitutions, active)
            }
            Some(UnitTypeKind::TypeParameter(symbol)) => {
                if let Some(actual) = substitutions.get(symbol).copied()
                    && actual != ty
                {
                    return self.transferability_with(actual, substitutions, active);
                }
                match self
                    .signatures
                    .type_parameter(*symbol)
                    .map(|item| item.bound())
                {
                    Some(UnitTypeParameterBound::Capability(Capability::Transferable)) => {
                        UnitTransferability::Transferable
                    }
                    Some(UnitTypeParameterBound::Error) => UnitTransferability::Error,
                    _ => UnitTransferability::NotTransferable,
                }
            }
            Some(UnitTypeKind::Deferred(DeferredReason::AnyValueRepresentation)) => {
                UnitTransferability::NotTransferable
            }
            Some(UnitTypeKind::Deferred(_)) => UnitTransferability::Unknown,
            Some(
                UnitTypeKind::Error | UnitTypeKind::StaticSelf(_) | UnitTypeKind::Capability(_),
            )
            | None => UnitTransferability::Error,
        }
    }

    fn nominal_transferability(
        &self,
        declaration: DeclarationId,
        arguments: &[UnitTypeId],
        outer: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
    ) -> UnitTransferability {
        if self
            .signatures
            .invalid_inline_nominals()
            .contains(&declaration)
        {
            return UnitTransferability::Error;
        }
        if !active.insert(declaration) {
            return UnitTransferability::Transferable;
        }
        let Some(nominal) = self
            .signatures
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
        else {
            active.remove(&declaration);
            return UnitTransferability::Error;
        };
        let result = match nominal.kind() {
            NominalKind::Object => UnitTransferability::NotTransferable,
            NominalKind::Interface => UnitTransferability::Error,
            NominalKind::Class | NominalKind::ValueClass | NominalKind::EnumClass => {
                let mut substitutions = outer.clone();
                substitutions.extend(
                    nominal
                        .type_parameters()
                        .iter()
                        .copied()
                        .zip(arguments.iter().copied()),
                );
                let components = if nominal.kind() == NominalKind::EnumClass {
                    nominal
                        .enum_cases()
                        .iter()
                        .flat_map(|case| case.payloads().iter().map(|payload| payload.ty()))
                        .collect::<Vec<_>>()
                } else {
                    nominal
                        .fields()
                        .iter()
                        .map(|field| field.ty())
                        .collect::<Vec<_>>()
                };
                components.into_iter().fold(
                    UnitTransferability::Transferable,
                    |state, component| {
                        combine_transferability(
                            state,
                            self.transferability_with(component, &substitutions, active),
                        )
                    },
                )
            }
        };
        active.remove(&declaration);
        result
    }
}

pub(in crate::type_checking::compilation_unit) fn unit_copyability(
    signatures: &CompilationUnitSignatures,
    ty: UnitTypeId,
) -> Copyability {
    CapabilityQuery { signatures }.copyability_of(ty)
}

pub(in crate::type_checking::compilation_unit) fn unit_copyability_error_is_diagnostic(
    signatures: &CompilationUnitSignatures,
    ty: UnitTypeId,
) -> bool {
    let mut trace = CopyabilityFailureTrace::default();
    let result = CapabilityQuery { signatures }.copyability_with(
        ty,
        &BTreeMap::new(),
        &mut BTreeSet::new(),
        Some(&mut trace),
    );
    result == Copyability::Error && trace.source_error && !trace.unexplained
}

pub(in crate::type_checking) fn unit_transferability(
    signatures: &CompilationUnitSignatures,
    ty: UnitTypeId,
) -> UnitTransferability {
    CapabilityQuery { signatures }.transferability_of(ty)
}

impl BodyChecker<'_> {
    pub(super) fn copyability_of(&self, ty: UnitTypeId) -> Copyability {
        unit_copyability(&self.signatures, ty)
    }

    pub(super) fn transferability_of(&self, ty: UnitTypeId) -> UnitTransferability {
        CapabilityQuery {
            signatures: &self.signatures,
        }
        .transferability_of(ty)
    }
}

fn combine(left: Copyability, right: Copyability) -> Copyability {
    match (left, right) {
        (Copyability::Error, _) | (_, Copyability::Error) => Copyability::Error,
        (Copyability::Unknown, _) | (_, Copyability::Unknown) => Copyability::Unknown,
        (Copyability::MoveOnly, _) | (_, Copyability::MoveOnly) => Copyability::MoveOnly,
        (Copyability::Copyable, Copyability::Copyable) => Copyability::Copyable,
    }
}

fn combine_transferability(
    left: UnitTransferability,
    right: UnitTransferability,
) -> UnitTransferability {
    match (left, right) {
        (UnitTransferability::Error, _) | (_, UnitTransferability::Error) => {
            UnitTransferability::Error
        }
        (UnitTransferability::Unknown, _) | (_, UnitTransferability::Unknown) => {
            UnitTransferability::Unknown
        }
        (UnitTransferability::NotTransferable, _) | (_, UnitTransferability::NotTransferable) => {
            UnitTransferability::NotTransferable
        }
        (UnitTransferability::Transferable, UnitTransferability::Transferable) => {
            UnitTransferability::Transferable
        }
    }
}
