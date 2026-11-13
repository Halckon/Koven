//! Independent size-result, condition, both-arm and current-owner continuation proof.
use super::*;

// These are the original AST/SSA products already held by the parent query;
// keeping the roles explicit makes the cross-product identity checks reviewable.
#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    g: &Graph<'_>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    scan: Span,
    form: usize,
    sizes: &[&Instruction],
    calls: &[(&Instruction, &Instruction)],
    uses: &mut Uses,
) -> Result<(), Failure> {
    let conditions = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            let Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } = node.payload()
            else {
                return None;
            };
            inside(node.span(), scan).then_some((
                node.span(),
                *condition,
                *then_branch,
                *else_branch,
            ))
        })
        .collect::<Vec<_>>();
    if form == 1 || form == 3 {
        need(
            conditions.is_empty() && sizes.is_empty(),
            "call/repeat form has no invented size condition",
            &conditions,
        )?;
        need(
            calls.len() == if form == 1 { 2 } else { 0 },
            "call/repeat original reachable Borrow count",
            calls,
        )?;
        return Ok(());
    }
    let [(if_span, condition, then_branch, Some(else_branch))] = conditions.as_slice() else {
        return Err(failure(
            "one original size If with both real arms",
            &conditions,
        ));
    };
    let comparison = parsed.ast().expressions().get(*condition).unwrap();
    let Expression::Binary {
        left,
        operator,
        right,
        ..
    } = comparison.payload()
    else {
        return Err(failure("original size equality AST", comparison));
    };
    need(
        *operator == lang_frontend::parser::BinaryOperator::Equal,
        "original size condition is equality",
        comparison,
    )?;
    let zero_ast = parsed.ast().expressions().get(*right).unwrap();
    need(
        sources.slice(zero_ast.span()) == Ok("0"),
        "original right operand zero",
        zero_ast,
    )?;
    let [size] = sizes else {
        return Err(failure("one actual source size result", sizes));
    };
    let left_ast = parsed.ast().expressions().get(*left).unwrap();
    if form == 0 {
        need(
            left_ast.payload() == &Expression::Name
                && sources.slice(left_ast.span()) == Ok("count"),
            "later condition reads original count binding",
            left_ast,
        )?;
        let initializer = one(
            parsed.ast().items().iter().filter_map(|(_, node)| {
                let Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                (inside(node.span(), scan) && sources.slice(*name) == Ok("count"))
                    .then_some(*initializer)
            }),
            "original count declaration initializer",
        )?;
        let initializer = parsed.ast().expressions().get(initializer).unwrap();
        need(
            sources.slice(initializer.span()) == Ok("source.size")
                && size.origin == Origin::Source(initializer.span()),
            "count stores exact actual source.size result",
            size,
        )?;
        need(
            calls.is_empty(),
            "count-only form has no fabricated future Borrow",
            calls,
        )?;
    } else {
        need(
            form == 2 && calls.len() == 1,
            "CFG form has one real future Borrow",
            calls,
        )?;
        need(
            sources.slice(left_ast.span()) == Ok("source.size")
                && size.origin == Origin::Source(left_ast.span()),
            "condition reads exact original source.size",
            size,
        )?;
    }
    let zero = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(zero_ast.span())
                && i.operation == Operation::Constant(ScalarConstant::Integer(0))
        }),
        "actual original zero constant",
    )?;
    let compare = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(comparison.span())
                && i.operation
                    == Operation::Compare {
                        operator: ComparisonOperator::Equal,
                        left: value(size).unwrap(),
                        right: value(zero).unwrap(),
                    }
        }),
        "actual exact size/zero equality",
    )?;
    need(
        size.block == compare.block
            && zero.block == compare.block
            && g.position(size)? < g.position(zero)?
            && g.position(zero)? < g.position(compare)?,
        "actual size then original zero then equality",
        (size, zero, compare),
    )?;
    for i in [*size, zero] {
        need(g.function.entity(result(i)?).is_some_and(|e| matches!(e.ty, EntityType::Value(t) if module.type_kind(t) == Some(&SsaTypeKind::Integer {bits:32,signed:true}))), "size and original zero use Int storage", i)?;
        uses.protect(result(i)?);
        uses.permit(result(i)?, compare);
    }
    need(g.function.entity(result(compare)?).is_some_and(|e| matches!(e.ty,EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Boolean))), "comparison actual Boolean storage", compare)?;
    let term = g.term(compare.block)?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &term.kind
    else {
        return Err(failure("original If actual Conditional", term));
    };
    need(
        *condition == value(compare)?
            && term.origin == Origin::Source(*if_span)
            && when_true.target != when_false.target,
        "exact original If Boolean/Conditional origin",
        term,
    )?;
    control_use(g, compare)?;
    g.incoming_from(when_true.target, &[compare.block])?;
    g.incoming_from(when_false.target, &[compare.block])?;
    arm(
        g,
        sources,
        parsed,
        *then_branch,
        when_true.target,
        "zero",
        uses,
    )?;
    arm(
        g,
        sources,
        parsed,
        *else_branch,
        when_false.target,
        "nonzero",
        uses,
    )?;
    let true_exit = g.branch(when_true.target)?;
    let false_exit = g.branch(when_false.target)?;
    let merge = true_exit.target;
    need(
        false_exit.target == merge
            && g.term(when_true.target)?.origin == Origin::Source(*if_span)
            && g.term(when_false.target)?.origin == Origin::Source(*if_span),
        "both actual source arms merge at original If",
        (true_exit, false_exit),
    )?;
    g.incoming_from(merge, &[when_true.target, when_false.target])?;
    if form == 2 {
        let Operation::MapSize { owner: current } = size.operation else {
            unreachable!()
        };
        let then_owner = g.transported(when_true, current)?;
        let else_owner = g.transported(when_false, current)?;
        let merged_owner = g.transported(true_exit, then_owner)?;
        need(
            g.argument(false_exit, merged_owner)? == else_owner,
            "all current-owner If merge columns match actual size owner",
            (then_owner, else_owner, merged_owner),
        )?;
        let (call, end) = calls[0];
        let Operation::DirectCall { arguments, .. } = &call.operation else {
            unreachable!()
        };
        let [argument] = arguments.as_slice() else {
            return Err(failure("postmerge actual Borrow argument", call));
        };
        let birth = g.definition(*argument)?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("postmerge actual source loan", birth));
        };
        let root = g.definition(EntityId::Place(place))?;
        need(
            root.operation
                == match merged_owner {
                    EntityId::Value(owner) => Operation::RootPlace { owner },
                    _ => return Err(failure("actual merged owner Value", merged_owner)),
                }
                && root.block == merge
                && birth.block == merge
                && call.block == merge
                && end.block == merge,
            "actual later Borrow reads rebound owner only after complete merge",
            (root, birth, call, end),
        )?;
    } else {
        // count's source owner was consumed before this If; no Map/loan column
        // may silently survive into either arm or their merge.
        for block in [when_true.target, when_false.target, merge] {
            need(
                g.block(block)?.parameters.iter().all(|entity| {
                    !matches!(
                        module.type_kind(g.function.entity(*entity).unwrap().ty.semantic_type()),
                        Some(SsaTypeKind::MapContainer { .. })
                    )
                }),
                "count-only If carries no residual source owner/loan",
                g.block(block)?,
            )?;
        }
    }
    Ok(())
}

// Uses::check accounts for data operands and edge columns. A Boolean control
// result has exactly one different legal use: this precise Conditional field.
fn control_use(g: &Graph<'_>, compare: &Instruction) -> Result<(), Failure> {
    let entity = result(compare)?;
    let boolean = value(compare)?;
    need(g.instructions.iter().all(|i| !i.operation.entities().contains(&entity))
        && g.edges.iter().all(|(_, edge)| !edge.arguments.contains(&entity))
        && g.blocks.iter().all(|&block| match g.term(block) {
            Ok(term) if block == compare.block => matches!(term.kind,TerminatorKind::Conditional {condition,..} if condition==boolean),
            Ok(term) => !term.kind.entities().contains(&entity),
            Err(_) => false,
        }), "complete Boolean uses are exactly original size Conditional", compare)
}

fn arm(
    g: &Graph<'_>,
    sources: &SourceMap,
    parsed: &ParsedFile,
    body: lang_frontend::ast::StatementId,
    block: BlockId,
    expected: &str,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let span = parsed.ast().statements().get(body).unwrap().span();
    let (call_span, argument_span) = one(
        parsed.ast().expressions().iter().filter_map(|(_, node)| {
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                return None;
            };
            if !inside(node.span(), span)
                || sources.slice(parsed.ast().expressions().get(*callee).unwrap().span())
                    != Ok("println")
            {
                return None;
            }
            let [argument] = arguments.as_slice() else {
                return None;
            };
            Some((
                node.span(),
                parsed
                    .ast()
                    .expressions()
                    .get(argument.value)
                    .unwrap()
                    .span(),
            ))
        }),
        "original actual arm println",
    )?;
    let raw = sources
        .slice(argument_span)
        .map_err(|e| failure("original arm literal", e))?;
    need(
        raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')) == Some(expected),
        "original zero/nonzero branch bytes",
        raw,
    )?;
    let print = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(call_span)
                && matches!(
                    i.operation,
                    Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                )
        }),
        "actual original arm print",
    )?;
    need(
        print.block == block,
        "actual branch successor owns its original marker",
        print,
    )?;
    match &print.operation {
        Operation::PrintLiteral { bytes } => {
            let mut expected = expected.as_bytes().to_vec();
            expected.push(b'\n');
            need(bytes == &expected, "actual arm literal output bytes", print)?;
        }
        Operation::PrintString { value: loan } => {
            let birth = g.definition(EntityId::Loan(*loan))?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = birth.operation
            else {
                return Err(failure("arm actual String view", birth));
            };
            let root = g.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner } = root.operation else {
                return Err(failure("arm actual String root", root));
            };
            let literal = g.definition(EntityId::Value(owner))?;
            need(
                literal.origin == Origin::Source(argument_span)
                    && matches!(&literal.operation,Operation::StringLiteral {bytes,..} if bytes==expected.as_bytes()),
                "actual selected arm String bytes and original literal",
                literal,
            )?;
            let end = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: *loan }),
                "actual arm String loan End",
            )?;
            let drop = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::Drop { owner }),
                "actual arm String owner Drop",
            )?;
            need(
                [literal, root, birth, end, drop]
                    .iter()
                    .all(|i| i.block == block)
                    && g.position(literal)? < g.position(root)?
                    && g.position(root)? < g.position(birth)?
                    && g.position(birth)? < g.position(print)?
                    && g.position(print)? < g.position(end)?
                    && g.position(end)? < g.position(drop)?,
                "actual arm String creation/read/End/Drop sequence",
                print,
            )?;
            uses.protect(EntityId::Value(owner));
            uses.permit(EntityId::Value(owner), root);
            uses.permit(EntityId::Value(owner), drop);
            uses.protect(EntityId::Place(place));
            uses.permit(EntityId::Place(place), birth);
            uses.protect(EntityId::Loan(*loan));
            uses.permit(EntityId::Loan(*loan), print);
            uses.permit(EntityId::Loan(*loan), end);
        }
        _ => unreachable!(),
    }
    Ok(())
}
