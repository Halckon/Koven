use super::*;

// The actual while-false edge identifies the join that every break must reach.
pub(super) fn break_destination(
    source: &UnitSource,
    function: &Function,
    scope: Span,
    exit: Span,
) -> (Span, BlockId) {
    let loops = source
        .parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, node)| {
            let span = node.span();
            matches!(
                node.payload(),
                lang_frontend::parser::Statement::While { .. }
            ) && span.source_id() == exit.source_id()
                && scope.start() <= span.start()
                && span.end() <= scope.end()
                && span.start() <= exit.start()
                && exit.end() <= span.end()
        })
        .collect::<Vec<_>>();
    let [(_, statement)] = loops.as_slice() else {
        panic!("one real enclosing while for the source break")
    };
    let span = statement.span();
    let lang_frontend::parser::Statement::While { condition, .. } = statement.payload() else {
        unreachable!("only the enclosing while was selected");
    };
    let condition = source
        .parsed
        .ast()
        .expressions()
        .get(*condition)
        .unwrap()
        .span();
    let headers = function
        .blocks
        .iter()
        .filter(|block| {
            block.terminator.as_ref().is_some_and(|terminator| {
                [Origin::Source(span), Origin::Source(condition)].contains(&terminator.origin)
                    && matches!(terminator.kind, TerminatorKind::Conditional { .. })
            })
        })
        .collect::<Vec<_>>();
    let [header] = headers.as_slice() else {
        panic!("one actual while header")
    };
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        unreachable!("the header was selected by its actual terminator")
    };
    let false_exit = function.block(when_false.target).unwrap();
    let terminator = false_exit.terminator.as_ref().unwrap();
    assert_eq!(terminator.origin, Origin::Source(span));
    let TerminatorKind::Branch(edge) = &terminator.kind else {
        panic!("the actual while-false exit branches to the shared break join")
    };
    assert_ne!(edge.target, header.id);
    assert_ne!(when_true.target, false_exit.id);
    assert_ne!(edge.target, false_exit.id);
    assert_ne!(edge.target, when_true.target);
    (span, edge.target)
}
