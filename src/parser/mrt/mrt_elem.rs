#![allow(unused)]
//! This module handles converting MRT records into individual per-prefix BGP elements.
//!
//! Each MRT record may contain reachability information for multiple prefixes. This module breaks
//! down MRT records into corresponding BGP elements, and thus allowing users to more conveniently
//! process BGP information on a per-prefix basis.
use crate::models::*;
use crate::parser::bgp::messages::parse_bgp_update_message;
use itertools::Itertools;
use log::warn;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::net::IpAddr;

pub struct Elementor {
    peer_table: Option<PeerIndexTable>,
}

// use macro_rules! <name of macro>{<Body>}
macro_rules! get_attr_value {
    ($a:tt, $b:expr) => {
        if let Attribute::$a(x) = $b {
            Some(x)
        } else {
            None
        }
    };
}

#[allow(clippy::type_complexity)]
fn get_relevant_attributes(
    attributes: Attributes,
) -> (
    Option<AsPath>,
    Option<AsPath>,
    Option<Origin>,
    Option<IpAddr>,
    Option<u32>,
    Option<u32>,
    Option<Vec<MetaCommunity>>,
    bool,
    Option<(Asn, BgpIdentifier)>,
    Option<Nlri>,
    Option<Nlri>,
    Option<Asn>,
    Option<Vec<AttrRaw>>,
    Option<Vec<AttrRaw>>,
) {
    let mut as_path = None;
    let mut as4_path = None;
    let mut origin = None;
    let mut next_hop = None;
    let mut local_pref = Some(0);
    let mut med = Some(0);
    let mut atomic = false;
    let mut aggregator = None;
    let mut announced = None;
    let mut withdrawn = None;
    let mut otc = None;
    let mut unknown = vec![];
    let mut deprecated = vec![];

    let mut communities_vec: Vec<MetaCommunity> = vec![];

    for attr in attributes {
        match attr {
            AttributeValue::Origin(v) => origin = Some(v),
            AttributeValue::AsPath {
                path,
                is_as4: false,
            } => as_path = Some(path),
            AttributeValue::AsPath { path, is_as4: true } => as4_path = Some(path),
            AttributeValue::NextHop(v) => next_hop = Some(v),
            AttributeValue::MultiExitDiscriminator(v) => med = Some(v),
            AttributeValue::LocalPreference(v) => local_pref = Some(v),
            AttributeValue::AtomicAggregate => atomic = true,
            AttributeValue::Communities(v) => communities_vec.extend(
                v.into_iter()
                    .map(MetaCommunity::Plain)
                    .collect::<Vec<MetaCommunity>>(),
            ),
            AttributeValue::ExtendedCommunities(v) => communities_vec.extend(
                v.into_iter()
                    .map(MetaCommunity::Extended)
                    .collect::<Vec<MetaCommunity>>(),
            ),
            AttributeValue::LargeCommunities(v) => communities_vec.extend(
                v.into_iter()
                    .map(MetaCommunity::Large)
                    .collect::<Vec<MetaCommunity>>(),
            ),
            AttributeValue::Aggregator { asn, id, .. } => aggregator = Some((asn, id)),
            AttributeValue::MpReachNlri(nlri) => announced = Some(nlri),
            AttributeValue::MpUnreachNlri(nlri) => withdrawn = Some(nlri),
            AttributeValue::OnlyToCustomer(o) => otc = Some(o),

            AttributeValue::Unknown(t) => {
                unknown.push(t);
            }
            AttributeValue::Deprecated(t) => {
                deprecated.push(t);
            }

            AttributeValue::OriginatorId(_)
            | AttributeValue::Clusters(_)
            | AttributeValue::Development(_) => {}
        };
    }

    let communities = match !communities_vec.is_empty() {
        true => Some(communities_vec),
        false => None,
    };

    (
        as_path,
        as4_path,
        origin,
        next_hop,
        local_pref,
        med,
        communities,
        atomic,
        aggregator,
        announced,
        withdrawn,
        otc,
        if unknown.is_empty() {
            None
        } else {
            Some(unknown)
        },
        if deprecated.is_empty() {
            None
        } else {
            Some(deprecated)
        },
    )
}

impl Elementor {
    pub fn new() -> Elementor {
        Elementor { peer_table: None }
    }

    /// Convert a [BgpMessage] to a vector of [BgpElem]s.
    ///
    /// A [BgpMessage] may include `Update`, `Open`, `Notification` or `KeepAlive` messages,
    /// and only `Update` message contains [BgpElem]s.
    pub fn bgp_to_elems(
        msg: BgpMessage,
        timestamp: f64,
        peer_ip: &IpAddr,
        peer_asn: &Asn,
    ) -> Vec<BgpElem> {
        match msg {
            BgpMessage::Update(msg) => {
                Elementor::bgp_update_to_elems(msg, timestamp, peer_ip, peer_asn)
            }
            BgpMessage::Open(_) | BgpMessage::Notification(_) | BgpMessage::KeepAlive => {
                vec![]
            }
        }
    }

    /// Convert a [BgpUpdateMessage] to a vector of [BgpElem]s.
    pub fn bgp_update_to_elems(
        msg: BgpUpdateMessage,
        timestamp: f64,
        peer_ip: &IpAddr,
        peer_asn: &Asn,
    ) -> Vec<BgpElem> {
        let mut elems = vec![];

        let (
            as_path,
            as4_path, // Table dump v1 does not have 4-byte AS number
            origin,
            next_hop,
            local_pref,
            med,
            communities,
            atomic,
            aggregator,
            announced,
            withdrawn,
            only_to_customer,
            unknown,
            deprecated,
        ) = get_relevant_attributes(msg.attributes);

        let path = match (as_path, as4_path) {
            (None, None) => None,
            (Some(v), None) => Some(v),
            (None, Some(v)) => Some(v),
            (Some(v1), Some(v2)) => Some(AsPath::merge_aspath_as4path(&v1, &v2).unwrap()),
        };

        let origin_asns = path
            .as_ref()
            .map(|as_path| as_path.iter_origins().collect());

        elems.extend(msg.announced_prefixes.into_iter().map(|p| BgpElem {
            timestamp,
            elem_type: ElemType::ANNOUNCE,
            peer_ip: *peer_ip,
            peer_asn: *peer_asn,
            prefix: p,
            next_hop,
            as_path: path.clone(),
            origin_asns: origin_asns.clone(),
            origin,
            local_pref,
            med,
            communities: communities.clone(),
            atomic,
            aggr_asn: aggregator.as_ref().map(|v| v.0),
            aggr_ip: aggregator.as_ref().map(|v| v.1),
            only_to_customer,
            unknown: unknown.clone(),
            deprecated: deprecated.clone(),
        }));

        if let Some(nlri) = announced {
            elems.extend(nlri.prefixes.into_iter().map(|p| BgpElem {
                timestamp,
                elem_type: ElemType::ANNOUNCE,
                peer_ip: *peer_ip,
                peer_asn: *peer_asn,
                prefix: p,
                next_hop,
                as_path: path.clone(),
                origin,
                origin_asns: origin_asns.clone(),
                local_pref,
                med,
                communities: communities.clone(),
                atomic,
                aggr_asn: aggregator.as_ref().map(|v| v.0),
                aggr_ip: aggregator.as_ref().map(|v| v.1),
                only_to_customer,
                unknown: unknown.clone(),
                deprecated: deprecated.clone(),
            }));
        }

        elems.extend(msg.withdrawn_prefixes.into_iter().map(|p| BgpElem {
            timestamp,
            elem_type: ElemType::WITHDRAW,
            peer_ip: *peer_ip,
            peer_asn: *peer_asn,
            prefix: p,
            next_hop: None,
            as_path: None,
            origin: None,
            origin_asns: None,
            local_pref: None,
            med: None,
            communities: None,
            atomic: false,
            aggr_asn: None,
            aggr_ip: None,
            only_to_customer,
            unknown: None,
            deprecated: None,
        }));
        if let Some(nlri) = withdrawn {
            elems.extend(nlri.prefixes.into_iter().map(|p| BgpElem {
                timestamp,
                elem_type: ElemType::WITHDRAW,
                peer_ip: *peer_ip,
                peer_asn: *peer_asn,
                prefix: p,
                next_hop: None,
                as_path: None,
                origin: None,
                origin_asns: None,
                local_pref: None,
                med: None,
                communities: None,
                atomic: false,
                aggr_asn: None,
                aggr_ip: None,
                only_to_customer,
                unknown: None,
                deprecated: None,
            }));
        };
        elems
    }

    /// Convert a [MrtRecord] to a vector of [BgpElem]s.
    pub fn record_to_elems(&mut self, record: MrtRecord) -> Vec<BgpElem> {
        let mut elems = vec![];
        let t = record.common_header.timestamp;
        let timestamp: f64 = if let Some(micro) = &record.common_header.microsecond_timestamp {
            let m = (*micro as f64) / 1000000.0;
            t as f64 + m
        } else {
            f64::from(t)
        };

        match record.message {
            MrtMessage::TableDumpMessage(msg) => {
                let (
                    as_path,
                    _as4_path, // Table dump v1 does not have 4-byte AS number
                    origin,
                    next_hop,
                    local_pref,
                    med,
                    communities,
                    atomic,
                    aggregator,
                    _announced,
                    _withdrawn,
                    only_to_customer,
                    unknown,
                    deprecated,
                ) = get_relevant_attributes(msg.attributes);

                let origin_asns = as_path
                    .as_ref()
                    .map(|as_path| as_path.iter_origins().collect());

                elems.push(BgpElem {
                    timestamp,
                    elem_type: ElemType::ANNOUNCE,
                    peer_ip: msg.peer_address,
                    peer_asn: msg.peer_asn,
                    prefix: msg.prefix,
                    next_hop,
                    as_path,
                    origin,
                    origin_asns,
                    local_pref,
                    med,
                    communities,
                    atomic,
                    aggr_asn: aggregator.map(|v| v.0),
                    aggr_ip: aggregator.map(|v| v.1),
                    only_to_customer,
                    unknown,
                    deprecated,
                });
            }

            MrtMessage::TableDumpV2Message(msg) => {
                match msg {
                    TableDumpV2Message::PeerIndexTable(p) => {
                        self.peer_table = Some(p);
                    }
                    TableDumpV2Message::RibAfi(t) => {
                        let prefix = t.prefix;
                        for e in t.rib_entries {
                            let pid = e.peer_index;
                            let peer = self
                                .peer_table
                                .as_ref()
                                .unwrap()
                                .peers_map
                                .get(&(pid as u32))
                                .unwrap();
                            let (
                                as_path,
                                as4_path, // Table dump v1 does not have 4-byte AS number
                                origin,
                                next_hop,
                                local_pref,
                                med,
                                communities,
                                atomic,
                                aggregator,
                                announced,
                                _withdrawn,
                                only_to_customer,
                                unknown,
                                deprecated,
                            ) = get_relevant_attributes(e.attributes);

                            let path = match (as_path, as4_path) {
                                (None, None) => None,
                                (Some(v), None) => Some(v),
                                (None, Some(v)) => Some(v),
                                (Some(v1), Some(v2)) => {
                                    Some(AsPath::merge_aspath_as4path(&v1, &v2).unwrap())
                                }
                            };

                            let next = match next_hop {
                                None => {
                                    if let Some(v) = announced {
                                        if let Some(h) = v.next_hop {
                                            match h {
                                                NextHopAddress::Ipv4(v) => Some(IpAddr::from(v)),
                                                NextHopAddress::Ipv6(v) => Some(IpAddr::from(v)),
                                                NextHopAddress::Ipv6LinkLocal(v, _) => {
                                                    Some(IpAddr::from(v))
                                                }
                                            }
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                }
                                Some(v) => Some(v),
                            };

                            let origin_asns = path
                                .as_ref()
                                .map(|as_path| as_path.iter_origins().collect());

                            elems.push(BgpElem {
                                timestamp,
                                elem_type: ElemType::ANNOUNCE,
                                peer_ip: peer.peer_address,
                                peer_asn: peer.peer_asn,
                                prefix,
                                next_hop: next,
                                as_path: path,
                                origin,
                                origin_asns,
                                local_pref,
                                med,
                                communities,
                                atomic,
                                aggr_asn: aggregator.map(|v| v.0),
                                aggr_ip: aggregator.map(|v| v.1),
                                only_to_customer,
                                unknown,
                                deprecated,
                            });
                        }
                    }
                    TableDumpV2Message::RibGeneric(_t) => {
                        warn!(
                            "to_elem for TableDumpV2Message::RibGenericEntries not yet implemented"
                        );
                    }
                }
            }
            MrtMessage::Bgp4Mp(msg) => match msg {
                Bgp4Mp::StateChange(_) => {}
                Bgp4Mp::Message(v) => {
                    elems.extend(Elementor::bgp_to_elems(
                        v.bgp_message,
                        timestamp,
                        &v.peer_ip,
                        &v.peer_asn,
                    ));
                }
            },
        }
        elems
    }
}

#[inline(always)]
pub fn option_to_string<T>(o: &Option<T>) -> String
where
    T: Display,
{
    if let Some(v) = o {
        v.to_string()
    } else {
        String::new()
    }
}
