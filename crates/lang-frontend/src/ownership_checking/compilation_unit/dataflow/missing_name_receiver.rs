//! 无名实例函数的诊断 receiver；仅从同一真实 AST 与已有 nominal 取上下文。
use crate::{
    ast::ItemId,
    diagnostic::{Severity, codes},
    parser::{DeclarationModifiers, Item, NameMarker, ParameterModeMarker},
    type_checking::{NominalKind, ParameterMode, UnitTypeKind},
};

use super::{Checker, OwnershipCheckingError, ReceiverContext};

impl Checker<'_> {
    pub(super) fn missing_name_receiver(
        &self,
        item: ItemId,
        marker: NameMarker,
    ) -> Result<Option<ReceiverContext>, OwnershipCheckingError> {
        let span = match marker {
            NameMarker::Present(_) => return Ok(None),
            NameMarker::Missing(span) | NameMarker::Error(span) => span,
        };
        if !self.names.diagnostics().iter().any(|diagnostic| {
            let primary = diagnostic.primary_span();
            diagnostic.severity() == Severity::Error
                && primary.source_id() == span.source_id()
                && span.start() <= primary.start()
                && primary.end() <= span.end()
        }) {
            return Err(OwnershipCheckingError::MismatchedCompilationUnitTypes);
        }
        for signature in self.typed.signatures().declarations() {
            let Some(nominal) = signature.nominal() else {
                continue;
            };
            let declaration = self
                .names
                .index()
                .declarations()
                .get(signature.declaration().index())
                .filter(|declaration| declaration.id() == nominal.declaration())
                .ok_or(OwnershipCheckingError::MismatchedCompilationUnitTypes)?;
            if declaration.source_unit() != self.source_unit {
                continue;
            }
            let (root, _) = self.unwrapped_receiver_item(declaration.root())?;
            let Item::Classifier(classifier) = self.parsed.ast().items().get(root)?.payload()
            else {
                return Err(OwnershipCheckingError::MismatchedCompilationUnitTypes);
            };
            let Some(body) = &classifier.body else {
                continue;
            };
            for &member in &body.members {
                let (member, modifiers) = self.unwrapped_receiver_item(member)?;
                if member != item {
                    continue;
                }
                let Item::Function {
                    name,
                    extension_receiver,
                    ..
                } = self.parsed.ast().items().get(member)?.payload()
                else {
                    return Err(OwnershipCheckingError::MismatchedCompilationUnitTypes);
                };
                if *name != marker {
                    return Err(OwnershipCheckingError::MismatchedCompilationUnitTypes);
                }
                if let Some(extension) = extension_receiver {
                    if self.typed.diagnostics().iter().any(|diagnostic| {
                        diagnostic.severity() == Severity::Error
                            && diagnostic.primary_span() == extension.dot_span
                            && diagnostic.code().to_string() == codes::UNSUPPORTED_BORROW_FLOW
                    }) {
                        return Ok(None);
                    }
                    return Err(OwnershipCheckingError::MismatchedCompilationUnitTypes);
                }
                let mode = match modifiers.receiver_mode {
                    None | Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
                    Some(ParameterModeMarker::Own(_)) => ParameterMode::Value,
                    Some(ParameterModeMarker::Inout(_)) => ParameterMode::Inout,
                };
                if nominal.kind() == NominalKind::Object && mode != ParameterMode::Borrow {
                    return Ok(None);
                }
                let ty = if nominal.kind() == NominalKind::Interface {
                    self.typed
                        .types()
                        .find(&UnitTypeKind::StaticSelf(nominal.ty()))
                        .ok_or(OwnershipCheckingError::MismatchedCompilationUnitTypes)?
                } else {
                    nominal.ty()
                };
                return Ok(Some(ReceiverContext {
                    owner: nominal.declaration(),
                    mode,
                    ty,
                    declaration_span: span,
                }));
            }
        }
        // 顶层、companion 或无身份 owner 没有可借用的真实 nominal receiver。
        Ok(None)
    }

    fn unwrapped_receiver_item(
        &self,
        mut item: ItemId,
    ) -> Result<(ItemId, DeclarationModifiers), OwnershipCheckingError> {
        let mut modifiers = DeclarationModifiers::default();
        loop {
            match self.parsed.ast().items().get(item)?.payload() {
                Item::Modified {
                    modifiers: actual,
                    declaration,
                } => {
                    modifiers = *actual;
                    item = *declaration;
                }
                _ => return Ok((item, modifiers)),
            }
        }
    }
}
