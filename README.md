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
