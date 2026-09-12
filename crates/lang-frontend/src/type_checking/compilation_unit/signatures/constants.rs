//! SPEC-0197 classifier 内普通 constant 类型预声明。

use crate::{
    name_resolution::{DeclarationId, SourceUnitId},
    parser::{ClassifierBody, Item},
    type_checking::{CompilationUnitTypeError, DeferredReason, UnitTypeKind},
};

use super::{SignatureCollector, item_visibility, unwrapped_item};

impl SignatureCollector<'_> {
    pub(super) fn collect_member_constant_types(
        &mut self,
        source: SourceUnitId,
        owner: DeclarationId,
        body: &ClassifierBody,
    ) -> Result<(), CompilationUnitTypeError> {
        for item in &body.members {
            match unwrapped_item(self.inputs[source.index()].ast(), *item)? {
                Item::Constant { name, type_ref, .. } => {
                    let Some(symbol) = self.marker_symbol(source, *name) else {
                        continue;
                    };
                    let ty = match type_ref {
                        Some(type_ref) => self.resolve_type_ref(source, *type_ref)?,
                        None => self
                            .types
                            .intern(UnitTypeKind::Deferred(DeferredReason::ForwardValueType)),
                    };
                    self.symbol_types.insert(symbol, ty);
                    let visibility = item_visibility(self.inputs[source.index()].ast(), *item)?;
                    self.constant_declarations
                        .insert(symbol, (owner, visibility));
                }
                Item::Companion(companion) => {
                    self.collect_member_constant_types(source, owner, &companion.body)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}
