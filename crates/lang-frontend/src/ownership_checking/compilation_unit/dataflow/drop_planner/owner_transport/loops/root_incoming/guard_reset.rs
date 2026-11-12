//! Optional terminal false assignment, not general loop induction.
use super::*;
pub(super) mod counter;
use counter::CounterTail;

#[derive(Clone, Debug, Default)]
pub(super) struct Tail {
    pub(super) reset: Option<UnitExpressionId>,
    pub(super) counter: Option<CounterTail>,
}

#[cfg(test)]
pub(super) fn premature_completion() -> bool {
    tests::fault() == 3
}

pub(super) fn body_parts<'a>(
    checker: &Checker<'_>,
    elements: &'a [StatementId],
    count: usize,
    guard: UnitSymbolId,
) -> Option<(&'a [StatementId], Tail)> {
    if elements.len() == count {
        return Some((elements, Tail::default()));
    }
    if elements.len() == count + 2 {
        let counter = CounterTail::admit(checker, &elements[count..], guard)?;
        return Some((
            &elements[..count],
            Tail {
                reset: Some(counter.reset()),
                counter: Some(counter),
            },
        ));
    }
    if elements.len() != count + 1 {
        return None;
    }
    let ast = checker.parsed.ast();
    let Statement::Expression { expression } =
        ast.statements().get(elements[count]).ok()?.payload()
    else {
        return None;
    };
    let Expression::Assignment {
        target,
        operator: crate::parser::AssignmentOperator::Assign,
        value,
        ..
    } = ast.expressions().get(*expression).ok()?.payload()
    else {
        return None;
    };
    let target_node = ast.expressions().get(*target).ok()?;
    if !matches!(target_node.payload(), Expression::Name)
        || checker.reference_symbol(target_node.span()) != Some(guard)
        || !exact_type(checker, *target, BuiltinType::Boolean)
        || !matches!(
            ast.expressions().get(*value).ok()?.payload(),
            Expression::Literal(crate::parser::LiteralKind::Boolean(false))
        )
    {
        return None;
    }
    Some((
        &elements[..count],
        Tail {
            reset: Some(checker.unit_expression(*expression)),
            counter: None,
        },
    ))
}

impl DropPlanner<'_, '_> {
    pub(in super::super::super::super) fn complete_root_guard_reset(
        &mut self,
        id: ExpressionId,
        state: &ValueState,
    ) {
        let Some(control) = self.owner_controls.current() else {
            return;
        };
        let id = self.checker.unit_expression(id);
        if let Some(body) = self.owner_loop.bridge_body_mut(control) {
            body.assignment(id, state);
        }
        for (site, evidence) in &mut self.owner_loop.root_entries {
            if *site == control
                && clean_root_shape(state, &evidence.roots)
                && let Some(transaction) = &mut evidence.token_transaction
            {
                #[cfg(test)]
                if tests::fault() == 1 {
                    continue;
                }
                transaction.complete_guard_reset(id);
                #[cfg(test)]
                if tests::fault() == 2 {
                    transaction.complete_guard_reset(id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    thread_local! { static FAULT: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
    pub(super) fn fault() -> u8 {
        FAULT.get()
    }
    struct Restore(u8);
    impl Drop for Restore {
        fn drop(&mut self) {
            FAULT.set(self.0);
        }
    }
    fn source(tail: &str) -> String {
        format!(
            "fun run(flag: Boolean) {{ var again = flag; var other = flag; var a = \"a\"; var b = \"b\"; while(again) {{ val x = replace(&a, \"x\"); val y = replace(&b, \"y\"); {tail} }} }}"
        )
    }
    #[test]
    fn terminal_reset_accepts_only_exact_false_guard_tail() {
        for (tail, accepted) in [
            ("again = false", true),
            ("again = true", false),
            ("other = false", false),
            ("again = !again", false),
            ("again = false; val n = 1", false),
        ] {
            let text = source(tail);
            let fixture = Fixture::new(&[("guard-reset-shape.ko", &text)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty());
            let boundary = owned
                .owner_observations()
                .iter()
                .find_map(|o| o.multi_root_boundary_v1());
            assert_eq!(boundary.is_some(), accepted, "{text}");
            if let Some(boundary) = boundary {
                assert!(boundary.guard_reset().is_some());
            }
        }
    }
    #[test]
    fn ci_only_guard_reset_requires_exactly_one_actual_completion() {
        let text = source("again = false");
        let fixture = Fixture::new(&[("guard-reset-completion.ko", &text)]);
        for fault in [0, 1, 2, 3] {
            let restore = Restore(FAULT.replace(fault));
            let owned = fixture.shared();
            drop(restore);
            assert_eq!(
                owned
                    .owner_observations()
                    .iter()
                    .any(|o| o.multi_root_boundary_v1().is_some()),
                fault == 0
            );
        }
    }
    #[test]
    fn ci_only_counter_tail_requires_ordered_actual_completions() {
        use super::super::super::super::tests::compare_and_inspect;
        use super::*;
        let fixture = Fixture::new(&[(
            "counter-completion.ko",
            "fun run(n: Int, flag: Boolean) { var remaining = n; var again = flag; var a = \"a\"; var b = \"b\"; while(again) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\"); remaining = remaining - 1; again = remaining > 0 } }",
        )]);
        let owned = fixture.shared();
        for fault in [0, 1, 2, 3] {
            compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
                let mut planner = DropPlanner::new(
                    checker,
                    super::super::super::super::super::super::liveness::build(checker, true)
                        .unwrap(),
                );
                planner.owner_control_probe = Some(Vec::new());
                let restore = Restore(FAULT.replace(fault));
                let planner = planner.run().unwrap();
                drop(restore);
                let equation = planner
                    .owner_control_probe
                    .as_ref()
                    .unwrap()
                    .iter()
                    .find_map(|visit| visit.normal_equation.as_ref());
                assert_eq!(equation.is_some(), fault == 0);
                if let Some(equation) = equation {
                    assert!(equation.clone().into_boundary().is_none());
                }
            });
        }
    }
}
