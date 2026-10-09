use super::ParameterMode;
use crate::{
    parser::{
        BorrowReturnSource, FunctionForm, FunctionResultSource, NameMarker, ParameterModeMarker,
        ValueParameter,
    },
    source::{SourceError, SourceMap, Span},
};

/// 普通借用结果的唯一签名来源；不构造一等借用类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorrowReturnOrigin {
    /// 源码声明顺序的非 owning 参数。
    Parameter(usize),
    /// 当前实例 callable 的非 owning receiver。
    Receiver,
}

/// Phase 2 验证的普通借用结果合同；实际返回 origin 仍由 Phase 3 检查。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowReturnContract {
    origin: BorrowReturnOrigin,
    marker_span: Span,
    source_span: Span,
}

impl BorrowReturnContract {
    pub(crate) const fn intrinsic_receiver(marker_span: Span, source_span: Span) -> Self {
        Self {
            origin: BorrowReturnOrigin::Receiver,
            marker_span,
            source_span,
        }
    }

    /// 返回唯一参数/receiver 来源。
    pub const fn origin(self) -> BorrowReturnOrigin {
        self.origin
    }
    /// 返回声明的真实 borrow marker。
    pub const fn marker_span(self) -> Span {
        self.marker_span
    }
    /// 返回声明的来源范围。
    pub const fn source_span(self) -> Span {
        self.source_span
    }
}

pub(crate) struct BorrowReturnIssue {
    pub(crate) span: Span,
    pub(crate) message: &'static str,
}

pub(crate) fn resolve_borrow_return(
    sources: &SourceMap,
    form: FunctionForm,
    parameters: &[ValueParameter],
    receiver: Option<ParameterMode>,
) -> Result<Result<Option<BorrowReturnContract>, BorrowReturnIssue>, SourceError> {
    let FunctionForm::Explicit {
        result_source: Some(FunctionResultSource::Borrow(syntax)),
        ..
    } = form
    else {
        return Ok(Ok(None));
    };
    let (origin, source_span) = match syntax.source {
        BorrowReturnSource::Receiver(span)
            if matches!(receiver, Some(ParameterMode::Borrow | ParameterMode::Inout)) =>
        {
            (BorrowReturnOrigin::Receiver, span)
        }
        BorrowReturnSource::Receiver(span) => {
            return Ok(Err(BorrowReturnIssue {
                span,
                message: "borrow result requires a non-owning receiver source",
            }));
        }
        BorrowReturnSource::Parameter(NameMarker::Present(span)) => {
            let name = sources.slice(span)?;
            let mut source = None;
            for (index, parameter) in parameters.iter().enumerate() {
                if let NameMarker::Present(parameter_name) = parameter.name
                    && sources.slice(parameter_name)? == name
                {
                    if matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_))) {
                        return Ok(Err(BorrowReturnIssue {
                            span,
                            message: "owned parameter cannot be a borrow result source",
                        }));
                    }
                    source = Some(index);
                    break;
                }
            }
            let Some(index) = source else {
                return Ok(Err(BorrowReturnIssue {
                    span,
                    message: "borrow result source must name one parameter or this",
                }));
            };
            (BorrowReturnOrigin::Parameter(index), span)
        }
        BorrowReturnSource::Parameter(NameMarker::Missing(span) | NameMarker::Error(span)) => {
            return Ok(Err(BorrowReturnIssue {
                span,
                message: "borrow result requires a unique source",
            }));
        }
    };
    Ok(Ok(Some(BorrowReturnContract {
        origin,
        marker_span: syntax.borrow_span,
        source_span,
    })))
}
