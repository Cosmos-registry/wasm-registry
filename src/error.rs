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
}
