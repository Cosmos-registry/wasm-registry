use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("unauthorized")]
    Unauthorized,

    #[error("chain already exists: {chain_id}")]
    ChainAlreadyExists { chain_id: String },

    #[error("chain not found: {chain_id}")]
    ChainNotFound { chain_id: String },

    #[error("invalid chain id")]
    InvalidChainId,

    #[error("invalid field: {field}")]
    InvalidField { field: String },

    #[error("endpoint already exists for this chain and kind")]
    EndpointAlreadyExists,

    #[error("endpoint not found")]
    EndpointNotFound,

    #[error("invalid endpoint url")]
    InvalidEndpointUrl,

    #[error("invalid endpoint scheme for type")]
    InvalidEndpointScheme,

    #[error("deposit below minimum")]
    DepositBelowMinimum,

    #[error("invalid funds denom: only uatom is accepted")]
    InvalidFundsDenom,

    #[error("invalid funds amount for deposit/top-up")]
    InvalidFundsAmount,

    #[error("max endpoints per chain exceeded")]
    MaxEndpointsPerChainExceeded,

    #[error("invalid params")]
    InvalidParams,

    #[error("overflow in arithmetic")]
    Overflow,

    #[error("sender not in trusted oracle whitelist")]
    UnauthorizedOracle,

    #[error("oracle batch exceeds max size: {max}")]
    OracleBatchTooLarge { max: u32 },

    #[error("oracle batch must not be empty")]
    EmptyOracleBatch,

    #[error("duplicate endpoint in batch: {endpoint_id}")]
    DuplicateEndpointInBatch { endpoint_id: u64 },

    #[error("endpoint {endpoint_id} does not belong to chain {chain_id}")]
    EndpointNotInChain { endpoint_id: u64, chain_id: String },

    #[error("online observation must include latency_ms")]
    MissingLatencyForOnline,

    #[error("offline observation must not include latency_ms")]
    InvalidLatencyForOffline,

    #[error("query filters verification_state and only_unverified conflict")]
    ConflictingEndpointFilters,
}
