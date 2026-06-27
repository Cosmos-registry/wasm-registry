use cosmwasm_std::{
    to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
    Storage, Uint128,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;

use crate::error::ContractError;
use crate::msg::{
    ApiEndpointEntry, ChainJsonApis, ChainListItem, ChainMeta, ChainMetaUpdate, ChainResponse,
    ChainsResponse, EndpointInput, EndpointKind, EndpointView, EndpointsResponse, ExecuteMsg,
    ExportChainJsonResponse, InstantiateMsg, MigrateMsg, OwnerResponse, ParamsUpdate, QueryMsg,
    RegistryParams, RegistryParamsResponse,
};
use crate::state::{
    ChainRecord, Config, EndpointRecord, CHAINS, CONFIG, ENDPOINTS, ENDPOINT_URL_INDEX,
    NEXT_ENDPOINT_ID, PARAMS, TREASURY_ACCUMULATED,
};

const CONTRACT_NAME: &str = "crates.io:cosm_registry";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const NATIVE_DENOM: &str = "uatom";
const DEFAULT_MIN_DEPOSIT: u128 = 1_000;
const DEFAULT_RENT_PER_EPOCH: u128 = 10;
const DEFAULT_EPOCH_SECONDS: u64 = 3600;
const DEFAULT_MAX_ENDPOINTS_PER_CHAIN: u32 = 64;
const MAX_CHAIN_ID_LEN: usize = 64;
const MAX_TEXT_LEN: usize = 128;
const MAX_URL_LEN: usize = 200;
const MAX_ASSETS: usize = 64;
const MAX_EXPLORERS: usize = 32;
const DEFAULT_QUERY_LIMIT: u32 = 20;
const MAX_QUERY_LIMIT: u32 = 100;

pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let owner: Addr = match msg.owner {
        Some(owner) => deps.api.addr_validate(&owner)?,
        None => info.sender,
    };
    let treasury: Addr = match msg.treasury {
        Some(treasury) => deps.api.addr_validate(&treasury)?,
        None => owner.clone(),
    };

    let params = msg.params.unwrap_or(RegistryParams {
        min_endpoint_deposit: Uint128::new(DEFAULT_MIN_DEPOSIT),
        rent_per_epoch: Uint128::new(DEFAULT_RENT_PER_EPOCH),
        epoch_seconds: DEFAULT_EPOCH_SECONDS,
        max_endpoints_per_chain: DEFAULT_MAX_ENDPOINTS_PER_CHAIN,
    });
    validate_params(&params)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: owner.clone(),
            treasury: treasury.clone(),
        },
    )?;
    PARAMS.save(deps.storage, &params)?;
    NEXT_ENDPOINT_ID.save(deps.storage, &1)?;
    TREASURY_ACCUMULATED.save(deps.storage, &Uint128::zero())?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("owner", owner)
        .add_attribute("treasury", treasury)
        .add_attribute("block_time", env.block.time.seconds().to_string()))
}

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RegisterChain { chain } => execute_register_chain(deps, env, info, chain),
        ExecuteMsg::UpdateChainMeta { chain_id, update } => {
            execute_update_chain_meta(deps, env, info, chain_id, update)
        }
        ExecuteMsg::RegisterEndpoint { chain_id, endpoint } => {
            execute_register_endpoint(deps, env, info, chain_id, endpoint)
        }
        ExecuteMsg::TopUpEndpoint {
            chain_id,
            endpoint_id,
            amount,
        } => execute_top_up_endpoint(deps, env, info, chain_id, endpoint_id, amount),
        ExecuteMsg::RemoveEndpoint {
            chain_id,
            endpoint_id,
        } => execute_remove_endpoint(deps, env, info, chain_id, endpoint_id),
        ExecuteMsg::SetParams { params } => execute_set_params(deps, info, params),
        ExecuteMsg::SetEndpointFlags {
            chain_id,
            endpoint_id,
            verified,
            preferred,
        } => execute_set_endpoint_flags(deps, info, chain_id, endpoint_id, verified, preferred),
    }
}

pub fn migrate(deps: DepsMut, env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    // No state transformation needed in this migration; historical deposits are preserved as-is.
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("action", "migrate")
        .add_attribute("preserve_historical_deposits", "true")
        .add_attribute("block_time", env.block.time.seconds().to_string()))
}

fn execute_register_chain(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    chain: ChainMeta,
) -> Result<Response, ContractError> {
    validate_chain_meta(&chain)?;

    if CHAINS.may_load(deps.storage, chain.chain_id.clone())?.is_some() {
        return Err(ContractError::ChainAlreadyExists {
            chain_id: chain.chain_id,
        });
    }

    let chain_id = chain.chain_id.clone();
    CHAINS.save(
        deps.storage,
        chain_id.clone(),
        &ChainRecord {
            meta: chain,
            owner: info.sender,
            created_at: env.block.time.seconds(),
            updated_at: env.block.time.seconds(),
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "register_chain")
        .add_attribute("chain_id", chain_id))
}

fn execute_update_chain_meta(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    chain_id: String,
    update: ChainMetaUpdate,
) -> Result<Response, ContractError> {
    validate_chain_id(&chain_id)?;

    let config = CONFIG.load(deps.storage)?;
    let mut record = CHAINS
        .may_load(deps.storage, chain_id.clone())?
        .ok_or(ContractError::ChainNotFound {
            chain_id: chain_id.clone(),
        })?;

    if info.sender != record.owner && info.sender != config.owner {
        return Err(ContractError::Unauthorized);
    }

    apply_chain_update(&mut record.meta, update);
    validate_chain_meta(&record.meta)?;
    record.updated_at = env.block.time.seconds();
    CHAINS.save(deps.storage, chain_id.clone(), &record)?;

    Ok(Response::new()
        .add_attribute("action", "update_chain_meta")
        .add_attribute("chain_id", chain_id))
}

fn execute_register_endpoint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    chain_id: String,
    endpoint: EndpointInput,
) -> Result<Response, ContractError> {
    validate_chain_id(&chain_id)?;

    ensure_exact_native_deposit(&info, endpoint.deposit)?;

    if CHAINS.may_load(deps.storage, chain_id.clone())?.is_none() {
        return Err(ContractError::ChainNotFound { chain_id });
    }

    let params = PARAMS.load(deps.storage)?;
    if endpoint.deposit < params.min_endpoint_deposit {
        return Err(ContractError::DepositBelowMinimum);
    }

    let endpoint_count = ENDPOINTS
        .prefix(chain_id.clone())
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .count() as u32;
    if endpoint_count >= params.max_endpoints_per_chain {
        return Err(ContractError::MaxEndpointsPerChainExceeded);
    }

    let normalized_url = normalize_url(&endpoint.kind, &endpoint.url)?;
    let url_key = endpoint_index_key(&endpoint.kind, &normalized_url);
    if ENDPOINT_URL_INDEX
        .may_load(deps.storage, (chain_id.clone(), url_key.clone()))?
        .is_some()
    {
        return Err(ContractError::EndpointAlreadyExists);
    }

    let endpoint_owner = match endpoint.owner {
        Some(owner) => deps.api.addr_validate(&owner)?,
        None => info.sender,
    };

    let endpoint_id = NEXT_ENDPOINT_ID.load(deps.storage)?;
    NEXT_ENDPOINT_ID.save(deps.storage, &(endpoint_id + 1))?;

    ENDPOINTS.save(
        deps.storage,
        (chain_id.clone(), endpoint_id),
        &EndpointRecord {
            endpoint_id,
            chain_id: chain_id.clone(),
            owner: endpoint_owner,
            kind: endpoint.kind,
            url: endpoint.url,
            normalized_url,
            deposit: endpoint.deposit,
            last_charged_at: env.block.time.seconds(),
            active: true,
            verified: false,
            preferred: false,
        },
    )?;
    ENDPOINT_URL_INDEX.save(deps.storage, (chain_id.clone(), url_key), &endpoint_id)?;

    Ok(Response::new()
        .add_attribute("action", "register_endpoint")
        .add_attribute("chain_id", chain_id)
        .add_attribute("denom", NATIVE_DENOM)
        .add_attribute("endpoint_id", endpoint_id.to_string()))
}

fn execute_top_up_endpoint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    chain_id: String,
    endpoint_id: u64,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount.is_zero() {
        return Err(ContractError::InvalidField {
            field: "amount".to_string(),
        });
    }

    ensure_exact_native_deposit(&info, amount)?;

    let config = CONFIG.load(deps.storage)?;
    let mut endpoint = charge_endpoint(deps.storage, &chain_id, endpoint_id, env.block.time.seconds())?;
    if info.sender != endpoint.owner && info.sender != config.owner {
        return Err(ContractError::Unauthorized);
    }

    endpoint.deposit = endpoint
        .deposit
        .checked_add(amount)
        .map_err(|_| ContractError::Overflow)?;
    endpoint.active = true;
    ENDPOINTS.save(deps.storage, (chain_id.clone(), endpoint_id), &endpoint)?;

    Ok(Response::new()
        .add_attribute("action", "top_up_endpoint")
        .add_attribute("chain_id", chain_id)
        .add_attribute("endpoint_id", endpoint_id.to_string())
        .add_attribute("denom", NATIVE_DENOM)
        .add_attribute("amount", amount.to_string()))
}

fn execute_remove_endpoint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    chain_id: String,
    endpoint_id: u64,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let endpoint = charge_endpoint(deps.storage, &chain_id, endpoint_id, env.block.time.seconds())?;
    if info.sender != endpoint.owner && info.sender != config.owner {
        return Err(ContractError::Unauthorized);
    }

    let mut treasury = TREASURY_ACCUMULATED.load(deps.storage)?;
    treasury = treasury
        .checked_add(endpoint.deposit)
        .map_err(|_| ContractError::Overflow)?;
    TREASURY_ACCUMULATED.save(deps.storage, &treasury)?;

    ENDPOINTS.remove(deps.storage, (chain_id.clone(), endpoint_id));
    ENDPOINT_URL_INDEX.remove(
        deps.storage,
        (
            chain_id.clone(),
            endpoint_index_key(&endpoint.kind, &endpoint.normalized_url),
        ),
    );

    Ok(Response::new()
        .add_attribute("action", "remove_endpoint")
        .add_attribute("chain_id", chain_id)
        .add_attribute("endpoint_id", endpoint_id.to_string()))
}

fn execute_set_params(
    deps: DepsMut,
    info: MessageInfo,
    params_update: ParamsUpdate,
) -> Result<Response, ContractError> {
    require_admin(deps.as_ref(), &info.sender)?;

    let mut params = PARAMS.load(deps.storage)?;
    if let Some(value) = params_update.min_endpoint_deposit {
        params.min_endpoint_deposit = value;
    }
    if let Some(value) = params_update.rent_per_epoch {
        params.rent_per_epoch = value;
    }
    if let Some(value) = params_update.epoch_seconds {
        params.epoch_seconds = value;
    }
    if let Some(value) = params_update.max_endpoints_per_chain {
        params.max_endpoints_per_chain = value;
    }
    validate_params(&params)?;
    PARAMS.save(deps.storage, &params)?;

    Ok(Response::new().add_attribute("action", "set_params"))
}

fn execute_set_endpoint_flags(
    deps: DepsMut,
    info: MessageInfo,
    chain_id: String,
    endpoint_id: u64,
    verified: Option<bool>,
    preferred: Option<bool>,
) -> Result<Response, ContractError> {
    require_admin(deps.as_ref(), &info.sender)?;

    let mut endpoint = ENDPOINTS
        .may_load(deps.storage, (chain_id.clone(), endpoint_id))?
        .ok_or(ContractError::EndpointNotFound)?;
    if let Some(value) = verified {
        endpoint.verified = value;
    }
    if let Some(value) = preferred {
        endpoint.preferred = value;
    }
    ENDPOINTS.save(deps.storage, (chain_id.clone(), endpoint_id), &endpoint)?;

    Ok(Response::new()
        .add_attribute("action", "set_endpoint_flags")
        .add_attribute("chain_id", chain_id)
        .add_attribute("endpoint_id", endpoint_id.to_string()))
}

pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetChain { chain_id } => to_json_binary(&query_chain(deps, env, chain_id)?),
        QueryMsg::GetChains { start_after, limit } => {
            to_json_binary(&query_chains(deps, start_after, limit)?)
        }
        QueryMsg::GetEndpoints {
            chain_id,
            kind,
            include_inactive,
        } => to_json_binary(&query_endpoints(
            deps,
            env,
            chain_id,
            kind,
            include_inactive.unwrap_or(false),
        )?),
        QueryMsg::ExportChainJson { chain_id } => {
            to_json_binary(&query_export_chain_json(deps, env, chain_id)?)
        }
        QueryMsg::GetOwner {} => to_json_binary(&query_owner(deps)?),
        QueryMsg::GetParams {} => to_json_binary(&query_params(deps)?),
    }
}

fn query_chain(deps: Deps, env: Env, chain_id: String) -> StdResult<ChainResponse> {
    validate_chain_id(&chain_id).map_err(to_std_error)?;

    let maybe_chain = CHAINS.may_load(deps.storage, chain_id.clone())?;
    let Some(chain_record) = maybe_chain else {
        return Ok(ChainResponse {
            chain: None,
            owner: None,
            endpoints: vec![],
        });
    };

    let endpoints =
        collect_endpoint_views(deps, env.block.time.seconds(), &chain_id, None, false)
            .map_err(to_std_error)?;

    Ok(ChainResponse {
        chain: Some(chain_record.meta),
        owner: Some(chain_record.owner.to_string()),
        endpoints,
    })
}

fn query_chains(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<ChainsResponse> {
    let limit = limit.unwrap_or(DEFAULT_QUERY_LIMIT).min(MAX_QUERY_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    let mut chains = Vec::with_capacity(limit);
    for item in CHAINS
        .range(deps.storage, start, None, cosmwasm_std::Order::Ascending)
        .take(limit)
    {
        let (_key, record) = item?;
        let endpoint_count = ENDPOINTS
            .prefix(record.meta.chain_id.clone())
            .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .count() as u32;

        chains.push(ChainListItem {
            chain_id: record.meta.chain_id,
            pretty_name: record.meta.pretty_name,
            network_type: record.meta.network_type,
            endpoint_count,
        });
    }

    Ok(ChainsResponse { chains })
}

fn query_endpoints(
    deps: Deps,
    env: Env,
    chain_id: String,
    kind: Option<EndpointKind>,
    include_inactive: bool,
) -> StdResult<EndpointsResponse> {
    validate_chain_id(&chain_id).map_err(to_std_error)?;
    let endpoints = collect_endpoint_views(
        deps,
        env.block.time.seconds(),
        &chain_id,
        kind,
        include_inactive,
    )
    .map_err(to_std_error)?;
    Ok(EndpointsResponse { endpoints })
}

fn query_export_chain_json(
    deps: Deps,
    env: Env,
    chain_id: String,
) -> StdResult<ExportChainJsonResponse> {
    validate_chain_id(&chain_id).map_err(to_std_error)?;
    let chain_record = CHAINS
        .may_load(deps.storage, chain_id.clone())?
        .ok_or_else(|| {
            to_std_error(ContractError::ChainNotFound {
                chain_id: chain_id.clone(),
            })
        })?;

    let endpoints =
        collect_endpoint_views(deps, env.block.time.seconds(), &chain_id, None, false)
            .map_err(to_std_error)?;

    let mut apis = ChainJsonApis {
        rpc: vec![],
        rest: vec![],
        grpc: vec![],
        wss: vec![],
    };

    for endpoint in endpoints {
        let entry = ApiEndpointEntry {
            address: endpoint.url,
            provider: endpoint.owner,
        };
        match endpoint.kind {
            EndpointKind::Rpc => apis.rpc.push(entry),
            EndpointKind::Rest => apis.rest.push(entry),
            EndpointKind::Grpc => apis.grpc.push(entry),
            EndpointKind::Wss => apis.wss.push(entry),
        }
    }

    Ok(ExportChainJsonResponse {
        chain_id: chain_record.meta.chain_id,
        chain_name: chain_record.meta.chain_name,
        pretty_name: chain_record.meta.pretty_name,
        bech32_prefix: chain_record.meta.bech32_prefix,
        network_type: chain_record.meta.network_type,
        website: chain_record.meta.website,
        assets: chain_record.meta.assets,
        apis,
    })
}

fn query_owner(deps: Deps) -> StdResult<OwnerResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(OwnerResponse {
        owner: config.owner.to_string(),
        treasury: config.treasury.to_string(),
    })
}

fn query_params(deps: Deps) -> StdResult<RegistryParamsResponse> {
    let params = PARAMS.load(deps.storage)?;
    Ok(RegistryParamsResponse { params })
}

fn collect_endpoint_views(
    deps: Deps,
    now: u64,
    chain_id: &str,
    kind_filter: Option<EndpointKind>,
    include_inactive: bool,
) -> Result<Vec<EndpointView>, ContractError> {
    let params = PARAMS.load(deps.storage)?;
    let mut endpoints = vec![];

    for item in ENDPOINTS
        .prefix(chain_id.to_string())
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
    {
        let (_id, endpoint) = item?;

        if let Some(kind_filter) = kind_filter.clone() {
            if endpoint.kind != kind_filter {
                continue;
            }
        }

        let (remaining_deposit, active, estimated_expiry) =
            simulate_endpoint_state(&endpoint, &params, now)?;
        if !include_inactive && !active {
            continue;
        }

        endpoints.push(EndpointView {
            endpoint_id: endpoint.endpoint_id,
            chain_id: endpoint.chain_id,
            owner: endpoint.owner.to_string(),
            kind: endpoint.kind,
            url: endpoint.url,
            normalized_url: endpoint.normalized_url,
            verified: endpoint.verified,
            preferred: endpoint.preferred,
            active,
            remaining_deposit,
            estimated_expiry,
        });
    }

    Ok(endpoints)
}

fn apply_chain_update(meta: &mut ChainMeta, update: ChainMetaUpdate) {
    if let Some(chain_name) = update.chain_name {
        meta.chain_name = chain_name;
    }
    if let Some(pretty_name) = update.pretty_name {
        meta.pretty_name = pretty_name;
    }
    if let Some(bech32_prefix) = update.bech32_prefix {
        meta.bech32_prefix = bech32_prefix;
    }
    if let Some(network_type) = update.network_type {
        meta.network_type = network_type;
    }
    if let Some(website) = update.website {
        meta.website = Some(website);
    }
    if let Some(assets) = update.assets {
        meta.assets = assets;
    }
    if let Some(explorers) = update.explorers {
        meta.explorers = explorers;
    }
}

fn validate_chain_meta(chain: &ChainMeta) -> Result<(), ContractError> {
    validate_chain_id(&chain.chain_id)?;
    validate_text_field("chain_name", &chain.chain_name)?;
    validate_text_field("pretty_name", &chain.pretty_name)?;
    validate_text_field("bech32_prefix", &chain.bech32_prefix)?;

    if let Some(website) = &chain.website {
        let _ = normalize_http_url(website)?;
    }

    if chain.assets.is_empty() || chain.assets.len() > MAX_ASSETS {
        return Err(ContractError::InvalidField {
            field: "assets".to_string(),
        });
    }
    for asset in &chain.assets {
        validate_text_field("asset.denom", &asset.denom)?;
        validate_text_field("asset.display", &asset.display)?;
        validate_text_field("asset.symbol", &asset.symbol)?;
        if asset.decimals > 30 {
            return Err(ContractError::InvalidField {
                field: "asset.decimals".to_string(),
            });
        }
    }

    if chain.explorers.len() > MAX_EXPLORERS {
        return Err(ContractError::InvalidField {
            field: "explorers".to_string(),
        });
    }
    for explorer in &chain.explorers {
        validate_text_field("explorer.kind", &explorer.kind)?;
        let _ = normalize_http_url(&explorer.url)?;
    }

    Ok(())
}

fn validate_chain_id(chain_id: &str) -> Result<(), ContractError> {
    if chain_id.len() < 3 || chain_id.len() > MAX_CHAIN_ID_LEN {
        return Err(ContractError::InvalidChainId);
    }
    if !chain_id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ContractError::InvalidChainId);
    }
    Ok(())
}

fn validate_text_field(field: &str, value: &str) -> Result<(), ContractError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_TEXT_LEN {
        return Err(ContractError::InvalidField {
            field: field.to_string(),
        });
    }
    Ok(())
}

fn validate_params(params: &RegistryParams) -> Result<(), ContractError> {
    if params.epoch_seconds == 0 || params.max_endpoints_per_chain == 0 {
        return Err(ContractError::InvalidParams);
    }
    if params.rent_per_epoch.is_zero() || params.min_endpoint_deposit.is_zero() {
        return Err(ContractError::InvalidParams);
    }
    Ok(())
}

fn require_admin(deps: Deps, sender: &Addr) -> Result<(), ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if sender != &config.owner {
        return Err(ContractError::Unauthorized);
    }
    Ok(())
}

fn normalize_url(kind: &EndpointKind, url: &str) -> Result<String, ContractError> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_URL_LEN {
        return Err(ContractError::InvalidEndpointUrl);
    }

    let lower = trimmed.to_ascii_lowercase();
    let valid_scheme = match kind {
        EndpointKind::Rpc | EndpointKind::Rest | EndpointKind::Grpc => {
            lower.starts_with("http://") || lower.starts_with("https://")
        }
        EndpointKind::Wss => lower.starts_with("ws://") || lower.starts_with("wss://"),
    };
    if !valid_scheme {
        return Err(ContractError::InvalidEndpointScheme);
    }
    if lower.contains(' ') {
        return Err(ContractError::InvalidEndpointUrl);
    }

    let mut normalized = trimmed.to_string();
    if normalized.ends_with('/') {
        normalized.pop();
    }

    Ok(normalized)
}

fn normalize_http_url(url: &str) -> Result<String, ContractError> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_URL_LEN {
        return Err(ContractError::InvalidEndpointUrl);
    }
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(ContractError::InvalidEndpointScheme);
    }
    if lower.contains(' ') {
        return Err(ContractError::InvalidEndpointUrl);
    }
    Ok(trimmed.to_string())
}

fn endpoint_index_key(kind: &EndpointKind, normalized_url: &str) -> String {
    format!("{}|{}", kind.as_str(), normalized_url.to_ascii_lowercase())
}

fn charge_endpoint(
    storage: &mut dyn Storage,
    chain_id: &str,
    endpoint_id: u64,
    now: u64,
) -> Result<EndpointRecord, ContractError> {
    let mut endpoint = ENDPOINTS
        .may_load(storage, (chain_id.to_string(), endpoint_id))?
        .ok_or(ContractError::EndpointNotFound)?;
    let params = PARAMS.load(storage)?;

    if endpoint.active {
        let elapsed = now.saturating_sub(endpoint.last_charged_at);
        let epochs = elapsed / params.epoch_seconds;
        if epochs > 0 {
            let due = params
                .rent_per_epoch
                .checked_mul(Uint128::from(epochs as u128))
                .map_err(|_| ContractError::Overflow)?;
            let charged = due.min(endpoint.deposit);
            endpoint.deposit = endpoint
                .deposit
                .checked_sub(charged)
                .map_err(|_| ContractError::Overflow)?;
            endpoint.last_charged_at = endpoint
                .last_charged_at
                .saturating_add(epochs.saturating_mul(params.epoch_seconds));
            if endpoint.deposit.is_zero() {
                endpoint.active = false;
            }

            let mut treasury = TREASURY_ACCUMULATED.load(storage)?;
            treasury = treasury
                .checked_add(charged)
                .map_err(|_| ContractError::Overflow)?;
            TREASURY_ACCUMULATED.save(storage, &treasury)?;
        }
    }

    ENDPOINTS.save(storage, (chain_id.to_string(), endpoint_id), &endpoint)?;
    Ok(endpoint)
}

fn simulate_endpoint_state(
    endpoint: &EndpointRecord,
    params: &RegistryParams,
    now: u64,
) -> Result<(Uint128, bool, Option<u64>), ContractError> {
    if !endpoint.active {
        return Ok((endpoint.deposit, false, None));
    }

    let elapsed = now.saturating_sub(endpoint.last_charged_at);
    let epochs = elapsed / params.epoch_seconds;
    let due = params
        .rent_per_epoch
        .checked_mul(Uint128::from(epochs as u128))
        .map_err(|_| ContractError::Overflow)?;
    let remaining = endpoint.deposit.saturating_sub(due);
    let active = !remaining.is_zero();

    let estimated_expiry = if active {
        let remaining_epochs = remaining
            .u128()
            .checked_div(params.rent_per_epoch.u128())
            .unwrap_or(0);
        Some(
            endpoint.last_charged_at.saturating_add(
                (epochs + remaining_epochs as u64).saturating_mul(params.epoch_seconds),
            ),
        )
    } else {
        None
    };

    Ok((remaining, active, estimated_expiry))
}

fn to_std_error(err: ContractError) -> StdError {
    StdError::generic_err(err.to_string())
}

fn ensure_exact_native_deposit(info: &MessageInfo, expected: Uint128) -> Result<(), ContractError> {
    let mut total = Uint128::zero();

    for coin in &info.funds {
        if coin.denom != NATIVE_DENOM {
            return Err(ContractError::InvalidFundsDenom);
        }
        total = total.checked_add(coin.amount).map_err(|_| ContractError::Overflow)?;
    }

    if total != expected {
        return Err(ContractError::InvalidFundsAmount);
    }

    Ok(())
}
