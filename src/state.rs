use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

use crate::msg::{
    ChainMeta, EndpointKind, EndpointStatus, RegistryParams, VerificationState,
};

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub treasury: Addr,
}

#[cw_serde]
pub struct ChainRecord {
    pub meta: ChainMeta,
    pub owner: Addr,
    pub created_at: u64,
    pub updated_at: u64,
}

#[cw_serde]
pub struct EndpointRecord {
    pub endpoint_id: u64,
    pub chain_id: String,
    pub owner: Addr,
    pub kind: EndpointKind,
    pub url: String,
    pub normalized_url: String,
    pub deposit: Uint128,
    pub last_charged_at: u64,
    pub active: bool,
    #[serde(default)]
    pub verification: EndpointVerification,
    pub preferred: bool,
}

#[cw_serde]
pub struct EndpointVerification {
    pub verification_state: VerificationState,
    pub last_status: Option<EndpointStatus>,
    pub last_latency_ms: Option<u32>,
    pub last_checked_at: Option<u64>,
    pub last_checked_by: Option<Addr>,
    pub last_success_at: Option<u64>,
    pub consecutive_successes: u32,
    pub consecutive_failures: u32,
}

impl Default for EndpointVerification {
    fn default() -> Self {
        Self {
            verification_state: VerificationState::Unverified,
            last_status: None,
            last_latency_ms: None,
            last_checked_at: None,
            last_checked_by: None,
            last_success_at: None,
            consecutive_successes: 0,
            consecutive_failures: 0,
        }
    }
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const PARAMS: Item<RegistryParams> = Item::new("params");
pub const TREASURY_ACCUMULATED: Item<Uint128> = Item::new("treasury_accumulated");
pub const NEXT_ENDPOINT_ID: Item<u64> = Item::new("next_endpoint_id");

pub const CHAINS: Map<String, ChainRecord> = Map::new("chains");
pub const ENDPOINTS: Map<(String, u64), EndpointRecord> = Map::new("endpoints");
pub const ENDPOINT_URL_INDEX: Map<(String, String), u64> = Map::new("endpoint_url_index");
