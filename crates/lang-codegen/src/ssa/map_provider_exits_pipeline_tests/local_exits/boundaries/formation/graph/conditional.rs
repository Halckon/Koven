//! Original literal condition and exact true/false arm identity, including the unexecuted arm.
use super::*;
use lang_frontend::parser::LiteralKind;
pub(super) fn check(g: &Graph<'_>, parsed: &ParsedFile, input: &Inputs) -> Result<(), Failure> {
    for (_, node) in parsed.ast().expressions().iter() {
        let Expression::If {
            condition,
            then_branch,
            else_branch: Some(else_branch),
            ..
        } = node.payload()
        else {
            continue;
        };
        if !inside(node.span(), input.site.for_span) {
            continue;
        }
        let condition_node = parsed.ast().expressions().get(*condition).unwrap();
        let Expression::Literal(LiteralKind::Boolean(expected)) = condition_node.payload() else {
            return Err(failure(
                "formation original conditional literal Boolean",
                condition_node,
            ));
        };
        let boolean = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(condition_node.span())
                    && matches!(i.operation, Operation::Constant(ScalarConstant::Boolean(_)))
            }),
            "formation exact original Boolean condition producer",
        )?;
        need(
            boolean.operation == Operation::Constant(ScalarConstant::Boolean(*expected)),
            "formation original Boolean polarity",
            boolean,
        )?;
        let block=one(g.blocks.iter().copied().filter(|&b|g.term(b).is_ok_and(|t|t.origin==Origin::Source(node.span()) && matches!(t.kind,TerminatorKind::Conditional{condition,..} if condition==value(boolean).unwrap()))),"formation actual original If Conditional")?;
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &g.term(block)?.kind
        else {
            unreachable!()
        };
        let branches = [(*then_branch, when_true, 0), (*else_branch, when_false, 1)];
        let mut exits = vec![];
        for (branch, edge, index) in branches {
            let original = parsed.ast().statements().get(branch).unwrap().span();
            need(
                g.block(edge.target)?.origin == Origin::Source(original),
                "formation actual true/false block matches original AST arm",
                (edge, original),
            )?;
            g.incoming_from(edge.target, &[block])?;
            let call = one(
                input
                    .uses
                    .iter()
                    .filter(|u| u.print && inside(u.at, original)),
                "formation original arm print use",
            )?;
            need(
                matches!(call.args.as_slice(),[arg] if arg.root==Some(input.components[index])),
                "formation true prints original K and false prints original V",
                call,
            )?;
            let print = one(
                g.instructions.iter().copied().filter(|i| {
                    i.origin == Origin::Source(call.at)
                        && matches!(i.operation, Operation::PrintString { .. })
                }),
                "formation original arm actual PrintString",
            )?;
            need(
                print.block == edge.target,
                "formation print resides in its actual original arm",
                print,
            )?;
            let outgoing = g.branch(print.block)?;
            exits.push((print.block, outgoing));
        }
        need(
            exits[0].1.target == exits[1].1.target,
            "formation both original arms join one actual continuation",
            &exits,
        )?;
        g.incoming_from(exits[0].1.target, &[exits[0].0, exits[1].0])?;
    }
    Ok(())
}
