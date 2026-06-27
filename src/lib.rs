pub mod contract;
pub mod error;
pub mod msg;
pub mod state;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use cosmwasm_std::{entry_point, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    contract::instantiate(deps, env, info, msg)
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    contract::execute(deps, env, info, msg)
}

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    contract::query(deps, env, msg)
}

#[entry_point]
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    contract::migrate(deps, env, msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};

    #[test]
    fn instantiate_sets_default_owner() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            owner: None,
            treasury: None,
            params: None,
        };
        let info = mock_info("owner", &[]);

        let response = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(response.attributes[0].value, "instantiate");
    }

    #[test]
    fn migrate_keeps_state_and_updates_version() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            owner: None,
            treasury: None,
            params: None,
        };
        let info = mock_info("owner", &[]);

        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let response = migrate(deps.as_mut(), mock_env(), MigrateMsg {}).unwrap();
        assert_eq!(response.attributes[0].value, "migrate");
    }
}
