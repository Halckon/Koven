//! Source-order evaluation and declared-slot ABI preserve Borrow versus Value delivery.
use super::*;
use lang_frontend::type_checking::ParameterMode;

pub(super) struct Delivery<'ir> {
    pub call: &'ir Instruction,
    pub cleanup: Vec<&'ir Instruction>,
    pub argument_owners: BTreeSet<ValueId>,
    pub map_capabilities: BTreeSet<EntityId>,
    pub map_types: Vec<SsaTypeId>,
}

pub(super) fn check<'ir>(
    g: &Graph<'ir>,
    module: &Module,
    factory: &Function,
    parsed: &ParsedFile,
    input: &ast::Input,
    uses: &mut Uses,
) -> Result<Delivery<'ir>, Failure> {
    let call = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation,Operation::DirectCall {callee,..} if callee==factory.id),
        ),
        "delivery selected original factory called exactly once",
    )?;
    let Operation::DirectCall {
        receiver: None,
        ref arguments,
        ..
    } = call.operation
    else {
        return Err(failure("delivery direct factory ABI", call));
    };
    let formals = &factory
        .block(factory.entry_block().unwrap())
        .unwrap()
        .parameters;
    need(
        call.origin == Origin::Source(ast::expression(parsed, input.call))
            && arguments.len() == input.arguments.len()
            && formals.len() == arguments.len(),
        "delivery actual Callspan and complete declared ABI",
        call,
    )?;
    let mut cleanup = Vec::new();
    let mut argument_owners = BTreeSet::new();
    let mut map_capabilities = BTreeSet::new();
    let mut map_types = Vec::new();
    let mut previous = None;
    for actual in &input.arguments {
        let evaluator = function(module, actual.function.span)?;
        let evaluate = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation,Operation::DirectCall {callee,..} if callee==evaluator.id),
            ),
            "delivery actual argument evaluator occurs once",
        )?;
        let owner = value(evaluate)?;
        let owner_entity = EntityId::Value(owner);
        let ty = g.function.entity(owner_entity).unwrap().ty.semantic_type();
        need(
            evaluate.origin == Origin::Source(ast::expression(parsed, actual.expression))
                && matches!(&evaluate.operation,Operation::DirectCall {receiver:None,arguments,..} if arguments.is_empty())
                && evaluator.return_types.as_slice() == [ty]
                && evaluator
                    .block(evaluator.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .is_empty()
                && evaluate.block == call.block
                && g.position(evaluate)? < g.position(call)?
                && previous.is_none_or(|position| position < g.position(evaluate).unwrap())
                && argument_owners.insert(owner),
            "delivery real argument evaluation once in source order",
            evaluate,
        )?;
        previous = Some(g.position(evaluate)?);
        uses.protect(owner_entity);
        if module.map_container(ty).is_some() {
            map_types.push(ty);
            map_capabilities.insert(owner_entity);
        }
        let mode = input.factory.parameters[actual.parameter].mode;
        if mode == ParameterMode::Value {
            need(
                arguments[actual.parameter] == owner_entity
                    && factory.entity(formals[actual.parameter]).unwrap().ty
                        == EntityType::Value(ty)
                    && matches!(module.type_kind(ty), Some(SsaTypeKind::HeapOwner { .. })),
                "delivery own Resource goes to its actual declared Value slot",
                (evaluate, call, actual.parameter),
            )?;
            uses.permit(owner_entity, call);
        } else {
            let EntityId::Loan(current) = arguments[actual.parameter] else {
                return Err(failure("delivery actual Borrow argument is loan", call));
            };
            let begin = g.definition(EntityId::Loan(current))?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = begin.operation
            else {
                return Err(failure("delivery actual shared argument loan", begin));
            };
            let root = g.definition(EntityId::Place(place))?;
            let end = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: current }),
                "delivery actual synchronous argument End",
            )?;
            let drop = one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::Drop { owner }),
                "delivery borrowed temporary unique original Drop",
            )?;
            let formal = EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            };
            need(
                root.operation == Operation::RootPlace { owner }
                    && root.origin == Origin::Source(actual.span)
                    && begin.origin == evaluate.origin
                    && drop.origin == evaluate.origin
                    && end.origin == call.origin
                    && [root, begin, end, drop]
                        .iter()
                        .all(|i| i.block == call.block)
                    && g.position(evaluate)? < g.position(root)?
                    && g.position(root)? < g.position(begin)?
                    && g.position(begin)? < g.position(call)?
                    && g.position(call)? < g.position(end)?
                    && g.position(end)? < g.position(drop)?
                    && g.function.entity(EntityId::Place(place)).unwrap().ty
                        == EntityType::Place(ty)
                    && g.function.entity(EntityId::Loan(current)).unwrap().ty == formal
                    && factory.entity(formals[actual.parameter]).unwrap().ty == formal,
                "delivery evaluator owner/Root/SharedBorrow/correct ABI/End/CallReturn Drop",
                (evaluate, root, begin, call, end, drop),
            )?;
            uses.permit(owner_entity, root);
            uses.permit(owner_entity, drop);
            let capabilities = [
                owner_entity,
                EntityId::Place(place),
                EntityId::Loan(current),
            ];
            need(
                g.edges.iter().all(|(_, edge)| {
                    edge.arguments
                        .iter()
                        .all(|entity| !capabilities.contains(entity))
                }),
                "Unit synchronous argument capabilities never escape after CallReturn retirement",
                capabilities,
            )?;
            if module.map_container(ty).is_some() {
                map_capabilities.extend(capabilities);
            }
            uses.protect(EntityId::Place(place));
            uses.permit(EntityId::Place(place), begin);
            uses.protect(EntityId::Loan(current));
            uses.permit(EntityId::Loan(current), call);
            uses.permit(EntityId::Loan(current), end);
            cleanup.push(drop);
        }
    }
    Ok(Delivery {
        call,
        cleanup,
        argument_owners,
        map_capabilities,
        map_types,
    })
}
