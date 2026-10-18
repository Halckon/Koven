//! 只读真实 Group/Name AST 身份；不调用生产 qualification。
use super::super::fixture::{File, LoopSite};
use super::{
    FOR_DIRECT, FOR_GROUP,
    fixture::{Fixture, Stopped, stopped},
};
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, FunctionBody, FunctionForm, Item, NameMarker, Statement},
    source::Span,
};

#[derive(Debug)]
pub(super) struct Source {
    pub grouped: bool,
    pub name: ExpressionId,
    pub name_span: Span,
}

pub(super) fn source(fixture: &Fixture, file: &File, site: &LoopSite) -> Result<Source, Stopped> {
    let expressions = file.parsed.ast().expressions();
    let outer = expressions
        .get(site.source)
        .map_err(|error| stopped("Group AST source", error))?;
    let (grouped, name) = match outer.payload() {
        Expression::Name => (false, site.source),
        Expression::Group { expression } => (true, *expression),
        other => return Err(stopped("Group AST source shape", other)),
    };
    let inner = expressions
        .get(name)
        .map_err(|error| stopped("Group AST inner Name", error))?;
    let expected_for = if grouped { FOR_GROUP } else { FOR_DIRECT };
    let expected_source = if grouped { "(source)" } else { "source" };
    if inner.payload() != &Expression::Name
        || outer.span() != site.source_span
        || fixture.sources.slice(site.span) != Ok(expected_for)
        || fixture.sources.slice(site.source_span) != Ok(expected_source)
        || fixture.sources.slice(inner.span()) != Ok("source")
        || fixture.sources.source_name(site.span.source_id()) != Ok(file.path.as_str())
        || (grouped && (name == site.source || inner.span() == site.source_span))
    {
        return Err(stopped(
            "Group AST exact full For/source/inner Name",
            (site, outer, inner),
        ));
    }
    Ok(Source {
        grouped,
        name,
        name_span: inner.span(),
    })
}

pub(super) fn empty_entry(file: &File, name_span: Span) -> bool {
    file.parsed.ast().items().iter().any(|(_, node)| {
        let Item::Function {
            name: NameMarker::Present(name),
            form,
            ..
        } = node.payload()
        else {
            return false;
        };
        if *name != name_span {
            return false;
        }
        let block = match form {
            FunctionForm::Explicit {
                body: FunctionBody::Block(block),
                ..
            }
            | FunctionForm::ImplicitUnitBlock(block) => *block,
            _ => return false,
        };
        file.parsed.ast().statements().get(block).is_ok_and(
            |node| matches!(node.payload(), Statement::Block { elements } if elements.is_empty()),
        )
    })
}
