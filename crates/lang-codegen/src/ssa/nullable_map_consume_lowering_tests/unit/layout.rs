//! Explicit original input roles; memory roles share one source, never a fabricated second file.
use super::*;
use lang_frontend::name_resolution::{SourceUnit, SourceUnitKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ssa::nullable_map_consume_lowering_tests) enum Layout {
    Memory,
    Rooted,
}

impl Layout {
    pub(in crate::ssa::nullable_map_consume_lowering_tests) fn entry_name(self) -> &'static str {
        match self {
            Self::Memory => "main",
            Self::Rooted => "entry",
        }
    }

    pub(in crate::ssa::nullable_map_consume_lowering_tests) fn provider_matches(
        self,
        case: &Case,
        sources: &SourceMap,
        names: &ValidatedCompilationUnitNames,
        unit: &SourceUnit,
    ) -> bool {
        self.matches(case, sources, names, unit, true)
    }

    pub(in crate::ssa::nullable_map_consume_lowering_tests) fn consumer_matches(
        self,
        case: &Case,
        sources: &SourceMap,
        names: &ValidatedCompilationUnitNames,
        unit: &SourceUnit,
    ) -> bool {
        self.matches(case, sources, names, unit, false)
    }

    fn matches(
        self,
        case: &Case,
        sources: &SourceMap,
        names: &ValidatedCompilationUnitNames,
        unit: &SourceUnit,
        provider: bool,
    ) -> bool {
        let package = names
            .names()
            .index()
            .packages()
            .iter()
            .find(|package| package.id() == unit.package())
            .and_then(|package| package.name());
        match self {
            Self::Memory => {
                unit.key() == &SourceUnitKey::MemoryDocument
                    && sources.source_name(unit.source_id()).ok() == Some(case.path)
                    && package.is_some_and(|name| name.segments().is_empty())
            }
            Self::Rooted => {
                let (path, package_name) = if provider {
                    ("p/provider.ko", "p")
                } else {
                    ("q/consumer.ko", "q")
                };
                unit.key()
                    .root()
                    .is_some_and(|root| root.as_str() == "root")
                    && unit
                        .key()
                        .logical_path()
                        .is_some_and(|value| value.as_str() == path)
                    && sources.source_name(unit.source_id()).ok() == Some(path)
                    && package.is_some_and(|name| name.segments() == [package_name])
            }
        }
    }
}
