use cosmwasm_std::{
    to_json_binary, Addr, Api, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Storage, Uint128,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;
use std::collections::HashSet;

use crate::error::ContractError;
use crate::msg::{
    ApiEndpointEntry, ChainJsonApis, ChainListItem, ChainMeta, ChainMetaUpdate, ChainResponse,
    ChainsResponse, EndpointInput, EndpointKind, EndpointObservationInput, EndpointStatus,
    EndpointView, EndpointsResponse, ExecuteMsg, ExportChainJsonResponse, InstantiateMsg,
    MigrateMsg, OwnerResponse, ParamsUpdate, QueryMsg, RegistryParams, RegistryParamsResponse,
    VerificationState,
};
use crate::state::{
    ChainRecord, Config, EndpointRecord, EndpointVerification, CHAINS, CONFIG, ENDPOINTS,
    ENDPOINT_URL_INDEX, NEXT_ENDPOINT_ID, PARAMS, TREASURY_ACCUMULATED,
};

const CONTRACT_NAME: &str = "crates.io:cosm_registry";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const NATIVE_DENOM: &str = "uatom";
const DEFAULT_MIN_DEPOSIT: u128 = 1_000;
const DEFAULT_RENT_PER_EPOCH: u128 = 10;
const DEFAULT_EPOCH_SECONDS: u64 = 3600;
const DEFAULT_MAX_ENDPOINTS_PER_CHAIN: u32 = 64;
const DEFAULT_ORACLE_MAX_BATCH_SIZE: u32 = 100;
const DEFAULT_AUTO_UNVERIFY_FAILURE_STREAK: u32 = 5;
const DEFAULT_AUTO_UNVERIFY_LAST_SUCCESS_OLDER_THAN_SECS: u64 = 432_000;
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
        trusted_oracles: vec![],
        oracle_max_batch_size: DEFAULT_ORACLE_MAX_BATCH_SIZE,
        auto_unverify_failure_streak: DEFAULT_AUTO_UNVERIFY_FAILURE_STREAK,
        auto_unverify_last_success_older_than_secs:
            DEFAULT_AUTO_UNVERIFY_LAST_SUCCESS_OLDER_THAN_SECS,
    });
    let params = validated_params(deps.api, params)?;

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
        ExecuteMsg::SubmitEndpointStatuses {
            chain_id,
            observations,
        } => execute_submit_endpoint_statuses(deps, env, info, chain_id, observations),
    }
}

pub fn migrate(deps: DepsMut, env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let legacy_params_item: cw_storage_plus::Item<LegacyRegistryParams> =
        cw_storage_plus::Item::new("params");

    if let Ok(legacy_params) = legacy_params_item.load(deps.storage) {
        let config = CONFIG.load(deps.storage)?;
        let new_params = RegistryParams {
            min_endpoint_deposit: legacy_params.min_endpoint_deposit,
            rent_per_epoch: legacy_params.rent_per_epoch,
            epoch_seconds: legacy_params.epoch_seconds,
            max_endpoints_per_chain: legacy_params.max_endpoints_per_chain,
            trusted_oracles: vec![config.owner.to_string()],
            oracle_max_batch_size: DEFAULT_ORACLE_MAX_BATCH_SIZE,
            auto_unverify_failure_streak: DEFAULT_AUTO_UNVERIFY_FAILURE_STREAK,
            auto_unverify_last_success_older_than_secs:
                DEFAULT_AUTO_UNVERIFY_LAST_SUCCESS_OLDER_THAN_SECS,
        };
        PARAMS.save(deps.storage, &new_params)?;
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("action", "migrate")
        .add_attribute("migration_mode", "lazy_endpoint_compat")
        .add_attribute("migrated_endpoints", "0")
        .add_attribute("preserve_historical_deposits", "true")
        .add_attribute("block_time", env.block.time.seconds().to_string()))
}

#[cfg(test)]
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
struct LegacyEndpointRecord {
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
#[derive(serde::Serialize, serde::Deserialize)]
struct LegacyRegistryParams {
    pub min_endpoint_deposit: Uint128,
    pub rent_per_epoch: Uint128,
    pub epoch_seconds: u64,
    pub max_endpoints_per_chain: u32,
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
            verification: EndpointVerification::default(),
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
    let mut endpoint = get_endpoint(deps.storage, &chain_id, endpoint_id)?;
    if info.sender != endpoint.owner && info.sender != config.owner {
        return Err(ContractError::Unauthorized);
    }
    let charged = charge_endpoint(deps.storage, &mut endpoint, env.block.time.seconds())?;
    apply_treasury_delta(deps.storage, charged)?;
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
    let mut endpoint = get_endpoint(deps.storage, &chain_id, endpoint_id)?;
    if info.sender != endpoint.owner && info.sender != config.owner {
        return Err(ContractError::Unauthorized);
    }
    let charged = charge_endpoint(deps.storage, &mut endpoint, env.block.time.seconds())?;
    apply_treasury_delta(deps.storage, charged)?;
    let refund_amount = endpoint.deposit;
    
    ENDPOINTS.remove(deps.storage, (chain_id.clone(), endpoint_id));
    ENDPOINT_URL_INDEX.remove(
        deps.storage,
        (
            chain_id.clone(),
            endpoint_index_key(&endpoint.kind, &endpoint.normalized_url),
        ),
    );

    let mut res = Response::new()
        .add_attribute("action", "remove_endpoint")
        .add_attribute("chain_id", chain_id)
        .add_attribute("endpoint_id", endpoint_id.to_string());

    if !refund_amount.is_zero() {
        let refund_msg = BankMsg::Send {
            to_address: endpoint.owner.to_string(),
            amount: vec![Coin {
                denom: NATIVE_DENOM.to_string(),
                amount: refund_amount,
            }],
        };
        res = res.add_message(refund_msg);
        res = res.add_attribute("refund_amount", refund_amount.to_string());
    }

    Ok(res)
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
    if let Some(value) = params_update.trusted_oracles {
        params.trusted_oracles = value;
    }
    if let Some(value) = params_update.oracle_max_batch_size {
        params.oracle_max_batch_size = value;
    }
    if let Some(value) = params_update.auto_unverify_failure_streak {
        params.auto_unverify_failure_streak = value;
    }
    if let Some(value) = params_update.auto_unverify_last_success_older_than_secs {
        params.auto_unverify_last_success_older_than_secs = value;
    }

    let params = validated_params(deps.api, params)?;
    PARAMS.save(deps.storage, &params)?;

    Ok(Response::new().add_attribute("action", "set_params"))
}

fn execute_submit_endpoint_statuses(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    chain_id: String,
    observations: Vec<EndpointObservationInput>,
) -> Result<Response, ContractError> {
    validate_chain_id(&chain_id)?;

    let params = PARAMS.load(deps.storage)?;
    if !params
        .trusted_oracles
        .iter()
        .any(|oracle| oracle == info.sender.as_str())
    {
        return Err(ContractError::UnauthorizedOracle);
    }

    if observations.is_empty() {
        return Err(ContractError::EmptyOracleBatch);
    }
    if observations.len() as u32 > params.oracle_max_batch_size {
        return Err(ContractError::OracleBatchTooLarge {
            max: params.oracle_max_batch_size,
        });
    }

    if CHAINS.may_load(deps.storage, chain_id.clone())?.is_none() {
        return Err(ContractError::ChainNotFound { chain_id });
    }

    let now = env.block.time.seconds();
    let mut seen_ids = HashSet::with_capacity(observations.len());
    let mut endpoints_to_update: Vec<(EndpointRecord, EndpointObservationInput)> =
        Vec::with_capacity(observations.len());
    let mut total_charged = Uint128::zero();

    for observation in observations {
        if !seen_ids.insert(observation.endpoint_id) {
            return Err(ContractError::DuplicateEndpointInBatch {
                endpoint_id: observation.endpoint_id,
            });
        }

        match (&observation.status, observation.latency_ms) {
            (EndpointStatus::Online, None) => return Err(ContractError::MissingLatencyForOnline),
            (EndpointStatus::Offline, Some(_)) => {
                return Err(ContractError::InvalidLatencyForOffline)
            }
            _ => {}
        }

        let mut endpoint = ENDPOINTS
            .may_load(deps.storage, (chain_id.clone(), observation.endpoint_id))?
            .ok_or(ContractError::EndpointNotInChain {
                endpoint_id: observation.endpoint_id,
                chain_id: chain_id.clone(),
            })?;

        let charged = charge_endpoint(deps.storage, &mut endpoint, now)?;
        total_charged = total_charged
            .checked_add(charged)
            .map_err(|_| ContractError::Overflow)?;
        if !endpoint.active {
            return Err(ContractError::EndpointNotFound);
        }

        endpoints_to_update.push((endpoint, observation));
    }

    apply_treasury_delta(deps.storage, total_charged)?;

    let response = Response::new()
        .add_attribute("action", "submit_endpoint_statuses")
        .add_attribute("oracle", info.sender.to_string())
        .add_attribute("chain_id", chain_id.clone())
        .add_attribute("count", endpoints_to_update.len().to_string());

    for (mut endpoint, observation) in endpoints_to_update {
        endpoint.verification.last_status = Some(observation.status.clone());
        endpoint.verification.last_checked_at = Some(now);
        endpoint.verification.last_checked_by = Some(info.sender.clone());

        match observation.status {
            EndpointStatus::Online => {
                endpoint.verification.last_latency_ms = observation.latency_ms;
                endpoint.verification.last_success_at = Some(now);
                endpoint.verification.consecutive_successes =
                    endpoint.verification.consecutive_successes.saturating_add(1);
                endpoint.verification.consecutive_failures = 0;
                endpoint.verification.verification_state = VerificationState::VerifiedOnline;
            }
            EndpointStatus::Offline => {
                endpoint.verification.last_latency_ms = None;
                endpoint.verification.consecutive_failures =
                    endpoint.verification.consecutive_failures.saturating_add(1);
                endpoint.verification.consecutive_successes = 0;
                endpoint.verification.verification_state = VerificationState::VerifiedOffline;
            }
        }

        if endpoint.verification.consecutive_failures > params.auto_unverify_failure_streak {
            if let Some(last_success_at) = endpoint.verification.last_success_at {
                if now.saturating_sub(last_success_at)
                    > params.auto_unverify_last_success_older_than_secs
                {
                    endpoint.verification.verification_state = VerificationState::Unverified;
                }
            }
        }

        ENDPOINTS.save(
            deps.storage,
            (endpoint.chain_id.clone(), endpoint.endpoint_id),
            &endpoint,
        )?;

        // response = response
        //     .add_attribute("endpoint_id", endpoint.endpoint_id.to_string())
        //     .add_attribute(
        //         "status",
        //         endpoint
        //             .verification
        //             .last_status
        //             .as_ref()
        //             .map(EndpointStatus::as_str)
        //             .unwrap_or("unknown"),
        //     )
        //     .add_attribute(
        //         "verification_state",
        //         endpoint.verification.verification_state.as_str(),
        //     )
        //     .add_attribute(
        //         "latency_ms",
        //         endpoint
        //             .verification
        //             .last_latency_ms
        //             .map(|value| value.to_string())
        //             .unwrap_or_else(|| "".to_string()),
        //     )
        //     .add_attribute(
        //         "consecutive_successes",
        //         endpoint.verification.consecutive_successes.to_string(),
        //     )
        //     .add_attribute(
        //         "consecutive_failures",
        //         endpoint.verification.consecutive_failures.to_string(),
        //     );
    }

    Ok(response)
}

pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetChain { chain_id } => to_json_binary(&query_chain(deps, env, chain_id)?),
        QueryMsg::GetChains { start_after, limit } => {
            to_json_binary(&query_chains(deps, start_after, limit)?)
        }
        QueryMsg::GetEndpoints {
            chain_id,
            start_after,
            limit,
            kind,
            include_inactive,
            verification_state,
            only_unverified,
            last_success_before,
            last_success_after,
        } => to_json_binary(&query_endpoints(
            deps,
            env,
            chain_id,
            start_after,
            limit,
            kind,
            include_inactive.unwrap_or(false),
            verification_state,
            only_unverified.unwrap_or(false),
            last_success_before,
            last_success_after,
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
        collect_endpoint_views(
            deps,
            env.block.time.seconds(),
            &chain_id,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
        )
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
    start_after: Option<u64>,
    limit: Option<u32>,
    kind: Option<EndpointKind>,
    include_inactive: bool,
    verification_state: Option<VerificationState>,
    only_unverified: bool,
    last_success_before: Option<u64>,
    last_success_after: Option<u64>,
) -> StdResult<EndpointsResponse> {
    validate_chain_id(&chain_id).map_err(to_std_error)?;

    let effective_verification_filter = if only_unverified {
        if let Some(state) = verification_state.clone() {
            if state != VerificationState::Unverified {
                return Err(to_std_error(ContractError::ConflictingEndpointFilters));
            }
        }
        Some(VerificationState::Unverified)
    } else {
        verification_state
    };

    let endpoints = collect_endpoint_views(
        deps,
        env.block.time.seconds(),
        &chain_id,
        start_after,
        limit,
        kind,
        include_inactive,
        effective_verification_filter,
        last_success_before,
        last_success_after,
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
        collect_endpoint_views(
            deps,
            env.block.time.seconds(),
            &chain_id,
            None,
            None,
            None,
            false,
            None,
            None,
            None,
        )
            .map_err(to_std_error)?;

    let mut apis = ChainJsonApis {
        rpc: vec![],
        rest: vec![],
        grpc: vec![],
        wss: vec![],
    };

    for endpoint in endpoints {
        if endpoint.verification_state != VerificationState::VerifiedOnline {
            continue;
        }

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
    start_after: Option<u64>,
    limit: Option<u32>,
    kind_filter: Option<EndpointKind>,
    include_inactive: bool,
    verification_state_filter: Option<VerificationState>,
    last_success_before: Option<u64>,
    last_success_after: Option<u64>,
) -> Result<Vec<EndpointView>, ContractError> {
    let params = PARAMS.load(deps.storage)?;
    let query_limit = limit.unwrap_or(DEFAULT_QUERY_LIMIT).min(MAX_QUERY_LIMIT) as usize;
    let mut endpoints = vec![];
    let start = start_after.map(Bound::exclusive);

    for item in ENDPOINTS
        .prefix(chain_id.to_string())
        .range(deps.storage, start, None, cosmwasm_std::Order::Ascending)
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

        if let Some(filter) = verification_state_filter.clone() {
            if endpoint.verification.verification_state != filter {
                continue;
            }
        }

        if let Some(before) = last_success_before {
            match endpoint.verification.last_success_at {
                Some(last_success_at) if last_success_at < before => {}
                _ => continue,
            }
        }

        if let Some(after) = last_success_after {
            match endpoint.verification.last_success_at {
                Some(last_success_at) if last_success_at > after => {}
                _ => continue,
            }
        }

        endpoints.push(EndpointView {
            endpoint_id: endpoint.endpoint_id,
            chain_id: endpoint.chain_id,
            owner: endpoint.owner.to_string(),
            kind: endpoint.kind,
            url: endpoint.url,
            normalized_url: endpoint.normalized_url,
            verification_state: endpoint.verification.verification_state,
            last_status: endpoint.verification.last_status,
            last_latency_ms: endpoint.verification.last_latency_ms,
            last_checked_at: endpoint.verification.last_checked_at,
            last_checked_by: endpoint.verification.last_checked_by.map(|value| value.to_string()),
            last_success_at: endpoint.verification.last_success_at,
            consecutive_successes: endpoint.verification.consecutive_successes,
            consecutive_failures: endpoint.verification.consecutive_failures,
            preferred: endpoint.preferred,
            active,
            remaining_deposit,
            estimated_expiry,
        });

        if endpoints.len() >= query_limit {
            break;
        }
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

fn validate_params(api: &dyn Api, params: &RegistryParams) -> Result<(), ContractError> {
    if params.epoch_seconds == 0 || params.max_endpoints_per_chain == 0 {
        return Err(ContractError::InvalidParams);
    }
    if params.rent_per_epoch.is_zero() || params.min_endpoint_deposit.is_zero() {
        return Err(ContractError::InvalidParams);
    }
    if params.oracle_max_batch_size == 0
        || params.auto_unverify_failure_streak == 0
        || params.auto_unverify_last_success_older_than_secs == 0
    {
        return Err(ContractError::InvalidParams);
    }

    let mut uniques = HashSet::with_capacity(params.trusted_oracles.len());
    for oracle in &params.trusted_oracles {
        let addr = api.addr_validate(oracle)?;
        if !uniques.insert(addr.to_string()) {
            return Err(ContractError::InvalidParams);
        }
    }

    Ok(())
}

fn validated_params(api: &dyn Api, mut params: RegistryParams) -> Result<RegistryParams, ContractError> {
    let mut validated_oracles = Vec::with_capacity(params.trusted_oracles.len());
    for oracle in &params.trusted_oracles {
        validated_oracles.push(api.addr_validate(oracle)?.to_string());
    }
    params.trusted_oracles = validated_oracles;
    validate_params(api, &params)?;
    Ok(params)
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

fn get_endpoint(
    storage: &mut dyn Storage,
    chain_id: &str,
    endpoint_id: u64
) -> Result<EndpointRecord, ContractError> {
    let endpoint = ENDPOINTS
        .may_load(storage, (chain_id.to_string(), endpoint_id))?
        .ok_or(ContractError::EndpointNotFound)?;
    Ok(endpoint)
}

fn charge_endpoint(
    storage: &mut dyn Storage,
    endpoint: &mut EndpointRecord,
    now: u64,
) -> Result<Uint128, ContractError> {
    let params = PARAMS.load(storage)?;
    let mut charged = Uint128::zero();

    if endpoint.active {
        let elapsed = now.saturating_sub(endpoint.last_charged_at);
        let epochs = elapsed / params.epoch_seconds;
        if epochs > 0 {
            let due = params
                .rent_per_epoch
                .checked_mul(Uint128::from(epochs as u128))
                .map_err(|_| ContractError::Overflow)?;
            charged = due.min(endpoint.deposit);
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
        }
    }

    Ok(charged)
}

fn apply_treasury_delta(storage: &mut dyn Storage, delta: Uint128) -> Result<(), ContractError> {
    if delta.is_zero() {
        return Ok(());
    }

    let mut treasury = TREASURY_ACCUMULATED.load(storage)?;
    treasury = treasury
        .checked_add(delta)
        .map_err(|_| ContractError::Overflow)?;
    TREASURY_ACCUMULATED.save(storage, &treasury)?;

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};

    #[test]
    fn migrate_converts_legacy_endpoint_to_v1_verification() {
        let mut deps = mock_dependencies();
        let chain_id = "legacy-chain-1".to_string();
        let endpoint_id = 7u64;

        let legacy_endpoints: cw_storage_plus::Map<(String, u64), LegacyEndpointRecord> =
            cw_storage_plus::Map::new("endpoints");

        let owner = Addr::unchecked("cosmos1legacyowner");
        legacy_endpoints
            .save(
                deps.as_mut().storage,
                (chain_id.clone(), endpoint_id),
                &LegacyEndpointRecord {
                    endpoint_id,
                    chain_id: chain_id.clone(),
                    owner: owner.clone(),
                    kind: EndpointKind::Rpc,
                    url: "https://rpc.legacy.example".to_string(),
                    normalized_url: "https://rpc.legacy.example".to_string(),
                    deposit: Uint128::new(777),
                    last_charged_at: 1_700_000_000,
                    active: true,
                    verified: true,
                    preferred: true,
                },
            )
            .unwrap();

        let response = migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();

        let migrated = ENDPOINTS
            .load(deps.as_ref().storage, (chain_id, endpoint_id))
            .unwrap();

        assert_eq!(
            response
                .attributes
                .iter()
                .find(|attr| attr.key == "migration_mode")
                .map(|attr| attr.value.clone()),
            Some("lazy_endpoint_compat".to_string())
        );
        assert_eq!(migrated.owner, owner);
        assert_eq!(migrated.deposit, Uint128::new(777));
        assert_eq!(migrated.preferred, true);
        assert_eq!(migrated.verification.verification_state, VerificationState::Unverified);
        assert_eq!(migrated.verification.last_status, None);
        assert_eq!(migrated.verification.last_latency_ms, None);
        assert_eq!(migrated.verification.last_checked_at, None);
        assert_eq!(migrated.verification.last_checked_by, None);
        assert_eq!(migrated.verification.last_success_at, None);
        assert_eq!(migrated.verification.consecutive_successes, 0);
        assert_eq!(migrated.verification.consecutive_failures, 0);
    }
}
