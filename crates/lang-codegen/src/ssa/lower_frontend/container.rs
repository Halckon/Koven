//! 已完成 typed container descriptor 到顺序容器 SSA 的窄化 lowering。

pub(in crate::ssa) mod access;

use lang_frontend::{
    name_resolution::NameResolution,
    source::Span,
    type_checking::{IntrinsicTypeConstructor, TypeId, TypedFile},
};

use super::{LoweringError, LoweringErrorKind, error, nominal::NominalTypeMapper};
use crate::ssa::model::{Module, SequentialContainerKind, SsaTypeId};

impl NominalTypeMapper {
    /// 将 frontend 的封闭顺序容器 identity 递归映射为保留元素类型的 SSA identity。
    pub(in crate::ssa) fn intern_container(
        &mut self,
        module: &mut Module,
        names: &NameResolution,
        typed: &TypedFile,
        constructor: IntrinsicTypeConstructor,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<SsaTypeId, LoweringError> {
        let [element] = arguments else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        let kind = match constructor {
            IntrinsicTypeConstructor::Array => SequentialContainerKind::Array,
            IntrinsicTypeConstructor::List => SequentialContainerKind::List,
            IntrinsicTypeConstructor::MutableList => SequentialContainerKind::MutableList,
            _ => return Err(error(LoweringErrorKind::MissingFact, span)),
        };
        let element = self.intern_inner(module, names, typed, *element, span)?;
        module
            .add_sequential_container_type(kind, element)
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))
    }
}
