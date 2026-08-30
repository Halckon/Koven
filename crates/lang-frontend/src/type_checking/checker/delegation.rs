use crate::parser::VariableKind;

use super::*;

impl Checker<'_> {
    pub(super) fn check_delegations(&mut self) -> Result<(), TypeCheckingError> {
        let classifiers = self
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| match node.payload() {
                Item::Classifier(classifier) => Some(classifier.as_ref().clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for classifier in classifiers {
            let NameMarker::Present(owner_span) = classifier.name else {
                continue;
            };
            let Some(owner_symbol) = self.symbol_at(owner_span) else {
                continue;
            };
            let Some(owner) = self.nominal_by_symbol.get(&owner_symbol).copied() else {
                continue;
            };
            for supertype in &classifier.supertypes {
                let Some(delegation) = supertype.delegation else {
                    continue;
                };
                let interface = self.resolve_static_type_ref(supertype.type_ref)?;
                if self.is_error(interface) {
                    continue;
                }
                let target_span = match delegation.target {
                    NameMarker::Present(span)
                    | NameMarker::Missing(span)
                    | NameMarker::Error(span) => span,
                };
                let target = self.reference(target_span, Namespace::Value).cloned();
                let field_symbol = match target {
                    Some(ReferenceTarget::Symbol(symbol))
                        if self.symbol_kinds.get(symbol.index()) == Some(&SymbolKind::Field) =>
                    {
                        Some(symbol)
                    }
                    _ => None,
                };
                let field = field_symbol.and_then(|symbol| {
                    classifier.primary_constructor.as_ref()?.fields.iter().find(|field| {
                        matches!(field.name, NameMarker::Present(span) if self.symbol_at(span) == Some(symbol))
                    }).copied()
                });
                let valid_target = matches!(classifier.kind, ClassifierKind::Class { .. })
                    && matches!(field, Some(field) if field.kind == VariableKind::Val);
                if !valid_target {
                    self.emit(
                        self.invalid_delegation_target_code,
                        "delegation target must be an immutable field of the same primary constructor",
                        target_span,
                    )?;
                    self.invalid_delegations.push((owner, interface));
                    continue;
                }
                let Some(field) = field else {
                    return Err(TypeCheckingError::InvalidExternalBinding);
                };
                let field_type = self.resolve_type_ref(field.type_ref)?;
                if !self.satisfies_interface(field_type, interface)? {
                    self.emit_with_label(
                        self.delegate_interface_mismatch_code,
                        "delegate field type does not satisfy the target interface",
                        target_span,
                        self.ast().type_refs().get(supertype.type_ref)?.span(),
                        "delegated interface declared here",
                    )?;
                    self.invalid_delegations.push((owner, interface));
                    continue;
                }
                self.delegations.push(DelegationPlan {
                    owner,
                    interface,
                    target: field_symbol.ok_or(TypeCheckingError::InvalidExternalBinding)?,
                    delegation_span: delegation.span,
                    by_span: delegation.by_span,
                    forwarders: Vec::new(),
                });
            }
        }
        Ok(())
    }
}
