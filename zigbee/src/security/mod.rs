//! Security Service
//!
//! Security services provided for ZigBee include methods for key establishment,
//! key transport, frame protection, and device management.
use core::convert::TryInto;
use core::slice;

use aead::generic_array::GenericArray;
use aead::AeadMutInPlace;
use aes::cipher::generic_array::GenericArray as AesGenericArray;
use aes::cipher::BlockEncrypt;
use aes::cipher::KeyInit as AesKeyInit;
use aes::Aes128;
use byte::BytesExt;
use byte::TryRead;
use byte::TryWrite;
use ccm::consts::U13;
use ccm::consts::U4;
use ccm::Ccm;
use ccm::KeyInit;
use frame::AuxFrameHeader;
use frame::SecurityControl;
use frame::SecurityLevel;
use thiserror::Error;

use crate::aps::aib;
use crate::aps::aib::Aib;
use crate::aps::aib::AibStorage;
use crate::aps::aib::DeviceKeyPairDescriptor;
use crate::aps::aib::KeyAttribute;
use crate::aps::aib::LinkKeyType;
use crate::aps::apdu::frame::command::Command as ApsCommand;
use crate::aps::apdu::frame::command::TransportKey;
use crate::aps::apdu::frame::frame_control::FrameType as ApsFrameType;
use crate::aps::apdu::frame::header::Header as ApsHeader;
use crate::aps::apdu::frame::CommandFrame as ApsCommandFrame;
use crate::aps::apdu::frame::Frame as ApsFrame;
use crate::aps::types::TxOptions;
use crate::internal::types::ByteArray;
use crate::internal::types::ByteArrayRef;
use crate::internal::types::IeeeAddress;
use crate::nwk::frame::header::Header as NwkHeader;
use crate::nwk::frame::Frame as NwkFrame;
use crate::nwk::nib;
use crate::nwk::nib::Nib;
use crate::nwk::nib::NibStorage;
use crate::security::frame::KeyIdentifier;
use crate::security::primitives::Aes128Mmo;
use crate::security::primitives::HmacAes128Mmo;

pub mod frame;
pub mod primitives;

/// Default ZigbeeAlliance09 centralized security global trust center link key
const TRUST_CENTER_LINK_KEY: [u8; 16] = [
    0x5a, 0x69, 0x67, 0x42, 0x65, 0x65, 0x41, 0x6c, 0x6c, 0x69, 0x61, 0x6e, 0x63, 0x65, 0x30, 0x39,
];

// AES-128 CCM with MIC32
pub type Aes128Ccm = Ccm<Aes128, U4, U13>;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("invalid key")]
    InvalidKey,
    #[error("invalid data")]
    InvalidData,
    #[error("parse error")]
    ParseError(byte::Error),
    #[error("ccm error")]
    CcmError(ccm::Error),
    #[error("frame security failed")]
    Unspecified,
}

impl From<byte::Error> for SecurityError {
    fn from(value: byte::Error) -> Self {
        Self::ParseError(value)
    }
}

impl From<SecurityError> for byte::Error {
    fn from(value: SecurityError) -> Self {
        match value {
            SecurityError::InvalidKey => Self::BadInput {
                err: "security: invalid key",
            },
            SecurityError::InvalidData => Self::BadInput {
                err: "security: invalid data",
            },
            SecurityError::ParseError(e) => e,
            SecurityError::CcmError(_) => Self::BadInput {
                err: "security: ccm error",
            },
            SecurityError::Unspecified => Self::BadInput {
                err: "frame security failed",
            },
        }
    }
}

pub struct SecurityContext<'a> {
    nib: &'a Nib<NibStorage>,
    aib: &'a Aib<AibStorage>,
}

impl<'a> SecurityContext<'a> {
    pub fn new(nib: &'a Nib<NibStorage>, aib: &'a Aib<AibStorage>) -> Self {
        Self { nib, aib }
    }

    pub fn no_security() -> Self {
        Self {
            nib: nib::get_ref(),
            aib: aib::get_ref(),
        }
    }

    // section 4.3.1.1
    pub fn encrypt_nwk_frame_in_place(
        &self,
        nwk_frame: NwkFrame<'_>,
        frame_buffer: &mut [u8],
    ) -> Result<usize, SecurityError> {
        let sec_level = self.nib.security_level();
        let _mic_len = sec_level.mic_length();

        let key_sequence_number = self.nib.active_key_seq_number();
        let sec_material = self.nib.security_material_set();
        let sec_material = sec_material
            .iter()
            .find(|k| k.key_seq_number == key_sequence_number)
            .ok_or(SecurityError::Unspecified)?;
        let frame_counter = sec_material.outgoing_frame_counter;
        let local_addr = self.nib.ieee_address();
        let key = sec_material.key;

        let mut security_control = SecurityControl::default();
        security_control.set_security_level(sec_level);
        security_control.set_key_identifier(KeyIdentifier::Network);
        security_control.set_extended_nonce(true);

        let aux_hdr = AuxFrameHeader {
            security_control,
            frame_counter,
            key_sequence_number: Some(key_sequence_number),
            source_address: Some(local_addr),
        };

        match nwk_frame {
            NwkFrame::Data(data_frame) => Self::write_and_encrypt_in_place(
                frame_buffer,
                aux_hdr,
                key.as_slice(),
                data_frame.header,
                ByteArrayRef(data_frame.payload),
            ),
            NwkFrame::NwkCommand(command_frame) => Self::write_and_encrypt_in_place(
                frame_buffer,
                aux_hdr,
                key.as_slice(),
                command_frame.header,
                command_frame.command,
            ),
            NwkFrame::Reserved(header) | NwkFrame::InterPan(header) => {
                // no security required
                let offset = &mut 0;
                frame_buffer.write_with(offset, header, ())?;
                Ok(*offset)
            }
        }
    }

    // section 4.3.1.2
    pub fn decrypt_nwk_frame_in_place<'b>(
        &self,
        frame_buffer: &'b mut [u8],
    ) -> Result<NwkFrame<'b>, SecurityError> {
        // Sec 4.3.1.2: overwrite the security level with the value from the NIB
        // (default 0x05)
        let sec_level = self.nib.security_level();

        let mic_length = sec_level.mic_length();
        byte::check_len(frame_buffer, mic_length)?;

        let (_, nwk_hdr_len) = NwkHeader::try_read(frame_buffer, ())?;
        // SAFETY: the buffer for the header is not mutated
        // we can safely remove the &mut to satisfy the
        // borrow checker when returning NwkFrame<'_>
        let hdr_buf = unsafe { slice::from_raw_parts(frame_buffer.as_ptr(), nwk_hdr_len) };
        let (nwk_hdr, _) = NwkHeader::try_read(hdr_buf, ())?;
        if !nwk_hdr.frame_control.security_flag() {
            // no security enabled for frame, exit with payload
            return Ok(NwkFrame::from_payload(nwk_hdr, frame_buffer)?);
        }

        let (mut aux_hdr, aux_hdr_len) =
            AuxFrameHeader::try_read(&frame_buffer[nwk_hdr_len..], ())?;

        if aux_hdr.frame_counter == u32::MAX {
            return Err(SecurityError::InvalidData);
        }

        // 2) select the key from NIB
        let sec_material = self.nib.security_material_set();
        let sec_material = sec_material
            .iter()
            .find(|k| {
                aux_hdr
                    .key_sequence_number
                    .is_some_and(|ksn| ksn == k.key_seq_number)
            })
            .ok_or(SecurityError::Unspecified)?;
        let key = sec_material.key.as_slice();

        // 3) check if frame_counter is equal or greater of the NIB
        let Some(source_address) = aux_hdr.source_address else {
            return Err(SecurityError::InvalidData);
        };
        if let Some(inc_frame_counter) = sec_material
            .incoming_frame_counter_set
            .iter()
            .find(|i| source_address == i.sender_address)
        {
            if aux_hdr.frame_counter < inc_frame_counter.incoming_frame_counter {
                return Err(SecurityError::InvalidData);
            }
        }

        // write back the security level from NIB to aux header
        // the updated values is required as input to ccm
        aux_hdr.security_control.set_security_level(sec_level);
        let mut offset = nwk_hdr_len;
        frame_buffer.write_with(&mut offset, aux_hdr, ())?;

        let (aad, frame) = frame_buffer.split_at_mut(nwk_hdr_len + aux_hdr_len);
        let (enc_data, tag) = frame.split_at_mut(frame.len() - mic_length);
        let tag = GenericArray::from_slice(tag);

        let nonce: GenericArray<u8, _> = create_nonce(&aux_hdr)?.into();
        let mut cipher = Aes128Ccm::new(key.into());

        cipher
            .decrypt_in_place_detached(&nonce, aad, enc_data, tag)
            .map_err(SecurityError::CcmError)?;

        Ok(NwkFrame::from_payload(nwk_hdr, enc_data)?)
    }

    pub fn encrypt_aps_frame_in_place(
        &self,
        aps_frame: ApsFrame<'_>,
        frame_buffer: &mut [u8],
        dest: IeeeAddress,
        tx_options: TxOptions,
    ) -> Result<usize, SecurityError> {
        if let ApsFrame::Acknowledgement(header) = aps_frame {
            // no encryption for acknowledgements
            let offset = &mut 0;
            frame_buffer.write_with(&mut 0, header, ())?;
            return Ok(*offset);
        }

        // get link key associated with destination from AIB
        let mut key_set = self.aib.device_key_pair_set();
        let key_config = key_set
            .iter_mut()
            .find(|k| {
                k.device_address == dest
                    && matches!(
                        k.key_attributes,
                        KeyAttribute::ProvisionalKey | KeyAttribute::VerifiedKey
                    )
            })
            .ok_or(SecurityError::Unspecified)?;

        // Step 1: Obtain security material and key identifier
        let (key, key_id) = match &aps_frame {
            ApsFrame::ApsCommand(ApsCommandFrame {
                command: ApsCommand::TransportKey(TransportKey::StandardNetworkKey(_)),
                ..
            }) => (
                // Section 4.5.3: key-transport key uses 1-octet string '0x00'
                HmacAes128Mmo::hmac(key_config.link_key.as_slice(), &[0x00])?,
                KeyIdentifier::KeyTransport,
            ),
            ApsFrame::ApsCommand(ApsCommandFrame {
                command:
                    ApsCommand::TransportKey(
                        TransportKey::ApplicationLinkKey(_) | TransportKey::TrustCenterLinkKey(_),
                    ),
                ..
            }) => (
                // Section 4.5.3: key-load key uses 1-octet string '0x02'
                HmacAes128Mmo::hmac(key_config.link_key.as_slice(), &[0x02])?,
                KeyIdentifier::KeyLoad,
            ),
            _ => (key_config.link_key.0, KeyIdentifier::Data),
        };

        // Step 2: Extract frame counter (and key sequence number if needed)
        let frame_counter = key_config.outgoing_frame_counter;
        if frame_counter == u32::MAX {
            return Err(SecurityError::InvalidData);
        }

        // Step 3: Obtain security level from NIB
        let sec_level = self.nib.security_level();
        //let mic_length = sec_level.mic_length();

        // Set key identifier
        let mut security_control = SecurityControl::default();
        security_control.set_security_level(sec_level);
        security_control.set_key_identifier(key_id);

        if matches!(aps_frame, ApsFrame::ApsCommand(_))
            || matches!(tx_options, TxOptions::IncludeExtendedNonce)
        {
            security_control.set_extended_nonce(true);
        }

        let source_address = if security_control.extended_nonce() {
            Some(self.nib.ieee_address())
        } else {
            None
        };

        // Step 4: Construct auxiliary header
        let aux_hdr = AuxFrameHeader {
            security_control,
            frame_counter,
            source_address,
            key_sequence_number: None, /* this is should be never set because key_id = 0x01
                                        * (NetworkKey) is invalid */
        };

        // Write APS header
        let offset = match aps_frame {
            ApsFrame::Data(data_frame) => Self::write_and_encrypt_in_place(
                frame_buffer,
                aux_hdr,
                key.as_slice(),
                data_frame.header,
                data_frame.payload,
            ),
            ApsFrame::ApsCommand(command_frame) => Self::write_and_encrypt_in_place(
                frame_buffer,
                aux_hdr,
                key.as_slice(),
                command_frame.header,
                command_frame.command,
            ),
            // already covered
            ApsFrame::Acknowledgement(_) => unreachable!(),
        }?;

        // step 9:
        // increment and write back frame counter
        key_config.outgoing_frame_counter += 1;
        self.aib.set_device_key_pair_set(key_set);

        Ok(offset)
    }

    fn write_and_encrypt_in_place(
        frame_buffer: &mut [u8],
        aux_hdr: AuxFrameHeader,
        key: &[u8],
        hdr: impl TryWrite,
        payload: impl TryWrite,
    ) -> Result<usize, SecurityError> {
        let mic_len = aux_hdr.security_control.security_level().mic_length();
        let nonce = create_nonce(&aux_hdr)?;

        let offset = &mut 0;
        frame_buffer.write_with(offset, hdr, ())?;

        let aux_hdr_offset = *offset;
        frame_buffer.write_with(offset, aux_hdr, ())?;

        let payload_offset = *offset;
        frame_buffer.write_with(offset, payload, ())?;

        let (aad, payload) = frame_buffer.split_at_mut(payload_offset);
        let (payload, tag) = payload.split_at_mut(payload.len() - mic_len);
        let len = payload_offset + payload.len() + mic_len;

        let nonce = GenericArray::from(nonce);

        let mut cipher = Aes128Ccm::new(GenericArray::from_slice(key));
        let t = cipher
            .encrypt_in_place_detached(&nonce, aad, payload)
            .map_err(SecurityError::CcmError)?;
        tag.copy_from_slice(t.as_slice());

        // overwrite sec level in aux header with 000
        let mut sec_ctl = SecurityControl(frame_buffer[aux_hdr_offset]);
        sec_ctl.set_security_level(SecurityLevel::None);
        frame_buffer[aux_hdr_offset] = sec_ctl.0;

        Ok(len)
    }

    // section 4.4.1.2
    pub fn decrypt_aps_frame_in_place<'b>(
        &self,
        frame_buffer: &'b mut [u8],
    ) -> Result<ApsFrame<'b>, SecurityError> {
        // 5) overwrite the security level with the value from the NIB
        // (default 0x05)
        let sec_level = self.nib.security_level();
        let mic_length = sec_level.mic_length();
        byte::check_len(frame_buffer, mic_length)?;

        let (_, aps_hdr_len) = ApsHeader::try_read(frame_buffer, ())?;
        // SAFETY: the buffer for the header is not mutated
        // we can safely remove the &mut to satisfy the
        // borrow checker when returning NwkFrame<'_>
        let hdr_buf = unsafe { slice::from_raw_parts(frame_buffer.as_ptr(), aps_hdr_len) };
        let (aps_hdr, _) = ApsHeader::try_read(hdr_buf, ())?;

        let (mut aux_hdr, aux_hdr_len) =
            AuxFrameHeader::try_read(&frame_buffer[aps_hdr_len..], ())?;

        if aux_hdr.frame_counter == u32::MAX {
            return Err(SecurityError::InvalidData);
        }

        let Some(source_address) = aux_hdr.source_address else {
            return Err(SecurityError::Unspecified);
        };

        // step 2: select the security material matching the source address
        // TODO: the spec says "using the source address in the APS frame as the index"
        // but the APS frame does not have a source field, only the security header
        let mut key_set = self.aib.device_key_pair_set();
        let key_config = key_set.find_or_insert_with_mut(
            |k| k.device_address == source_address,
            // TODO: what do we set here if the source device is new and unknown?
            || DeviceKeyPairDescriptor {
                device_address: source_address,
                key_attributes: KeyAttribute::VerifiedKey,
                link_key: ByteArray(TRUST_CENTER_LINK_KEY),
                outgoing_frame_counter: 0,
                incoming_frame_counter: 0,
                link_key_type: LinkKeyType::GlobalLinkKey,
            },
        );

        // step 3: obtain the key
        let key = match aux_hdr.security_control.key_identifier() {
            KeyIdentifier::Data => key_config.link_key.0,
            KeyIdentifier::KeyTransport => {
                // Section 4.5.3: key-transport key uses 1-octet string '0x00'
                HmacAes128Mmo::hmac(key_config.link_key.as_slice(), &[0x00])?
            }
            KeyIdentifier::KeyLoad => {
                // Section 4.5.3: key-load key uses 1-octet string '0x02'
                HmacAes128Mmo::hmac(key_config.link_key.as_slice(), &[0x02])?
            }
            KeyIdentifier::Network => return Err(SecurityError::InvalidData),
        };

        // step 4
        if matches!(key_config.link_key_type, LinkKeyType::UniqueLinkKey)
            && aux_hdr.frame_counter < key_config.incoming_frame_counter
        {
            return Err(SecurityError::Unspecified);
        }

        // write back the security level from NIB to aux header
        // the updated values is required as input to ccm
        aux_hdr.security_control.set_security_level(sec_level);
        let mut offset = aps_hdr_len;
        frame_buffer.write_with(&mut offset, aux_hdr, ())?;

        let (aad, frame) = frame_buffer.split_at_mut(aps_hdr_len + aux_hdr_len);
        let (enc_data, tag) = frame.split_at_mut(frame.len() - mic_length);
        let tag = GenericArray::from_slice(tag);

        // TODO:verify the source address
        let Some(_source_address) = aux_hdr.source_address else {
            return Err(SecurityError::InvalidData);
        };

        let nonce: GenericArray<u8, _> = create_nonce(&aux_hdr)?.into();
        let mut cipher = Aes128Ccm::new(&key.into());

        cipher
            .decrypt_in_place_detached(&nonce, aad, enc_data, tag)
            .map_err(SecurityError::CcmError)?;

        Ok(ApsFrame::from_payload(aps_hdr, enc_data)?)
    }
}

// Figure 4-20
#[allow(clippy::needless_range_loop)]
fn create_nonce(aux_header: &AuxFrameHeader) -> Result<[u8; 13], SecurityError> {
    let AuxFrameHeader {
        security_control,
        frame_counter,
        source_address: Some(IeeeAddress(source_address)),
        ..
    } = aux_header
    else {
        return Err(SecurityError::InvalidData);
    };

    let mut nonce = [0u8; 13];
    for i in 0..8 {
        nonce[i] = (source_address >> (8 * i) & 0xff) as u8;
    }

    for i in 0..4 {
        nonce[i + 8] = (frame_counter >> (8 * i) & 0xff) as u8;
    }
    nonce[12] = security_control.0;
    Ok(nonce)
}

#[cfg(test)]
mod tests {

    use heapless::Vec;

    use super::*;
    use crate::internal::types::ByteArray;
    use crate::internal::types::StorageVec;
    use crate::nwk::nib::NetworkSecurityMaterialDescriptor;

    const NETWORK_KEY: [u8; 16] = [
        0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];

    fn setup_nib() -> Nib<NibStorage> {
        let nib = Nib::new(NibStorage::default());
        nib.init();

        let mut set = Vec::new();
        set.push(NetworkSecurityMaterialDescriptor {
            key_seq_number: 0,
            outgoing_frame_counter: 1,
            incoming_frame_counter_set: StorageVec(Vec::new()),
            key: ByteArray(NETWORK_KEY),
            network_key_type: 0,
        })
        .unwrap();
        nib.set_security_material_set(StorageVec(set));
        nib.set_ieee_address(IeeeAddress(0x1234_5678_90ab_cdef));
        nib.set_security_level(SecurityLevel::EncMic32);

        assert_eq!(nib.security_material_set()[0].key, ByteArray(NETWORK_KEY));

        nib
    }

    fn setup_aib() -> Aib<AibStorage> {
        let aib = Aib::new(AibStorage::default());
        aib.init();
        aib
    }

    fn append_aib_device_key_pair_set(aib: &Aib<AibStorage>, k: DeviceKeyPairDescriptor) {
        let mut set = aib.device_key_pair_set();
        set.push(k).unwrap();
        aib.set_device_key_pair_set(set);
    }

    #[test]
    fn test_create_nonce() {
        let source_address = Some(IeeeAddress(0xaaaa_bbbb_cccc_dddd));
        let frame_counter = 0x0;
        let security_control = SecurityControl(0x40);
        let aux_hdr = AuxFrameHeader {
            security_control,
            frame_counter,
            source_address,
            key_sequence_number: None,
        };
        let nonce = create_nonce(&aux_hdr).unwrap();
        assert_eq!(
            nonce,
            [0xdd, 0xdd, 0xcc, 0xcc, 0xbb, 0xbb, 0xaa, 0xaa, 0x00, 0x00, 0x00, 0x00, 0x40]
        );
    }

    #[test]
    fn decrypt_aps_frame_data() {
        let nib = setup_nib();
        let aib = setup_aib();

        // aps cmd request key
        let frame_buffer = [
            0x21, 0x66, // aps header
            0x20, 0x4, 0x0, 0x0, 0x0, 0xe5, 0x1, 0x30, 0x38, 0x9c, 0x38, 0xc1,
            0xa4, // aux header
            0x1a, 0x31, // enc data
            0xa4, 0xd7, 0xf4, 0xd7, //mic
        ];

        let security_context = SecurityContext::new(&nib, &aib);

        let mut buf = frame_buffer;
        let frame = security_context
            .decrypt_aps_frame_in_place(&mut buf)
            .unwrap();

        let dest = IeeeAddress(0x1234_5678_90ab_cdef);
        append_aib_device_key_pair_set(
            &aib,
            DeviceKeyPairDescriptor {
                device_address: dest,
                key_attributes: KeyAttribute::VerifiedKey,
                link_key: ByteArray(TRUST_CENTER_LINK_KEY),
                outgoing_frame_counter: 4,
                incoming_frame_counter: 0,
                link_key_type: LinkKeyType::GlobalLinkKey,
            },
        );
        nib.set_ieee_address(IeeeAddress(0xa4c1_389c_3830_01e5));
        let mut got_buffer = [0u8; 21];

        let offset = security_context
            .encrypt_aps_frame_in_place(frame, &mut got_buffer, dest, TxOptions::SecurityEnabled)
            .unwrap();

        assert_eq!(offset, frame_buffer.len());
        assert_eq!(frame_buffer, got_buffer);
    }

    #[test]
    fn decrypt_aps_frame_key_transport() {
        let nib = setup_nib();
        let aib = setup_aib();

        let frame_buffer = [
            0x21, 0x95, // aps hdr
            0x30, 0x0, 0x0, 0x0, 0x0, 0xe1, 0x52, 0x38, 0x7d, 0xc1, 0x36, 0xce, // aux hdr
            0xf4, 0xcc, 0x56, 0x50, 0x5e, 0x7, 0x2d, 0xc5, 0xc1, 0xe8, 0x40, 0xf2, 0xd5, 0xce, 0xc,
            0xa9, 0x2d, 0x64, 0x23, 0xcc, 0xc, 0x56, 0xcc, 0xc4, 0xcc, 0xf, 0x18, 0xa2, 0xe4, 0x82,
            0x88, 0x58, 0x4a, 0x90, 0x3e, 0x0, // enc data
            0x47, 0x60, 0xf2, 0x5d, // mic
        ];

        let security_context = SecurityContext::new(&nib, &aib);

        let mut buf = frame_buffer;
        let frame = security_context
            .decrypt_aps_frame_in_place(&mut buf)
            .unwrap();

        let dest = IeeeAddress(0xa4c1_389c_3830_01e5);
        append_aib_device_key_pair_set(
            &aib,
            DeviceKeyPairDescriptor {
                device_address: dest,
                key_attributes: KeyAttribute::VerifiedKey,
                link_key: ByteArray(TRUST_CENTER_LINK_KEY),
                outgoing_frame_counter: 0,
                incoming_frame_counter: 0,
                link_key_type: LinkKeyType::GlobalLinkKey,
            },
        );
        nib.set_ieee_address(IeeeAddress(0xf4ce_36c1_7d38_52e1));

        let mut got_buffer = [0u8; 54];
        let offset = security_context
            .encrypt_aps_frame_in_place(frame, &mut got_buffer, dest, TxOptions::SecurityEnabled)
            .unwrap();

        assert_eq!(offset, frame_buffer.len());
        assert_eq!(frame_buffer, got_buffer);
    }

    #[test]
    fn decrypt_aps_frame_key_load() {
        let nib = setup_nib();
        let aib = setup_aib();

        let frame_buffer = [
            0x21, 0x97, // aps hdr
            0x38, 0x1, 0x0, 0x0, 0x0, 0xe1, 0x52, 0x38, 0x7d, 0xc1, 0x36, 0xce, // aux hdr
            0xf4, 0xe0, 0x4b, 0x37, 0xdb, 0x35, 0xc7, 0x13, 0x41, 0x71, 0xf0, 0xdf, 0xdb, 0x22,
            0xa5, 0xa1, 0x65, 0xbf, 0xfe, 0x41, 0x5a, 0xb2, 0x5f, 0xd9, 0x85, 0x79, 0x92, 0x5a,
            0xd4, 0xe6, 0x48, 0xfa, 0x6, 0xfb, 0x11, // enc data
            0xb7, 0xc9, 0x4, 0x3e, // mic
        ];

        let security_context = SecurityContext::new(&nib, &aib);

        let mut buf = frame_buffer;
        let frame = security_context
            .decrypt_aps_frame_in_place(&mut buf)
            .unwrap();

        let dest = IeeeAddress(0xa4c1_389c_3830_01e5);
        append_aib_device_key_pair_set(
            &aib,
            DeviceKeyPairDescriptor {
                device_address: dest,
                key_attributes: KeyAttribute::VerifiedKey,
                link_key: ByteArray(TRUST_CENTER_LINK_KEY),
                outgoing_frame_counter: 1,
                incoming_frame_counter: 0,
                link_key_type: LinkKeyType::GlobalLinkKey,
            },
        );
        nib.set_ieee_address(IeeeAddress(0xf4ce_36c1_7d38_52e1));

        let mut got_buffer = [0u8; 53];
        let offset = security_context
            .encrypt_aps_frame_in_place(frame, &mut got_buffer, dest, TxOptions::SecurityEnabled)
            .unwrap();

        assert_eq!(offset, frame_buffer.len());
        assert_eq!(frame_buffer, got_buffer);
    }

    // encrypted NWK EndDeviceTimeoutRequest
    const NWK_FRAME_CMD_BUFFER: [u8; 45] = [
        0x9, 0x1a, // frame control
        0x0, 0x0, 0xe1, 0xcd, 0x1, 0x93, 0xe1, 0x52, 0x38, 0x7d, 0xc1, 0x36, 0xce, 0xf4, 0xe5, 0x1,
        0x30, 0x38, 0x9c, 0x38, 0xc1, 0xa4, // nwk header
        0x28, // security control
        0x1, 0x0, 0x0, 0x0, 0xe5, 0x1, 0x30, 0x38, 0x9c, 0x38, 0xc1, 0xa4, 0x0, //aad
        0xa6, 0xac, 0x13, // enc_data
        0xf8, 0x5, 0x7f, 0x53, // mic
    ];

    #[test]
    fn decrypt_nwk_frame() {
        let nib = setup_nib();
        let aib = setup_aib();

        let security_context = SecurityContext::new(&nib, &aib);
        let mut frame_buffer = NWK_FRAME_CMD_BUFFER;

        let frame = security_context
            .decrypt_nwk_frame_in_place(&mut frame_buffer)
            .unwrap();

        assert!(matches!(frame, NwkFrame::NwkCommand(_)));
    }

    #[test]
    fn encrypt_nwk_frame() {
        let nib = setup_nib();
        let aib = setup_aib();
        let security_context = SecurityContext::new(&nib, &aib);
        let mut frame_buffer = NWK_FRAME_CMD_BUFFER;
        let frame = security_context
            .decrypt_nwk_frame_in_place(&mut frame_buffer)
            .unwrap();

        let mut frame_buffer = [0u8; 45];
        nib.set_ieee_address(IeeeAddress(0xa4c1_389c_3830_01e5));

        let offset = security_context
            .encrypt_nwk_frame_in_place(frame, &mut frame_buffer)
            .unwrap();

        assert_eq!(offset, NWK_FRAME_CMD_BUFFER.len());
        assert_eq!(frame_buffer[..38], NWK_FRAME_CMD_BUFFER[..38]);
        assert_eq!(frame_buffer[38..45], NWK_FRAME_CMD_BUFFER[38..45]);
    }
}
