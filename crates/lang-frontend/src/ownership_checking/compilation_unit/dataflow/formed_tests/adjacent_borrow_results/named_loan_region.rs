//! Ordinary source rejection after a real Named-argument Complete prefix.
use super::*;

#[test]
fn loan_named_complete_prefix_is_revoked_from_raw_and_public_products() {
    assert_rejections_clear(
        "fun inspect(value: String): Int = 1\nfun run(own source: String): Unit { inspect(source) }",
        false,
        |accepted, raw| {
            assert_eq!(raw.direct_call_loan_ends.len(), 1);
            assert_eq!(
                raw.direct_call_loan_ends,
                accepted.shared().direct_call_loan_ends()
            );
            assert!(
                raw.loan_action_observations[0]
                    .regions()
                    .iter()
                    .any(|region| region.protocol_version() == 3
                        && region.coverage()
                            == crate::ownership_checking::UnitLoanActionCoverage::Complete)
            );
        },
    );
}
