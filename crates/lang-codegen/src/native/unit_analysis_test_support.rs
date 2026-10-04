//! Same analyzed fixture owner shared by public unit native contracts.
use super::*;
pub(super) struct UnitAnalysis {
    pub(super) sources: SourceMap,
    pub(super) provider_source: SourceId,
    pub(super) provider: ParsedFile,
    pub(super) consumer_source: SourceId,
    pub(super) consumer: ParsedFile,
    pub(super) names: ValidatedCompilationUnitNames,
    pub(super) environment: TypeEnvironment,
    pub(super) typed: ValidatedCompilationUnitTypes,
    pub(super) owned: ValidatedCompilationUnitOwnership,
}

impl UnitAnalysis {
    pub(super) fn inputs(&self) -> [SourceUnitInput<'_>; 2] {
        [
            SourceUnitInput::new(
                "root",
                "p/provider.ko",
                self.provider_source,
                &self.provider,
            ),
            SourceUnitInput::new(
                "root",
                "q/consumer.ko",
                self.consumer_source,
                &self.consumer,
            ),
        ]
    }

    pub(super) fn declaration(&self, package: &str, name: &str) -> DeclarationId {
        self.names
            .names()
            .index()
            .declarations()
            .iter()
            .find(|declaration| {
                declaration.name() == name
                    && self.names.names().index().packages()[declaration.package().index()]
                        .name()
                        .segments()
                        .iter()
                        .map(String::as_str)
                        .eq(package.split('.'))
            })
            .expect("fixture declaration exists")
            .id()
    }
}
