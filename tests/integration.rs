use cosmwasm_std::{Addr, Empty, Timestamp, Uint128};
use cw_multi_test::{App, Contract, ContractWrapper, Executor};

use cosm_registry::msg::{
    Asset, ChainMeta, EndpointInput, EndpointKind, EndpointsResponse, ExecuteMsg,
    ExportChainJsonResponse, InstantiateMsg, NetworkType, QueryMsg, RegistryParams,
    RegistryParamsResponse,
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

fn instantiate_msg() -> InstantiateMsg {
    InstantiateMsg {
        owner: Some("admin".to_string()),
        treasury: Some("treasury".to_string()),
        params: Some(RegistryParams {
            min_endpoint_deposit: Uint128::new(100),
            rent_per_epoch: Uint128::new(10),
            epoch_seconds: 10,
            max_endpoints_per_chain: 4,
        }),
    }
}

#[test]
fn register_chain_and_export_chain_json() {
    let mut app = App::default();
    let admin = Addr::unchecked("admin");

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(code_id, admin.clone(), &instantiate_msg(), &[], "registry", None)
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
    let mut app = App::default();
    let admin = Addr::unchecked("admin");

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(code_id, admin.clone(), &instantiate_msg(), &[], "registry", None)
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
        &[],
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
            &[],
        );

    assert!(result.is_err());

    let endpoints: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::GetEndpoints {
                chain_id: "osmosis-1".to_string(),
                kind: Some(EndpointKind::Rpc),
                include_inactive: Some(true),
            },
        )
        .unwrap();
    assert_eq!(endpoints.endpoints.len(), 1);
}

#[test]
fn rejects_endpoint_below_min_deposit() {
    let mut app = App::default();
    let admin = Addr::unchecked("admin");

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(code_id, admin.clone(), &instantiate_msg(), &[], "registry", None)
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
            &[],
        );

    assert!(result.is_err());

    let endpoints: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::GetEndpoints {
                chain_id: "juno-1".to_string(),
                kind: None,
                include_inactive: Some(true),
            },
        )
        .unwrap();
    assert_eq!(endpoints.endpoints.len(), 0);
}

#[test]
fn non_admin_cannot_change_params() {
    let mut app = App::default();
    let admin = Addr::unchecked("admin");
    let user = Addr::unchecked("user1");

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(code_id, admin, &instantiate_msg(), &[], "registry", None)
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
    let mut app = App::default();
    let admin = Addr::unchecked("admin");

    let code_id = app.store_code(contract_cosm_registry());
    let contract_addr = app
        .instantiate_contract(code_id, admin.clone(), &instantiate_msg(), &[], "registry", None)
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
        &[],
    )
    .unwrap();

    app.update_block(|block| {
        block.time = Timestamp::from_seconds(block.time.seconds() + 120);
    });

    let before_top_up: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::GetEndpoints {
                chain_id: "injective-1".to_string(),
                kind: Some(EndpointKind::Grpc),
                include_inactive: Some(false),
            },
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
        &[],
    )
    .unwrap();

    let after_top_up: EndpointsResponse = app
        .wrap()
        .query_wasm_smart(
            contract_addr,
            &QueryMsg::GetEndpoints {
                chain_id: "injective-1".to_string(),
                kind: Some(EndpointKind::Grpc),
                include_inactive: Some(false),
            },
        )
        .unwrap();

    assert_eq!(after_top_up.endpoints.len(), 1);
    assert!(after_top_up.endpoints[0].active);
}
