//! SSA type rendering for the deterministic debug renderer.
use std::fmt::{self, Write};

use super::super::model::{ClosureCaptureMode, SsaTypeKind};
use super::{write_callable_signature, write_type_id, write_type_ids};

pub(super) fn write_type_kind(output: &mut String, kind: &SsaTypeKind) -> fmt::Result {
    match kind {
        SsaTypeKind::Unit => output.write_str("unit"),
        SsaTypeKind::Boolean => output.write_str("bool"),
        SsaTypeKind::Char => output.write_str("char"),
        SsaTypeKind::Integer { bits, signed } => {
            write!(output, "{}{bits}", if *signed { 'i' } else { 'u' })
        }
        SsaTypeKind::Opaque { name, ownership } => {
            write!(output, "opaque {name:?} {:?}", ownership)
        }
        SsaTypeKind::ZeroSized { name, ownership } => {
            write!(output, "zst {name:?} {:?}", ownership)
        }
        SsaTypeKind::Aggregate {
            name,
            fields,
            ownership,
        } => {
            write!(output, "aggregate {name:?} {:?} (", ownership)?;
            write_type_ids(output, fields)?;
            output.push(')');
            Ok(())
        }
        SsaTypeKind::TaggedUnion {
            name,
            variants,
            ownership,
        } => {
            write!(output, "tagged_union {name:?} {:?} (", ownership)?;
            write_type_ids(output, variants)?;
            output.push(')');
            Ok(())
        }
        SsaTypeKind::HeapOwner { name, payload } => {
            write!(output, "heap_owner {name:?}")?;
            if let Some(payload) = payload {
                output.write_str(" payload ")?;
                write_type_id(output, *payload)?;
            } else {
                output.write_str(" <declared>")?;
            }
            Ok(())
        }
        SsaTypeKind::SharedOwner { name, payload } => {
            write!(output, "shared_owner {name:?}")?;
            if let Some(payload) = payload {
                output.write_str(" payload ")?;
                write_type_id(output, *payload)?;
            } else {
                output.write_str(" <declared>")?;
            }
            Ok(())
        }
        SsaTypeKind::StringOwner => output.write_str("string_owner"),
        SsaTypeKind::NullableHandle { inner } => {
            output.write_str("nullable_handle<")?;
            write_type_id(output, *inner)?;
            output.push('>');
            Ok(())
        }
        SsaTypeKind::InlineNullable { inner, niche } => {
            let form = if *niche { "niche" } else { "inline" };
            write!(output, "nullable_{form}<")?;
            write_type_id(output, *inner)?;
            output.push('>');
            Ok(())
        }
        SsaTypeKind::SequentialContainer { kind, element } => {
            write!(output, "container {kind:?}<")?;
            write_type_id(output, *element)?;
            output.push('>');
            Ok(())
        }
        SsaTypeKind::RangeView { source } => {
            output.write_str("range_view<")?;
            write_type_id(output, *source)?;
            output.write_str(">")
        }
        SsaTypeKind::MapContainer { kind, key, value } => {
            write!(output, "map {kind:?}<")?;
            write_type_id(output, *key)?;
            output.write_str(", ")?;
            write_type_id(output, *value)?;
            output.push('>');
            Ok(())
        }
        SsaTypeKind::SharedReference { target } => {
            output.write_str("shared_ref<")?;
            write_type_id(output, *target)?;
            output.push('>');
            Ok(())
        }
        SsaTypeKind::FunctionPointer { signature } => {
            output.write_str("function_pointer ")?;
            write_callable_signature(output, signature)
        }
        SsaTypeKind::CallableView { signature } => {
            output.write_str("callable_view ")?;
            write_callable_signature(output, signature)
        }
        SsaTypeKind::ConcreteClosure {
            name,
            signature,
            environment,
            captures,
        } => {
            write!(output, "closure {name:?} env ")?;
            write_type_id(output, *environment)?;
            output.write_str(" captures (")?;
            for (index, capture) in captures.iter().enumerate() {
                if index != 0 {
                    output.write_str(", ")?;
                }
                output.write_str(match capture.mode {
                    ClosureCaptureMode::Shared => "shared ",
                    ClosureCaptureMode::Owned => "owned ",
                })?;
                write_type_id(output, capture.ty)?;
            }
            output.write_str(") ")?;
            write_callable_signature(output, signature)
        }
    }
}
