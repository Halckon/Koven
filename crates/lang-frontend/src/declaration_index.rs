//! Syntax-only declaration views retaining the parser's source identity.
use crate::{
    ast::AstError,
    parser::{FunctionBody, FunctionForm, Item, NameMarker, ParameterModeMarker, ParsedFile},
    source::Span,
};

/// A top-level ordinary function; all text remains in the original SourceMap.
pub struct FunctionDeclaration {
    /// Complete declaration range, including modifiers and body.
    pub span: Span,
    /// Function name token.
    pub name: Span,
    /// Exclusive end of the signature, before a body delimiter.
    pub signature_end: usize,
    /// Source-ordered parameters.
    pub parameters: Vec<ParameterDeclaration>,
}

/// A parameter's declared syntax, without type or ownership inference.
pub struct ParameterDeclaration {
    /// Name token.
    pub name: Span,
    /// Type syntax.
    pub type_span: Span,
    /// Explicit source marker; None preserves omission.
    pub mode: Option<ParameterModeMarker>,
}

/// Every root is either represented or explicitly omitted; members are not traversed.
pub struct DeclarationIndex {
    /// Source-ordered ordinary functions.
    pub functions: Vec<FunctionDeclaration>,
    /// Root spans outside this bounded extraction scope.
    pub omitted: Vec<Span>,
}

/// Borrow the existing AST without reparsing text or making semantic claims.
/// Callers must reject parser diagnostics before publishing this view.
pub fn index_declarations(parsed: &ParsedFile) -> Result<DeclarationIndex, AstError> {
    let ast = parsed.ast();
    let mut index = DeclarationIndex {
        functions: Vec::new(),
        omitted: Vec::new(),
    };
    for root in parsed.roots() {
        let outer = ast.items().get(*root)?;
        let mut node = outer;
        while let Item::Modified { declaration, .. } = node.payload() {
            node = ast.items().get(*declaration)?;
        }
        let Item::Function {
            name: NameMarker::Present(name),
            extension_receiver: None,
            parameters,
            form,
            ..
        } = node.payload()
        else {
            index.omitted.push(outer.span());
            continue;
        };
        let body = match form {
            FunctionForm::ImplicitUnitAbsent => FunctionBody::Absent,
            FunctionForm::ImplicitUnitBlock(id) => FunctionBody::Block(*id),
            FunctionForm::Explicit { body, .. } => *body,
        };
        let signature_end = match body {
            FunctionBody::Absent => node.span().end(),
            FunctionBody::Expression { equals_span, .. } => equals_span.start(),
            FunctionBody::Block(id) => ast.statements().get(id)?.span().start(),
        };
        let mut exported = Vec::new();
        for parameter in parameters {
            let NameMarker::Present(name) = parameter.name else {
                continue;
            };
            exported.push(ParameterDeclaration {
                name,
                type_span: ast.type_refs().get(parameter.type_ref)?.span(),
                mode: parameter.mode_marker,
            });
        }
        index.functions.push(FunctionDeclaration {
            span: outer.span(),
            name: *name,
            signature_end,
            parameters: exported,
        });
    }
    Ok(index)
}
