/*!
## Message Filters

The filters package defines a number of available filters that users can utilize, and implements
the filtering mechanism for [BgpElem].

The available filters are:
- `origin_asn` -- origin AS number
- `prefix` -- network prefix and match type
- `peer_ip` -- peer's IP address
- `peer_ips` -- peers' IP addresses
- `peer_asn` -- peer's IP address
- `type` -- message type (`withdraw` or `announce`)
- `ts_start` -- start and end unix timestamp
- `as_path` -- regular expression for AS path string

[Filter::new] function takes a `str` for filter type and `str` for filter value and returns a Result
of a [Filter] or a parsing error.

[BgpkitParser](crate::BgpkitParser) implements the function `add_filter("filter_type", "filter_value")` that takes the parser's ownership itself
and returns a new parser with specified filter added. See the example below.

### Example

```no_run
use bgpkit_parser::BgpkitParser;

/// This example shows how to parse a MRT file and filter by prefix.
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

log::info!("downloading updates file");
let parser = BgpkitParser::new("http://archive.routeviews.org/bgpdata/2021.10/UPDATES/updates.20211001.0000.bz2").unwrap()
    .add_filter("prefix", "211.98.251.0/24").unwrap()
    .add_filter("type", "a").unwrap();

// iterating through the parser. the iterator returns `BgpElem` one at a time.
log::info!("parsing updates file");
for elem in parser {
    log::info!("{}", &elem);
}
log::info!("done");
```

Note, by default, the prefix filtering is for the exact prefix. You can include super-prefixes or
sub-prefixes when fitlering by using `"prefix_super"`, `"prefix_sub"`, or  `"prefix_super_sub"` as
the filter type string.

### Note

Currently, only [BgpElem] implements the filtering capability. Support for [MrtRecord] will come in
later releases.

*/
use crate::models::*;
use crate::ParserError;
use crate::ParserError::FilterError;
use ipnet::IpNet;
use regex::Regex;
use std::net::IpAddr;
use std::str::FromStr;

/// Filter enum: definition o types of filters
///
/// The available filters are (`filter_type` (`FilterType`) -- definition):
/// - `origin_asn` (`OriginAsn(u32)`) -- origin AS number
/// - `prefix(_super, _sub, _super_sub)` (`Prefix(IpNet, PrefixMatchType)`) -- network prefix and match type
/// - `peer_ip` (`PeerIp(IpAddr)`) -- peer's IP address
/// - `peer_ips` (`Vec<PeerIp(IpAddr)>`) -- peers' IP addresses
/// - `peer_asn` (`PeerAsn(u32)`) -- peer's IP address
/// - `type` (`Type(ElemType)`) -- message type (`withdraw` or `announce`)
/// - `ts_start` (`TsStart(f64)`) and `ts_end` (`TsEnd(f64)`) -- start and end unix timestamp
/// - `as_path` (`AsPath(Regex)`) -- regular expression for AS path string
pub enum Filter {
    OriginAsn(u32),
    Prefix(IpNet, PrefixMatchType),
    PeerIp(IpAddr),
    PeerIps(Vec<IpAddr>),
    PeerAsn(u32),
    Type(ElemType),
    TsStart(f64),
    TsEnd(f64),
    AsPath(Regex),
}

pub enum PrefixMatchType {
    Exact,
    IncludeSuper,
    IncludeSub,
    IncludeSuperSub,
}

fn parse_time_str(time_str: &str) -> Option<chrono::NaiveDateTime> {
    if let Ok(t) = time_str.parse::<f64>() {
        return chrono::NaiveDateTime::from_timestamp_opt(t as i64, 0);
    }
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(time_str) {
        return Some(t.naive_utc());
    }
    None
}

impl Filter {
    pub fn new(filter_type: &str, filter_value: &str) -> Result<Filter, ParserError> {
        match filter_type {
            "origin_asn" => match u32::from_str(filter_value) {
                Ok(v) => Ok(Filter::OriginAsn(v)),
                Err(_) => Err(FilterError(format!(
                    "cannot parse origin asn from {}",
                    filter_value
                ))),
            },
            "prefix" => match IpNet::from_str(filter_value) {
                Ok(v) => Ok(Filter::Prefix(v, PrefixMatchType::Exact)),
                Err(_) => Err(FilterError(format!(
                    "cannot parse prefix from {}",
                    filter_value
                ))),
            },
            "prefix_super" => match IpNet::from_str(filter_value) {
                Ok(v) => Ok(Filter::Prefix(v, PrefixMatchType::IncludeSuper)),
                Err(_) => Err(FilterError(format!(
                    "cannot parse prefix from {}",
                    filter_value
                ))),
            },
            "prefix_sub" => match IpNet::from_str(filter_value) {
                Ok(v) => Ok(Filter::Prefix(v, PrefixMatchType::IncludeSub)),
                Err(_) => Err(FilterError(format!(
                    "cannot parse prefix from {}",
                    filter_value
                ))),
            },
            "prefix_super_sub" => match IpNet::from_str(filter_value) {
                Ok(v) => Ok(Filter::Prefix(v, PrefixMatchType::IncludeSuperSub)),
                Err(_) => Err(FilterError(format!(
                    "cannot parse prefix from {}",
                    filter_value
                ))),
            },
            "peer_ip" => match IpAddr::from_str(filter_value) {
                Ok(v) => Ok(Filter::PeerIp(v)),
                Err(_) => Err(FilterError(format!(
                    "cannot parse peer IP from {}",
                    filter_value
                ))),
            },
            "peer_ips" => {
                let mut ips = vec![];
                for ip_str in filter_value.replace(' ', "").split(',') {
                    match IpAddr::from_str(ip_str) {
                        Ok(v) => ips.push(v),
                        Err(_) => {
                            return Err(FilterError(format!(
                                "cannot parse peer IP from {}",
                                ip_str
                            )))
                        }
                    }
                }
                Ok(Filter::PeerIps(ips))
            }
            "peer_asn" => match u32::from_str(filter_value) {
                Ok(v) => Ok(Filter::PeerAsn(v)),
                Err(_) => Err(FilterError(format!(
                    "cannot parse peer asn from {}",
                    filter_value
                ))),
            },
            "type" => match filter_value {
                "w" | "withdraw" | "withdrawal" => Ok(Filter::Type(ElemType::WITHDRAW)),
                "a" | "announce" | "announcement" => Ok(Filter::Type(ElemType::ANNOUNCE)),
                _ => Err(FilterError(format!(
                    "cannot parse elem type from {}",
                    filter_value
                ))),
            },
            "ts_start" | "start_ts" => match parse_time_str(filter_value) {
                Some(t) => Ok(Filter::TsStart(t.timestamp() as f64)),
                None => Err(FilterError(format!(
                    "cannot parse TsStart filter from {}",
                    filter_value
                ))),
            },
            "ts_end" | "end_ts" => match parse_time_str(filter_value) {
                Some(t) => Ok(Filter::TsEnd(t.timestamp() as f64)),
                None => Err(FilterError(format!(
                    "cannot parse TsEnd filter from {}",
                    filter_value
                ))),
            },
            "as_path" => match Regex::from_str(filter_value) {
                Ok(v) => Ok(Filter::AsPath(v)),
                Err(_) => Err(FilterError(format!(
                    "cannot parse AS path regex from {}",
                    filter_value
                ))),
            },
            _ => Err(FilterError(format!("unknown filter type: {}", filter_type))),
        }
    }
}

pub trait Filterable {
    fn match_filter(&self, filter: &Filter) -> bool;
    fn match_filters(&self, filters: &[Filter]) -> bool;
}

const fn same_family(prefix_1: &IpNet, prefix_2: &IpNet) -> bool {
    matches!(
        (prefix_1, prefix_2),
        (IpNet::V4(_), IpNet::V4(_)) | (IpNet::V6(_), IpNet::V6(_))
    )
}

fn prefix_match(match_prefix: &IpNet, input_prefix: &IpNet, t: &PrefixMatchType) -> bool {
    let exact = input_prefix.eq(match_prefix);
    match t {
        PrefixMatchType::Exact => exact,
        PrefixMatchType::IncludeSuper => {
            if exact {
                exact
            } else if !same_family(match_prefix, input_prefix) {
                // version not match
                false
            } else {
                // input_prefix is super prefix of match_prefix
                match_prefix.addr() >= input_prefix.addr()
                    && match_prefix.broadcast() <= input_prefix.broadcast()
            }
        }
        PrefixMatchType::IncludeSub => {
            if exact {
                exact
            } else if !same_family(match_prefix, input_prefix) {
                // version not match
                false
            } else {
                // input_prefix is sub prefix of match_prefix
                match_prefix.addr() <= input_prefix.addr()
                    && match_prefix.broadcast() >= input_prefix.broadcast()
            }
        }
        PrefixMatchType::IncludeSuperSub => {
            if exact {
                exact
            } else if !same_family(match_prefix, input_prefix) {
                // version not match
                false
            } else {
                // input_prefix is super prefix of match_prefix
                (match_prefix.addr() >= input_prefix.addr()
                    && match_prefix.broadcast() <= input_prefix.broadcast())
                    || (match_prefix.addr() <= input_prefix.addr()
                        && match_prefix.broadcast() >= input_prefix.broadcast())
            }
        }
    }
}

impl Filterable for BgpElem {
    fn match_filter(&self, filter: &Filter) -> bool {
        match filter {
            Filter::OriginAsn(v) => {
                let asn: Asn = (*v).into();
                if let Some(origins) = &self.origin_asns {
                    origins.contains(&asn)
                } else {
                    false
                }
            }
            Filter::Prefix(v, t) => prefix_match(v, &self.prefix.prefix, t),
            Filter::PeerIp(v) => self.peer_ip == *v,
            Filter::PeerIps(v) => v.contains(&self.peer_ip),
            Filter::PeerAsn(v) => self.peer_asn.eq(v),
            Filter::Type(v) => self.elem_type.eq(v),
            Filter::TsStart(v) => self.timestamp >= *v,
            Filter::TsEnd(v) => self.timestamp <= *v,
            Filter::AsPath(v) => {
                if let Some(path) = &self.as_path {
                    v.is_match(path.to_string().as_str())
                } else {
                    false
                }
            }
        }
    }

    fn match_filters(&self, filters: &[Filter]) -> bool {
        filters.iter().all(|f| self.match_filter(f))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BgpkitParser;
    use anyhow::Result;
    use std::str::FromStr;

    #[test]
    fn test_filter() {
        let url = "https://spaces.bgpkit.org/parser/update-example.gz";
        let parser = BgpkitParser::new(url).unwrap();
        let elems = parser.into_elem_iter().collect::<Vec<BgpElem>>();

        let filters = vec![Filter::PeerIp(IpAddr::from_str("185.1.8.65").unwrap())];
        let count = elems.iter().filter(|e| e.match_filters(&filters)).count();
        assert_eq!(count, 3393);

        let filters = vec![
            Filter::PeerIp(IpAddr::from_str("185.1.8.65").unwrap()),
            Filter::Prefix(
                IpNet::from_str("190.115.192.0/22").unwrap(),
                PrefixMatchType::Exact,
            ),
        ];
        let count = elems.iter().filter(|e| e.match_filters(&filters)).count();
        assert_eq!(count, 5);

        let filters = vec![Filter::Prefix(
            IpNet::from_str("190.115.192.0/24").unwrap(),
            PrefixMatchType::IncludeSuper,
        )];
        let count = elems.iter().filter(|e| e.match_filters(&filters)).count();
        assert_eq!(count, 18);

        let filters = vec![Filter::Prefix(
            IpNet::from_str("190.115.192.0/22").unwrap(),
            PrefixMatchType::IncludeSub,
        )];
        let count = elems.iter().filter(|e| e.match_filters(&filters)).count();
        assert_eq!(count, 42);

        let filters = vec![Filter::Prefix(
            IpNet::from_str("190.115.192.0/23").unwrap(),
            PrefixMatchType::IncludeSuperSub,
        )];
        let count = elems.iter().filter(|e| e.match_filters(&filters)).count();
        assert_eq!(count, 24);

        let filters = vec![Filter::AsPath(Regex::new(r" ?174 1916 52888$").unwrap())];
        let count = elems.iter().filter(|e| e.match_filters(&filters)).count();
        assert_eq!(count, 12);

        let filters = vec![
            Filter::TsStart(1637437798_f64),
            Filter::TsEnd(1637437798_f64),
        ];
        let count = elems.iter().filter(|e| e.match_filters(&filters)).count();
        assert_eq!(count, 13);

        let filters = vec![Filter::Type(ElemType::WITHDRAW)];
        let count = elems.iter().filter(|e| e.match_filters(&filters)).count();
        assert_eq!(count, 379);

        let filters = vec![
            Filter::Type(ElemType::WITHDRAW),
            Filter::Prefix(
                IpNet::from_str("2804:100::/32").unwrap(),
                PrefixMatchType::Exact,
            ),
        ];
        let count = elems.iter().filter(|e| e.match_filters(&filters)).count();
        assert_eq!(count, 1);

        // test filtering by multiple peers
        /*
        1167 185.1.8.3
        1563 185.1.8.50
        3393 185.1.8.65
          51 185.1.8.89
         834 2001:7f8:73:0:3:fa4:0:1
          94 2001:7f8:73::c2a8:0:1
        1058 2001:7f8:73::edfc:0:2
         */
        let filters = vec![Filter::PeerIps(vec![
            IpAddr::from_str("185.1.8.65").unwrap(),
            IpAddr::from_str("2001:7f8:73:0:3:fa4:0:1").unwrap(),
        ])];
        let count = elems.iter().filter(|e| e.match_filters(&filters)).count();
        assert_eq!(count, 3393 + 834);
    }

    #[test]
    fn test_parsing_time_str() {
        let ts = chrono::NaiveDateTime::from_str("2021-11-20T19:49:58").unwrap();
        assert_eq!(parse_time_str("1637437798"), Some(ts));
        assert_eq!(parse_time_str("2021-11-20T19:49:58Z"), Some(ts));
        assert_eq!(parse_time_str("2021-11-20T19:49:58+00:00"), Some(ts));

        assert_eq!(parse_time_str("2021-11-20T19:49:58"), None);
        assert_eq!(parse_time_str("2021-11-20T19:49:58ZDXV"), None);
        assert_eq!(parse_time_str("2021-11-20 19:49:58"), None);
        assert_eq!(parse_time_str("2021-11-20"), None);
    }

    #[test]
    fn test_filter_iter() -> Result<()> {
        let url = "https://spaces.bgpkit.org/parser/update-example.gz";
        let parser = BgpkitParser::new(url)?
            .add_filter("peer_ip", "185.1.8.50")?
            .add_filter("type", "w")?;
        let count = parser.into_elem_iter().count();
        assert_eq!(count, 39);

        let parser = BgpkitParser::new(url)?
            .add_filter("ts_start", "1637437798")?
            .add_filter("ts_end", "2021-11-20T19:49:58Z")?;
        let count = parser.into_elem_iter().count();
        assert_eq!(count, 13);
        Ok(())
    }

    #[test]
    fn test_filter_iter_multi_peers() {
        let url = "https://spaces.bgpkit.org/parser/update-example.gz";
        let parser = BgpkitParser::new(url)
            .unwrap()
            .add_filter("peer_ips", "185.1.8.65, 2001:7f8:73:0:3:fa4:0:1")
            .unwrap();
        let count = parser.into_elem_iter().count();
        assert_eq!(count, 3393 + 834);
    }

    #[test]
    fn test_prefix_match() {
        // network
        let p1 = IpNet::from_str("10.1.1.0/24").unwrap();
        let p1_exact = IpNet::from_str("10.1.1.0/24").unwrap();
        let p1_super = IpNet::from_str("10.1.0.0/16").unwrap();
        let p1_sub = IpNet::from_str("10.1.1.0/25").unwrap();

        let p2 = IpNet::from_str("2001:0DB8:0000:000b::/64").unwrap();

        // exact
        assert!(prefix_match(&p1, &p1_exact, &PrefixMatchType::Exact));
        assert!(!prefix_match(&p1, &p1_sub, &PrefixMatchType::Exact));
        assert!(!prefix_match(&p1, &p1_super, &PrefixMatchType::Exact));
        assert!(!prefix_match(&p1, &p2, &PrefixMatchType::Exact));

        // include super
        assert!(prefix_match(&p1, &p1_exact, &PrefixMatchType::IncludeSuper));
        assert!(!prefix_match(&p1, &p1_sub, &PrefixMatchType::IncludeSuper));
        assert!(prefix_match(&p1, &p1_super, &PrefixMatchType::IncludeSuper));
        assert!(!prefix_match(&p1, &p2, &PrefixMatchType::IncludeSuper));

        // include sub
        assert!(prefix_match(&p1, &p1_exact, &PrefixMatchType::IncludeSub));
        assert!(prefix_match(&p1, &p1_sub, &PrefixMatchType::IncludeSub));
        assert!(!prefix_match(&p1, &p1_super, &PrefixMatchType::IncludeSub));
        assert!(!prefix_match(&p1, &p2, &PrefixMatchType::IncludeSub));

        // include both
        assert!(prefix_match(
            &p1,
            &p1_exact,
            &PrefixMatchType::IncludeSuperSub
        ));
        assert!(prefix_match(
            &p1,
            &p1_sub,
            &PrefixMatchType::IncludeSuperSub
        ));
        assert!(prefix_match(
            &p1,
            &p1_super,
            &PrefixMatchType::IncludeSuperSub
        ));
        assert!(!prefix_match(&p1, &p2, &PrefixMatchType::IncludeSuperSub));
    }
}
