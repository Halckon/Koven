//! 只读公共 Map typed/ownership identity oracle；原八观察和新 query 分开报告。
#[path = "fact_identity_single.rs"]
mod single;
#[path = "fact_identity_unit.rs"]
mod unit;

use super::{
    Report,
    cases::{CASES, Case},
    fixture::{Fixture, Sites, sites},
    pipeline::{Entry, Inspection, Observation, observe},
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, Statement},
    type_checking::{IntrinsicTypeConstructor, MapIterationKind},
};

type QueryResult<T = ()> = Result<T, &'static str>;

fn require(passed: bool, reason: &'static str) -> QueryResult {
    if passed { Ok(()) } else { Err(reason) }
}

fn map_kind(case: &Case) -> QueryResult<MapIterationKind> {
    match case.constructor {
        IntrinsicTypeConstructor::Map => Ok(MapIterationKind::Map),
        IntrinsicTypeConstructor::MutableMap => Ok(MapIterationKind::MutableMap),
        _ => Err("fixture source constructor is not intrinsic Map/MutableMap"),
    }
}

#[derive(Clone, Copy, Debug)]
enum BodyExit {
    Fallthrough(StatementId),
    Continue(ExpressionId),
    Break(ExpressionId),
    Return(ExpressionId),
}

/// 正常回边来自真实 For.body Block；显式转移来自其末句，不使用日志数字或 factory 内 return。
fn body_exit(fixture: &Fixture, sites: &Sites) -> QueryResult<BodyExit> {
    let ast = fixture.file.ast();
    let node = ast
        .statements()
        .get(sites.statement)
        .map_err(|_| "missing For")?;
    let Statement::For { source, body, .. } = node.payload() else {
        return Err("fixture statement is not For");
    };
    require(
        *source == sites.source,
        "For source differs from retained AST site",
    )?;
    let node = ast
        .statements()
        .get(*body)
        .map_err(|_| "missing For body")?;
    let Statement::Block { elements } = node.payload() else {
        return Err("fixture For body is not a block");
    };
    let last = *elements.last().ok_or("fixture For body is empty")?;
    let node = ast
        .statements()
        .get(last)
        .map_err(|_| "missing body last statement")?;
    let Some(transfer) = sites.transfer else {
        require(
            matches!(node.payload(), Statement::LocalVariable { .. }),
            "P1 fallthrough must follow its actual last local declaration",
        )?;
        return Ok(BodyExit::Fallthrough(*body));
    };
    require(
        matches!(node.payload(), Statement::Expression { expression } if *expression == transfer),
        "body transfer is not the actual last expression statement",
    )?;
    match ast
        .expressions()
        .get(transfer)
        .map_err(|_| "missing body transfer")?
        .payload()
    {
        Expression::Continue { .. } => Ok(BodyExit::Continue(transfer)),
        Expression::Break { .. } => Ok(BodyExit::Break(transfer)),
        Expression::Return { .. } => Ok(BodyExit::Return(transfer)),
        _ => Err("body transfer is not continue/break/return"),
    }
}

fn query(case: &Case, observation: &Observation<'_>, report: &mut Report) -> QueryResult {
    match observation
        .inspect()
        .map_err(|_| "original observation is BLOCKED; no facts fabricated")?
    {
        Inspection::Single {
            fixture,
            names,
            typed,
            owned,
        } => {
            report.note(
                "actual public typed Map descriptors",
                typed.map_iterations(),
            );
            let owned = owned.ok_or("original ownership was not reached")?;
            // 在任何新硬判据之前保留完整真实表；某条 query 失败不截断诊断。
            report.note(
                "actual public Map ownership plans including exits",
                owned.map_iterations(),
            );
            report.note("actual complete cleanup steps", owned.cleanup_steps());
            let sites = sites(fixture, case).map_err(|_| "retained AST site extraction failed")?;
            let validation = lang_frontend::ownership_checking::validate_iteration_facts(
                &fixture.file,
                names,
                typed,
                owned,
            );
            report.record(
                "single complete public iteration validator",
                validation.is_ok(),
                validation,
            );
            single::query(case, fixture, &sites, names, typed, owned)
        }
        Inspection::Unit {
            fixture,
            names,
            typed,
            owned,
        } => {
            report.note(
                "actual public typed Map descriptors",
                typed.map_iterations(),
            );
            let owned = owned.ok_or("original ownership was not reached")?;
            report.note(
                "actual public Map ownership plans including exits",
                owned.map_iterations(),
            );
            for plan in owned.map_iterations() {
                for exit in plan.exits() {
                    report.note(
                        "actual complete unit cleanup at point",
                        (exit.point(), owned.iteration_cleanup_at(exit.point())),
                    );
                }
            }
            // unit 的原 pipeline 已 consume/validate/into_ownership 同一产物；不复制或伪造资格。
            let sites = sites(fixture, case).map_err(|_| "retained AST site extraction failed")?;
            unit::query(case, fixture, &sites, names, typed, owned)
        }
    }
}

#[test]
fn public_map_facts_preserve_exact_identity_and_cleanup_in_both_entries() {
    let observations = CASES[..4]
        .iter()
        .flat_map(|case| {
            [Entry::Single, Entry::Unit].map(|entry| (case, entry, observe(case, entry)))
        })
        .collect::<Vec<_>>();
    // 先保活全部八份原观察。原 assess/报告不接收新 query 的结果，入口行为不变。
    let original = observations
        .iter()
        .map(|(_, _, item)| item.assess())
        .collect::<Vec<_>>();
    let queries = observations
        .iter()
        .map(|(case, entry, item)| {
            let mut report = Report::default();
            let result = query(case, item, &mut report);
            report.record(
                "independent public Map identity query",
                result.is_ok(),
                result,
            );
            (
                report.passed(),
                format!(
                    "{}/{entry:?}: PUBLIC_MAP_QUERY\nCHECKS {:#?}\nDETAILS {}",
                    case.label,
                    report.checks,
                    report.details.join("\n")
                ),
            )
        })
        .collect::<Vec<_>>();
    for ((_, original), (_, query)) in original.iter().zip(&queries) {
        println!("{original}\n{query}");
    }
    assert_eq!(observations.len(), 8);
    assert_eq!(original.len(), 8);
    assert_eq!(queries.len(), 8);
    assert!(
        original.iter().all(|(passed, _)| *passed) && queries.iter().all(|(passed, _)| *passed),
        "SPEC-0331 public Map query failed after all eight original and query observations were retained.\n{}",
        original
            .iter()
            .zip(&queries)
            .map(|((_, old), (_, new))| format!("{old}\n{new}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
