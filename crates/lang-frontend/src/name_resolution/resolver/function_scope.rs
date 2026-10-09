//! 函数作用域：泛型声明先于 receiver、参数、结果与 body 的名称解析。
use super::*;
impl Resolver<'_> {
    pub(super) fn resolve_function(
        &mut self,
        span: Span,
        parent: ScopeId,
        type_parameters: &[TypeParameter],
        receiver_type: Option<TypeRefId>,
        parameters: &[ValueParameter],
        form: FunctionForm,
    ) -> Result<(), NameResolutionError> {
        let scope = self.add_scope(Some(parent), ScopeKind::Function, Some(span));
        self.resolve_type_parameters(type_parameters, scope)?;
        if let Some(receiver_type) = receiver_type {
            self.resolve_type(receiver_type, scope)?;
            let anchor = self.ast().type_refs().get(receiver_type)?.span();
            let symbol = SymbolId(self.symbols.len());
            self.symbols
                .push(Symbol::synthetic_receiver(symbol, anchor, scope));
            self.scopes[scope.index()]
                .values
                .insert("this".to_owned(), Binding::Single(symbol));
            self.receiver_symbols
                .insert((anchor.start(), anchor.end()), symbol);
        }
        for parameter in parameters {
            self.resolve_type(parameter.type_ref, scope)?;
            self.insert_marker(
                scope,
                parameter.name,
                Namespace::Value,
                SymbolKind::ValueParameter,
            )?;
        }
        match form {
            FunctionForm::ImplicitUnitAbsent => Ok(()),
            FunctionForm::ImplicitUnitBlock(body) => self.resolve_statement(body, scope),
            FunctionForm::Explicit { type_ref, body, .. } => {
                self.resolve_type(type_ref, scope)?;
                match body {
                    FunctionBody::Absent => Ok(()),
                    FunctionBody::Expression { expression, .. } => {
                        self.resolve_expression(expression, scope)
                    }
                    FunctionBody::Block(body) => self.resolve_statement(body, scope),
                }
            }
        }
    }
}
