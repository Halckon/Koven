//! Read-only original-analysis facts; Unit never borrows Single cleanup tables.
mod deep;
mod provider_contract;
mod unit;
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{
    Case,
    ast::{Input, inside, span},
};
pub(super) use deep::Proof;
use lang_frontend::{
    parser::{Expression, ParsedFile},
    type_checking::{
        BuiltinType, CallableResultSource, ExpressionCategory, IntrinsicTypeConstructor,
        ParameterMode,
    },
};
pub(super) use unit::check as unit;
