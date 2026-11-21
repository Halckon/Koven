//! Trusted direct class-field exchange, separate from whole-root owner replacement.

pub(in crate::ssa) mod readonly_receiver {
    //! Narrow single-file Borrow receiver admission and the shared readonly entry view.
    use lang_frontend::type_checking::{
        BuiltinType, CallableDescriptor, CallableResultSource, NominalId, NominalKind,
        ParameterMode, TypeId, TypeKind, TypedFile,
    };

    pub(in crate::ssa) fn admitted(
        typed: &TypedFile,
        callable: &CallableDescriptor,
    ) -> Option<(NominalId, TypeId)> {
        let owner = callable.owner()?;
        let receiver = callable.receiver()?;
        let TypeKind::Nominal { nominal, arguments } = typed.types().get(receiver.ty())? else {
            return None;
        };
        let descriptor = typed
            .nominals()
            .iter()
            .find(|descriptor| descriptor.id() == owner)?;
        (receiver.mode() == ParameterMode::Borrow
            && *nominal == owner
            && arguments.is_empty()
            && descriptor.kind() == NominalKind::ValueClass
            && descriptor.type_parameters().is_empty()
            && descriptor.interfaces().is_empty()
            && callable.type_parameters().is_empty()
            && callable.result_source() == CallableResultSource::Owned
            && matches!(
                super::super::builtin_type(typed, callable.return_type()),
                Some(BuiltinType::Int | BuiltinType::Boolean)
            )
            && callable
                .parameters()
                .iter()
                .all(|parameter| parameter.mode != ParameterMode::Inout))
        .then_some((owner, receiver.ty()))
    }
}
