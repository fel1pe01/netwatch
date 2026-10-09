use anyhow::{anyhow, Result};
use std::net::Ipv4Addr;

#[derive(Debug, serde::Serialize)]
pub struct RouteEntry {
    pub destination: String,
    pub gateway: Option<String>,
    pub interface: String,
    pub metric: Option<u32>,
    pub mask: Option<String>,
    pub evidence: &'static str, // "routing_table"
}

pub async fn discover_routes() -> Result<Vec<RouteEntry>> {
    let data = std::fs::read_to_string("/proc/net/route")?;
    let mut routes: Vec<RouteEntry> = Vec::new();

    for (i, line) in data.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }
        let iface = parts[0].to_string();
        let dest_hex = parts[1];
        let gateway_hex = parts[2];
        let mask_hex = parts[7];
        let metric_str = parts[8];

        let hex_to_u32 =
            |s: &str| u32::from_str_radix(s, 16).map_err(|e| anyhow!("hex parse error: {}", e));

        let dest_val = hex_to_u32(dest_hex)?;
        let dest_addr = Ipv4Addr::from(dest_val.to_le_bytes());
        let dest_str = dest_addr.to_string();

        let gateway = if gateway_hex != "00000000" {
            let gw_val = hex_to_u32(gateway_hex)?;
            Some(Ipv4Addr::from(gw_val.to_le_bytes()).to_string())
        } else {
            None
        };

        let mask = if mask_hex != "00000000" {
            let mask_val = hex_to_u32(mask_hex)?;
            let prefix_len = mask_val.leading_ones() as u8;
            Some(format!(
                "{}/{}",
                Ipv4Addr::from(mask_val.to_le_bytes()),
                prefix_len
            ))
        } else {
            None
        };

        let metric = metric_str.parse::<u32>().ok();

        routes.push(RouteEntry {
            destination: dest_str,
            gateway,
            interface: iface,
            metric,
            mask,
            evidence: "routing_table",
        });
    }
    Ok(routes)
}
