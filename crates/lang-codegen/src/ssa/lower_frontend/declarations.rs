//! Source function declaration collection, separate from lowering orchestration.
use super::{LoweringError, LoweringErrorKind, error, orchestrate::unwrap_modified, present_name};
use lang_frontend::{
    ast::ItemId,
    name_resolution::{NameResolution, SymbolId, SymbolKind},
    parser::{ClassifierKind, Item, ParsedFile},
    source::Span,
    type_checking::{CallableDescriptor, TypedFile},
};

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
    Ok(functions)
}
