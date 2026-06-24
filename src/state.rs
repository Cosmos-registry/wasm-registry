use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};

use crate::msg::{ChainMeta, EndpointKind, RegistryParams};

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
    pub verified: bool,
    pub preferred: bool,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const PARAMS: Item<RegistryParams> = Item::new("params");
pub const TREASURY_ACCUMULATED: Item<Uint128> = Item::new("treasury_accumulated");
pub const NEXT_ENDPOINT_ID: Item<u64> = Item::new("next_endpoint_id");

pub const CHAINS: Map<String, ChainRecord> = Map::new("chains");
pub const ENDPOINTS: Map<(String, u64), EndpointRecord> = Map::new("endpoints");
pub const ENDPOINT_URL_INDEX: Map<(String, String), u64> = Map::new("endpoint_url_index");
