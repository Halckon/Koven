//! caller 的真实 local → scan initializer → if reference 链，以及有序 size AST。
use super::{
    Call, Exit, ExpressionId, Failure, Function, Input, ReturnCheck, Site, Size, Span, StatementId,
    contains, failure, function, need,
};
use lang_frontend::parser::{BinaryOperator, Expression, Statement};

pub(super) struct Entry {
    pub scans: [Call; 2],
    pub factory: Call,
    pub locals: [(Span, ExpressionId); 2],
    pub sizes: Vec<Size>,
    pub return_checks: Vec<ReturnCheck>,
}

pub(super) fn entry(
    f: &Input,
    functions: &[Function],
    outer: &Site,
    scan_sizes: &[StatementId],
) -> Result<Entry, Failure> {
    let entry = function(functions, "entry")?;
    let scan = function(functions, "scan")?;
    need(
        entry.parameters.is_empty(),
        "nested exits AST zero-argument entry",
        entry,
    )?;
    let elements = f.elements(entry.body)?;
    let mut return_checks = Vec::new();
    let (locals, scans, caller_sizes) = match (f.exit, elements) {
        (
            Exit::Return,
            [
                nonempty,
                populated,
                result,
                result_check,
                populated_size,
                empty_marker,
                empty,
                empty_result,
                empty_check,
                empty_size,
            ],
        ) => {
            f.marker(*nonempty, "\"nonempty\"")?;
            f.marker(*empty_marker, "\"empty\"")?;
            let locals = [f.local(*populated, "populated")?, f.local(*empty, "empty")?];
            let result = f.local(*result, "result")?;
            let empty_result = f.local(*empty_result, "emptyResult")?;
            let scans = [
                f.call(result.1, "scan", &["populated"])?,
                f.call(empty_result.1, "scan", &["empty"])?,
            ];
            return_checks.push(return_check(
                f,
                *result_check,
                result,
                "result",
                "7",
                ("\"returned-7\"", "\"unexpected-nonempty-result\""),
            )?);
            return_checks.push(return_check(
                f,
                *empty_check,
                empty_result,
                "emptyResult",
                "0",
                ("\"returned-0\"", "\"unexpected-empty-result\""),
            )?);
            (locals, scans, [*populated_size, *empty_size])
        }
        (
            Exit::Continue | Exit::Break,
            [
                nonempty,
                populated,
                first,
                populated_size,
                empty_marker,
                empty,
                second,
                empty_size,
            ],
        ) => {
            f.marker(*nonempty, "\"nonempty\"")?;
            f.marker(*empty_marker, "\"empty\"")?;
            let locals = [f.local(*populated, "populated")?, f.local(*empty, "empty")?];
            let scans = [
                f.call(f.statement_expression(*first)?, "scan", &["populated"])?,
                f.call(f.statement_expression(*second)?, "scan", &["empty"])?,
            ];
            (locals, scans, [*populated_size, *empty_size])
        }
        _ => {
            return Err(failure(
                "nested exits AST exact executable entry body",
                elements,
            ));
        }
    };
    let factory = f.call(
        locals[0].1,
        if f.map { "makeMap" } else { "makeMutable" },
        &[],
    )?;
    empty_factory(f, locals[1].1)?;
    need(
        locals[0].0 != locals[1].0
            && locals[0].1 != locals[1].1
            && scans[0].expression != scans[1].expression
            && factory.span.end() <= scans[0].span.start()
            && scans[0].span.end() <= locals[1].0.start()
            && locals[1].0.end() <= scans[1].span.start()
            && scans.iter().all(|call| contains(entry.span, call.span)),
        "nested exits AST two ordered caller roots and distinct scan calls",
        (&locals, &scans),
    )?;
    let all_scans = f
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::Call { callee, .. } = node.payload() else {
                return None;
            };
            let (target, span) = f.expression(*callee).ok()?;
            (matches!(target, Expression::Name) && f.sources.slice(span) == Ok("scan"))
                .then_some(id)
        })
        .collect::<Vec<_>>();
    need(
        all_scans.len() == 2
            && scans
                .iter()
                .all(|call| all_scans.contains(&call.expression)),
        "nested exits AST exactly two actual scan calls",
        &all_scans,
    )?;
    let mut sizes = Vec::new();
    if f.exit != Exit::Return {
        let [two, zero] = scan_sizes else {
            return Err(failure(
                "nested exits AST exact scan size statements",
                scan_sizes,
            ));
        };
        let [parameter] = scan.parameters.as_slice() else {
            return Err(failure("nested exits AST sole scan parameter", scan));
        };
        sizes.push(size_check(
            f,
            *two,
            (*parameter, "source", "2"),
            ("\"scan-size-2\"", None),
            outer.span,
        )?);
        sizes.push(size_check(
            f,
            *zero,
            (*parameter, "source", "0"),
            ("\"scan-size-0\"", None),
            outer.span,
        )?);
    } else {
        need(
            scan_sizes.is_empty(),
            "nested exits AST return has no synthetic scan size",
            scan_sizes,
        )?;
    }
    sizes.push(size_check(
        f,
        caller_sizes[0],
        (locals[0].0, "populated", "2"),
        ("\"caller-size-2\"", Some("\"unexpected-nonempty-size\"")),
        scans[0].span,
    )?);
    sizes.push(size_check(
        f,
        caller_sizes[1],
        (locals[1].0, "empty", "0"),
        ("\"caller-size-0\"", Some("\"unexpected-empty-size\"")),
        scans[1].span,
    )?);
    let all_sizes = f
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| {
            let Expression::Member { name_span, .. } = node.payload() else {
                return None;
            };
            (f.sources.slice(*name_span) == Ok("size")).then_some(id)
        })
        .collect::<Vec<_>>();
    need(
        all_sizes.len() == sizes.len()
            && sizes
                .iter()
                .all(|size| all_sizes.contains(&size.expression)),
        "nested exits AST all actual size references accounted",
        (&all_sizes, &sizes),
    )?;
    for (check, call) in return_checks.iter().zip(&scans) {
        let (_, reference_span) = f.expression(check.reference)?;
        need(
            check.initializer == call.expression
                && call.span.end() <= reference_span.start()
                && contains(entry.span, reference_span)
                && check.marker.end() <= call.span.start(),
            "nested exits AST caller result initializer/reference identity",
            (check, call),
        )?;
    }
    Ok(Entry {
        scans,
        factory,
        locals,
        sizes,
        return_checks,
    })
}

fn empty_factory(f: &Input, id: ExpressionId) -> Result<(), Failure> {
    let (expression, _) = f.expression(id)?;
    let Expression::Call {
        callee,
        type_arguments,
        arguments,
        ..
    } = expression
    else {
        return Err(failure(
            "nested exits AST empty constructor Call",
            expression,
        ));
    };
    f.name(*callee, if f.map { "mapOf" } else { "mutableMapOf" })?;
    need(
        arguments.is_empty() && type_arguments.len() == 2,
        "nested exits AST empty map constructor arity",
        expression,
    )?;
    for argument in type_arguments {
        let ty = f
            .parsed
            .ast()
            .type_refs()
            .get(*argument)
            .map_err(|e| failure("nested exits AST empty map type argument", e))?;
        need(
            f.sources.slice(ty.span()) == Ok("String"),
            "nested exits AST original empty map String components",
            ty,
        )?;
    }
    Ok(())
}

fn return_check(
    f: &Input,
    statement: StatementId,
    local: (Span, ExpressionId),
    name: &str,
    expected: &'static str,
    markers: (&str, &str),
) -> Result<ReturnCheck, Failure> {
    let reference = equality(f, statement, expected, (markers.0, Some(markers.1)))?;
    f.name(reference, name)?;
    Ok(ReturnCheck {
        reference,
        marker: local.0,
        initializer: local.1,
        expected,
    })
}

fn size_check(
    f: &Input,
    statement: StatementId,
    root: (Span, &str, &str),
    markers: (&str, Option<&str>),
    predecessor: Span,
) -> Result<Size, Failure> {
    let expression = equality(f, statement, root.2, markers)?;
    let (member, span) = f.expression(expression)?;
    let Expression::Member {
        receiver,
        name_span,
        safe: false,
        ..
    } = member
    else {
        return Err(failure(
            "nested exits AST actual non-safe size member",
            member,
        ));
    };
    f.name(*receiver, root.1)?;
    need(
        f.sources.slice(*name_span) == Ok("size") && predecessor.end() <= span.start(),
        "nested exits AST size after actual loop/call",
        (member, span, predecessor),
    )?;
    Ok(Size {
        expression,
        receiver: *receiver,
        root_marker: root.0,
        span,
    })
}

fn equality(
    f: &Input,
    statement: StatementId,
    expected: &str,
    markers: (&str, Option<&str>),
) -> Result<ExpressionId, Failure> {
    let id = f.statement_expression(statement)?;
    let (expression, _) = f.expression(id)?;
    let Expression::If {
        condition,
        then_branch,
        else_branch,
        ..
    } = expression
    else {
        return Err(failure("nested exits AST real caller/scan if", expression));
    };
    let (condition, _) = f.expression(*condition)?;
    let Expression::Binary {
        left,
        operator: BinaryOperator::Equal,
        right,
        ..
    } = condition
    else {
        return Err(failure(
            "nested exits AST real equality condition",
            condition,
        ));
    };
    f.literal(*right, expected, false)?;
    control_marker(f, *then_branch, markers.0)?;
    match (else_branch, markers.1) {
        (Some(branch), Some(expected)) => control_marker(f, *branch, expected)?,
        (None, None) => (),
        _ => {
            return Err(failure(
                "nested exits AST exact comparison else",
                expression,
            ));
        }
    }
    Ok(*left)
}

fn control_marker(f: &Input, id: StatementId, expected: &str) -> Result<(), Failure> {
    let node = f
        .parsed
        .ast()
        .statements()
        .get(id)
        .map_err(|e| failure("nested exits AST comparison control body", e))?;
    let Statement::ControlBody { elements } = node.payload() else {
        return Err(failure("nested exits AST actual ControlBody", node));
    };
    let [statement] = elements.as_slice() else {
        return Err(failure("nested exits AST sole comparison marker", elements));
    };
    f.marker(*statement, expected)
}
