pub(crate) struct Case {
    pub label: &'static str,
    pub provider: bool,
    pub source: &'static str,
    pub golden: &'static str,
}
pub(crate) fn sources() -> [Case; 2] {
    [
        Case {
            label: "P06-for",
            provider: true,
            source: include_str!(
                "../../../../../tests/fixtures/map-provider-scope-0331/P06-for.ko"
            ),
            golden: include_str!(
                "../../../../../tests/fixtures/map-provider-scope-0331/P06-for.stdout"
            ),
        },
        Case {
            label: "P06-control",
            provider: false,
            source: include_str!(
                "../../../../../tests/fixtures/map-provider-scope-0331/P06-control.ko"
            ),
            golden: include_str!(
                "../../../../../tests/fixtures/map-provider-scope-0331/P06-control.stdout"
            ),
        },
    ]
}
