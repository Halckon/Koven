//! Source function declaration collection, statement dispatch, and constant materialization.
use super::{
    ExpressionLowerer, LoweredValue, LoweringError, LoweringErrorKind, error,
    orchestrate::unwrap_modified, present_name, value,
};
use crate::ssa::{
    constant_materialization,
    model::EntityType,
    statement_core::StatementLowering,
};
use lang_frontend::{
    ast::{ExpressionId, ItemId, StatementId},
    name_resolution::{NameResolution, SymbolId, SymbolKind},
    ownership_checking::DropPoint,
    parser::{ClassifierKind, Item, ParsedFile},
    source::Span,
    type_checking::{CallableDescriptor, TypedFile},
};

impl StatementLowering for ExpressionLowerer<'_> {
    fn lower_inner(&mut self, statement: StatementId) -> Result<LoweredValue, LoweringError> {
        self.lower_statement_inner(statement)
    }

    fn emit_after_statement(&mut self, statement: StatementId) -> Result<(), LoweringError> {
        self.emit_drops(DropPoint::AfterStatement(statement))
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn lower_constant(
        &mut self,
        expression: ExpressionId,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let Some(descriptor) = self
            .typed
            .constants()
            .and_then(|facts| facts.use_at(expression))
        else {
            return Ok(None);
        };
        let span = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let plan = self
            .owned
            .constant_materializations()
            .and_then(|facts| facts.plan_at(expression))
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let planned = plan.descriptor();
        if planned.expression() != descriptor.expression()
            || planned.target() != descriptor.target()
            || planned.ty() != descriptor.ty()
            || planned.value() != descriptor.value()
        {
            return Err(error(LoweringErrorKind::MismatchedAnalysis, span));
        }
        let ty = self.expression_ssa_type(expression, span)?;
        let operation = constant_materialization::operation(plan.kind(), descriptor.value(), ty)
            .map_err(|kind| error(kind, span))?;
        let (_, results) = self.append(operation, vec![EntityType::Value(ty)], span)?;
        Ok(Some(LoweredValue::Value(value(results[0]))))
    }
}

pub(super) struct FunctionDeclaration {
    pub(super) item_id: ItemId,
    pub(super) item: Item,
    pub(super) symbol: SymbolId,
    pub(super) callable: CallableDescriptor,
    pub(super) span: Span,
}

pub(super) fn collect_functions(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
) -> Result<Vec<FunctionDeclaration>, LoweringError> {
    let mut functions = Vec::new();
    for root in parsed.roots() {
        let node = parsed.ast().items().get(*root).map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let (item, span) = unwrap_modified(parsed, *root)?;
        // Entry drop facts refer to the function itself, not its modifier wrapper.
        let mut item_id = *root;
        while let Item::Modified { declaration, .. } = parsed
            .ast()
            .items()
            .get(item_id)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?
            .payload()
        {
            item_id = *declaration;
        }
        let name = match &item {
            Item::Function { name, .. } => name,
            Item::Constant { .. } => continue,
            Item::Classifier(declaration)
                if matches!(
                    declaration.kind,
                    ClassifierKind::Object { .. }
                        | ClassifierKind::Class { .. }
                        | ClassifierKind::ValueClass { .. }
                        | ClassifierKind::Interface { .. }
                        | ClassifierKind::EnumClass { .. }
                ) =>
            {
                continue;
            }
            _ => return Err(error(LoweringErrorKind::UnsupportedNode, node.span())),
        };
        let name_span =
            present_name(*name).ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let symbol = names
            .symbols()
            .iter()
            .find(|symbol| symbol.span() == name_span && symbol.kind() == SymbolKind::Function)
            .map(|symbol| symbol.id())
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, name_span))?;
        let callable = typed
            .callables()
            .iter()
            .find(|callable| callable.symbol() == symbol && callable.owner().is_none())
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, name_span))?;
        functions.push(FunctionDeclaration {
            item_id,
            item,
            symbol,
            callable,
            span,
        });
    }
    // Member templates are indexed by their selected callable identity, but are not roots.
    // Only a real source call will enqueue one in the instance planner.
    for callable in typed.callables() {
        if super::readonly_receiver::admitted(typed, callable).is_none() {
            continue;
        }
        let symbol = names
            .symbols()
            .get(callable.symbol().index())
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        let (item_id, node) = parsed
            .ast()
            .items()
            .iter()
            .find(|(_, node)| {
                matches!(node.payload(), Item::Function { name, .. }
                if present_name(*name) == Some(symbol.span()))
            })
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, symbol.span()))?;
        functions.push(FunctionDeclaration {
            item_id,
            item: node.payload().clone(),
            symbol: callable.symbol(),
            callable: callable.clone(),
            span: node.span(),
        });
    }
    functions.sort_by_key(|declaration| declaration.span.start());
    Ok(functions)
}
