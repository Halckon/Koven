use crate::{diagnostic::Diagnostic, source::SourceId};

/// Phase 3 单文件所有权检查产物。
#[derive(Clone, Debug)]
pub struct OwnershipCheckedFile {
    source_id: SourceId,
    diagnostics: Vec<Diagnostic>,
}

impl OwnershipCheckedFile {
    pub(crate) const fn new(source_id: SourceId, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            source_id,
            diagnostics,
        }
    }

    /// 返回输入源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.source_id
    }

    /// 返回稳定源码顺序的所有权诊断。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
