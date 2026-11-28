//! Index ownership-confirmed format receiver reads by their enclosing source body.
use super::*;

pub(super) struct DisplayRead {
    pub(super) operand: UnitExpressionId,
    pub(super) requirement: UnitCallableTarget,
    pub(super) span: Span,
}

pub(super) fn index_display_reads(
    owned: &dyn UnitLoweringFacts,
    parsed: &[&ParsedFile],
    templates: &[UnitFunctionTemplate],
) -> Result<Vec<Vec<DisplayRead>>, LoweringError> {
    let mut reads = (0..templates.len()).map(|_| Vec::new()).collect::<Vec<_>>();
    if owned.interpolation_evaluations().is_empty() {
        return Ok(reads);
    }
    let mut by_source = BTreeMap::<SourceUnitId, Vec<(Span, usize)>>::new();
    for (index, template) in templates.iter().enumerate() {
        by_source
            .entry(template.source_unit)
            .or_default()
            .push((template.span, index));
    }
    for scopes in by_source.values_mut() {
        scopes.sort_by_key(|(span, _)| span.start());
        if let Some(pair) = scopes
            .windows(2)
            .find(|pair| pair[0].0.end() > pair[1].0.start())
        {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, pair[1].0));
        }
    }
    for evaluation in owned.interpolation_evaluations() {
        let Some(target) = evaluation.requirement().filter(|_| evaluation.completes()) else {
            continue;
        };
        let operand = evaluation.operand();
        let span = parsed
            .get(operand.source_unit().index())
            .and_then(|file| file.ast().expressions().get(operand.expression()).ok())
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let Some(scopes) = by_source.get(&operand.source_unit()) else {
            continue;
        };
        let end = scopes.partition_point(|(scope, _)| scope.start() <= span.start());
        if let Some(&(scope, index)) = scopes.get(..end).and_then(|scopes| scopes.last())
            && span_contains(scope, span)
        {
            reads[index].push(DisplayRead {
                operand,
                requirement: target,
                span,
            });
        }
    }
    Ok(reads)
}
