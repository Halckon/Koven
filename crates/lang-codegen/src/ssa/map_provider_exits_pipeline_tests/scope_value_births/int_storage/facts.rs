//! Original concrete Int facts and exact current owner/drop facts from one analysis.
mod unit;
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, ast};
use lang_frontend::{
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::*,
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
    type_checking::*,
};
#[derive(Debug)]
pub(super) struct Proof {
    pub drop: Span,
}
type Input<'a> = (&'a Case, &'a SourceMap, &'a ParsedFile, &'a ast::Input);

fn receiver(
    p: &ParsedFile,
    id: lang_frontend::ast::ExpressionId,
) -> lang_frontend::ast::ExpressionId {
    let Expression::Call { callee, .. } = p.ast().expressions().get(id).unwrap().payload() else {
        unreachable!()
    };
    let Expression::Member { receiver, .. } = p.ast().expressions().get(*callee).unwrap().payload()
    else {
        unreachable!()
    };
    *receiver
}
pub(super) use unit::check as unit;
