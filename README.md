# Cosm Registry - CosmWasm Contract Workspace

Workspace Rust/CosmWasm avec devcontainer pour développer un smart contract dans l'ecosysteme Cosmos.

## Prerequis

- VS Code avec l'extension Dev Containers
- Docker actif

## Ouvrir dans le devcontainer

1. Ouvrir ce dossier dans VS Code.
2. Lancer la commande: Reopen in Container.
3. Attendre la fin du postCreateCommand.

## Commandes utiles

- Verifier le code: `cargo fmt --all --check`
- Linter: `cargo clippy --all-targets -- -D warnings`
- Tests: `cargo test`
- Build wasm: `cargo wasm`
- Generer schemas JSON: `cargo schema`

## Structure

- `src/`: code du contrat
- `examples/schema.rs`: generation des schemas
- `tests/`: tests integration cw-multi-test
- `.devcontainer/`: environnement reproductible

## Contrat de base

Le scaffold fournit:

- une instantiation avec `owner` optionnel
- une execution `SetValue { key, value }` reservee a l'owner
- des queries `GetValue` et `GetOwner`

Ce socle sera adapte une fois votre expression du besoin fournie.

## Fonctions V1 implementees

### Execute messages

- `RegisterChain`: cree une chaine avec metadata normalisee et assets.
- `UpdateChainMeta`: met a jour la metadata d'une chaine existante (owner de la chaine ou admin).
- `RegisterEndpoint`: ajoute un endpoint RPC/REST/gRPC/WSS avec depot minimal obligatoire.
- `TopUpEndpoint`: recharge le depot d'un endpoint (owner endpoint ou admin).
- `RemoveEndpoint`: retire un endpoint (owner endpoint ou admin).
- `SetParams`: met a jour les parametres economiques (admin uniquement).
- `SetEndpointFlags`: positionne `verified/preferred` (admin uniquement).

### Query messages

- `GetChain`: retourne metadata + endpoints actifs pour une chaine.
- `GetChains`: liste paginee des chaines.
- `GetEndpoints`: liste des endpoints avec filtre de type et inclusion optionnelle des inactifs.
- `ExportChainJson`: projection compatible chain-registry pour `apis.rpc/rest/grpc/wss`.
- `GetOwner`: retourne owner et treasury.
- `GetParams`: retourne les parametres economiques actifs.

## Regles de securite appliquees

- Controle d'acces strict:
	- admin requis pour `SetParams` et `SetEndpointFlags`.
	- owner chaine ou admin pour `UpdateChainMeta`.
	- owner endpoint ou admin pour `TopUpEndpoint` et `RemoveEndpoint`.
- Validation d'entrees:
	- `chain_id` en `[a-z0-9-]` avec bornes de longueur.
	- validation des champs texte (non vide, taille max).
	- validation stricte des schemes URL selon type d'endpoint.
	- normalisation URL + index d'unicite pour bloquer les doublons triviaux.
- Modele economique:
	- depot minimal pour enregistrer un endpoint.
	- rent deduite en lazy accounting par epochs.
	- endpoint inactif quand depot epuise.
	- accumulation des frais vers la tresorerie logique.

## Tests couverts (integration)

- Enregistrement d'une chaine et export `chain.json` compatible.
- Rejet des endpoints dupliques apres normalisation URL.
- Rejet d'un endpoint sous depot minimal.
- Rejet des updates de params par non-admin.
- Expiration lazy puis reactivation via top-up.

# déployement et Mise a Jour

```
~/go/bin/gaiad tx wasm store artifacts/cosm_registry.wasm --from test-dev --chain-id provider --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 --gas auto --gas-adjustment 1.4 --gas-prices 0.005uatom

~/go/bin/gaiad query wasm list-code --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443  --output json --reverse | jq -r '.code_infos[-1].code_id'

export CODE_ID=<code_id>

INIT='{"owner":"'"cosmos1abcd"'","treasury":"'"cosmos1abcd"'"}'

~/go/bin/gaiad tx wasm instantiate 520 "$INIT" --label "chain-registry" --admin cosmos1abcd --from test-dev --chain-id provider --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 --gas auto --gas-adjustment 1.4 --gas-prices 0.005uatom

```

# Update et migration
```
export ACCOUNT=test-dev
export ADDRESS=$(~/go/bin/gaiad keys show ${ACCOUNT} --address)
~/go/bin/gaiad query wasm list-contracts-by-creator ${ADDRESS} --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 -o json


~/go/bin/gaiad tx wasm store artifacts/cosm_registry.wasm --from ${ACCOUNT} --chain-id provider --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 --gas auto --gas-adjustment 1.4 --gas-prices 0.005uatom -y --broadcast-mode sync -o json

~/go/bin/gaiad query tx 36A49A9A6E896866C37212EE02A15C70CF9C27652ADCA3EC55C4A15A2172A7C9 --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 -o json | jq -r '[.. | objects | select(.type? == "store_code") | .attributes[]? | select(.key == "code_id") | .value][0]'

export NEW_CODE_ID=<code_id>

~/go/bin/gaiad tx wasm migrate cosmos1jeurekn4zrz4k5welwlvcngte337cnckarjp7csted7ay3xn668qyer6sn ${NEW_CODE_ID} '{}' --from ${ACCOUNT} --chain-id provider --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 --gas auto --gas-adjustment 1.4 --gas-prices 0.005uatom -y --broadcast-mode sync -o json

```
