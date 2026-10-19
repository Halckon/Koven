//! Ordinary source-order evaluators feed the exact declared argument slots.
use super::*;
use lang_frontend::{parser::LiteralKind, type_checking::ParameterMode};
pub(super) struct Delivery<'ir> {
    pub call: &'ir Instruction,
    pub cleanup: Vec<&'ir Instruction>,
    pub argument_owners: BTreeSet<ValueId>,
}
pub(super) fn check<'ir>(
    sources: &SourceMap,
    g: &Graph<'ir>,
    module: &Module,
    factory: &Function,
    parsed: &ParsedFile,
    input: &ast::Input,
    uses: &mut Uses,
) -> Result<Delivery<'ir>, Failure> {
    let call = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation,Operation::DirectCall{callee,..}if callee==factory.id),
        ),
        "local argument selected factory called once",
    )?;
    let Operation::DirectCall { arguments, .. } = &call.operation else {
        unreachable!()
    };
    let formals = &factory
        .block(factory.entry_block().unwrap())
        .unwrap()
        .parameters;
    need(
        call.origin == Origin::Source(ast::span(parsed, input.call))
            && arguments.len() == input.arguments.len()
            && formals.len() == arguments.len() + usize::from(factory.receiver.is_some()),
        "local argument actual Callspan and full ABI",
        call,
    )?;
    let offset = usize::from(factory.receiver.is_some());
    let mut previous = None;
    let mut cleanup = Vec::new();
    let mut argument_owners = BTreeSet::new();
    for a in &input.arguments {
        let node = parsed.ast().expressions().get(a.expression).unwrap();
        let evaluate = match node.payload() {
            Expression::Call {
                callee, arguments, ..
            } => {
                need(
                    arguments.is_empty(),
                    "local argument evaluator is zero-argument Call",
                    node,
                )?;
                let name = ast::span(parsed, *callee);
                let declaration=one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok()).filter(|n|matches!(n.payload(),lang_frontend::parser::Item::Function{name:lang_frontend::parser::NameMarker::Present(marker),..}if sources.slice(*marker)==sources.slice(name))),"local argument evaluator actual declaration")?;
                let f = function(module, declaration.span())?;
                one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::DirectCall{callee,receiver:None,ref arguments}if callee==f.id&&arguments.is_empty())&&i.origin==Origin::Source(node.span())),"local argument actual selected evaluator once")?
            }
            Expression::Literal(LiteralKind::Integer(_)) => at(g, node.span(), |o| {
                *o == Operation::Constant(ScalarConstant::Integer(1))
            })?,
            _ => return Err(failure("local argument supported real evaluator", node)),
        };
        let owner = value(evaluate)?;
        let entity = EntityId::Value(owner);
        let ty = g.function.entity(entity).unwrap().ty.semantic_type();
        need(
            evaluate.block == call.block
                && g.position(evaluate)? < g.position(call)?
                && previous.is_none_or(|p| p < g.position(evaluate).unwrap())
                && argument_owners.insert(owner),
            "local argument each evaluator once in source order",
            evaluate,
        )?;
        previous = Some(g.position(evaluate)?);
        uses.protect(entity);
        let actual = arguments[a.parameter];
        let formal = formals[a.parameter + offset];
        if input.factory.parameters[a.parameter].mode == ParameterMode::Value {
            need(
                actual == entity && factory.entity(formal).unwrap().ty == EntityType::Value(ty),
                "local argument exact declared Value slot",
                (actual, formal),
            )?;
            uses.permit(entity, call);
        } else {
            let EntityId::Loan(current) = actual else {
                return Err(failure("local argument Shared ABI loan", actual));
            };
            let begin = g.definition(actual)?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = begin.operation
            else {
                return Err(failure("local argument Shared Borrow birth", begin));
            };
            let root = g.definition(EntityId::Place(place))?;
            let end = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: current }),
                "local argument unique synchronous End",
            )?;
            let formal_ty = EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            };
            need(
                root.operation == Operation::RootPlace { owner }
                    && root.origin == Origin::Source(a.span)
                    && begin.origin == Origin::Source(node.span())
                    && end.origin == call.origin
                    && [root, begin, end].iter().all(|i| i.block == call.block)
                    && g.position(evaluate)? < g.position(root)?
                    && g.position(root)? < g.position(begin)?
                    && g.position(begin)? < g.position(call)?
                    && g.position(call)? < g.position(end)?
                    && g.function.entity(actual).unwrap().ty == formal_ty
                    && factory.entity(formal).unwrap().ty == formal_ty,
                "local argument owner/Root/Shared/declared ABI/End",
                (evaluate, root, begin, call, end),
            )?;
            uses.permit(entity, root);
            uses.protect(EntityId::Place(place));
            uses.permit(EntityId::Place(place), begin);
            uses.protect(actual);
            uses.permit(actual, call);
            uses.permit(actual, end);
            cleanup.push(end);
            let drops = g
                .instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner })
                .collect::<Vec<_>>();
            let scalar = matches!(
                module.type_kind(ty),
                Some(SsaTypeKind::Integer { .. } | SsaTypeKind::Boolean)
            );
            if scalar {
                need(
                    drops.is_empty(),
                    "local argument Copyable Int has no owner Drop",
                    drops,
                )?;
            } else {
                let drop = one(
                    drops.into_iter(),
                    "local argument exact borrowed temporary Drop",
                )?;
                need(
                    drop.block == call.block
                        && drop.origin == Origin::Source(node.span())
                        && g.position(end)? < g.position(drop)?,
                    "local argument synchronous End before CallReturn Drop",
                    drop,
                )?;
                uses.permit(entity, drop);
                cleanup.push(drop);
            }
        }
    }
    Ok(Delivery {
        call,
        cleanup,
        argument_owners,
    })
}
