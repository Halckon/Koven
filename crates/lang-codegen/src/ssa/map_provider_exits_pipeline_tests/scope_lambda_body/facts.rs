//! Same raw analysis: actual Fn0 Unit callback and the Map born inside its LambdaBody.
mod unit;
use super::super::{
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
    pub callback_drop: Span,
    pub source_drop: Span,
}
type Input<'a> = (&'a Case, &'a SourceMap, &'a ParsedFile, &'a ast::Input);

pub(super) use unit::check as unit;
