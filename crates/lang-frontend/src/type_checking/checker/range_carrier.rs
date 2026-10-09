//! N1a 已规范化类型的使用位置检查。
use super::*;
use crate::type_checking::range_type_uses::{file_contains_range, range_type_issues};

impl Checker<'_> {
    pub(super) fn check_carrier_contract(
        &mut self,
        contract: crate::type_checking::CallableResultSource,
        result: TypeId,
        parameters: &[FunctionParameterType],
        receiver: Option<CallableReceiverDescriptor>,
    ) -> Result<(), TypeCheckingError> {
        use crate::type_checking::{BorrowReturnOrigin, CallableResultSource};
        let CallableResultSource::Carrier(carrier) = contract else {
            return Ok(());
        };
        let source = match carrier.origin() {
            BorrowReturnOrigin::Parameter(index) => parameters.get(index).map(|p| p.ty),
            BorrowReturnOrigin::Receiver => receiver.map(|r| r.ty()),
        };
        let result = match self.types.get(result) {
            Some(TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            }) => arguments.first().copied(),
            _ => None,
        };
        let source = source.and_then(|ty| match self.types.get(ty) {
            Some(TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View | IntrinsicTypeConstructor::List,
                arguments,
            }) => arguments.first().copied(),
            _ => None,
        });
        if let Some(issue) =
            crate::type_checking::result_source::carrier_shape_issue(contract, result, source)
        {
            self.emit(
                codes::catalog()?.resolve(codes::INVALID_BORROW_CONTRACT)?,
                issue.message,
                issue.span,
            )?;
        }
        Ok(())
    }

    pub(super) fn check_range_types(&mut self) -> Result<(), TypeCheckingError> {
        let issues = range_type_issues(
            self.ast(),
            |id| {
                self.type_ref_types[id.index()]
                    .is_some_and(|ty| file_contains_range(&self.types, ty))
            },
            |id| {
                self.expression_types[id.index()]
                    .is_some_and(|ty| file_contains_range(&self.types, ty))
            },
        )?;
        for issue in issues {
            self.emit(
                codes::catalog()?.resolve(issue.code)?,
                issue.message,
                issue.span,
            )?;
        }
        Ok(())
    }
}

impl Checker<'_> {
    pub(super) fn bind_range_extension(
        &self,
        symbol: SymbolId,
        receiver: CallableReceiverDescriptor,
        result: TypeId,
        contract: crate::type_checking::CallableResultSource,
    ) -> Option<
        crate::type_checking::RangeExtensionBinding<crate::type_checking::CallableTarget, TypeId>,
    > {
        use crate::type_checking::{CallableTarget, RangeExtensionBinding, RangeSourceKind};
        let shape = match self.types.get(receiver.ty())? {
            TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments,
            } if arguments.len() == 1 => Some((RangeSourceKind::List, arguments[0])),
            TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            } if arguments.len() == 1 => Some((RangeSourceKind::View, arguments[0])),
            _ => None,
        };
        let element = match self.types.get(result)? {
            TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            } if arguments.len() == 1 => Some(arguments[0]),
            _ => None,
        };
        RangeExtensionBinding::bind(
            self.environment
                .is_authorized_range_extension_source(self.parsed.source_id()),
            self.parsed.source_id(),
            CallableTarget::Source(symbol),
            receiver.ty(),
            shape,
            element,
            receiver.mode(),
            contract,
            receiver.declaration_span(),
        )
    }
}

impl Checker<'_> {
    pub(super) fn marker_symbol_for_receiver(
        &self,
        name: NameMarker,
    ) -> Option<CallableReceiverDescriptor> {
        let NameMarker::Present(span) = name else {
            return None;
        };
        let symbol = self.symbol_at(span)?;
        let callable = self.typed_callables.iter().find(|c| c.symbol() == symbol)?;
        callable.range_extension()?;
        callable.receiver()
    }
}
