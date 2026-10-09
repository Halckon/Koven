//! 宿主显式授予 N1a 标准来源能力；不从源码文本或路径推断。
use super::{TypeCheckingError, TypeEnvironment};
use crate::source::{SourceId, SourceMap};

impl TypeEnvironment {
    /// 单独授权宿主加载的标准来源登记受限 Borrow List/View 扩展签名。
    /// 不授予 producer/构造权限，也不代表实际来源或后端交付已证明。
    pub fn authorize_range_extension_source(
        &mut self,
        sources: &SourceMap,
        source: SourceId,
    ) -> Result<(), TypeCheckingError> {
        sources.source_text(source)?;
        if !self.range_extension_sources.contains(&source) {
            self.range_extension_sources.push(source);
        }
        Ok(())
    }

    /// 查询独立扩展权限；既有 producer 权限不会隐式升级。
    pub fn is_authorized_range_extension_source(&self, source: SourceId) -> bool {
        self.range_extension_sources.contains(&source)
    }

    /// 授权已加载的标准来源使用受限 N1a producer/构造原语。
    /// SourceId 包含 SourceMap 身份；同名文件和其他 map 的同下标不能冒充。
    pub fn authorize_range_source(
        &mut self,
        sources: &SourceMap,
        source: SourceId,
    ) -> Result<(), TypeCheckingError> {
        sources.source_text(source)?;
        if !self.range_sources.contains(&source) {
            self.range_sources.push(source);
        }
        Ok(())
    }

    /// 查询宿主授权；源码不能构造或扩大该能力。
    pub fn is_authorized_range_source(&self, source: SourceId) -> bool {
        self.range_sources.contains(&source)
    }
}
