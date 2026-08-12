use lang_frontend::diagnostic::DiagnosticCode;

use super::support::{catalog, code};

const PHASE0_FIXTURE_CODES: &[&str] = &["L9000"];
const PHASE0_FIXTURE_WIRING_CODE: &str = "L9000";

pub(crate) fn phase0_fixture_code() -> DiagnosticCode {
    let catalog = catalog(PHASE0_FIXTURE_CODES);
    code(&catalog, PHASE0_FIXTURE_WIRING_CODE)
        .expect("the Phase 0 fixture code must be registered in test support")
}
