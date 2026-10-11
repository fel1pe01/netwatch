use anyhow::Result;
use async_snmp::{oid, Auth, AuthProtocol, Client, Oid, PrivProtocol, Retry};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnmpCredentials {
    pub version: SnmpVersion,
    pub community: Option<String>,     // v2c
    pub username: Option<String>,      // v3
    pub auth_protocol: Option<String>, // v3: SHA, MD5
    pub auth_password: Option<String>, // v3
    pub priv_protocol: Option<String>, // v3: AES, DES
    pub priv_password: Option<String>, // v3
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SnmpVersion {
    V2c,
    V3,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnmpTarget {
    pub id: String,
    pub address: IpAddr,
    pub port: u16,
    pub credentials: SnmpCredentials,
    pub timeout_ms: u64,
    pub retries: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InterfaceMetrics {
    pub if_index: u32,
    pub if_name: String,
    pub if_descr: String,
    pub if_type: u32,
    pub if_mtu: u32,
    pub if_speed: u64,
    pub if_admin_status: u32,
    pub if_oper_status: u32,
    pub if_in_octets: u64,
    pub if_out_octets: u64,
    pub if_in_errors: u64,
    pub if_out_errors: u64,
    pub if_in_discards: u64,
    pub if_out_discards: u64,
    pub if_in_ucast_pkts: u64,
    pub if_out_ucast_pkts: u64,
    pub if_high_speed: Option<u64>,
    pub if_promiscuous_mode: Option<u32>,
    pub if_connector_present: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceSystemInfo {
    pub sys_descr: String,
    pub sys_object_id: String,
    pub sys_up_time: u64,
    pub sys_contact: Option<String>,
    pub sys_name: Option<String>,
    pub sys_location: Option<String>,
    pub sys_services: u32,
}

/// SNMP Collector - real implementation using async-snmp crate
pub struct SnmpCollector {
    semaphore: Arc<Semaphore>,
    #[allow(dead_code)]
    max_concurrent: usize,
    #[allow(dead_code)]
    rate_limit_per_second: u32,
}

impl SnmpCollector {
    pub fn new(max_concurrent: usize, rate_limit_per_second: u32) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
            rate_limit_per_second,
        }
    }

    fn build_auth(&self, target: &SnmpTarget) -> Result<Auth> {
        match &target.credentials.version {
            SnmpVersion::V2c => {
                let community = target
                    .credentials
                    .community
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Community string required for SNMPv2c"))?;
                Ok(Auth::v2c(community))
            }
            SnmpVersion::V3 => {
                let username = target
                    .credentials
                    .username
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Username required for SNMPv3"))?
                    .to_string();
                let auth_protocol = target
                    .credentials
                    .auth_protocol
                    .as_deref()
                    .unwrap_or("SHA256");
                let auth_password = target
                    .credentials
                    .auth_password
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Auth password required for SNMPv3"))?;
                let priv_protocol = target
                    .credentials
                    .priv_protocol
                    .as_deref()
                    .unwrap_or("AES128");
                let priv_password = target
                    .credentials
                    .priv_password
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("Priv password required for SNMPv3"))?;

                let auth_protocol = match auth_protocol.to_uppercase().as_str() {
                    "SHA" | "SHA1" => AuthProtocol::Sha1,
                    "SHA256" => AuthProtocol::Sha256,
                    "SHA384" => AuthProtocol::Sha384,
                    "SHA512" => AuthProtocol::Sha512,
                    "MD5" => AuthProtocol::Md5,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Unsupported auth protocol: {}",
                            auth_protocol
                        ))
                    }
                };

                let priv_protocol = match priv_protocol.to_uppercase().as_str() {
                    "AES" | "AES128" => PrivProtocol::Aes128,
                    "AES192" => PrivProtocol::Aes192Blumenthal,
                    "AES256" => PrivProtocol::Aes256Blumenthal,
                    "DES" => PrivProtocol::Des,
                    "3DES" | "DES3" => PrivProtocol::Des3,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Unsupported priv protocol: {}",
                            priv_protocol
                        ))
                    }
                };

                // Use UsmConfig for SNMPv3 with authPriv
                let usm_config = async_snmp::UsmConfig::new(username)
                    .auth_priv(auth_protocol, auth_password, priv_protocol, priv_password)
                    .map_err(|e| anyhow::anyhow!("Failed to create USM config: {}", e))?;

                Ok(Auth::from(usm_config))
            }
        }
    }

    async fn build_client(&self, target: &SnmpTarget) -> Result<Client> {
        let auth = self.build_auth(target)?;

        let addr_str = format!("{}:{}", target.address, target.port);
        let _retries = Retry::fixed(target.retries, Duration::from_millis(100))?;

        let client = Client::builder(addr_str, auth)
            .request_timeout(Duration::from_millis(target.timeout_ms))
            .retry(Retry::fixed(target.retries, Duration::from_millis(100))?)
            .connect()
            .await?;
        Ok(client)
    }

    pub async fn poll_device(&self, target: &SnmpTarget) -> Result<DeviceSystemInfo> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        let client = self.build_client(target).await?;

        let sys_descr = self
            .get_string(&client, oid!(1, 3, 6, 1, 2, 1, 1, 1, 0))
            .await?;
        let sys_object_id = self
            .get_string(&client, oid!(1, 3, 6, 1, 2, 1, 1, 2, 0))
            .await?;
        let sys_up_time = self
            .get_u64(&client, oid!(1, 3, 6, 1, 2, 1, 1, 3, 0))
            .await?;
        let sys_contact = self
            .get_string_opt(&client, oid!(1, 3, 6, 1, 2, 1, 1, 4, 0))
            .await?;
        let sys_name = self
            .get_string_opt(&client, oid!(1, 3, 6, 1, 2, 1, 1, 5, 0))
            .await?;
        let sys_location = self
            .get_string_opt(&client, oid!(1, 3, 6, 1, 2, 1, 1, 6, 0))
            .await?;
        let sys_services = self
            .get_u32(&client, oid!(1, 3, 6, 1, 2, 1, 1, 7, 0))
            .await?;

        Ok(DeviceSystemInfo {
            sys_descr,
            sys_object_id,
            sys_up_time,
            sys_contact,
            sys_name,
            sys_location,
            sys_services,
        })
    }

    pub async fn poll_interfaces(&self, target: &SnmpTarget) -> Result<Vec<InterfaceMetrics>> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

        let client = self.build_client(target).await?;

        // Get ifNumber first
        let if_number = self
            .get_u32(&client, oid!(1, 3, 6, 1, 2, 1, 2, 1, 0))
            .await? as usize;

        let mut interfaces = Vec::new();

        for i in 1..=if_number {
            let if_index = i as u32;
            let idx = i as u32;

            let if_name = self
                .get_string_opt(&client, oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, idx))
                .await?;
            let if_descr = self
                .get_string(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 2, idx))
                .await?;
            let if_type = self
                .get_u32(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 3, idx))
                .await?;
            let if_mtu = self
                .get_u32(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 4, idx))
                .await?;
            let if_speed = self
                .get_u64(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 5, idx))
                .await?;
            let if_admin_status = self
                .get_u32(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 7, idx))
                .await?;
            let if_oper_status = self
                .get_u32(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 8, idx))
                .await?;

            // 32-bit counters
            let if_in_octets = self
                .get_u64(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 10, idx))
                .await?;
            let if_out_octets = self
                .get_u64(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 16, idx))
                .await?;
            let if_in_errors = self
                .get_u64(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 14, idx))
                .await?;
            let if_out_errors = self
                .get_u64(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 20, idx))
                .await?;
            let if_in_discards = self
                .get_u64(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 13, idx))
                .await?;
            let if_out_discards = self
                .get_u64(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 19, idx))
                .await?;
            let if_in_ucast_pkts = self
                .get_u64(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 11, idx))
                .await?;
            let if_out_ucast_pkts = self
                .get_u64(&client, oid!(1, 3, 6, 1, 2, 1, 2, 2, 1, 17, idx))
                .await?;

            // 64-bit counters (ifXTable)
            let if_high_speed = self
                .get_u64_opt(&client, oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 15, idx))
                .await?;
            let if_promiscuous_mode = self
                .get_u32_opt(&client, oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 16, idx))
                .await?;
            let if_connector_present = self
                .get_u32_opt(&client, oid!(1, 3, 6, 1, 2, 1, 31, 1, 1, 1, 17, idx))
                .await?;

            interfaces.push(InterfaceMetrics {
                if_index,
                if_name: if_name.unwrap_or_default(),
                if_descr,
                if_type,
                if_mtu,
                if_speed,
                if_admin_status,
                if_oper_status,
                if_in_octets,
                if_out_octets,
                if_in_errors,
                if_out_errors,
                if_in_discards,
                if_out_discards,
                if_in_ucast_pkts,
                if_out_ucast_pkts,
                if_high_speed,
                if_promiscuous_mode,
                if_connector_present,
            });
        }

        Ok(interfaces)
    }

    async fn get_string(&self, client: &Client, oid: Oid) -> Result<String> {
        let response = client
            .get(&oid)
            .await
            .map_err(|e| anyhow::anyhow!("SNMP get failed: {}", e))?;

        let binding = response
            .varbinds
            .first()
            .ok_or_else(|| anyhow::anyhow!("No binding in response"))?;

        Ok(binding.value.to_string())
    }

    async fn get_string_opt(&self, client: &Client, oid: Oid) -> Result<Option<String>> {
        match self.get_string(client, oid).await {
            Ok(s) if !s.is_empty() => Ok(Some(s)),
            Ok(_) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    async fn get_u32(&self, client: &Client, oid: Oid) -> Result<u32> {
        let response = client
            .get(&oid)
            .await
            .map_err(|e| anyhow::anyhow!("SNMP get failed: {}", e))?;

        let binding = response
            .varbinds
            .first()
            .ok_or_else(|| anyhow::anyhow!("No binding in response"))?;

        let val_str = binding.value.to_string();
        val_str
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse u32: {}", e))
    }

    async fn get_u32_opt(&self, client: &Client, oid: Oid) -> Result<Option<u32>> {
        match self.get_u32(client, oid).await {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }

    async fn get_u64(&self, client: &Client, oid: Oid) -> Result<u64> {
        let response = client
            .get(&oid)
            .await
            .map_err(|e| anyhow::anyhow!("SNMP get failed: {}", e))?;

        let binding = response
            .varbinds
            .first()
            .ok_or_else(|| anyhow::anyhow!("No binding in response"))?;

        let val_str = binding.value.to_string();
        val_str
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse u64: {}", e))
    }

    async fn get_u64_opt(&self, client: &Client, oid: Oid) -> Result<Option<u64>> {
        match self.get_u64(client, oid).await {
            Ok(v) => Ok(Some(v)),
            Err(_) => Ok(None),
        }
    }
}

// Rate calculator for counter deltas
pub struct RateCalculator {
    previous_values: HashMap<String, (u64, DateTime<Utc>)>,
}

impl RateCalculator {
    pub fn new() -> Self {
        Self {
            previous_values: HashMap::new(),
        }
    }

    pub fn calculate_rate(
        &mut self,
        key: String,
        current_value: u64,
        timestamp: DateTime<Utc>,
    ) -> Option<f64> {
        if let Some((prev_value, prev_time)) = self.previous_values.get(&key) {
            let time_diff = (timestamp - *prev_time).num_seconds() as f64;
            if time_diff > 0.0 {
                let value_diff = if current_value >= *prev_value {
                    current_value - prev_value
                } else {
                    // Handle counter wrap (32-bit or 64-bit)
                    (u64::MAX - *prev_value) + current_value + 1
                };
                let rate = value_diff as f64 / time_diff;
                self.previous_values.insert(key, (current_value, timestamp));
                return Some(rate);
            }
        }
        self.previous_values.insert(key, (current_value, timestamp));
        None
    }
}

impl Default for RateCalculator {
    fn default() -> Self {
        Self::new()
    }
}
