/// The status of the corresponding request.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ApsdeSapConfirmStatus {
    /// indicating that the request to transmit was successful
    #[default]
    Success,
    /// No corresponding 16-bit NKW address found
    NoShortAddress,
    /// No binding table entries found with the respectively SrcEndpoint and
    /// ClusterId parameter
    NoBoundDevice,
    /// the security processing failed
    SecurityFail,
    /// one or more APS acknowledgements were not correctly received
    NoAck,
    /// ASDU to be transmitted is larger than will fit in a single frame and
    /// fragmentation is not possible
    AsduTooLong,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum SecurityStatus {
    #[default]
    Unsecured,
    SecuredNwkKey,
    SecuredLinkKey,
}

