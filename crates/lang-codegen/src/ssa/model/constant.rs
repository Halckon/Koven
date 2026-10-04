//! Scalar constants; synthetic zero exists solely in internal test SSA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScalarConstant {
    #[cfg(test)]
    SyntheticZero,
    Unit,
    Boolean(bool),
    /// Verifier rejects surrogate and out-of-range codepoints.
    Char(u32),
    Integer(i128),
}
