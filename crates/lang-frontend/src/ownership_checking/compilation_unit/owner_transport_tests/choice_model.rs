//! Synthetic graph tests for the library value validator, not AST Choice admission.
mod cases;
mod projection;

use super::*;

pub(in crate::ownership_checking::compilation_unit) const CONSUMERS: &[&str] = &[
    "var a = \"old\"\nval old = replace(&a, if (flag) \"new\" else \"other\")",
    "var a = if (flag) \"old\" else \"other\"\nval old = replace(&a, \"new\")",
];

pub(in crate::ownership_checking::compilation_unit) fn consumer_source(body: &str) -> String {
    format!(
        "fun run(flag: Boolean): Unit {{\n{body}\n}}\nfun sibling(): Unit {{ val x = \"safe\" }}"
    )
}

use super::super::owner_transport::values::{
    Slot, UnitOwnerChoiceIdV1 as ChoiceId, UnitOwnerChoiceInputV1 as Input,
    UnitOwnerChoiceV1 as Choice, UnitOwnerValueEventV1 as Event,
    UnitOwnerValueObservationV1 as Model, UnitOwnerValueV1 as Value,
};
