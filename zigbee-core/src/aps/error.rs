#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ApsError {
    /// value is not within the valid range
    InvalidValue,
}
