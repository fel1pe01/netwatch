use anyhow::anyhow;
use anyhow::Result;
use if_addrs::get_if_addrs;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::read_to_string;
use std::io;

#[derive(Debug, Serialize)]
pub struct LocalInterface {
    pub name: String,
    pub oper_state: Option<String>,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub mac: Option<String>,
    pub mtu: Option<u32>,
    pub evidence: &'static str, // "local_interface"
}

fn read_file_opt(path: &str) -> Result<Option<String>> {
    match read_to_string(path) {
        Ok(s) => Ok(Some(s.trim().to_string())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow!(e)),
    }
}

pub async fn discover_interfaces() -> Result<Vec<LocalInterface>> {
    let addrs = get_if_addrs()?;

    let mut map: HashMap<String, LocalInterface> = HashMap::new();

    for iface in addrs {
        let name = iface.name.clone();
        if !map.contains_key(&name) {
            let oper = read_file_opt(&format!("/sys/class/net/{}/operstate", name)).unwrap_or(None);
            let mac = read_file_opt(&format!("/sys/class/net/{}/address", name)).unwrap_or(None);
            let mtu = match read_file_opt(&format!("/sys/class/net/{}/mtu", name)) {
                Ok(opt) => opt.and_then(|s| s.parse::<u32>().ok()),
                Err(_) => None,
            };
            map.insert(
                name.clone(),
                LocalInterface {
                    name: name.clone(),
                    oper_state: oper,
                    ipv4: None,
                    ipv6: None,
                    mac,
                    mtu,
                    evidence: "local_interface",
                },
            );
        }
        let addr_ip = iface.addr.ip();
        match addr_ip {
            std::net::IpAddr::V4(ipv4) => {
                let entry = map.get_mut(&iface.name).unwrap();
                if entry.ipv4.is_none() {
                    entry.ipv4 = Some(ipv4.to_string());
                }
            }
            std::net::IpAddr::V6(ipv6) => {
                let entry = map.get_mut(&iface.name).unwrap();
                if entry.ipv6.is_none() {
                    entry.ipv6 = Some(ipv6.to_string());
                }
            }
        }
    }

    Ok(map.into_values().collect())
}
