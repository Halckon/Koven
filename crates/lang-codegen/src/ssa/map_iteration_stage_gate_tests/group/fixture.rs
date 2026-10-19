//! Successful Group admission retains the original source-set and returned entry identities.
pub(super) use super::super::fixture::{Fixture, Location, Stopped, clean, location, stopped};
use crate::ssa::{
    LoweringError,
    model::{FunctionId, Origin, Program},
    verify::verify_program,
};
use lang_frontend::source::SourceMap;

#[derive(Debug)]
pub(super) struct LowerOutcome {
    expected_rejection: Result<Location, Stopped>,
    error: Option<LoweringError>,
    actual_rejection: Option<Location>,
    success: Option<String>,
    verified: bool,
}

impl LowerOutcome {
    pub fn new(
        sources: &SourceMap,
        expected_rejection: Result<Location, Stopped>,
        expected_entry: Option<Location>,
        result: Result<(Program, FunctionId), LoweringError>,
    ) -> Self {
        let mut verified = false;
        let (error, success) = match result {
            Err(error) => (Some(error), None),
            Ok((program, entry)) => {
                // 使用 lower 返回的真实 ID 查询本 Program，不按函数名或固定槽索引替代。
                let function = program
                    .module(entry.module())
                    .and_then(|module| module.function(entry));
                let actual_entry = function.and_then(|function| match &function.origin {
                    Origin::Source(span) => location(sources, *span),
                    Origin::Synthetic { .. } => None,
                });
                let entry_identity = function
                    .filter(|_| expected_entry.is_some() && actual_entry == expected_entry)
                    .map(|function| function.id());
                // 来源 query 即使失败仍执行两条真实 verifier 路径；LLVM 入口会再次验证 SSA。
                let ssa_verifier = verify_program(&program).map_err(|error| format!("{error:?}"));
                let verified_llvm_bytes = crate::llvm::render_verified_program(&program)
                    .map(|llvm| llvm.len())
                    .map_err(|error| format!("{error:?}"));
                verified = entry_identity == Some(entry)
                    && ssa_verifier.is_ok()
                    && verified_llvm_bytes.is_ok();
                (
                    None,
                    Some(format!(
                        "entry={entry:?} expected={expected_entry:?} actual={actual_entry:?} \
                         identity={entry_identity:?} ssa={ssa_verifier:?} llvm={verified_llvm_bytes:?}"
                    )),
                )
            }
        };
        let actual_rejection = error
            .and_then(|error| error.span)
            .and_then(|span| location(sources, span));
        Self {
            expected_rejection,
            error,
            actual_rejection,
            success,
            verified,
        }
    }

    pub fn matches(&self) -> bool {
        self.expected_rejection
            .as_ref()
            .is_ok_and(|original| original.text == super::FOR_GROUP)
            && self.error.is_none()
            && self.actual_rejection.is_none()
            && self.success.is_some()
            && self.verified
    }
}
