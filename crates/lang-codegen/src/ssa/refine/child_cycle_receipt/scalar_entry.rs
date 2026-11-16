//! Captured scalar prefix roles; never inferred from a final diamond shape.
use super::*;
use lang_frontend::type_checking::UnitExpressionId;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ssa::refine) struct ScalarEntry {
    pub prefix: UnitExpressionId,
    pub condition: UnitExpressionId,
    pub calls: [(FunctionId, Span, i128, Span); 2],
    pub split: Option<[BlockRef; 3]>,
    pub join: Option<BlockRef>,
    pub used: Option<(BlockRef, Slot)>,
}
impl ScalarEntry {
    pub(in crate::ssa::refine) fn new(
        inputs: &crate::ssa::refine::RefineInputs<'_>,
        nested: &lang_frontend::ownership_checking::UnitOwnerNestedNormalBodyV1,
    ) -> Result<Option<Self>, LoweringError> {
        use lang_frontend::parser::{Expression, LiteralKind};
        let Some(fact) = nested.scalar_entry_v1() else {
            return Ok(None);
        };
        let fail = || invalid(inputs.span);
        if !fact.is_consistent()
            || fact.parent() != nested.parent()
            || fact.owners().iter().map(|(r, _)| *r).collect::<Vec<_>>()
                != nested.normal().header_roots()
        {
            return Err(fail());
        }
        let mut calls = Vec::new();
        for id in fact.calls() {
            let call = inputs
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .map_err(|_| fail())?;
            let Expression::Call { arguments, .. } = call.payload() else {
                return Err(fail());
            };
            let [argument] = arguments.as_slice() else {
                return Err(fail());
            };
            let arg = inputs
                .parsed
                .ast()
                .expressions()
                .get(argument.value)
                .map_err(|_| fail())?;
            let Expression::Literal(LiteralKind::Integer(kind)) = arg.payload() else {
                return Err(fail());
            };
            let value = lang_frontend::type_checking::integer_literal_magnitude(
                inputs.sources.slice(arg.span()).map_err(|_| fail())?,
                *kind,
            )
            .and_then(|n| i128::try_from(n).ok())
            .ok_or_else(fail)?;
            calls.push((
                super::super::fresh_loop_receipt::callee(inputs, id)?,
                call.span(),
                value,
                arg.span(),
            ));
        }
        Ok(Some(Self {
            prefix: fact.prefix(),
            condition: fact.condition(),
            calls: calls.try_into().map_err(|_| fail())?,
            split: None,
            join: None,
            used: None,
        }))
    }
    pub(in crate::ssa::refine) fn split(
        &mut self,
        id: UnitExpressionId,
        roles: [BlockRef; 3],
        span: Span,
    ) -> Result<(), LoweringError> {
        if id == self.prefix && self.split.replace(roles).is_some() {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn join(
        &mut self,
        id: UnitExpressionId,
        join: BlockRef,
        predecessors: &[BlockRef],
        span: Span,
    ) -> Result<(), LoweringError> {
        if id == self.prefix {
            let [_, a, b] = self.split.ok_or_else(|| invalid(span))?;
            if predecessors != [a, b] || self.join.replace(join).is_some() {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn condition(
        &mut self,
        id: UnitExpressionId,
        block: BlockRef,
        slot: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        if id == self.condition && self.used.replace((block, slot)).is_some() {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(super) fn fold(&mut self, block: BlockRef, successor: BlockRef) {
        if let Some(roles) = &mut self.split {
            for role in roles {
                if *role == successor {
                    *role = block;
                }
            }
        }
        if self.join == Some(successor) {
            self.join = Some(block);
        }
        if let Some((role, _)) = &mut self.used
            && *role == successor
        {
            *role = block;
        }
    }
}
