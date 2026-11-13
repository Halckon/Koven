//! Captured counter operations; the specification is supplied only by FE admission.
use super::*;
use lang_frontend::name_resolution::UnitSymbolId;

#[derive(Clone, Debug)]
pub(super) struct CounterSpec {
    pub(super) counter: UnitSymbolId,
    pub(super) rounds: UnitSymbolId,
    pub(super) initializer: UnitExpressionId,
    pub(super) update_rhs: UnitExpressionId,
    pub(super) initializer_span: Span,
    pub(super) condition_span: Span,
    pub(super) zero_span: Span,
    pub(super) one_span: Span,
    pub(super) update_span: Span,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct AliasUse {
    pub(super) block: BlockRef,
    pub(super) index: usize,
    pub(super) input: Slot,
    pub(super) output: Slot,
}
#[derive(Clone, Debug)]
pub(super) struct Counter {
    pub(super) spec: CounterSpec,
    pub(super) init: Option<AliasUse>,
    pub(super) update: Option<AliasUse>,
    pub(super) checked: Option<([BlockRef; 3], [Slot; 4])>,
}
impl Counter {
    pub(super) fn new(spec: CounterSpec) -> Self {
        Self {
            spec,
            init: None,
            update: None,
            checked: None,
        }
    }
    pub(super) fn bind(
        &mut self,
        symbol: UnitSymbolId,
        id: UnitExpressionId,
        usage: AliasUse,
        s: Span,
    ) -> Result<(), LoweringError> {
        if symbol != self.spec.counter {
            return Ok(());
        }
        if id != self.spec.initializer || self.init.is_some() || usage.input == usage.output {
            return Err(invalid(s));
        }
        self.init = Some(usage);
        Ok(())
    }
    pub(super) fn assign(
        &mut self,
        symbol: UnitSymbolId,
        rhs: UnitExpressionId,
        usage: AliasUse,
        s: Span,
    ) -> Result<(), LoweringError> {
        if symbol != self.spec.counter {
            return Ok(());
        }
        let init = self.init.ok_or_else(|| invalid(s))?;
        let (blocks, values) = self.checked.ok_or_else(|| invalid(s))?;
        if rhs != self.spec.update_rhs
            || self.update.is_some()
            || usage.output != init.output
            || usage.input != values[0]
            || usage.block != blocks[1]
        {
            return Err(invalid(s));
        }
        self.update = Some(usage);
        Ok(())
    }
    pub(super) fn checked(
        &mut self,
        id: UnitExpressionId,
        blocks: [BlockRef; 3],
        values: [Slot; 4],
        s: Span,
    ) -> Result<(), LoweringError> {
        if id != self.spec.update_rhs {
            return Ok(());
        }
        if self.checked.is_some()
            || self.init.is_none()
            || blocks
                .iter()
                .enumerate()
                .any(|(i, b)| blocks[..i].contains(b))
            || values
                .iter()
                .enumerate()
                .any(|(i, v)| values[..i].contains(v))
            || values[2] != self.init.ok_or_else(|| invalid(s))?.output
        {
            return Err(invalid(s));
        }
        self.checked = Some((blocks, values));
        Ok(())
    }
    pub(super) fn fold(&mut self, block: BlockRef, successor: BlockRef, offset: usize) {
        for usage in [&mut self.init, &mut self.update].into_iter().flatten() {
            if usage.block == successor {
                usage.block = block;
                usage.index += offset;
            }
        }
        if let Some((blocks, _)) = &mut self.checked {
            for b in blocks {
                if *b == successor {
                    *b = block;
                }
            }
        }
    }
    pub(super) fn ready(&self, s: Span) -> Result<(), LoweringError> {
        let a = self.init.ok_or_else(|| invalid(s))?;
        let b = self.update.ok_or_else(|| invalid(s))?;
        let (blocks, values) = self.checked.ok_or_else(|| invalid(s))?;
        if self.spec.counter == self.spec.rounds
            || a.output != b.output
            || b.block != blocks[1]
            || b.input != values[0]
            || values[2] != a.output
            || blocks
                .iter()
                .enumerate()
                .any(|(i, x)| blocks[..i].contains(x))
        {
            return Err(invalid(s));
        }
        Ok(())
    }
}
