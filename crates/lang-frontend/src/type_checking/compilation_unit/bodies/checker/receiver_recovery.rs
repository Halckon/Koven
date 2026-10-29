//! 无 receiver 的真实 this 源错误；不把内部缺事实或合法 receiver 当成 Deferred。

use super::{BodyChecker, CompilationUnitTypeError, FlowKey, unwrapped_item};
use crate::{
    diagnostic::{Severity, codes},
    name_resolution::SourceUnitId,
    parser::Item,
    source::Span,
    type_checking::{TypeCheckingError, UnitCallableSignature, UnitCallableTarget, UnitTypeId},
};

impl BodyChecker<'_> {
    /// 只认真实 extension 声明与其既有 signature 错误；缺 binding 本身不是恢复证据。
    pub(super) fn rejected_extension_receiver(
        &self,
        source: SourceUnitId,
        callable: &UnitCallableSignature,
    ) -> Result<bool, CompilationUnitTypeError> {
        if self.current_receiver.is_some()
            || callable.range_extension().is_some()
            || callable.text_extension().is_some()
        {
            return Ok(false);
        }
        let Some(receiver) = callable.receiver() else {
            return Ok(false);
        };
        let UnitCallableTarget::Declaration(id) = callable.target() else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let declaration = self
            .names
            .index()
            .declarations()
            .get(id.index())
            .filter(|declaration| declaration.id() == id && declaration.source_unit() == source)
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        let Item::Function {
            extension_receiver: Some(extension),
            ..
        } = unwrapped_item(self.file(source), declaration.root())?
        else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let receiver_span = self
            .file(source)
            .ast()
            .type_refs()
            .get(extension.type_ref)
            .map_err(TypeCheckingError::from)?
            .span();
        if receiver.declaration_span() != receiver_span
            || !self.signatures.diagnostics().iter().any(|diagnostic| {
                diagnostic.severity() == Severity::Error
                    && diagnostic.primary_span() == extension.dot_span
                    && diagnostic.code().to_string() == codes::UNSUPPORTED_BORROW_FLOW
            })
        {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        }
        Ok(true)
    }

    pub(super) fn this_type(&mut self, span: Span) -> Result<UnitTypeId, CompilationUnitTypeError> {
        if let Some(ty) = self
            .flow_facts
            .get(&FlowKey::This)
            .copied()
            .or(self.current_receiver)
        {
            return Ok(ty);
        }
        if self.current_rejected_extension_receiver {
            return Ok(self.error_type());
        }
        // companion const 的 names producer 已在同一 token 给出 L0153；不叠同根诊断。
        let diagnosed_constant_context = self.names.diagnostics().iter().any(|diagnostic| {
            diagnostic.primary_span() == span
                && diagnostic.code().to_string() == codes::INVALID_CONSTANT_CONTEXT
        });
        if !diagnosed_constant_context {
            self.emit(
                codes::THIS_WITHOUT_RECEIVER,
                "this is not available without a receiver",
                span,
            )?;
        }
        Ok(self.error_type())
    }
}
