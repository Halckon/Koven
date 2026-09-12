//! 关联常量声明索引与类型阶段选择；不创建 object receiver 或运行时字段投影。

mod expressions;
mod graph;

use crate::parser::{ClassifierBody, Expression, VisibilityModifier};

use super::*;

pub(super) struct AssociatedNamespace {
    span: Span,
    constants: BTreeMap<String, (SymbolId, bool)>,
    functions: BTreeSet<String>,
}

impl Checker<'_> {
    /// 使用 AST 声明归属建索引，object 的 type/value 双身份归一到同一声明。
    pub(super) fn collect_associated_constants(&mut self) -> Result<(), TypeCheckingError> {
        let classifiers = self
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| {
                if let Item::Classifier(classifier) = node.payload() {
                    Some((node.span(), classifier.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for (span, classifier) in classifiers {
            let NameMarker::Present(name_span) = classifier.name else {
                continue;
            };
            let Some(owner) = self.symbol_at(name_span) else {
                continue;
            };
            let mut namespace = AssociatedNamespace {
                span,
                constants: BTreeMap::new(),
                functions: BTreeSet::new(),
            };
            if let Some(body) = classifier.body {
                if matches!(classifier.kind, ClassifierKind::Object { .. }) {
                    self.index_associated_body(&body, &mut namespace)?;
                } else {
                    for member in body.members {
                        let (item, _) = self.associated_item(member)?;
                        if let Item::Companion(companion) = item {
                            self.index_associated_body(&companion.body, &mut namespace)?;
                        }
                    }
                }
            }
            self.associated_constants.insert(owner, namespace);
        }
        Ok(())
    }

    fn associated_item(&self, id: ItemId) -> Result<(Item, bool), TypeCheckingError> {
        let mut item = self.ast().items().get(id)?.payload().clone();
        let mut private = false;
        while let Item::Modified {
            modifiers,
            declaration,
        } = item
        {
            private |= matches!(modifiers.visibility, Some(VisibilityModifier::Private(_)));
            item = self.ast().items().get(declaration)?.payload().clone();
        }
        Ok((item, private))
    }

    fn index_associated_body(
        &self,
        body: &ClassifierBody,
        namespace: &mut AssociatedNamespace,
    ) -> Result<(), TypeCheckingError> {
        for &member in &body.members {
            let (item, private) = self.associated_item(member)?;
            match item {
                Item::Constant {
                    name: NameMarker::Present(span),
                    ..
                } => {
                    if let Some(symbol) = self.symbol_at(span) {
                        namespace
                            .constants
                            .insert(self.sources.slice(span)?.to_owned(), (symbol, private));
                    }
                }
                Item::Function {
                    name: NameMarker::Present(span),
                    ..
                } => {
                    namespace
                        .functions
                        .insert(self.sources.slice(span)?.to_owned());
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn check_associated_constant(
        &mut self,
        expression: ExpressionId,
        receiver: ExpressionId,
        name_span: Span,
        safe: bool,
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        if safe {
            return Ok(None);
        }
        let node = self.ast().expressions().get(receiver)?;
        if !matches!(node.payload(), Expression::Name) {
            return Ok(None);
        }
        let target = self
            .reference(node.span(), Namespace::Value)
            .or_else(|| self.reference(node.span(), Namespace::Type));
        let Some(ReferenceTarget::Symbol(symbol)) = target else {
            return Ok(None);
        };
        if !matches!(
            self.symbol_kinds[symbol.index()],
            SymbolKind::Classifier | SymbolKind::ObjectValue
        ) {
            return Ok(None);
        }
        let Some(owner) = self.symbol_at(self.symbol_spans[symbol.index()]) else {
            return Ok(None);
        };
        let Some(namespace) = self.associated_constants.get(&owner) else {
            return Ok(None);
        };
        let name = self.sources.slice(name_span)?;
        let selected = namespace.constants.get(name).copied();
        if selected.is_none() && namespace.functions.contains(name) {
            // 关联函数仍交给既有 callable 路径，本索引不扩展其调用能力。
            return Ok(None);
        }
        let inside_owner =
            namespace.span.start() <= name_span.start() && name_span.end() <= namespace.span.end();
        let ty = match selected {
            Some((_, true)) if !inside_owner => {
                self.emit(
                    self.invisible_constant_code,
                    "associated constant is private to its declaring classifier",
                    name_span,
                )?;
                self.error_type()
            }
            Some((symbol, _)) => {
                self.associated_constant_uses
                    .insert(expression.index(), symbol);
                self.symbol_type(symbol)
                    .unwrap_or_else(|| self.deferred(DeferredReason::ForwardValueType))
            }
            None => {
                self.emit(
                    self.unresolved_constant_code,
                    "unresolved associated constant",
                    name_span,
                )?;
                self.error_type()
            }
        };
        Ok(Some(ExprCheck {
            ty,
            falls_through: true,
        }))
    }
}
