//! Group 原位置 oracle 与 selected-entry 结果；全部根另在同次 report 中逐一验证。
use super::super::fixture::LowerOutcome as RootOutcome;
pub(super) use super::super::fixture::{Fixture, Location, Stopped, clean, location, stopped};

#[derive(Debug)]
pub(super) struct LowerOutcome {
    expected_group: Result<Location, Stopped>,
    selected_entry: RootOutcome,
}

impl LowerOutcome {
    pub fn new(expected_group: Result<Location, Stopped>, selected_entry: RootOutcome) -> Self {
        Self {
            expected_group,
            selected_entry,
        }
    }

    pub fn matches(&self) -> bool {
        self.expected_group
            .as_ref()
            .is_ok_and(|original| original.text == super::FOR_GROUP)
            && self.selected_entry.matches()
    }
}
