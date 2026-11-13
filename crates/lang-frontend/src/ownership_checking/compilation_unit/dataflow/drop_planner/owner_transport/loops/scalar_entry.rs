//! Entry-only scalar branch observation; no authorization of general calls.
use super::*;
use crate::ownership_checking::UnitOwnerScalarEntryV1;
#[derive(Clone)]
struct Plan {
    prefix: ExpressionId,
    condition: ExpressionId,
    calls: [ExpressionId; 2],
    parent: StatementId,
}
pub(in super::super::super) struct Tracker {
    plan: Plan,
    owners: Option<Vec<(UnitSymbolId, UnitOwnerId)>>,
    stage: usize,
    valid: bool,
}
fn discover(checker: &Checker<'_>, item: ItemId) -> Option<Plan> {
    let ast = checker.parsed.ast();
    let Item::Function { form, .. } = ast.items().get(item).ok()?.payload() else {
        return None;
    };
    let body = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        _ => return None,
    };
    let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
        return None;
    };
    for (i, statement) in elements.iter().enumerate() {
        match ast.statements().get(*statement).ok()?.payload() {
            Statement::LocalVariable { declaration } => {
                let Item::Variable {
                    initializer, kind, ..
                } = ast.items().get(*declaration).ok()?.payload()
                else {
                    return None;
                };
                if matches!(kind, crate::parser::VariableKind::BorrowVal(_)) {
                    return None;
                }
                let pure = match ast.expressions().get(*initializer).ok()?.payload() {
                    Expression::Name | Expression::Literal(_) => true,
                    Expression::String { parts } => parts
                        .iter()
                        .all(|p| !matches!(p, StringPart::Interpolation { .. })),
                    Expression::Binary {
                        left,
                        operator: BinaryOperator::Greater,
                        right,
                        ..
                    } => {
                        matches!(
                            ast.expressions().get(*left).ok()?.payload(),
                            Expression::Name
                        ) && matches!(
                            ast.expressions().get(*right).ok()?.payload(),
                            Expression::Literal(_)
                        )
                    }
                    _ => false,
                };
                if !pure {
                    return None;
                }
            }
            Statement::Expression { expression } => {
                let (condition, calls) = checker.scalar_prefix_parts(*expression)?;
                let parent = *elements.get(i + 1)?;
                if !matches!(
                    ast.statements().get(parent).ok()?.payload(),
                    Statement::While { .. }
                ) {
                    return None;
                }
                return Some(Plan {
                    prefix: *expression,
                    condition,
                    calls,
                    parent,
                });
            }
            _ => return None,
        }
    }
    None
}
// Pure static discovery permits the counter predicate to be tracked across this
// one prefix. Publication still requires all actual observations below.
pub(in super::super) fn is_prefix(checker: &Checker<'_>, expression: ExpressionId) -> bool {
    checker
        .parsed
        .ast()
        .items()
        .iter()
        .any(|(item, _)| discover(checker, item).is_some_and(|p| p.prefix == expression))
}
impl Tracker {
    pub(in super::super::super) fn begin(checker: &Checker<'_>, item: ItemId) -> Option<Self> {
        Some(Self {
            plan: discover(checker, item)?,
            owners: None,
            stage: 0,
            valid: true,
        })
    }
    pub(in super::super::super) fn finish(
        self,
        nested: &crate::ownership_checking::UnitOwnerNestedNormalBodyV1,
    ) -> Option<UnitOwnerScalarEntryV1> {
        let owners = self.owners?;
        let parent = UnitStatementId::new(nested.parent().source_unit(), self.plan.parent);
        if !self.valid
            || self.stage != 10
            || nested.parent() != parent
            || nested.child_generation_rule_v1()?.entry_owners() != owners
        {
            return None;
        }
        let source = parent.source_unit();
        let fact = UnitOwnerScalarEntryV1 {
            prefix: UnitExpressionId::new(source, self.plan.prefix),
            condition: UnitExpressionId::new(source, self.plan.condition),
            calls: self.plan.calls.map(|e| UnitExpressionId::new(source, e)),
            parent,
            owners,
        };
        fact.is_consistent().then_some(fact)
    }
}
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn scalar_entry_call_supported(&self, id: ExpressionId) -> bool {
        self.scalar_entry.as_ref().is_some_and(|t| {
            t.valid
                && ((t.stage == 3 && t.plan.calls[0] == id)
                    || (t.stage == 6 && t.plan.calls[1] == id))
        })
    }
    fn scalar_entry_snapshot(
        &self,
        parent: StatementId,
        state: &ValueState,
    ) -> Option<Vec<(UnitSymbolId, UnitOwnerId)>> {
        let control = UnitStatementId::new(self.checker.source_unit, parent);
        let frame = root_frame::ControlRootFrame::capture(control, state)?;
        frame
            .roots_for(control)?
            .iter()
            .map(|v| Some((v.symbol, v.owner.clone()?)))
            .collect()
    }
    pub(in super::super::super) fn scalar_entry_boundary(
        &mut self,
        id: ExpressionId,
        stage: usize,
        continues: bool,
        state: &ValueState,
    ) {
        let Some(t) = self.scalar_entry.as_ref() else {
            return;
        };
        if id != t.plan.prefix {
            return;
        }
        let snapshot = self.scalar_entry_snapshot(t.plan.parent, state);
        let t = self.scalar_entry.as_mut().unwrap();
        if stage == 0 {
            t.owners = snapshot.clone();
            t.valid &= self.owner_observation.as_ref().is_some_and(|o| {
                o.status == UnitOwnerObservationStatus::Complete
                    && snapshot.as_ref().is_some_and(|roots| {
                        roots.len() >= 2
                            && roots.iter().all(|(_, owner)| {
                                o.definitions.iter().any(|d| {
                                    &d.id == owner
                                        && matches!(
                                            d.origin,
                                            UnitOwnerOrigin::Parameter(_)
                                                | UnitOwnerOrigin::String(_)
                                        )
                                })
                            })
                    })
            });
        }
        t.valid &= continues && t.stage == stage && snapshot.is_some() && snapshot == t.owners;
        t.stage = stage + 1;
    }
    pub(in super::super::super) fn scalar_entry_call_completed(
        &mut self,
        id: ExpressionId,
        continues: bool,
    ) {
        if let Some(t) = self.scalar_entry.as_mut()
            && let Some(arm) = t.plan.calls.iter().position(|call| *call == id)
        {
            let stage = if arm == 0 { 3 } else { 6 };
            t.valid &= continues && t.stage == stage;
            t.stage = stage + 1;
        }
    }
    pub(in super::super::super) fn scalar_entry_parent(
        &mut self,
        id: StatementId,
        state: &ValueState,
    ) {
        let Some(t) = self.scalar_entry.as_ref() else {
            return;
        };
        if t.plan.parent != id {
            return;
        }
        let snapshot = self.scalar_entry_snapshot(id, state);
        let t = self.scalar_entry.as_mut().unwrap();
        t.valid &= t.stage == 9 && snapshot.is_some() && snapshot == t.owners;
        t.stage = 10;
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    const SOURCE: &str = r#"fun touch(own value:Int) {}
    fun run(own flag:Boolean, outer:Int, inner:Int, own seed:String) {
        var a=seed; var b="b"; var n0=outer; var g0=n0>0;
        if(flag) { touch(1) } else { touch(2) };
        while(g0) { var n1=inner; var g1=n1>0;
            while(g1) { val x=replace(&a,"inner-a"); val y=replace(&b,"inner-b"); n1=n1-1; g1=n1>0 };
            val old=replace(&b,"outer-b"); n0=n0-1; g0=n0>0
        }; println(a); println(b)
    }"#;
    #[test]
    fn scalar_entry_records_both_actual_arms_join_and_parent_owners() {
        for source in [
            SOURCE.to_owned(),
            SOURCE.replace("own flag:Boolean", "flag:Boolean"),
            SOURCE.replace("var a=seed", "var a=\"a\""),
        ] {
            let fixture = Fixture::new(&[("scalar-entry.ko", &source)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            let nested = owned
                .owner_observations()
                .iter()
                .find_map(|o| o.nested_normal_body_v1())
                .expect("nested evidence");
            let fact = nested.scalar_entry_v1().expect("actual prefix receipt");
            assert!(nested.is_consistent());
            assert!(fact.is_consistent());
            assert_eq!(fact.parent(), nested.parent());
            assert_eq!(
                fact.owners(),
                nested.child_generation_rule_v1().unwrap().entry_owners()
            );
            assert_eq!(fact.owners().len(), 2);
            assert_ne!(fact.calls()[0], fact.calls()[1]);
        }
    }
    #[test]
    fn scalar_entry_rejects_resource_or_general_prefixes() {
        for source in [
            SOURCE.replace("own value:Int", "value:Int"),
            SOURCE.replace(
                "fun touch(own value:Int) {}",
                "fun touch(own value:Int) { val unused=1 }",
            ),
            SOURCE.replace("var a=seed", "var a=\"left\"+\"right\""),
            SOURCE.replace("touch(1)", "println(a)"),
            SOURCE.replace("touch(2)", "val old=replace(&a,\"new\")"),
            SOURCE.replace("touch(2)", "return"),
            SOURCE.replace(
                "if(flag) { touch(1) } else { touch(2) };",
                "if(flag) { touch(1) } else { touch(2) }; if(flag) { touch(1) } else { touch(2) };",
            ),
        ] {
            let fixture = Fixture::new(&[("scalar-entry-rejected.ko", &source)]);
            let owned = fixture.shared();
            assert!(owned.owner_observations().iter().all(|o| {
                o.nested_normal_body_v1()
                    .and_then(|n| n.scalar_entry_v1())
                    .is_none()
            }));
        }
    }
}
