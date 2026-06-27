use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: Option<String>,
    pub treasury: Option<String>,
    pub params: Option<RegistryParams>,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cw_serde]
pub enum ExecuteMsg {
    RegisterChain { chain: ChainMeta },
    UpdateChainMeta {
        chain_id: String,
        update: ChainMetaUpdate,
    },
    RegisterEndpoint {
        chain_id: String,
        endpoint: EndpointInput,
    },
    TopUpEndpoint {
        chain_id: String,
        endpoint_id: u64,
        amount: Uint128,
    },
    RemoveEndpoint {
        chain_id: String,
        endpoint_id: u64,
    },
    SetParams { params: ParamsUpdate },
    SetEndpointFlags {
        chain_id: String,
        endpoint_id: u64,
        verified: Option<bool>,
        preferred: Option<bool>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ChainResponse)]
    GetChain { chain_id: String },
    #[returns(ChainsResponse)]
    GetChains {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(EndpointsResponse)]
    GetEndpoints {
        chain_id: String,
        kind: Option<EndpointKind>,
        include_inactive: Option<bool>,
    },
    #[returns(ExportChainJsonResponse)]
    ExportChainJson { chain_id: String },
    #[returns(OwnerResponse)]
    GetOwner {},
    #[returns(RegistryParamsResponse)]
    GetParams {},
}

#[cw_serde]
pub enum NetworkType {
    Mainnet,
    Testnet,
    Devnet,
}

#[cw_serde]
pub struct Asset {
    pub denom: String,
    pub display: String,
    pub symbol: String,
    pub decimals: u8,
}

#[cw_serde]
pub struct Explorer {
    pub kind: String,
    pub url: String,
}

#[cw_serde]
pub struct ChainMeta {
    pub chain_id: String,
    pub chain_name: String,
    pub pretty_name: String,
    pub bech32_prefix: String,
    pub network_type: NetworkType,
    pub website: Option<String>,
    pub assets: Vec<Asset>,
    pub explorers: Vec<Explorer>,
}

#[cw_serde]
pub struct ChainMetaUpdate {
    pub chain_name: Option<String>,
    pub pretty_name: Option<String>,
    pub bech32_prefix: Option<String>,
    pub network_type: Option<NetworkType>,
    pub website: Option<String>,
    pub assets: Option<Vec<Asset>>,
    pub explorers: Option<Vec<Explorer>>,
}

#[cw_serde]
pub enum EndpointKind {
    Rpc,
    Rest,
    Grpc,
    Wss,
}

impl EndpointKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EndpointKind::Rpc => "rpc",
            EndpointKind::Rest => "rest",
            EndpointKind::Grpc => "grpc",
            EndpointKind::Wss => "wss",
        }
    }
}

#[cw_serde]
pub struct EndpointInput {
    pub kind: EndpointKind,
    pub url: String,
    pub deposit: Uint128,
    pub owner: Option<String>,
}

#[cw_serde]
pub struct EndpointView {
    pub endpoint_id: u64,
    pub chain_id: String,
    pub owner: String,
    pub kind: EndpointKind,
    pub url: String,
    pub normalized_url: String,
    pub verified: bool,
    pub preferred: bool,
    pub active: bool,
    pub remaining_deposit: Uint128,
    pub estimated_expiry: Option<u64>,
}

#[cw_serde]
pub struct ChainResponse {
    pub chain: Option<ChainMeta>,
    pub owner: Option<String>,
    pub endpoints: Vec<EndpointView>,
}

#[cw_serde]
pub struct ChainListItem {
    pub chain_id: String,
    pub pretty_name: String,
    pub network_type: NetworkType,
    pub endpoint_count: u32,
}

#[cw_serde]
pub struct ChainsResponse {
    pub chains: Vec<ChainListItem>,
}

#[cw_serde]
pub struct EndpointsResponse {
    pub endpoints: Vec<EndpointView>,
}

#[cw_serde]
pub struct OwnerResponse {
    pub owner: String,
    pub treasury: String,
}

#[cw_serde]
pub struct RegistryParams {
    pub min_endpoint_deposit: Uint128,
    pub rent_per_epoch: Uint128,
    pub epoch_seconds: u64,
    pub max_endpoints_per_chain: u32,
}

#[cw_serde]
pub struct ParamsUpdate {
    pub min_endpoint_deposit: Option<Uint128>,
    pub rent_per_epoch: Option<Uint128>,
    pub epoch_seconds: Option<u64>,
    pub max_endpoints_per_chain: Option<u32>,
}

#[cw_serde]
pub struct RegistryParamsResponse {
    pub params: RegistryParams,
}

#[cw_serde]
pub struct ApiEndpointEntry {
    pub address: String,
    pub provider: String,
}

#[cw_serde]
pub struct ChainJsonApis {
    pub rpc: Vec<ApiEndpointEntry>,
    pub rest: Vec<ApiEndpointEntry>,
    pub grpc: Vec<ApiEndpointEntry>,
    pub wss: Vec<ApiEndpointEntry>,
}

#[cw_serde]
pub struct ExportChainJsonResponse {
    pub chain_id: String,
    pub chain_name: String,
    pub pretty_name: String,
    pub bech32_prefix: String,
    pub network_type: NetworkType,
    pub website: Option<String>,
    pub assets: Vec<Asset>,
    pub apis: ChainJsonApis,
}
