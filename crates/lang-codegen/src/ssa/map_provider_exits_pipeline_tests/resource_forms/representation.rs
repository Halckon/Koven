//! Query the actual represented storage, original Resource deinit and shared component ABI.
use super::*;
use crate::ssa::{
    map_provider_exits_pipeline_tests::{fixture::need, query_inputs::one},
    model::*,
};
use lang_frontend::parser::{Item, NameMarker};

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &lang_frontend::parser::ParsedFile,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "one Resource form module")?;
    let find = |wanted: &str| {
        one(parsed.ast().items().iter().filter_map(|(_,n)|matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok(wanted)).then_some(n.span())),"original function span")
    };
    let scan_span = find("scan")?;
    let read_span = find("read")?;
    let scan = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(scan_span)),
        "actual original scan",
    )?;
    let entry = scan.block(scan.entry_block().unwrap()).unwrap();
    let anchor = one(
        entry.parameters.iter().copied(),
        "actual owned source parameter",
    )?;
    let (kind, key, value) = module
        .map_container(scan.entity(anchor).unwrap().ty.semantic_type())
        .ok_or_else(|| failure("real represented Map", anchor))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        },
        "exact original container kind",
        kind,
    )?;
    need(
        if case.representation == 2 {
            module.type_kind(key)
                == Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
        } else {
            module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
        },
        "exact original key storage",
        key,
    )?;
    let resource = match (case.representation, module.type_kind(value)) {
        (
            1,
            Some(SsaTypeKind::Aggregate {
                fields,
                ownership: Ownership::MoveOnly,
                ..
            }),
        ) if fields.len() == 1 => fields[0],
        (3, Some(SsaTypeKind::NullableHandle { inner })) => *inner,
        (0 | 2, Some(SsaTypeKind::HeapOwner { .. })) => value,
        _ => {
            return Err(failure(
                "existing Resource representation",
                module.type_kind(value),
            ));
        }
    };
    let Some(SsaTypeKind::HeapOwner { payload, .. }) = module.type_kind(resource) else {
        return Err(failure("underlying Resource heap owner", resource));
    };
    if case.representation == 0 || case.representation == 3 {
        let payload = payload.ok_or_else(|| failure("actual Resource fields", resource))?;
        need(
            strings(module, payload) == 1,
            "owned String field remains in original Resource payload",
            module.type_kind(payload),
        )?;
    }
    let class = one(
        parsed.ast().items().iter().filter_map(|(_, n)| {
            let Item::Classifier(class) = n.payload() else {
                return None;
            };
            matches!(class.name,NameMarker::Present(name) if sources.slice(name)==Ok("Resource"))
                .then_some(class)
        }),
        "original Resource declaration",
    )?;
    let deinit = one(
        class
            .body
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                matches!(n.payload(), Item::Deinit { .. }).then_some(n.span())
            }),
        "original Resource deinit span",
    )?;
    let actual = module
        .deinits
        .get(&resource)
        .and_then(|id| module.function(*id))
        .ok_or_else(|| failure("actual Resource deinit", resource))?;
    need(
        actual.origin == Origin::Source(deinit),
        "Resource storage uses its original deinit",
        actual.id,
    )?;
    let reader = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(read_span)),
        "actual reader",
    )?;
    let call = one(
        scan.instructions.iter().filter(
            |i| matches!(&i.operation,Operation::DirectCall{callee,..} if *callee==reader.id),
        ),
        "actual component reader call",
    )?;
    let Operation::DirectCall { arguments, .. } = &call.operation else {
        unreachable!()
    };
    let key_loan = one(
        scan.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
        "actual provider key loan",
    )?;
    let value_loan = one(
        scan.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
        "actual provider value loan",
    )?;
    need(
        key_loan.results.len() == 1
            && value_loan.results.len() == 1
            && arguments.as_slice() == [key_loan.results[0], value_loan.results[0]],
        "reader consumes the actual current K/V loan operands",
        (call, key_loan, value_loan),
    )?;
    for component in [key_loan, value_loan] {
        let EntityId::Loan(loan) = component.results[0] else {
            return Err(failure("actual Shared component", component));
        };
        let end = one(
            scan.instructions
                .iter()
                .filter(|i| matches!(i.operation,Operation::BorrowEnd{loan:l} if l==loan)),
            "actual component End",
        )?;
        let block = scan.block(call.block).unwrap();
        need(
            end.block == call.block
                && block.instructions.iter().position(|id| *id == call.id)
                    < block.instructions.iter().position(|id| *id == end.id),
            "actual component ends after its read",
            (call, end),
        )?;
    }
    let parameters = &reader
        .block(reader.entry_block().unwrap())
        .unwrap()
        .parameters;
    need(
        arguments.len() == 2 && parameters.len() == 2,
        "actual two-component reader ABI",
        arguments,
    )?;
    for (slot, target) in [key, value].into_iter().enumerate() {
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        };
        need(
            scan.entity(arguments[slot]).unwrap().ty == ty
                && reader.entity(parameters[slot]).unwrap().ty == ty,
            "shared component preserves exact original represented type",
            (slot, target),
        )?;
    }
    need(!scan.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if [value,resource].contains(&scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()))),"provider never extracts or drops Resource component owner",scan.id)
}
fn strings(module: &Module, ty: SsaTypeId) -> usize {
    match module.type_kind(ty) {
        Some(SsaTypeKind::StringOwner) => 1,
        Some(SsaTypeKind::Aggregate { fields, .. }) => {
            fields.iter().map(|&f| strings(module, f)).sum()
        }
        _ => 0,
    }
}
