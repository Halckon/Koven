//! 可选 member 拼写的词法查询；不把普通 member 变成未解析名称错误。
use super::*;
impl Resolver<'_> {
    pub(super) fn record_member_lookup(
        &mut self,
        span: Span,
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        let name = self.sources.slice(span)?;
        let target = self
            .lookup(scope, Namespace::Value, name)
            .map(Self::target)
            .unwrap_or(ReferenceTarget::Unresolved);
        self.value_lookup_hints
            .push(NameReference::new(span, scope, Namespace::Value, target));
        Ok(())
    }
}

impl Resolver<'_> {
    pub(super) fn resolve_extension_this(
        &mut self,
        span: Span,
        scope: ScopeId,
    ) -> Result<(), NameResolutionError> {
        self.check_constant_this(span)?;
        if let Some(Binding::Single(symbol)) = self.lookup(scope, Namespace::Value, "this") {
            self.references.push(NameReference::new(
                span,
                scope,
                Namespace::Value,
                ReferenceTarget::Symbol(*symbol),
            ));
        }
        Ok(())
    }
}
