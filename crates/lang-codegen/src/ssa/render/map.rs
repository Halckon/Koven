//! Map 的确定性 SSA 调试文本；保持通用渲染器只负责分发。
use super::*;

pub(super) fn write_operation(output: &mut String, operation: &Operation) -> fmt::Result {
    match operation {
        Operation::MapConstruct { map_type } => {
            output.write_str("map.construct ")?;
            write_type_id(output, *map_type)
        }
        Operation::MapSize { owner } => {
            output.write_str("map.size ")?;
            write_entity_id(output, *owner)
        }
        Operation::MapContains { owner, key } => {
            output.write_str("map.contains ")?;
            write_entity_id(output, *owner)?;
            output.write_str(", ")?;
            write_entity_id(output, *key)
        }
        Operation::MapRequireValue { source, key } => {
            write!(output, "map.require_value %l{}, ", source.index())?;
            write_entity_id(output, *key)
        }
        Operation::MapWithValue {
            source,
            key,
            action,
        } => {
            write!(output, "map.with_value %l{}, ", source.index())?;
            write_entity_id(output, *key)?;
            write!(output, ", %l{}", action.index())
        }
        Operation::MapGet { owner, key } => {
            output.write_str("map.get ")?;
            write_entity_id(output, *owner)?;
            output.write_str(", ")?;
            write_entity_id(output, *key)
        }
        Operation::MapResultUnwrap { result } => {
            output.write_str("map.result.unwrap ")?;
            write_entity_id(output, EntityId::Value(*result))
        }
        Operation::MapPut { owner, key, value } => {
            output.write_str("map.put ")?;
            write_entity_id(output, EntityId::Value(*owner))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*key))?;
            output.write_str(", ")?;
            write_entity_id(output, EntityId::Value(*value))
        }
        Operation::MapRemove { owner, key } => {
            output.write_str("map.remove ")?;
            write_entity_id(output, EntityId::Value(*owner))?;
            output.write_str(", ")?;
            write_entity_id(output, *key)
        }
        _ => Err(fmt::Error),
    }
}
