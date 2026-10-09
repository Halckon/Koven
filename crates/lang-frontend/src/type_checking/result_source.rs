//! 结果的封闭交付模式；来源声明不是 Phase 3 的实际来源证明。
use super::{BorrowReturnContract, BorrowReturnOrigin, ParameterMode};
use crate::{
    parser::{
        BorrowReturnSource, FunctionForm, FunctionResultSource, NameMarker, ParameterModeMarker,
        ValueParameter,
    },
    source::{SourceError, SourceMap, Span},
};

/// Callable 与已选择调用共享的结果交付模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallableResultSource {
    /// 普通 owned 结果。
    Owned,
    /// 借用已有对象或已有 carrier 的 metadata 存储。
    Borrow(BorrowReturnContract),
    /// 从唯一根来源交付新的内联 carrier 描述符。
    Carrier(CarrierReturnContract),
}

impl CallableResultSource {
    /// 仅返回借用已有存储的合同；新 carrier 不使用普通借用 ABI。
    pub const fn borrow_return(self) -> Option<BorrowReturnContract> {
        match self {
            Self::Borrow(contract) => Some(contract),
            _ => None,
        }
    }
}

/// Phase 2 验证的新描述符唯一来源；真实根和 caller continuation 由 Phase 3 证明。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarrierReturnContract {
    origin: BorrowReturnOrigin,
    marker: CarrierSourceMarker,
    source_span: Span,
}

/// 来源的实际语法标记；原语构造不伪造声明 from。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarrierSourceMarker {
    /// 具名 producer 声明中的真实 from。
    DeclarationFrom(Span),
    /// compiler-bound 构造原语的真实调用范围。
    PrimitiveCall(Span),
}

impl CarrierReturnContract {
    /// 返回唯一 Borrow 参数或 receiver 来源。
    pub const fn origin(self) -> BorrowReturnOrigin {
        self.origin
    }
    /// 返回真实 from marker，不伪造 borrow marker。
    pub const fn from_span(self) -> Option<Span> {
        match self.marker {
            CarrierSourceMarker::DeclarationFrom(span) => Some(span),
            _ => None,
        }
    }
    /// 返回声明来源的真实范围。
    pub const fn source_span(self) -> Span {
        self.source_span
    }
    /// 返回来源交付的真实标记种类。
    pub const fn marker(self) -> CarrierSourceMarker {
        self.marker
    }
    /// 返回声明/原语的实际语法位置。
    pub const fn marker_span(self) -> Span {
        match self.marker {
            CarrierSourceMarker::DeclarationFrom(span)
            | CarrierSourceMarker::PrimitiveCall(span) => span,
        }
    }
    pub(crate) const fn range_primitive(call: Span, source_span: Span) -> Self {
        Self {
            origin: BorrowReturnOrigin::Parameter(0),
            marker: CarrierSourceMarker::PrimitiveCall(call),
            source_span,
        }
    }
}

pub(crate) fn resolve_result_source(
    sources: &SourceMap,
    form: FunctionForm,
    parameters: &[ValueParameter],
    receiver: Option<ParameterMode>,
) -> Result<Result<CallableResultSource, super::borrow_result::BorrowReturnIssue>, SourceError> {
    let FunctionForm::Explicit {
        result_source: Some(FunctionResultSource::Carrier(syntax)),
        ..
    } = form
    else {
        return Ok(super::borrow_result::resolve_borrow_return(
            sources, form, parameters, receiver,
        )?
        .map(|contract| {
            contract.map_or(CallableResultSource::Owned, CallableResultSource::Borrow)
        }));
    };
    let issue = |span, message| Err(super::borrow_result::BorrowReturnIssue { span, message });
    let (origin, source_span) = match syntax.source {
        BorrowReturnSource::Receiver(span) if receiver == Some(ParameterMode::Borrow) => {
            (BorrowReturnOrigin::Receiver, span)
        }
        BorrowReturnSource::Receiver(span) => {
            return Ok(issue(
                span,
                "carrier result requires a Borrow receiver source",
            ));
        }
        BorrowReturnSource::Parameter(NameMarker::Present(span)) => {
            let name = sources.slice(span)?;
            let mut source = None;
            for (index, parameter) in parameters.iter().enumerate() {
                if let NameMarker::Present(parameter_name) = parameter.name
                    && sources.slice(parameter_name)? == name
                {
                    if matches!(
                        parameter.mode_marker,
                        Some(ParameterModeMarker::Own(_) | ParameterModeMarker::Inout(_))
                    ) {
                        return Ok(issue(span, "carrier result source must be Borrow"));
                    }
                    source = Some(index);
                    break;
                }
            }
            let Some(index) = source else {
                return Ok(issue(
                    span,
                    "carrier result source must name one parameter or this",
                ));
            };
            (BorrowReturnOrigin::Parameter(index), span)
        }
        BorrowReturnSource::Parameter(NameMarker::Missing(span) | NameMarker::Error(span)) => {
            return Ok(issue(span, "carrier result requires a unique source"));
        }
    };
    Ok(Ok(CallableResultSource::Carrier(CarrierReturnContract {
        origin,
        marker: CarrierSourceMarker::DeclarationFrom(syntax.from_span),
        source_span,
    })))
}

/// 类型 id 由调用方所在 table 限定；只接收 compiler-bound List/View 的元素 identity。
pub(crate) fn carrier_shape_issue<T: Eq>(
    contract: CallableResultSource,
    result_element: Option<T>,
    source_element: Option<T>,
) -> Option<super::borrow_result::BorrowReturnIssue> {
    let CallableResultSource::Carrier(contract) = contract else {
        return None;
    };
    if result_element.is_none() || result_element != source_element {
        return Some(super::borrow_result::BorrowReturnIssue {
            span: contract.source_span,
            message: "carrier result and its List/View source must have the same element type",
        });
    }
    None
}
