use cosmwasm_std::{coin, coins, Addr, Empty, Timestamp, Uint128};
use cosmwasm_std::testing::MockApi;
use cw_multi_test::{App, Contract, ContractWrapper, Executor};

use cosm_registry::msg::{
    Asset, ChainMeta, EndpointInput, EndpointKind, EndpointObservationInput, EndpointStatus,
    EndpointsResponse, ExecuteMsg, ExportChainJsonResponse, InstantiateMsg, NetworkType,
    QueryMsg, RegistryParams, RegistryParamsResponse, VerificationState,
};

fn contract_cosm_registry() -> Box<dyn Contract<Empty>> {
    let contract = ContractWrapper::new(
        cosm_registry::execute,
        cosm_registry::instantiate,
        cosm_registry::query,
    );
    Box::new(contract)
}

fn sample_chain(chain_id: &str) -> ChainMeta {
    ChainMeta {
        chain_id: chain_id.to_string(),
        chain_name: "cosmoshub".to_string(),
        pretty_name: "Cosmos Hub".to_string(),
        bech32_prefix: "cosmos".to_string(),
        network_type: NetworkType::Mainnet,
        website: Some("https://hub.cosmos.network".to_string()),
        assets: vec![Asset {
            denom: "uatom".to_string(),
            display: "atom".to_string(),
            symbol: "ATOM".to_string(),
            decimals: 6,
        }],
        explorers: vec![],
    }
}

fn instantiate_msg(owner: &Addr, treasury: &Addr) -> InstantiateMsg {
    InstantiateMsg {
        owner: Some(owner.to_string()),
        treasury: Some(treasury.to_string()),
        params: Some(RegistryParams {
            min_endpoint_deposit: Uint128::new(100),
            rent_per_epoch: Uint128::new(10),
            epoch_seconds: 10,
            max_endpoints_per_chain: 4,
            trusted_oracles: vec![owner.to_string()],
            oracle_max_batch_size: 20,
            auto_unverify_failure_streak: 5,
            auto_unverify_last_success_older_than_secs: 432_000,
        }),
    }
}

fn instantiate_msg_with_oracle_policy(
    owner: &Addr,
    treasury: &Addr,
    failure_streak: u32,
    stale_secs: u64,
) -> InstantiateMsg {
    InstantiateMsg {
        owner: Some(owner.to_string()),
        treasury: Some(treasury.to_string()),
        params: Some(RegistryParams {
            min_endpoint_deposit: Uint128::new(100),
            rent_per_epoch: Uint128::new(10),
            epoch_seconds: 10,
            max_endpoints_per_chain: 10,
            trusted_oracles: vec![owner.to_string()],
            oracle_max_batch_size: 20,
            auto_unverify_failure_streak: failure_streak,
            auto_unverify_last_success_older_than_secs: stale_secs,
        }),
    }
}

fn base_get_endpoints(chain_id: &str, kind: Option<EndpointKind>, include_inactive: bool) -> QueryMsg {
    QueryMsg::GetEndpoints {
        chain_id: chain_id.to_string(),
        start_after: None,
        limit: None,
        kind,
        include_inactive: Some(include_inactive),
        verification_state: None,
        only_unverified: None,
        last_success_before: None,
        last_success_after: None,
    }
}

const NATIVE_DENOM: &str = "uatom";

fn test_addrs() -> (Addr, Addr, Addr) {
    let api = MockApi::default();
    (
        api.addr_make("admin"),
        api.addr_make("user1"),
        api.addr_make("treasury"),
    )
}

fn app_with_balances(admin: &Addr, user: &Addr) -> App {
    App::new(|router, _, storage| {
        router
            .bank
            .init_balance(storage, admin, coins(1_000_000, NATIVE_DENOM))
            .unwrap();
        router
            .bank
            .init_balance(storage, user, coins(1_000_000, NATIVE_DENOM))
            .unwrap();
    })
}

#[test]
fn register_chain_and_export_chain_json() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("cosmoshub-4"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "cosmoshub-4".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.cosmos.network/".to_string(),
                deposit: Uint128::new(120),
                owner: None,
            },
        },
        &[coin(120, NATIVE_DENOM)],
    )
    .unwrap();

    app.execute_contract(
        user,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "cosmoshub-4".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(120),
            }],
        },
        &[],
    )
    .unwrap_err();

    let (admin, _, _) = test_addrs();
    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "cosmoshub-4".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(120),
            }],
        },
        &[],
    )
    .unwrap();

    let exported: ExportChainJsonResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::ExportChainJson {
                chain_id: "cosmoshub-4".to_string(),
            },
        )
        .unwrap();

    assert_eq!(exported.chain_id, "cosmoshub-4");
    assert_eq!(exported.assets.len(), 1);
    assert_eq!(exported.apis.rpc.len(), 1);
    assert_eq!(exported.apis.rpc[0].address, "https://rpc.cosmos.network/");
}

#[test]
fn rejects_duplicate_endpoint_after_url_normalization() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("osmosis-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "osmosis-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.osmo.zone/".to_string(),
                deposit: Uint128::new(120),
                owner: None,
            },
        },
        &[coin(120, NATIVE_DENOM)],
    )
    .unwrap();

    let result = app
        .execute_contract(
            admin,
            contract_addr.clone(),
            &ExecuteMsg::RegisterEndpoint {
                chain_id: "osmosis-1".to_string(),
                endpoint: EndpointInput {
                    kind: EndpointKind::Rpc,
                    url: "https://rpc.osmo.zone".to_string(),
                    deposit: Uint128::new(120),
                    owner: None,
                },
            },
            &[coin(120, NATIVE_DENOM)],
        );

    assert!(result.is_err());

    let endpoints: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &base_get_endpoints("osmosis-1", Some(EndpointKind::Rpc), true),
        )
        .unwrap();
    assert_eq!(endpoints.endpoints.len(), 1);
}

#[test]
fn rejects_endpoint_below_min_deposit() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("juno-1"),
        },
        &[],
    )
    .unwrap();

    let result = app
        .execute_contract(
            admin,
            contract_addr.clone(),
            &ExecuteMsg::RegisterEndpoint {
                chain_id: "juno-1".to_string(),
                endpoint: EndpointInput {
                    kind: EndpointKind::Rest,
                    url: "https://rest.juno.strange.love".to_string(),
                    deposit: Uint128::new(99),
                    owner: None,
                },
            },
            &[coin(99, NATIVE_DENOM)],
        );

    assert!(result.is_err());

    let endpoints: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &base_get_endpoints("juno-1", None, true),
        )
        .unwrap();
    assert_eq!(endpoints.endpoints.len(), 0);
}

#[test]
fn non_admin_cannot_change_params() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin,
            &instantiate_msg(&test_addrs().0, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    let result = app
        .execute_contract(
            user,
            contract_addr.clone(),
            &ExecuteMsg::SetParams {
                params: cosm_registry::msg::ParamsUpdate {
                    min_endpoint_deposit: Some(Uint128::new(500)),
                    rent_per_epoch: None,
                    epoch_seconds: None,
                    max_endpoints_per_chain: None,
                    trusted_oracles: None,
                    oracle_max_batch_size: None,
                    auto_unverify_failure_streak: None,
                    auto_unverify_last_success_older_than_secs: None,
                },
            },
            &[],
        );

    assert!(result.is_err());

    let params: RegistryParamsResponse = app
        .wrap()
        .query_wasm_smart(contract_addr, &QueryMsg::GetParams {})
        .unwrap();
    assert_eq!(params.params.min_endpoint_deposit, Uint128::new(100));
}

#[test]
fn lazy_expiration_hides_endpoint_and_top_up_reactivates() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("injective-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "injective-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Grpc,
                url: "https://grpc.injective.network".to_string(),
                deposit: Uint128::new(100),
                owner: None,
            },
        },
        &[coin(100, NATIVE_DENOM)],
    )
    .unwrap();

    app.update_block(|block| {
        block.time = Timestamp::from_seconds(block.time.seconds() + 120);
    });

    let before_top_up: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr.clone(),
            &base_get_endpoints("injective-1", Some(EndpointKind::Grpc), false),
        )
        .unwrap();
    assert_eq!(before_top_up.endpoints.len(), 0);

    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::TopUpEndpoint {
            chain_id: "injective-1".to_string(),
            endpoint_id: 1,
            amount: Uint128::new(150),
        },
        &[coin(150, NATIVE_DENOM)],
    )
    .unwrap();

    let after_top_up: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &base_get_endpoints("injective-1", Some(EndpointKind::Grpc), false),
        )
        .unwrap();

    assert_eq!(after_top_up.endpoints.len(), 1);
    assert!(after_top_up.endpoints[0].active);
}

#[test]
fn oracle_online_status_makes_endpoint_exportable() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("stargaze-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "stargaze-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.stargaze.zone".to_string(),
                deposit: Uint128::new(200),
                owner: None,
            },
        },
        &[coin(200, NATIVE_DENOM)],
    )
    .unwrap();

    let exported_before: ExportChainJsonResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::ExportChainJson {
                chain_id: "stargaze-1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(exported_before.apis.rpc.len(), 0);

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "stargaze-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(88),
            }],
        },
        &[],
    )
    .unwrap();

    let endpoints: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::GetEndpoints {
                chain_id: "stargaze-1".to_string(),
                start_after: None,
                limit: None,
                kind: Some(EndpointKind::Rpc),
                include_inactive: Some(false),
                verification_state: Some(VerificationState::VerifiedOnline),
                only_unverified: None,
                last_success_before: None,
                last_success_after: None,
            },
        )
        .unwrap();
    assert_eq!(endpoints.endpoints.len(), 1);

    let exported_after: ExportChainJsonResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::ExportChainJson {
                chain_id: "stargaze-1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(exported_after.apis.rpc.len(), 1);
}

#[test]
fn oracle_batch_with_duplicate_endpoint_ids_is_rejected() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("axelar-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "axelar-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.axelar.zone".to_string(),
                deposit: Uint128::new(200),
                owner: None,
            },
        },
        &[coin(200, NATIVE_DENOM)],
    )
    .unwrap();

    let result = app.execute_contract(
        admin,
        contract_addr,
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "axelar-1".to_string(),
            observations: vec![
                EndpointObservationInput {
                    endpoint_id: 1,
                    status: EndpointStatus::Online,
                    latency_ms: Some(90),
                },
                EndpointObservationInput {
                    endpoint_id: 1,
                    status: EndpointStatus::Offline,
                    latency_ms: None,
                },
            ],
        },
        &[],
    );

    assert!(result.is_err());
}

#[test]
fn online_without_latency_is_rejected() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("celestia-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "celestia-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.celestia.org".to_string(),
                deposit: Uint128::new(200),
                owner: None,
            },
        },
        &[coin(200, NATIVE_DENOM)],
    )
    .unwrap();

    let result = app.execute_contract(
        admin,
        contract_addr,
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "celestia-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: None,
            }],
        },
        &[],
    );

    assert!(result.is_err());
}

#[test]
fn offline_preserves_last_success_and_increments_failures() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("stride-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "stride-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rest,
                url: "https://rest.stride.zone".to_string(),
                deposit: Uint128::new(250),
                owner: None,
            },
        },
        &[coin(250, NATIVE_DENOM)],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "stride-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(40),
            }],
        },
        &[],
    )
    .unwrap();

    let before: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::GetEndpoints {
                chain_id: "stride-1".to_string(),
                start_after: None,
                limit: None,
                kind: None,
                include_inactive: Some(false),
                verification_state: None,
                only_unverified: None,
                last_success_before: None,
                last_success_after: None,
            },
        )
        .unwrap();
    let last_success_at = before.endpoints[0].last_success_at;

    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "stride-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Offline,
                latency_ms: None,
            }],
        },
        &[],
    )
    .unwrap();

    let after: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::GetEndpoints {
                chain_id: "stride-1".to_string(),
                start_after: None,
                limit: None,
                kind: None,
                include_inactive: Some(false),
                verification_state: None,
                only_unverified: None,
                last_success_before: None,
                last_success_after: None,
            },
        )
        .unwrap();

    assert_eq!(after.endpoints[0].verification_state, VerificationState::VerifiedOffline);
    assert_eq!(after.endpoints[0].last_success_at, last_success_at);
    assert_eq!(after.endpoints[0].consecutive_failures, 1);
    assert_eq!(after.endpoints[0].consecutive_successes, 0);
}

#[test]
fn auto_unverify_triggers_on_stale_success_and_failure_streak() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg_with_oracle_policy(&admin, &treasury, 1, 5),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("noble-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "noble-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.noble.xyz".to_string(),
                deposit: Uint128::new(400),
                owner: None,
            },
        },
        &[coin(400, NATIVE_DENOM)],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "noble-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(55),
            }],
        },
        &[],
    )
    .unwrap();

    app.update_block(|block| {
        block.time = Timestamp::from_seconds(block.time.seconds() + 20);
    });

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "noble-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Offline,
                latency_ms: None,
            }],
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "noble-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Offline,
                latency_ms: None,
            }],
        },
        &[],
    )
    .unwrap();

    let endpoints: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &base_get_endpoints("noble-1", Some(EndpointKind::Rpc), false),
        )
        .unwrap();

    assert_eq!(endpoints.endpoints[0].verification_state, VerificationState::Unverified);
}

#[test]
fn get_endpoints_filter_by_unverified_and_dates() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("neutron-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "neutron-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.neutron.org".to_string(),
                deposit: Uint128::new(300),
                owner: None,
            },
        },
        &[coin(300, NATIVE_DENOM)],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "neutron-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rest,
                url: "https://rest.neutron.org".to_string(),
                deposit: Uint128::new(300),
                owner: None,
            },
        },
        &[coin(300, NATIVE_DENOM)],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "neutron-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(60),
            }],
        },
        &[],
    )
    .unwrap();

    let all: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr.clone(),
            &base_get_endpoints("neutron-1", None, false),
        )
        .unwrap();
    assert_eq!(all.endpoints.len(), 2);

    let unverified: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::GetEndpoints {
                chain_id: "neutron-1".to_string(),
                start_after: None,
                limit: None,
                kind: None,
                include_inactive: Some(false),
                verification_state: None,
                only_unverified: Some(true),
                last_success_before: None,
                last_success_after: None,
            },
        )
        .unwrap();
    assert_eq!(unverified.endpoints.len(), 1);
    assert_eq!(unverified.endpoints[0].verification_state, VerificationState::Unverified);

    let with_success_after: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::GetEndpoints {
                chain_id: "neutron-1".to_string(),
                start_after: None,
                limit: None,
                kind: None,
                include_inactive: Some(false),
                verification_state: None,
                only_unverified: None,
                last_success_before: None,
                last_success_after: Some(0),
            },
        )
        .unwrap();
    assert_eq!(with_success_after.endpoints.len(), 1);
}

#[test]
fn oracle_cannot_update_expired_endpoint_after_rent_check() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("dymension-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "dymension-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.dymension.xyz".to_string(),
                deposit: Uint128::new(100),
                owner: None,
            },
        },
        &[coin(100, NATIVE_DENOM)],
    )
    .unwrap();

    app.update_block(|block| {
        block.time = Timestamp::from_seconds(block.time.seconds() + 120);
    });

    let result = app.execute_contract(
        admin,
        contract_addr,
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "dymension-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(72),
            }],
        },
        &[],
    );

    assert!(result.is_err());
}

#[test]
fn non_whitelisted_cannot_submit_and_whitelisted_can_submit() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("sei-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "sei-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.sei.io".to_string(),
                deposit: Uint128::new(250),
                owner: None,
            },
        },
        &[coin(250, NATIVE_DENOM)],
    )
    .unwrap();

    let unauthorized = app.execute_contract(
        user,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "sei-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(10),
            }],
        },
        &[],
    );
    assert!(unauthorized.is_err());

    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "sei-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(10),
            }],
        },
        &[],
    )
    .unwrap();
}

#[test]
fn batch_with_endpoint_from_other_chain_is_rejected() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("chain-a-1"),
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("chain-b-1"),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "chain-b-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.chainb.io".to_string(),
                deposit: Uint128::new(250),
                owner: None,
            },
        },
        &[coin(250, NATIVE_DENOM)],
    )
    .unwrap();

    let result = app.execute_contract(
        admin,
        contract_addr,
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "chain-a-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(25),
            }],
        },
        &[],
    );
    assert!(result.is_err());
}

#[test]
fn online_observation_resets_failures_and_increments_successes() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("osmosis-2"),
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "osmosis-2".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.osmosis.zone".to_string(),
                deposit: Uint128::new(250),
                owner: None,
            },
        },
        &[coin(250, NATIVE_DENOM)],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "osmosis-2".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Offline,
                latency_ms: None,
            }],
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "osmosis-2".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(30),
            }],
        },
        &[],
    )
    .unwrap();

    let endpoints: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(contract_addr, &base_get_endpoints("osmosis-2", None, false))
        .unwrap();
    assert_eq!(endpoints.endpoints[0].consecutive_failures, 0);
    assert_eq!(endpoints.endpoints[0].consecutive_successes, 1);
    assert_eq!(endpoints.endpoints[0].verification_state, VerificationState::VerifiedOnline);
}

#[test]
fn not_auto_unverified_when_last_success_is_recent() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg_with_oracle_policy(&admin, &treasury, 1, 1000),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("evmos-1"),
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "evmos-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.evmos.org".to_string(),
                deposit: Uint128::new(250),
                owner: None,
            },
        },
        &[coin(250, NATIVE_DENOM)],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "evmos-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(35),
            }],
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "evmos-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Offline,
                latency_ms: None,
            }],
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "evmos-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Offline,
                latency_ms: None,
            }],
        },
        &[],
    )
    .unwrap();

    let endpoints: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(contract_addr, &base_get_endpoints("evmos-1", None, false))
        .unwrap();
    assert_eq!(
        endpoints.endpoints[0].verification_state,
        VerificationState::VerifiedOffline
    );
}

#[test]
fn never_successful_endpoint_is_not_auto_unverified() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg_with_oracle_policy(&admin, &treasury, 1, 1),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("agoric-3"),
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "agoric-3".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.agoric.net".to_string(),
                deposit: Uint128::new(250),
                owner: None,
            },
        },
        &[coin(250, NATIVE_DENOM)],
    )
    .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "agoric-3".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Offline,
                latency_ms: None,
            }],
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "agoric-3".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Offline,
                latency_ms: None,
            }],
        },
        &[],
    )
    .unwrap();

    let endpoints: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(contract_addr, &base_get_endpoints("agoric-3", None, false))
        .unwrap();
    assert_eq!(
        endpoints.endpoints[0].verification_state,
        VerificationState::VerifiedOffline
    );
    assert_eq!(endpoints.endpoints[0].last_success_at, None);
}

#[test]
fn get_endpoints_last_success_before_filter_works() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg(&admin, &treasury),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("archway-1"),
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterEndpoint {
            chain_id: "archway-1".to_string(),
            endpoint: EndpointInput {
                kind: EndpointKind::Rpc,
                url: "https://rpc.archway.io".to_string(),
                deposit: Uint128::new(250),
                owner: None,
            },
        },
        &[coin(250, NATIVE_DENOM)],
    )
    .unwrap();

    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "archway-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(22),
            }],
        },
        &[],
    )
    .unwrap();

    let all: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(contract_addr.clone(), &base_get_endpoints("archway-1", None, false))
        .unwrap();
    let last_success = all.endpoints[0].last_success_at.unwrap();

    let filtered: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::GetEndpoints {
                chain_id: "archway-1".to_string(),
                start_after: None,
                limit: None,
                kind: None,
                include_inactive: Some(false),
                verification_state: None,
                only_unverified: None,
                last_success_before: Some(last_success.saturating_add(1)),
                last_success_after: None,
            },
        )
        .unwrap();
    assert_eq!(filtered.endpoints.len(), 1);
}

#[test]
fn export_excludes_verified_offline_and_unverified() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg_with_oracle_policy(&admin, &treasury, 10, 999_999),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("kujira-1"),
        },
        &[],
    )
    .unwrap();

    for (id, url) in [(1u64, "https://rpc1.kujira.app"), (2, "https://rpc2.kujira.app"), (3, "https://rpc3.kujira.app")] {
        app.execute_contract(
            admin.clone(),
            contract_addr.clone(),
            &ExecuteMsg::RegisterEndpoint {
                chain_id: "kujira-1".to_string(),
                endpoint: EndpointInput {
                    kind: EndpointKind::Rpc,
                    url: url.to_string(),
                    deposit: Uint128::new(250),
                    owner: None,
                },
            },
            &[coin(250, NATIVE_DENOM)],
        )
        .unwrap();
        assert_eq!(id, id);
    }

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "kujira-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 1,
                status: EndpointStatus::Online,
                latency_ms: Some(11),
            }],
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "kujira-1".to_string(),
            observations: vec![EndpointObservationInput {
                endpoint_id: 2,
                status: EndpointStatus::Offline,
                latency_ms: None,
            }],
        },
        &[],
    )
    .unwrap();

    let exported: ExportChainJsonResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::ExportChainJson {
                chain_id: "kujira-1".to_string(),
            },
        )
        .unwrap();
    assert_eq!(exported.apis.rpc.len(), 1);
    assert_eq!(exported.apis.rpc[0].address, "https://rpc1.kujira.app");
}

#[test]
fn pagination_with_verification_filter_remains_correct() {
    let (admin, user, treasury) = test_addrs();
    let mut app = app_with_balances(&admin, &user);

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(
            code_id,
            admin.clone(),
            &instantiate_msg_with_oracle_policy(&admin, &treasury, 5, 100),
            &[],
            "registry",
            None,
        )
        .unwrap();

    app.execute_contract(
        admin.clone(),
        contract_addr.clone(),
        &ExecuteMsg::RegisterChain {
            chain: sample_chain("terra-2"),
        },
        &[],
    )
    .unwrap();

    for endpoint_id in 1..=4u64 {
        app.execute_contract(
            admin.clone(),
            contract_addr.clone(),
            &ExecuteMsg::RegisterEndpoint {
                chain_id: "terra-2".to_string(),
                endpoint: EndpointInput {
                    kind: EndpointKind::Rpc,
                    url: format!("https://rpc{}.terra.dev", endpoint_id),
                    deposit: Uint128::new(250),
                    owner: None,
                },
            },
            &[coin(250, NATIVE_DENOM)],
        )
        .unwrap();
    }

    app.execute_contract(
        admin,
        contract_addr.clone(),
        &ExecuteMsg::SubmitEndpointStatuses {
            chain_id: "terra-2".to_string(),
            observations: vec![
                EndpointObservationInput {
                    endpoint_id: 1,
                    status: EndpointStatus::Online,
                    latency_ms: Some(15),
                },
                EndpointObservationInput {
                    endpoint_id: 2,
                    status: EndpointStatus::Online,
                    latency_ms: Some(16),
                },
                EndpointObservationInput {
                    endpoint_id: 3,
                    status: EndpointStatus::Online,
                    latency_ms: Some(17),
                },
            ],
        },
        &[],
    )
    .unwrap();

    let page1: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::GetEndpoints {
                chain_id: "terra-2".to_string(),
                start_after: None,
                limit: Some(2),
                kind: None,
                include_inactive: Some(false),
                verification_state: Some(VerificationState::VerifiedOnline),
                only_unverified: None,
                last_success_before: None,
                last_success_after: None,
            },
        )
        .unwrap();
    assert_eq!(page1.endpoints.len(), 2);
    assert_eq!(page1.endpoints[0].endpoint_id, 1);
    assert_eq!(page1.endpoints[1].endpoint_id, 2);

    let page2: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::GetEndpoints {
                chain_id: "terra-2".to_string(),
                start_after: Some(2),
                limit: Some(2),
                kind: None,
                include_inactive: Some(false),
                verification_state: Some(VerificationState::VerifiedOnline),
                only_unverified: None,
                last_success_before: None,
                last_success_after: None,
            },
        )
        .unwrap();
    assert_eq!(page2.endpoints.len(), 1);
    assert_eq!(page2.endpoints[0].endpoint_id, 3);
}
