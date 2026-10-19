//! The caller keeps each actual make result through its exact scanner CallReturn.
use super::*;
pub(super) fn check(
    c: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    i: &ast::Input,
    module: &Module,
) -> Result<(), Failure> {
    let entry = function(module, i.entry.span)?;
    let scanner = function(module, i.scan.span)?;
    let g = Graph::with_abort(entry)?;
    let mut uses = Uses::default();
    let mut owners = BTreeSet::new();
    for call in &i.callers {
        let invoke = at(&g, ast::span(parsed, call.id), |o| {
            matches!(o, Operation::DirectCall { .. })
        })?;
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &invoke.operation
        else {
            return Err(failure(
                "scope original scanner ABI actual invocation",
                invoke,
            ));
        };
        need(
            *callee == scanner.id
                && arguments.len() == call.arguments.len()
                && invoke.results.is_empty(),
            "scope caller actual original scanner invocation",
            invoke,
        )?;
        if c.gate == 2 {
            continue;
        }
        let make = function(module, i.make.as_ref().unwrap().span)?;
        let birth = at(&g, ast::span(parsed, call.arguments[0]), |o| {
            matches!(o, Operation::DirectCall { .. })
        })?;
        need(
            birth.operation
                == Operation::DirectCall {
                    callee: make.id,
                    receiver: None,
                    arguments: vec![],
                },
            "scope actual owned result born at exact make()",
            birth,
        )?;
        let seed = value(birth)?;
        owners.insert(seed);
        uses.protect(EntityId::Value(seed));
        for (n, &argument) in arguments.iter().enumerate() {
            let begin = g.definition(argument)?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = begin.operation
            else {
                return Err(failure("scope actual caller temporary Shared loan", begin));
            };
            let root = g.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner } = root.operation else {
                return Err(failure("scope caller actual temporary root", root));
            };
            let producer = g.definition(EntityId::Value(owner))?;
            need(
                begin.origin == Origin::Source(ast::span(parsed, call.arguments[n]))
                    && root.origin == begin.origin
                    && if n == 0 {
                        owner == seed && producer.id == birth.id
                    } else {
                        matches!(
                            producer.operation,
                            Operation::Constant(ScalarConstant::Boolean(value)) if value == (sources.slice(ast::span(parsed, call.arguments[n])) == Ok("true"))
                        ) && producer.origin == begin.origin
                    },
                "scope actual ordered Map/Boolean root/loan producers",
                (root, begin, producer),
            )?;
            uses.protect(EntityId::Value(owner));
            uses.permit(EntityId::Value(owner), root);
            uses.protect(EntityId::Place(place));
            uses.permit(EntityId::Place(place), begin);
            uses.protect(argument);
            uses.permit(argument, invoke);
            before(&g, producer, root)?;
            before(&g, root, begin)?;
            before(&g, begin, invoke)?;
            let end=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==argument)),"scope unique exact caller argument End")?;
            need(
                end.origin == Origin::Source(ast::span(parsed, call.id)),
                "scope caller exact CallReturn End",
                end,
            )?;
            before(&g, invoke, end)?;
            uses.permit(argument, end);
            if n == 0 {
                let drop = one(
                    g.instructions
                        .iter()
                        .copied()
                        .filter(|op| op.operation == Operation::Drop { owner: seed }),
                    "scope sole original make owner cleanup",
                )?;
                need(
                    drop.origin == Origin::Source(ast::span(parsed, call.arguments[0])),
                    "scope actual temporary map cleanup identity",
                    drop,
                )?;
                before(&g, end, drop)?;
                uses.permit(EntityId::Value(seed), drop);
            }
        }
        let map = entry
            .entity(EntityId::Value(seed))
            .unwrap()
            .ty
            .semantic_type();
        let produce = ast::function(sources, parsed, "produce")?;
        let original = super::super::super::temporary_arguments::sources().remove(0);
        super::super::super::temporary_arguments::graph::factory::check(
            &original,
            sources,
            parsed,
            module,
            function(module, produce.span)?,
            map,
        )?;
        let mg = Graph::with_abort(make)?;
        let selected=one(mg.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::DirectCall{callee,..}if callee==function(module,produce.span).unwrap().id)),"scope make returns actual populated factory result")?;
        let result = value(selected)?;
        need(
            mg.blocks
                .iter()
                .filter_map(|&block| match &mg.term(block).unwrap().kind {
                    TerminatorKind::Return { values } => Some(values.as_slice()),
                    _ => None,
                })
                .eq([&[result][..]]),
            "scope make exact current factory result normal Return",
            selected,
        )?;
    }
    need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count()==owners.len(),"scope caller exactly one Drop per actual map result",&owners)?;
    uses.check(&g, None)
}
