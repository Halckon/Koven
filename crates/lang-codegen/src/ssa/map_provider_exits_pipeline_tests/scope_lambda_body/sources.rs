//! P15 keeps the original no-capture Unit callback and its internal Map local.
pub(crate) struct Case {
    pub label: &'static str,
    pub provider: bool,
    pub source: &'static str,
    pub golden: &'static str,
}
pub(crate) fn sources() -> [Case; 2] {
    [
        Case {
            label: "P15-for",
            provider: true,
            source: include_str!("../../../../tests/fixtures/map-provider-scope-0331/P15-for.ko"),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-scope-0331/P15-for.stdout"
            ),
        },
        Case {
            label: "P15-control",
            provider: false,
            source: include_str!(
                "../../../../tests/fixtures/map-provider-scope-0331/P15-control.ko"
            ),
            golden: include_str!(
                "../../../../tests/fixtures/map-provider-scope-0331/P15-control.stdout"
            ),
        },
    ]
}
