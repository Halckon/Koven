//! Feature-only preparation from the original Compatibility call site, not routing authority.
use super::*;
use crate::ssa::d10_project_audit::compatibility::Pending;
use lang_frontend::{
    parser::StringPart,
    type_checking::{CallableResultSource, UnitCallTarget},
};
impl UnitExpressionLowerer<'_> {
    pub(super) fn compatibility_call_audit(&self, call: UnitExpressionId) -> Option<Pending> {
        if !self.migrated_family_compatibility
            || !self.substitutions.is_empty()
            || self.static_self.is_some()
        {
            return None;
        }
        let callable = self.owned.d10_literal_call_link(call).ok()?;
        let key = self.source_plan.source(self.source_token)?;
        let UnitCallableTarget::Declaration(declaration) = key.target() else {
            return None;
        };
        if *key != UnitFunctionInstanceKey::for_entry(declaration)
            || self.function_ids.get(key) != Some(&self.function.id)
        {
            return None;
        }
        let declaration = self
            .names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|d| d.id() == declaration)?;
        if UnitItemId::new(declaration.source_unit(), declaration.root()) != callable
            || callable.source_unit() != call.source_unit()
        {
            return None;
        }
        let descriptor = self.typed.call(call)?;
        if descriptor.receiver().is_some()
            || descriptor.aborts()
            || descriptor.result_source() != CallableResultSource::Owned
            || !matches!(descriptor.target(), UnitCallTarget::Declaration(_))
            || !descriptor.instance().type_arguments().is_empty()
        {
            return None;
        }
        let Expression::Call { arguments: raw, .. } = self
            .parsed
            .ast()
            .expressions()
            .get(call.expression())
            .ok()?
            .payload()
        else {
            return None;
        };
        if raw.is_empty() || raw.len() != descriptor.arguments().len() {
            return None;
        }
        let source_text = self.sources.source_text(self.parsed.source_id()).ok()?;
        let mut arguments = Vec::new();
        for (i, (raw, argument)) in raw.iter().zip(descriptor.arguments()).enumerate() {
            let Expression::String { parts, .. } = self
                .parsed
                .ast()
                .expressions()
                .get(raw.value)
                .ok()?
                .payload()
            else {
                return None;
            };
            if !parts.iter().all(|p| matches!(p, StringPart::Text(_)))
                || argument.argument_index() != i
                || argument.mode() != ParameterMode::Borrow
                || argument.category() != ExpressionCategory::Temporary
                || argument.crosses_thread()
            {
                return None;
            }
            arguments.push((
                UnitExpressionId::new(self.source_unit, raw.value),
                argument.parameter_index(),
                string_literal::decode_plain(self.parsed, source_text, raw.value).ok()??,
            ));
        }
        let returns_value = match builtin_type(self.typed, descriptor.return_type())? {
            BuiltinType::Int => true,
            BuiltinType::Unit => false,
            _ => return None,
        };
        let site = self.source_plan.call_site(self.source_token, call)?;
        if !site.delegation().is_empty() {
            return None;
        }
        let callee = *self.function_ids.get(site.dispatch_key()?)?;
        if callee == self.function.id {
            return None;
        }
        Some(Pending {
            caller: self.function.id,
            callable,
            call,
            callee,
            arguments,
            returns_value,
            block: self.block,
            start: self.function.block(self.block)?.instructions.len(),
        })
    }
}
