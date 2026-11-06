//! Network calculations. These only do arithmetic: nothing goes on the network.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use saqviro_jobs::Params;

fn v4(address: Ipv4Addr, prefix: u32) -> String {
    let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
    let network = u32::from(address) & mask;
    let broadcast = network | !mask;
    let ip = Ipv4Addr::from;
    let mut out = format!(
        "Network: {}/{prefix}\nMask: {}\nWildcard: {}\n",
        ip(network),
        ip(mask),
        ip(!mask)
    );
    match prefix {
        32 => out.push_str(&format!("Host: {}\nAddresses: 1", ip(network))),
        31 => out.push_str(&format!("Hosts: {} – {}\nAddresses: 2", ip(network), ip(broadcast))),
        _ => {
            let total = 1u64 << (32 - prefix);
            out.push_str(&format!(
                "Broadcast: {}\nHosts: {} – {}\nUsable hosts: {}\nAddresses: {total}",
                ip(broadcast),
                ip(network + 1),
                ip(broadcast - 1),
                total - 2,
            ));
        }
    }
    let kind = if address.is_private() {
        "private"
    } else if address.is_loopback() {
        "loopback"
    } else if address.is_link_local() {
        "link-local"
    } else if address.is_multicast() {
        "multicast"
    } else {
        "public"
    };
    format!("{out}\nType: {kind}")
}

fn v6(address: Ipv6Addr, prefix: u32) -> String {
    let mask = if prefix == 0 { 0 } else { u128::MAX << (128 - prefix) };
    let network = u128::from(address) & mask;
    let last = network | !mask;
    let ip = Ipv6Addr::from;
    let count = if prefix == 0 {
        "2^128".to_owned()
    } else {
        format!("2^{}", 128 - prefix)
    };
    format!(
        "Network: {}/{prefix}\nFirst: {}\nLast: {}\nAddresses: {count}",
        ip(network),
        ip(network),
        ip(last)
    )
}

pub fn cidr(text: &str, _: &Params) -> Result<String, String> {
    let text = text.trim();
    let (address, prefix) = text.split_once('/').map_or((text, None), |(a, p)| (a, Some(p)));
    let address: IpAddr = address
        .parse()
        .map_err(|_| format!("`{address}` is not an IP address"))?;
    let longest = if address.is_ipv4() { 32 } else { 128 };
    let prefix = match prefix {
        None => longest,
        Some(p) => match p.parse::<u32>() {
            Ok(n) if n <= longest => n,
            _ => match (address, p.parse::<Ipv4Addr>()) {
                // A mask written out, like 255.255.255.0.
                (IpAddr::V4(_), Ok(mask))
                    if u32::from(mask).leading_ones() + u32::from(mask).trailing_zeros() == 32 =>
                {
                    u32::from(mask).leading_ones()
                }
                _ => return Err(format!("`{p}` is not a prefix from 0 to {longest}")),
            },
        },
    };
    Ok(match address {
        IpAddr::V4(a) => v4(a, prefix),
        IpAddr::V6(a) => v6(a, prefix),
    })
}
