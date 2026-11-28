use crate::{
    ast::{AstError, ExpressionId},
    parser::{WhenCondition, WhenEntry},
};

/// Walk the selector prefixes that can be evaluated. The return value says
/// whether all entries can be missed without a control transfer.
pub(super) fn visit_when_prefix<'a>(
    entries: &'a [WhenEntry],
    subjectful: bool,
    mut operand_stops: impl FnMut(ExpressionId) -> Result<bool, AstError>,
    possible_results: impl Fn(ExpressionId) -> (bool, bool),
    mut visit: impl FnMut(&'a WhenEntry, &'a [WhenCondition], bool),
) -> Result<bool, AstError> {
    for entry in entries {
        if entry.else_span.is_some() {
            visit(entry, &[], true);
            return Ok(false);
        }

        let mut may_match = false;
        for (index, condition) in entry.conditions.iter().enumerate() {
            if let Some(operand) = when_operand(condition)
                && operand_stops(operand)?
            {
                visit(entry, &entry.conditions[..=index], may_match);
                return Ok(false);
            }
            let (yes, no) = match condition {
                WhenCondition::Expression(operand) if !subjectful => possible_results(*operand),
                _ => (true, true),
            };
            may_match |= yes;
            if !no {
                visit(entry, &entry.conditions[..=index], may_match);
                return Ok(false);
            }
        }
        visit(entry, &entry.conditions, may_match);
    }
    Ok(true)
}

pub(super) fn when_operand(condition: &WhenCondition) -> Option<ExpressionId> {
    match condition {
        WhenCondition::Expression(expression) | WhenCondition::Contains { expression, .. } => {
            Some(*expression)
        }
        WhenCondition::TypeTest { .. } => None,
    }
}
