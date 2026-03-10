//! Network Management Entity
//!
//! The NLME shall provide a management service to allow an application to
//! interact with the stack.
//!
//! it provides:
//! * configuring a new device
//! * starting a network
//! * joining, rejoining and leaving a network
//! * addressing
//! * neighbor discovery
//! * route discovery
//! * reception control
//! * routing
#![allow(dead_code)]

use embedded_storage::Storage;
use management::NlmeEdScanConfirm;
use management::NlmeEdScanRequest;
use management::NlmeJoinConfirm;
use management::NlmeJoinRequest;
use management::NlmeJoinStatus;
use management::NlmeNetworkDiscoveryConfirm;
use management::NlmeNetworkFormationConfirm;
use management::NlmeNetworkFormationRequest;
use management::NlmePermitJoiningConfirm;
use management::NlmePermitJoiningRequest;
use management::NlmeStartRouterConfirm;
use management::NlmeStartRouterRequest;
#[cfg(feature = "mock")]
use mockall::automock;
#[cfg(feature = "mock")]
use mockall::mock;
use thiserror::Error;
use zigbee_mac::Address;
use zigbee_mac::MacShortAddress;
use zigbee_mac::PanId;
use zigbee_mac::mlme::MacError;
use zigbee_mac::mlme::Mlme;
use zigbee_mac::mlme::ScanType;
use zigbee_types::IeeeAddress;
use zigbee_types::ShortAddress;
use zigbee_types::StorageVec;

use crate::nwk::nib::CapabilityInformation;
use crate::nwk::nib::DeviceType;
use crate::nwk::nib::MAX_PARENT_LINK_COST;
use crate::nwk::nib::NWK_COORDINATOR_ADDRESS;
use crate::nwk::nib::Nib;
use crate::nwk::nib::NwkNeighbor;
use crate::nwk::nib::link_cost_from_lqi;

/// Network management entity
pub mod management;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("mac error")]
    MacError(#[from] MacError),
}

/// Network management service - service access point
///
/// 3.2.2
///
/// allows the transport of management commands between the next higher layer
/// and the NLME.
#[cfg_attr(feature = "mock", automock)]
pub trait NlmeSap {
    /// 3.2.2.3
    async fn network_discovery<C: Iterator<Item = u8> + 'static>(
        &mut self,
        channels: C,
        duration: u8,
    ) -> Result<NlmeNetworkDiscoveryConfirm, NetworkError>;
    /// 3.2.2.5
    async fn network_formation(
        &self,
        request: NlmeNetworkFormationRequest,
    ) -> NlmeNetworkFormationConfirm;
    /// 3.2.2.7
    async fn permit_joining(&self, request: NlmePermitJoiningRequest) -> NlmePermitJoiningConfirm;
    /// 3.2.2.9
    async fn start_router(&self, request: NlmeStartRouterRequest) -> NlmeStartRouterConfirm;
    /// 3.2.2.11
    async fn ed_scan(&self, request: NlmeEdScanRequest) -> NlmeEdScanConfirm;
    // 3.2.2.13
    async fn join(&mut self, request: NlmeJoinRequest) -> NlmeJoinConfirm;

    async fn rejoin(&mut self) -> NlmeJoinConfirm;
}

pub struct Nlme<S, M> {
    pub nib: Nib<S>,
    mac: M,
}

impl<S, M> Nlme<S, M>
where
    S: Storage,
    M: Mlme,
{
    pub fn new(storage: S, mac: M) -> Self {
        let nib = Nib::new(storage);
        Self { nib, mac }
    }

    /// Select candidate parents from the neighbor table populated
    /// during network discovery (§3.6.1.4.1.1).
    ///
    /// A suitable parent device is one for which **all** of the following
    /// conditions are true:
    ///
    /// 1. It belongs to the network identified by `extended_pan_id`.
    /// 2. It is open to join requests and advertises capacity for the correct
    ///    device type (`join_as_router`).
    /// 3. The link cost computed from LQI is at most 3 (§3.6.3.1).
    /// 4. The `potential_parent` field is `Some(1)`.
    /// 5. Its `update_id` is the most recent (accounting for wrap).
    ///
    /// When `nwkStackProfile == 1`, candidates at minimum depth are
    /// preferred.
    ///
    /// Returns an ordered list of candidate indices into the neighbor
    /// table, best candidate first.
    fn select_parent_candidates(
        &self,
        extended_pan_id: u64,
        join_as_router: bool,
    ) -> heapless::Vec<usize, 16> {
        let table = self.nib.neighbor_table();
        let stack_profile = self.nib.stack_profile();

        // Determine the most recent update_id among all matching neighbors.
        let max_update_id = table
            .iter()
            .filter(|n| {
                n.extended_pan_id == IeeeAddress(extended_pan_id)
                    && n.permit_joining
                    && n.potential_parent == 1
            })
            .map(|n| n.update_id)
            .max();

        let Some(best_update_id) = max_update_id else {
            return heapless::Vec::new();
        };

        // Collect indices of eligible parents.
        let mut candidates: heapless::Vec<usize, 16> = table
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                // (1) Correct network
                n.extended_pan_id == IeeeAddress(extended_pan_id)
                // (2) Accepting joins of correct type
                    && n.permit_joining
                    && if join_as_router {
                        n.router_capacity
                    } else {
                        n.end_device_capacity
                    }
                // (3) Link cost ≤ 3
                    && link_cost_from_lqi(n.lqi) <= MAX_PARENT_LINK_COST
                // (4) Potential parent
                    && n.potential_parent == 1
                // (5) Most recent update id
                    && n.update_id == best_update_id
            })
            .map(|(i, _)| i)
            .collect();

        // When nwkStackProfile == 1 prefer minimum depth (§3.6.1.4.1.1).
        if stack_profile == 1 {
            candidates.sort_unstable_by_key(|&i| table[i].depth);
        }

        candidates
    }

    /// Build the IEEE 802.15.4 MAC `CapabilityInformation` from the NWK
    /// layer `CapabilityInformation` bitmap (Table 3-62).
    fn build_mac_capabilities(cap: &CapabilityInformation) -> zigbee_mac::CapabilityInformation {
        zigbee_mac::CapabilityInformation {
            // Bit 1 — Device type: 1 if joining as router (FFD)
            full_function_device: cap.device_type(),
            // Bit 2 — Power source
            mains_power: cap.power_source(),
            // Bit 3 — Receiver on when idle
            idle_receive: cap.receiver_on_when_idle(),
            // Bit 6 — Security capability
            frame_protection: cap.security_capability(),
            // Bit 7 — Allocate address
            allocate_address: cap.allocate_address(),
        }
    }
}

impl<S, M> NlmeSap for Nlme<S, M>
where
    M: Mlme,
    S: Storage,
{
    async fn network_discovery<C: Iterator<Item = u8>>(
        &mut self,
        channels: C,
        duration: u8,
    ) -> Result<NlmeNetworkDiscoveryConfirm, NetworkError> {
        let scan_result = self
            .mac
            .scan_network(ScanType::Active, channels, duration)
            .await?;

        // Populate the neighbor table with mandatory fields (Table 3-63)
        // and optional discovery-time fields (Table 3-64).
        let neighbor_table = scan_result
            .pan_descriptor
            .iter()
            .filter_map(|pd| match pd.coord_address {
                Address::Short(_pan_id, short_address) => Some(NwkNeighbor {
                    network_address: ShortAddress(short_address.0),
                    device_type: if short_address.0 == NWK_COORDINATOR_ADDRESS {
                        DeviceType::Coordinator
                    } else {
                        DeviceType::Router
                    },
                    rx_on_when_idle: false,
                    end_device_configuration: 0,
                    relationship: 0x03, // none — not yet joined
                    transmit_failure: 0,
                    lqi: pd.link_quality,
                    outgoing_cost: 0,
                    age: 0,
                    keepalive_received: false,
                    // Table 3-64: optional discovery-time fields
                    extended_pan_id: pd.zigbee_beacon.extended_pan_id,
                    logical_channel: pd.channel,
                    depth: pd.zigbee_beacon.stack_profile.device_depth(),
                    permit_joining: pd.superframe_spec.association_permit,
                    potential_parent: 1,
                    router_capacity: pd.zigbee_beacon.stack_profile.router_capacity(),
                    end_device_capacity: pd.zigbee_beacon.stack_profile.end_device_capacity(),
                    update_id: pd.zigbee_beacon.update_id,
                    pan_id: pd.coord_pan_id.0,
                }),
                Address::Extended(_, _) => None,
            })
            .collect();

        self.nib.set_neighbor_table(StorageVec(neighbor_table));

        // Build network descriptors for the confirm primitive.
        let network_descriptors = scan_result
            .pan_descriptor
            .into_iter()
            .map(From::from)
            .collect();

        Ok(NlmeNetworkDiscoveryConfirm {
            network_descriptor: network_descriptors,
        })
    }

    async fn network_formation(
        &self,
        _request: NlmeNetworkFormationRequest,
    ) -> NlmeNetworkFormationConfirm {
        todo!()
    }

    // Permitting Devices to Join a Network
    // Figure 3-39
    async fn permit_joining(&self, _request: NlmePermitJoiningRequest) -> NlmePermitJoiningConfirm {
        NlmePermitJoiningConfirm {
            status: NlmeJoinStatus::InvalidRequest,
        }
    }

    async fn start_router(&self, _request: NlmeStartRouterRequest) -> NlmeStartRouterConfirm {
        todo!()
    }

    async fn ed_scan(&self, _request: NlmeEdScanRequest) -> NlmeEdScanConfirm {
        todo!()
    }

    async fn join(&mut self, request: NlmeJoinRequest) -> NlmeJoinConfirm {
        let fail = |status: NlmeJoinStatus| NlmeJoinConfirm {
            status,
            network_address: 0xffff,
            extended_pan_id: 0u64,
            channel: 0,
            enhanced_beacon_type: false,
            mac_interface_index: 0u8,
        };

        // --- Validate the request (§3.2.2.13.3) ---

        // Only RejoinNetwork == 0x00 (MAC association) is handled here.
        // 0x02 (NWK rejoin) is handled separately.
        if request.rejoin_network != 0x00 {
            // TODO: implement rejoin_network 0x01 (orphan) and 0x02 (NWK rejoin)
            return fail(NlmeJoinStatus::InvalidRequest);
        }

        // A device already joined must not re-associate (§3.6.1.4.1.1).
        if self.nib.network_address() != 0xffff {
            return fail(NlmeJoinStatus::InvalidRequest);
        }

        // --- Parent selection (§3.6.1.4.1.1) ---

        // Whether joining as router or end device, set nwkParentInformation
        // to 0 before searching (spec requirement).
        self.nib.set_parent_information(0);

        let join_as_router = request.capability_information.device_type();

        let candidates = self.select_parent_candidates(request.extended_pan_id, join_as_router);

        if candidates.is_empty() {
            return fail(NlmeJoinStatus::NotPermitted);
        }

        // Build MAC CapabilityInformation from NWK CapabilityInformation
        // bitmap (Table 3-62).
        let mac_caps = Self::build_mac_capabilities(&request.capability_information);

        // Store in NIB (§3.6.1.4.1.1: "the capability information shall be
        // stored as the value of the nwkCapabilityInformation NIB attribute").
        self.nib
            .set_capability_information(request.capability_information);

        // --- Try each candidate in order (§3.6.1.4.1.1) ---

        let mut last_status = NlmeJoinStatus::NotPermitted;

        for &candidate_idx in &candidates {
            // Read the neighbor info we need before the async call.
            let table = self.nib.neighbor_table();
            let neighbor = &table[candidate_idx];
            let channel = neighbor.logical_channel;
            let pan_id = PanId(neighbor.pan_id);
            let dest = Address::Short(pan_id, MacShortAddress(neighbor.network_address.0));
            drop(table);

            // Issue MLME-ASSOCIATE.request to MAC sub-layer.
            match self.mac.associate(channel, dest, mac_caps).await {
                Ok(response) => {
                    use zigbee_mac::AssociationStatus;

                    match response.status {
                        AssociationStatus::Successful => {
                            // --- Success: update NIB (§3.6.1.4.1.1) ---
                            let assigned_addr = response.association_address.0;
                            self.nib.set_network_address(assigned_addr);
                            self.nib.set_extended_panid(request.extended_pan_id);
                            self.nib.set_panid(pan_id.0);

                            // Read parent fields before the clearing loop
                            // zeroes them (§3.6.1.4.1.1).
                            let parent_update_id =
                                self.nib.neighbor_table()[candidate_idx].update_id;
                            let parent_channel =
                                self.nib.neighbor_table()[candidate_idx].logical_channel;
                            self.nib.set_update_id(parent_update_id);

                            // Update the neighbor table: set the relationship
                            // field to 0x00 (parent) and clear optional
                            // Table 3-64 fields on all entries (they should
                            // not be retained after joining).
                            let mut table = self.nib.neighbor_table();
                            table[candidate_idx].relationship = 0x00;
                            for neighbor in table.iter_mut() {
                                neighbor.extended_pan_id = IeeeAddress(0);
                                neighbor.logical_channel = 0;
                                neighbor.depth = 0;
                                neighbor.permit_joining = false;
                                neighbor.potential_parent = 0;
                                neighbor.router_capacity = false;
                                neighbor.end_device_capacity = false;
                                neighbor.update_id = 0;
                                neighbor.pan_id = 0xffff;
                            }
                            // Discard entries not on the chosen network
                            // (they are no longer relevant).
                            // TODO: retain only entries belonging to the
                            // joined network.
                            self.nib.set_neighbor_table(table);

                            return NlmeJoinConfirm {
                                status: NlmeJoinStatus::Success,
                                network_address: assigned_addr,
                                extended_pan_id: request.extended_pan_id,
                                channel: parent_channel,
                                enhanced_beacon_type: false,
                                mac_interface_index: 0u8,
                            };
                        }
                        AssociationStatus::NetworkAtCapacity => {
                            // Mark this neighbor as not a potential parent so
                            // we don't retry (§3.6.1.4.1.1).
                            let mut table = self.nib.neighbor_table();
                            table[candidate_idx].potential_parent = 0;
                            self.nib.set_neighbor_table(table);
                            last_status = NlmeJoinStatus::PanAtCapacity;
                        }
                        AssociationStatus::AccessDenied => {
                            let mut table = self.nib.neighbor_table();
                            table[candidate_idx].potential_parent = 0;
                            self.nib.set_neighbor_table(table);
                            last_status = NlmeJoinStatus::PanAccessDenied;
                        }
                        _ => {
                            // Other status codes (FastAssociationSuccesful,
                            // HoppingSequenceOffsetDuplication, etc.) are
                            // treated as a generic MAC-level failure.
                            last_status = NlmeJoinStatus::MacError;
                        }
                    }
                }
                Err(_mac_err) => {
                    // MAC-level failure (no ack, radio error, etc.)
                    last_status = NlmeJoinStatus::MacError;
                }
            }
        }

        // All candidates exhausted — return the last error status.
        fail(last_status)
    }

    async fn rejoin(&mut self) -> NlmeJoinConfirm {
        // TODO: read extended_pan_id from NIB
        let request = NlmeJoinRequest {
            // TODO: set ExtendedPANId parameter to the extended PAN identifier of the known network
            extended_pan_id: 0u64,
            rejoin_network: 0x02,
            // TODO: set ScanChannels parameter to 0x00000000
            scan_duration: 0x00,
            // TODO: set the CapabilityInformation appropriately for the node
            capability_information: CapabilityInformation(0x00),
            security_enabled: true,
        };

        self.join(request).await
    }
}

#[cfg(test)]
mod tests {
    use zigbee_mac::AssociationStatus;
    use zigbee_mac::CapabilityInformation as MacCapabilityInformation;
    use zigbee_mac::ExtendedAddress;
    use zigbee_mac::mlme::AssociationResponse;
    use zigbee_mac::mlme::ScanResult;
    use zigbee_mac::mlme::ScanType;
    use zigbee_types::ShortAddress as MacShortAddr;

    use super::*;
    use crate::nwk::nib::NibStorage;

    // -------------------------------------------------------------------
    // Minimal async block_on — the mock futures resolve immediately so a
    // single poll is sufficient.
    // -------------------------------------------------------------------

    #[allow(clippy::panic)]
    fn block_on<F: Future>(f: F) -> F::Output {
        use core::pin::pin;
        use core::task::Context;
        use core::task::Poll;
        use core::task::RawWaker;
        use core::task::RawWakerVTable;
        use core::task::Waker;

        fn noop(_: *const ()) {}
        fn clone(p: *const ()) -> RawWaker {
            RawWaker::new(p, &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);

        let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);
        let mut f = pin!(f);

        match f.as_mut().poll(&mut cx) {
            Poll::Ready(val) => val,
            Poll::Pending => panic!("block_on: future returned Pending"),
        }
    }

    // -------------------------------------------------------------------
    // A lightweight mock Mlme used exclusively by these tests.
    // -------------------------------------------------------------------

    /// Pre-canned response for a single `associate()` call.
    struct AssociateOutcome {
        result: Result<AssociationResponse, MacError>,
    }

    struct MockMlme {
        /// Queued responses for successive `associate()` calls, consumed
        /// in order.
        associate_outcomes: spin::Mutex<heapless::Vec<AssociateOutcome, 16>>,
    }

    impl MockMlme {
        fn new() -> Self {
            Self {
                associate_outcomes: spin::Mutex::new(heapless::Vec::new()),
            }
        }

        fn push_associate_ok(&mut self, addr: u16, status: AssociationStatus) {
            self.associate_outcomes
                .get_mut()
                .push(AssociateOutcome {
                    result: Ok(AssociationResponse {
                        device_address: ExtendedAddress(0),
                        association_address: MacShortAddr(addr),
                        status,
                    }),
                })
                .ok();
        }

        fn push_associate_err(&mut self, err: MacError) {
            self.associate_outcomes
                .get_mut()
                .push(AssociateOutcome { result: Err(err) })
                .ok();
        }
    }

    impl Mlme for MockMlme {
        async fn scan_network(
            &mut self,
            _ty: ScanType,
            _channels: impl Iterator<Item = u8>,
            _duration: u8,
        ) -> Result<ScanResult, MacError> {
            unimplemented!("scan_network not needed in join tests")
        }

        async fn associate(
            &mut self,
            _channel: u8,
            _dest: Address,
            _capabilities: MacCapabilityInformation,
        ) -> Result<AssociationResponse, MacError> {
            let mut outcomes = self.associate_outcomes.lock();
            assert!(
                !outcomes.is_empty(),
                "MockMlme: no more associate outcomes queued"
            );
            // Remove the first element (shift left).
            let outcome = outcomes.remove(0);
            outcome.result
        }

        async fn poll(&mut self, _coord_address: Address) -> Result<bool, MacError> {
            unimplemented!("poll not needed in join tests")
        }
    }

    // -------------------------------------------------------------------
    // Helpers
    // -------------------------------------------------------------------

    /// Create a default `NwkNeighbor` pre-filled for parent selection.
    fn make_neighbor(pan_id: u16, short_addr: u16, epid: u64, lqi: u8, depth: u8) -> NwkNeighbor {
        NwkNeighbor {
            network_address: ShortAddress(short_addr),
            device_type: if short_addr == 0 {
                DeviceType::Coordinator
            } else {
                DeviceType::Router
            },
            rx_on_when_idle: false,
            end_device_configuration: 0,
            relationship: 0x03,
            transmit_failure: 0,
            lqi,
            outgoing_cost: 0,
            age: 0,
            keepalive_received: false,
            extended_pan_id: IeeeAddress(epid),
            logical_channel: 11,
            depth,
            permit_joining: true,
            potential_parent: 1,
            router_capacity: true,
            end_device_capacity: true,
            update_id: 0,
            pan_id,
        }
    }

    fn make_nlme(mac: MockMlme) -> Nlme<NibStorage, MockMlme> {
        let nlme = Nlme::new(NibStorage::default(), mac);
        nlme.nib.init();
        nlme
    }

    fn default_join_request(epid: u64) -> NlmeJoinRequest {
        NlmeJoinRequest {
            extended_pan_id: epid,
            rejoin_network: 0x00,
            scan_duration: 0x00,
            // End device, no special capabilities, allocate address.
            capability_information: CapabilityInformation(0x80),
            security_enabled: false,
        }
    }

    // -------------------------------------------------------------------
    // select_parent_candidates tests
    // -------------------------------------------------------------------

    #[test]
    fn select_parent_no_neighbors() {
        let nlme = make_nlme(MockMlme::new());
        let candidates = nlme.select_parent_candidates(0x1234, false);
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_parent_filters_by_extended_pan_id() {
        let nlme = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        // Neighbor on the correct network
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0))
            .unwrap();
        // Neighbor on a different network
        table
            .push(make_neighbor(0xBBBB, 0x0001, 0x9999, 200, 0))
            .unwrap();
        nlme.nib.set_neighbor_table(table);

        let candidates = nlme.select_parent_candidates(0x1234, false);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], 0); // first neighbor
    }

    #[test]
    fn select_parent_filters_by_link_cost() {
        let nlme = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        // Good LQI => low cost => eligible
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0))
            .unwrap();
        // Bad LQI => high cost => filtered out
        table
            .push(make_neighbor(0xAAAA, 0x0001, 0x1234, 10, 0))
            .unwrap();
        nlme.nib.set_neighbor_table(table);

        let candidates = nlme.select_parent_candidates(0x1234, false);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], 0);
    }

    #[test]
    fn select_parent_filters_by_end_device_capacity() {
        let nlme = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n = make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0);
        n.end_device_capacity = false; // no room for end devices
        table.push(n).unwrap();
        nlme.nib.set_neighbor_table(table);

        // Joining as end device — should find no candidates.
        let candidates = nlme.select_parent_candidates(0x1234, false);
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_parent_filters_by_router_capacity() {
        let nlme = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n = make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0);
        n.router_capacity = false;
        table.push(n).unwrap();
        nlme.nib.set_neighbor_table(table);

        // Joining as router — should find no candidates.
        let candidates = nlme.select_parent_candidates(0x1234, true);
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_parent_sorts_by_depth_for_stack_profile_1() {
        let nlme = make_nlme(MockMlme::new());
        // Set stack profile to 1.
        nlme.nib.set_stack_profile(1);

        let mut table = StorageVec::new();
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 3))
            .unwrap();
        table
            .push(make_neighbor(0xAAAA, 0x0001, 0x1234, 200, 1))
            .unwrap();
        table
            .push(make_neighbor(0xAAAA, 0x0002, 0x1234, 200, 2))
            .unwrap();
        nlme.nib.set_neighbor_table(table);

        let candidates = nlme.select_parent_candidates(0x1234, false);
        assert_eq!(candidates.len(), 3);
        // Should be sorted: depth 1 (idx 1), depth 2 (idx 2), depth 3 (idx 0)
        assert_eq!(candidates[0], 1);
        assert_eq!(candidates[1], 2);
        assert_eq!(candidates[2], 0);
    }

    #[test]
    fn select_parent_filters_not_permitting_join() {
        let nlme = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n = make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0);
        n.permit_joining = false;
        table.push(n).unwrap();
        nlme.nib.set_neighbor_table(table);

        let candidates = nlme.select_parent_candidates(0x1234, false);
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_parent_filters_non_potential_parent() {
        let nlme = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n = make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0);
        n.potential_parent = 0; // previously rejected
        table.push(n).unwrap();
        nlme.nib.set_neighbor_table(table);

        let candidates = nlme.select_parent_candidates(0x1234, false);
        assert!(candidates.is_empty());
    }

    #[test]
    fn select_parent_prefers_most_recent_update_id() {
        let nlme = make_nlme(MockMlme::new());

        let mut table = StorageVec::new();
        let mut n1 = make_neighbor(0xAAAA, 0x0000, 0x1234, 200, 0);
        n1.update_id = 5;
        table.push(n1).unwrap();
        let mut n2 = make_neighbor(0xAAAA, 0x0001, 0x1234, 200, 0);
        n2.update_id = 3; // older update
        table.push(n2).unwrap();
        nlme.nib.set_neighbor_table(table);

        let candidates = nlme.select_parent_candidates(0x1234, false);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], 0); // only the one with update_id=5
    }

    // -------------------------------------------------------------------
    // join() integration tests (using MockMlme)
    // -------------------------------------------------------------------

    #[test]
    fn join_successful_association() {
        let mut mac = MockMlme::new();
        mac.push_associate_ok(0x1234, AssociationStatus::Successful);

        let mut nlme = make_nlme(mac);

        let mut table = StorageVec::new();
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0))
            .unwrap();
        nlme.nib.set_neighbor_table(table);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));

        assert_eq!(confirm.status, NlmeJoinStatus::Success);
        assert_eq!(confirm.network_address, 0x1234);
        assert_eq!(confirm.extended_pan_id, 0xDEAD);
        assert_eq!(confirm.channel, 11);

        // NIB should be updated.
        assert_eq!(nlme.nib.network_address(), 0x1234);
        assert_eq!(nlme.nib.extended_panid(), 0xDEAD);
        assert_eq!(nlme.nib.panid(), 0xAAAA);
        assert_eq!(nlme.nib.update_id(), 0); // make_neighbor defaults to update_id=0

        // Neighbor relationship should be 0x00 (parent).
        let table = nlme.nib.neighbor_table();
        assert_eq!(table[0].relationship, 0x00);
    }

    #[test]
    fn join_sets_nwk_update_id_from_parent() {
        let mut mac = MockMlme::new();
        mac.push_associate_ok(0x1234, AssociationStatus::Successful);

        let mut nlme = make_nlme(mac);

        let mut n = make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0);
        n.update_id = 7;
        let mut table = StorageVec::new();
        table.push(n).unwrap();
        nlme.nib.set_neighbor_table(table);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::Success);
        // nwkUpdateId must be set from the parent's update_id (§3.6.1.4.1.1).
        assert_eq!(nlme.nib.update_id(), 7);
    }

    #[test]
    fn join_fails_when_no_candidates() {
        let mac = MockMlme::new();
        let mut nlme = make_nlme(mac);
        // Empty neighbor table.
        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::NotPermitted);
    }

    #[test]
    fn join_fails_when_already_joined() {
        let mac = MockMlme::new();
        let mut nlme = make_nlme(mac);
        // Simulate already joined by setting a valid network address.
        nlme.nib.set_network_address(0x0001);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::InvalidRequest);
    }

    #[test]
    fn join_skips_capacity_rejected_parent_tries_next() {
        let mut mac = MockMlme::new();
        // First candidate rejects with NetworkAtCapacity.
        mac.push_associate_ok(0, AssociationStatus::NetworkAtCapacity);
        // Second candidate accepts.
        mac.push_associate_ok(0x5678, AssociationStatus::Successful);

        let mut nlme = make_nlme(mac);

        let mut table = StorageVec::new();
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0))
            .unwrap();
        table
            .push(make_neighbor(0xAAAA, 0x0001, 0xDEAD, 200, 0))
            .unwrap();
        nlme.nib.set_neighbor_table(table);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::Success);
        assert_eq!(confirm.network_address, 0x5678);

        // After successful join, Table 3-64 fields are cleared.
        // First neighbor was marked potential_parent=0 before the second
        // attempt, then all optional fields were cleared on success.
        let table = nlme.nib.neighbor_table();
        assert_eq!(table[0].potential_parent, 0);
        // Second neighbor should be parent.
        assert_eq!(table[1].relationship, 0x00);
    }

    #[test]
    fn join_all_candidates_rejected() {
        let mut mac = MockMlme::new();
        mac.push_associate_ok(0, AssociationStatus::AccessDenied);

        let mut nlme = make_nlme(mac);

        let mut table = StorageVec::new();
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0))
            .unwrap();
        nlme.nib.set_neighbor_table(table);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::PanAccessDenied);
        assert_eq!(confirm.network_address, 0xffff);
    }

    #[test]
    fn join_mac_error_reported() {
        let mut mac = MockMlme::new();
        mac.push_associate_err(MacError::NoAck);

        let mut nlme = make_nlme(mac);

        let mut table = StorageVec::new();
        table
            .push(make_neighbor(0xAAAA, 0x0000, 0xDEAD, 200, 0))
            .unwrap();
        nlme.nib.set_neighbor_table(table);

        let confirm = block_on(nlme.join(default_join_request(0xDEAD)));
        assert_eq!(confirm.status, NlmeJoinStatus::MacError);
    }

    #[test]
    fn join_invalid_rejoin_network() {
        let mac = MockMlme::new();
        let mut nlme = make_nlme(mac);

        let mut req = default_join_request(0xDEAD);
        req.rejoin_network = 0x01; // orphan rejoin — not yet supported

        let confirm = block_on(nlme.join(req));
        assert_eq!(confirm.status, NlmeJoinStatus::InvalidRequest);
    }
}
