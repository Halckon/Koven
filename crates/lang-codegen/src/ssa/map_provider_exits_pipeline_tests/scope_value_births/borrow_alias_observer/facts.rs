//! Exact published canonical alias storage and source lifetime from the original analysis.
mod callers;
mod unit;
use super::super::super::{
    births::ast::Site,
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Input, inside, span};
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::*,
    parser::{Expression, ForBinding, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
    type_checking::*,
};
#[derive(Debug)]
pub(super) struct Proof {
    pub declaration: Span,
    pub point: lang_frontend::ast::StatementId,
    pub endpoint: Span,
}
pub(super) fn site(p: &ParsedFile, i: &Input) -> Result<Site, Failure> {
    let (statement, source) = i.site.ok_or_else(|| failure("alias actual For site", i))?;
    let node = p.ast().statements().get(statement).unwrap();
    let Statement::For {
        binding: ForBinding::Destructuring { names, .. },
        body,
        ..
    } = node.payload()
    else {
        unreachable!()
    };
    let [NameMarker::Present(k), NameMarker::Present(v)] = names.as_slice() else {
        return Err(failure("alias actual K/V names", names));
    };
    let (read, arguments) = one(
        p.ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Call { arguments, .. } if inside(node.span(), n.span()) => {
                    Some((id, arguments))
                }
                _ => None,
            }),
        "alias actual sole For reader Call",
    )?;
    let [a, b] = arguments.as_slice() else {
        return Err(failure("alias actual reader argument slots", arguments));
    };
    Ok(Site {
        statement,
        span: node.span(),
        source,
        body: *body,
        bindings: [*k, *v],
        read,
        arguments: [a.value, b.value],
    })
}
pub(super) use unit::check as unit;
