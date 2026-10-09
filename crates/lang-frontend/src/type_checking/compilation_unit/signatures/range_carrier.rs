//! 签名中的 N1a 使用位置；body 推断结果由 body checker 检查。
use super::*;
use crate::type_checking::range_type_uses::{range_type_issues, unit_contains_range};

impl SignatureCollector<'_> {
    pub(super) fn check_carrier_contract(
        &mut self,
        contract: crate::type_checking::CallableResultSource,
        result: UnitTypeId,
        parameters: &[UnitCallableParameter],
        receiver: Option<UnitCallableReceiver>,
    ) -> Result<(), CompilationUnitTypeError> {
        use crate::type_checking::{
            BorrowReturnOrigin, CallableResultSource, IntrinsicTypeConstructor,
        };
        let CallableResultSource::Carrier(carrier) = contract else {
            return Ok(());
        };
        let source = match carrier.origin() {
            BorrowReturnOrigin::Parameter(index) => parameters.get(index).map(|p| p.ty()),
            BorrowReturnOrigin::Receiver => receiver.map(|r| r.ty()),
        };
        let result = match self.types.get(result) {
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            }) => arguments.first().copied(),
            _ => None,
        };
        let source = source.and_then(|ty| match self.types.get(ty) {
            Some(UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View | IntrinsicTypeConstructor::List,
                arguments,
            }) => arguments.first().copied(),
            _ => None,
        });
        if let Some(issue) =
            crate::type_checking::result_source::carrier_shape_issue(contract, result, source)
        {
            self.emit(codes::INVALID_BORROW_CONTRACT, issue.message, issue.span)?;
        }
        Ok(())
    }

    pub(super) fn check_range_types(&mut self) -> Result<(), CompilationUnitTypeError> {
        for source_index in 0..self.inputs.len() {
            let source = self.names.source_units()[source_index].source_unit();
            let issues = range_type_issues(
                self.inputs[source_index].ast(),
                |id| {
                    self.type_ref_types
                        .get(&UnitTypeRefId::new(source, id))
                        .is_some_and(|&ty| unit_contains_range(&self.types, ty))
                },
                |_| false,
            )
            .map_err(TypeCheckingError::from)?;
            for issue in issues {
                self.emit(issue.code, issue.message, issue.span)?;
            }
        }
        Ok(())
    }
}

impl SignatureCollector<'_> {
    pub(super) fn bind_range_extension(
        &self,
        source: SourceUnitId,
        target: UnitCallableTarget,
        receiver: UnitCallableReceiver,
        result: UnitTypeId,
        contract: crate::type_checking::CallableResultSource,
    ) -> Option<crate::type_checking::RangeExtensionBinding<UnitCallableTarget, UnitTypeId>> {
        use crate::type_checking::{
            IntrinsicTypeConstructor, RangeExtensionBinding, RangeSourceKind,
        };
        if !matches!(target, UnitCallableTarget::Declaration(_)) {
            return None;
        }
        let shape = match self.types.get(receiver.ty())? {
            UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::List,
                arguments,
            } if arguments.len() == 1 => Some((RangeSourceKind::List, arguments[0])),
            UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            } if arguments.len() == 1 => Some((RangeSourceKind::View, arguments[0])),
            _ => None,
        };
        let element = match self.types.get(result)? {
            UnitTypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::View,
                arguments,
            } if arguments.len() == 1 => Some(arguments[0]),
            _ => None,
        };
        let source_id = self.inputs[source.index()].source_id();
        RangeExtensionBinding::bind(
            self.environment
                .is_authorized_range_extension_source(source_id),
            source_id,
            target,
            receiver.ty(),
            shape,
            element,
            receiver.mode(),
            contract,
            receiver.declaration_span(),
        )
    }
}
