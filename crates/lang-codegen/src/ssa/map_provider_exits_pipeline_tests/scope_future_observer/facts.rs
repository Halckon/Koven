//! One original analysis, public identities and unmodified cleanup facts.
mod deep;
mod provider;
mod unit;
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{
    Case,
    ast::{self, Input},
};
pub(super) use deep::Proof;
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::OwnershipBindingKind,
    parser::ParsedFile,
    source::Span,
    type_checking::{
        BuiltinType, CallableResultSource, ExpressionCategory, IntrinsicTypeConstructor,
        ParameterMode,
    },
};
pub(super) use unit::check as unit;
