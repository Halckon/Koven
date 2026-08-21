use crate::parser::TypeRef;

use super::*;

impl Checker<'_> {
    pub(super) fn compute_interface_closures(&mut self) -> Result<(), TypeCheckingError> {
        let snapshot = self.nominals.clone();
        let mut memo = BTreeMap::new();
        for owner in snapshot.iter().map(NominalDescriptor::id) {
            let closure = self.interface_closure_for(owner, &snapshot, &mut memo)?;
            let descriptor = self
                .nominals
                .iter_mut()
                .find(|descriptor| descriptor.id() == owner)
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            descriptor.interfaces = closure;
        }
        Ok(())
    }

    /// Expands each validated interface DAG node once; additional work is proportional to the
    /// concrete closure entries that the typed result must materialize.
    fn interface_closure_for(
        &mut self,
        owner: NominalId,
        snapshot: &[NominalDescriptor],
        memo: &mut BTreeMap<NominalId, Vec<TypeId>>,
    ) -> Result<Vec<TypeId>, TypeCheckingError> {
        if let Some(closure) = memo.get(&owner) {
            return Ok(closure.clone());
        }
        let descriptor = snapshot
            .iter()
            .find(|descriptor| descriptor.id() == owner)
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        let direct_interfaces = descriptor.direct_interfaces().to_vec();
        let mut closure = direct_interfaces.clone();
        for direct in direct_interfaces {
            let TypeKind::Nominal {
                nominal: target,
                arguments,
            } = self.kind(direct).clone()
            else {
                continue;
            };
            let target_descriptor = snapshot
                .iter()
                .find(|descriptor| descriptor.id() == target)
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            let substitutions = target_descriptor
                .type_parameters()
                .iter()
                .copied()
                .zip(arguments)
                .collect::<BTreeMap<_, _>>();
            for inherited in self.interface_closure_for(target, snapshot, memo)? {
                let inherited = self.substitute_type(inherited, &substitutions)?;
                if !closure.contains(&inherited) {
                    closure.push(inherited);
                }
            }
        }
        memo.insert(owner, closure.clone());
        Ok(closure)
    }

    pub(super) fn validate_type_argument_bounds(&mut self) -> Result<(), TypeCheckingError> {
        let uses = self
            .ast()
            .type_refs()
            .iter()
            .map(|(id, node)| (id, node.payload().clone()))
            .collect::<Vec<_>>();
        for (id, payload) in uses {
            let TypeRef::Qualified { segments, .. } = payload else {
                continue;
            };
            let [segment] = segments.as_slice() else {
                continue;
            };
            let Some(ty) = self.type_ref_types[id.index()] else {
                continue;
            };
            let ty = match self.kind(ty) {
                TypeKind::Nullable(inner) => *inner,
                _ => ty,
            };
            let TypeKind::Nominal { nominal, arguments } = self.kind(ty).clone() else {
                continue;
            };
            let descriptor = self
                .nominals
                .iter()
                .find(|descriptor| descriptor.id() == nominal)
                .cloned()
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            let substitutions = descriptor
                .type_parameters()
                .iter()
                .copied()
                .zip(arguments.iter().copied())
                .collect::<BTreeMap<_, _>>();
            for (index, (&parameter, &argument)) in descriptor
                .type_parameters()
                .iter()
                .zip(&arguments)
                .enumerate()
            {
                let Some(parameter) = self
                    .type_parameters
                    .iter()
                    .find(|descriptor| descriptor.symbol() == parameter)
                    .copied()
                else {
                    return Err(TypeCheckingError::InvalidExternalBinding);
                };
                let TypeParameterBound::Interface(bound) = parameter.bound() else {
                    continue;
                };
                let bound = self.substitute_type(bound, &substitutions)?;
                if !self.is_error(argument) && !self.satisfies_interface(argument, bound)? {
                    self.emit_with_label(
                        self.type_argument_bound_code,
                        "type argument does not satisfy its interface bound",
                        self.ast().type_refs().get(segment.arguments[index])?.span(),
                        self.symbol_spans[parameter.symbol().index()],
                        "interface bound declared here",
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn substitute_type(
        &mut self,
        ty: TypeId,
        substitutions: &BTreeMap<SymbolId, TypeId>,
    ) -> Result<TypeId, TypeCheckingError> {
        let kind = self.kind(ty).clone();
        let substituted = match kind {
            TypeKind::TypeParameter(symbol) => {
                return Ok(substitutions.get(&symbol).copied().unwrap_or(ty));
            }
            TypeKind::StaticSelf(interface) => {
                TypeKind::StaticSelf(self.substitute_type(interface, substitutions)?)
            }
            TypeKind::Nullable(inner) => {
                TypeKind::Nullable(self.substitute_type(inner, substitutions)?)
            }
            TypeKind::Function {
                move_only,
                parameters,
                return_type,
            } => TypeKind::Function {
                move_only,
                parameters: parameters
                    .into_iter()
                    .map(|parameter| {
                        Ok(FunctionParameterType {
                            mode: parameter.mode,
                            ty: self.substitute_type(parameter.ty, substitutions)?,
                        })
                    })
                    .collect::<Result<Vec<_>, TypeCheckingError>>()?,
                return_type: self.substitute_type(return_type, substitutions)?,
            },
            TypeKind::Nominal { nominal, arguments } => TypeKind::Nominal {
                nominal,
                arguments: arguments
                    .into_iter()
                    .map(|argument| self.substitute_type(argument, substitutions))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            other => return Ok(self.types.intern(other)),
        };
        Ok(self.types.intern(substituted))
    }

    pub(super) fn satisfies_interface(
        &mut self,
        actual: TypeId,
        expected: TypeId,
    ) -> Result<bool, TypeCheckingError> {
        if actual == expected {
            return Ok(true);
        }
        match self.kind(actual).clone() {
            TypeKind::Nominal { nominal, arguments } => {
                let descriptor = self
                    .nominals
                    .iter()
                    .find(|descriptor| descriptor.id() == nominal)
                    .cloned()
                    .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                let substitutions = descriptor
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments)
                    .collect::<BTreeMap<_, _>>();
                for interface in descriptor.interfaces {
                    if self.substitute_type(interface, &substitutions)? == expected {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            TypeKind::TypeParameter(symbol) => {
                let bound = self
                    .type_parameters
                    .iter()
                    .find(|descriptor| descriptor.symbol() == symbol)
                    .map(|descriptor| descriptor.bound());
                let Some(TypeParameterBound::Interface(bound)) = bound else {
                    return Ok(false);
                };
                self.satisfies_interface(bound, expected)
            }
            _ => Ok(false),
        }
    }
}
