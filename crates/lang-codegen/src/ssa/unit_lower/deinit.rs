//! Declare source-qualified hidden deinit bodies with the existing readonly receiver ABI.

use super::*;
use crate::ssa::model::Module;

pub(super) fn declare(
    module: &mut Module,
    parsed_by_source: &[&lang_frontend::parser::ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    types: &mut type_lower::UnitTypeLowering,
    instance: UnitPlannedInstance,
) -> Result<FunctionPlan, LoweringError> {
    let owner = instance
        .key()
        .deinit_owner()
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
    let descriptor = typed
        .signatures()
        .declaration(owner)
        .and_then(|signature| signature.nominal())
        .and_then(|nominal| nominal.deinit())
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
    let parsed = parsed_by_source
        .get(instance.source_unit().index())
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
    let (function_item, item, _) = unwrap_modified(parsed, instance.item())?;
    let Item::Deinit { body, .. } = item else {
        return Err(lowering_error(
            LoweringErrorKind::MissingFact,
            instance.span(),
        ));
    };
    if descriptor.owner() != owner
        || descriptor.item() != UnitItemId::new(instance.source_unit(), function_item)
        || descriptor.body() != UnitStatementId::new(instance.source_unit(), body)
        || descriptor.receiver_mode() != ParameterMode::Borrow
    {
        return Err(lowering_error(
            LoweringErrorKind::MissingFact,
            instance.span(),
        ));
    }
    let receiver_type = descriptor.receiver_type();
    let owner_type = types.intern(module, typed, receiver_type, instance.span())?;
    let entity_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: owner_type,
    };
    let origin = Origin::Source(instance.span());
    let id = module
        .add_instance_function(
            instance_function_name(names, &instance),
            entity_type,
            Vec::new(),
            origin.clone(),
        )
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
    module
        .function_mut(id)
        .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?
        .add_block(vec![entity_type], origin)
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
    module
        .set_deinit(owner_type, id)
        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, instance.span()))?;
    let return_type = typed
        .types()
        .builtin(BuiltinType::Unit)
        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, instance.span()))?;
    let receiver = Some(ReceiverPlan {
        owner,
        mode: descriptor.receiver_mode(),
        template_ty: receiver_type,
        ty: receiver_type,
        entity_type,
        origin: instance.span(),
    });
    Ok(FunctionPlan {
        id,
        instance,
        function_item,
        body: FunctionPlanBody::Block(body),
        receiver,
        parameter_symbols: Vec::new(),
        return_type,
    })
}
