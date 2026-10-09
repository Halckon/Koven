//! member 候选复用原有文件绑定和 canonical 声明；无新增可见性规则。
use super::*;
impl UnitResolver<'_> {
    pub(super) fn resolve_member_lookups(&mut self) -> Result<(), CompilationUnitNameError> {
        for source_index in 0..self.local.len() {
            let source = SourceUnitId(source_index);
            for hint in self.local[source_index].value_lookup_hints() {
                let target = match hint.target() {
                    ReferenceTarget::Symbol(symbol) => self.local_symbol_target(source, *symbol),
                    ReferenceTarget::OverloadSet(symbols) => {
                        self.local_symbols_target(source, symbols)
                    }
                    _ => {
                        let name = self
                            .sources
                            .slice(hint.span())
                            .map_err(NameResolutionError::from)?;
                        match self.lookup_file_binding(source, Namespace::Value, name) {
                            Some(Ok(binding)) => binding.target(),
                            _ => UnitReferenceTarget::Unresolved,
                        }
                    }
                };
                self.value_lookup_hints.push(UnitNameReference::new(
                    source,
                    hint.span(),
                    Some(Namespace::Value),
                    target,
                ));
            }
        }
        Ok(())
    }
}
