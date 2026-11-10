//! Ordinary Not/Group conditions keep actual true-body and false-exit owner roles.
use super::replace_break_grouped_condition::{Consumer, check};

// Adjacent `!!` is one non-null-assertion token, not two prefix Not operators.
const CONDITIONS: &[(&str, usize, usize)] = &[
    ("!flag", 0, 1),
    ("(!flag)", 1, 1),
    ("!((flag))", 2, 1),
    ("((!((flag))))", 4, 1),
    ("! !flag", 0, 2),
    ("!(!flag)", 1, 2),
    ("!(!(!flag))", 2, 3),
];

#[test]
fn negated_replace_break_preserves_bind_consumer_and_actual_edges() {
    for &(condition, groups, negations) in CONDITIONS {
        check(Consumer::Bind, condition, groups, negations);
    }
}

#[test]
fn negated_replace_break_preserves_call_consumer_and_actual_edges() {
    for &(condition, groups, negations) in CONDITIONS {
        check(Consumer::Call, condition, groups, negations);
    }
}

#[test]
fn negated_replace_break_preserves_return_consumer_and_actual_edges() {
    for &(condition, groups, negations) in CONDITIONS {
        check(Consumer::Return, condition, groups, negations);
    }
}
