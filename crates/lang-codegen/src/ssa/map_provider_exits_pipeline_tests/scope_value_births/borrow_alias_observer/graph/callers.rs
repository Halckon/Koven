//! The real caller keeps each factory owner until its matching scan returns.
use super::super::super::super::{scope_lambda_body::graph::factory, temporary_arguments};
use super::*;
use lang_frontend::parser::Expression;
pub(super) fn check(
    sources: &SourceMap,
    p: &ParsedFile,
    i: &Input,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "alias caller original module")?;
    let entry = function(module, i.entry)?;
    let scanner = function(module, i.scan)?;
    let make = temporary_arguments::ast::function(sources, p, "make", None)?;
    let produce = temporary_arguments::ast::function(sources, p, "produce", Some("Int"))?;
    let maker = function(module, make.span)?;
    let g = Graph::new(entry)?;
    let mut calls = p
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, n)| match n.payload() {
            Expression::Call {
                callee, arguments, ..
            } if super::super::inside(i.entry, n.span())
                && sources.slice(span(p, *callee)) == Ok("scan") =>
            {
                Some((id, arguments.iter().map(|v| v.value).collect::<Vec<_>>()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    calls.sort_by_key(|(id, _)| span(p, *id).start());
    need(
        calls.len() == 2 && g.blocks.len() == 1 && g.edges.is_empty(),
        "alias actual two straight-line caller invocations",
        &calls,
    )?;
    let mut uses = Uses::default();
    let mut owners = BTreeSet::new();
    for ((id, args), flag) in calls.into_iter().zip([true, false]) {
        let call = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, id))
                    && matches!(op.operation, Operation::DirectCall { .. })
            }),
            "alias actual original scanner call",
        )?;
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &call.operation
        else {
            return Err(failure("alias original caller ABI", call));
        };
        need(
            *callee == scanner.id
                && arguments.len() == 2
                && args.len() == 2
                && call.results.is_empty(),
            "alias exact original selected scanner ABI",
            call,
        )?;
        let birth = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, args[0]))
                    && op.operation
                        == Operation::DirectCall {
                            callee: maker.id,
                            receiver: None,
                            arguments: vec![],
                        }
            }),
            "alias actual caller owned make result",
        )?;
        let owner = value(birth)?;
        need(
            owners.insert(owner),
            "alias distinct actual invocation owner",
            birth,
        )?;
        for (index, &argument) in arguments.iter().enumerate() {
            let begin = g.definition(argument)?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = begin.operation
            else {
                return Err(failure("alias actual caller argument Shared birth", begin));
            };
            let root = g.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner: root_owner } = root.operation else {
                return Err(failure("alias actual caller argument root", root));
            };
            let producer = g.definition(EntityId::Value(root_owner))?;
            need(
                begin.origin == Origin::Source(span(p, args[index]))
                    && root.origin == begin.origin
                    && if index == 0 {
                        root_owner == owner && producer.id == birth.id
                    } else {
                        producer.operation == Operation::Constant(ScalarConstant::Boolean(flag))
                            && producer.origin == begin.origin
                    }
                    && g.position(producer)? < g.position(root)?
                    && g.position(root)? < g.position(begin)?
                    && g.position(begin)? < g.position(call)?,
                "alias exact current Map and original Boolean argument roots/order",
                (producer, root, begin, call),
            )?;
            uses.protect(EntityId::Value(root_owner));
            uses.permit(EntityId::Value(root_owner), root);
            uses.protect(EntityId::Place(place));
            uses.permit(EntityId::Place(place), begin);
            uses.protect(argument);
            uses.permit(argument, call);
            let end=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==argument)),"alias caller unique actual argument End")?;
            need(
                end.origin == call.origin && g.position(call)? < g.position(end)?,
                "alias actual caller loan ends at original scanner return",
                end,
            )?;
            uses.permit(argument, end);
            if index == 0 {
                let drop = one(
                    g.instructions
                        .iter()
                        .copied()
                        .filter(|op| op.operation == Operation::Drop { owner }),
                    "alias sole actual caller Map owner cleanup",
                )?;
                need(
                    drop.origin == birth.origin && g.position(end)? < g.position(drop)?,
                    "alias actual current caller Map Drop follows scanner/alias end",
                    drop,
                )?;
                uses.permit(EntityId::Value(owner), drop);
            }
        }
        let map = entry
            .entity(EntityId::Value(owner))
            .unwrap()
            .ty
            .semantic_type();
        factory::check(
            sources,
            p,
            &factory::Input {
                make: &make,
                produce: &produce,
            },
            module,
            map,
        )?;
    }
    need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count()==owners.len(),"alias exactly one caller Map Drop per actual birth",&owners)?;
    for (index, v) in entry.values.iter().enumerate() {
        if module.map_container(v.ty.semantic_type()).is_some() {
            need(
                owners.contains(&ValueId {
                    function: entry.id,
                    index,
                }),
                "alias all caller Map values are actual make births",
                (index, v),
            )?;
        }
    }
    uses.check(&g, None)
}
