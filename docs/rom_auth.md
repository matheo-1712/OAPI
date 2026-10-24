# Liaison ROM / Discord (token `rom_`)

Permet à un jeu (ROM) d'être lié à un compte Discord sans cookie ni mot de passe : le membre génère un **code à 6 chiffres** sur le site, le saisit dans le jeu, et le jeu l'échange contre un **token `rom_…`** à usage de lecture seule. Les données sont dans la collection PocketBase `discord_identity_token`. Toutes les routes sont sous le préfixe configuré (`/api` par défaut).

## 1. Flux

```
Site (session)            OAPI                         Jeu (ROM)
  |-- POST /auth/rom/code ->|                              |
  |<-- code + expires_at ---|                              |
  |        (le membre recopie le code dans le jeu)         |
  |                         |<-- POST /auth/rom/token -----|  { "code": "123456" }
  |                         |--- token rom_… (une fois) -->|
  |                         |<-- GET /friends/me ---------|  Authorization: Bearer rom_…
```

1. Le site, connecté par cookie de session, appelle `POST /auth/rom/code` (avec un `label` optionnel : nom de l'appareil).
2. Le membre saisit le code dans le jeu. Le code expire après **15 minutes** et ne sert qu'une fois.
3. Le jeu appelle `POST /auth/rom/token` **sans authentification**. La réponse contient le token en clair, **affiché une seule fois**.
4. Le jeu lit la liste d'amis avec `Authorization: Bearer rom_…`.

Demander un nouveau code supprime les codes en attente précédents du même compte (et les codes expirés de tout le monde). Les appareils déjà liés ne sont pas touchés.

## 2. Authentification des routes

| Route | Identité requise |
|---|---|
| `POST /auth/rom/code`, `GET /auth/rom/devices`, `POST /auth/rom/devices/{device}/revoke` | Session OAPI (cookie `auth_token`). Le token de service est refusé (`403`), tout comme un token `rom_` (`401`). |
| `POST /auth/rom/token` | Aucune : le code à 6 chiffres sert de preuve. Protégé par un budget global d'échecs (voir §3). |
| `GET /friends/{account}` | Session, token de service **ou** token `rom_` (voir §5). |

Les routes à cookie suivent les règles de [friends.md](friends.md) : un `POST` portant un en-tête `Origin` doit venir du portail OAPI ou de `site_auth.url` (`site_auth.extra_urls` inclus), sinon `403 Cross-origin request rejected.`.

## 3. Routes

### `POST /auth/rom/code?label=…`

Génère un code de liaison pour le compte de la session. Pas de corps. `label` (optionnel) est nettoyé : caractères de contrôle et caractères Unicode invisibles retirés (espaces de largeur nulle, marques et forçages bidirectionnels `U+202A`–`U+202E`, `U+2066`–`U+2069`, BOM…), tronqué à **32 caractères**, puis espaces de bord supprimés.

Réponse `200` :

```json
{ "code": "483915", "expires_at": "2026-10-24 12:15:00.000Z", "device_id": "<id enregistrement>" }
```

Le code compte 6 caractères, chacun de `1` à `9` (jamais `0`). `device_id` est l'ID PocketBase de l'appareil, utilisable pour le révoquer une fois le token émis.

### `POST /auth/rom/token`

Échange un code contre un token. Corps JSON :

```json
{ "code": "483915" }
```

Réponse `200` :

```json
{ "token": "rom_<64 hex>", "device_id": "<id enregistrement>", "label": "Ma console" }
```

- `400 Code invalide.` si le code n'est pas exactement 6 chiffres de `1` à `9` (rejeté avant tout accès PocketBase).
- `404 Code inconnu ou expiré.` si le code n'existe pas, a déjà été utilisé ou a expiré. Aussi renvoyé à l'appelant qui perd une course : si deux échanges simultanés du même code passent, seul celui dont le hash est resté enregistré reçoit le token, l'autre reçoit `404` et doit relancer la liaison avec un nouveau code (voir §8).
- `429 Trop d'échecs de liaison. Réessayez plus tard.` avec `Retry-After` (secondes avant la fin de la fenêtre) : au-delà de **30 échanges échoués (`400` ou `404`) en 60 secondes, tous appelants confondus**, la route refuse toute demande, sans appeler PocketBase, jusqu'à la fin de la fenêtre. Les échanges réussis (et les erreurs `502`) ne comptent pas. Chaque tentative réserve une place avant d'interroger PocketBase et la rend si elle réussit : des tentatives simultanées ne peuvent donc pas dépasser le budget, mais pendant qu'elles sont en cours, elles peuvent faire refuser (`429`) une demande légitime.
- Le code est consommé à l'échange. Le token n'est **jamais** renvoyé ensuite : le perdre impose de relier l'appareil.

### `GET /auth/rom/devices`

Liste les appareils liés (token déjà émis) du compte de la session.

```json
{ "devices": [ { "id": "…", "label": "Ma console", "created": "…", "updated": "…", "is_disabled": false } ] }
```

Les codes en attente (token non encore émis) n'apparaissent pas. Le token n'est jamais exposé.

### `POST /auth/rom/devices/{device}/revoke`

Révoque un appareil du compte de la session : l'enregistrement est supprimé et son token cesse immédiatement de fonctionner. Pas de corps. Réponse `204`. `404 Appareil introuvable.` si l'ID n'existe pas ou appartient à un autre compte.

## 4. Format du token

`rom_` suivi de **64 caractères hexadécimaux minuscules** (32 octets aléatoires de l'OS), soit 68 caractères. OAPI ne stocke que le **hash SHA-256** (hex) du token complet, préfixe `rom_` inclus : une fuite de la base ne révèle aucun token utilisable. Un en-tête mal formé (mauvaise longueur, majuscules, espace final) est rejeté en `401` avant tout appel PocketBase.

Pour désactiver un appareil sans le supprimer, passer `is_disabled` à `true` dans PocketBase : le token est alors refusé (`401`). La révocation depuis l'API supprime l'enregistrement.

## 5. Ce que peut faire un token `rom_`

Un token `rom_` ne donne accès qu'à **`GET /friends/{account}`**, en lecture seule, sur **son propre compte** (`me` ou l'ID de son enregistrement `discord_users`). Il est traité comme un utilisateur de rôle `Normal` : viser un autre compte renvoie `403`. Toute autre route protégée (`/friends/.../request|accept|remove`, `/auth/rom/code`, `/auth/rom/devices`, etc.) le refuse avec `401`, sauf un `POST` portant un en-tête `Origin` étranger, rejeté d'abord par le contrôle d'origine (`403 Cross-origin request rejected.`).

Comme pour le token de service, un en-tête `Authorization` présent prend le pas sur le cookie : s'il commence par `Bearer rom_` et est invalide ou révoqué, la requête est refusée (`401`) sans essayer le cookie.

## 6. Codes d'erreur

| Code | Cause |
|---|---|
| `400` | Code de liaison invalide, ou ID d'appareil invalide |
| `401` | Pas de session valide, token `rom_` mal formé, inconnu, révoqué ou désactivé |
| `403` | Token de service sur les routes de liaison, `POST` cross-origin, ou token `rom_` visant un autre compte |
| `404` | Code inconnu ou expiré, appareil introuvable, ou compte introuvable |
| `429` | Quota dépassé : 10 requêtes par minute et par route, ou budget global d'échecs de `POST /auth/rom/token` épuisé (30 échecs / 60 s) ; `Retry-After` en secondes |
| `502` | PocketBase indisponible (détails dans les logs, jamais dans la réponse) |

Les réponses sont privées : `Cache-Control: no-store`.

## 7. Configuration PocketBase requise

Collection `discord_identity_token` :

| Champ | Type | Remarque |
|---|---|---|
| `discord_user` | relation vers `discord_users` | Propriétaire de l'appareil |
| `label` | texte | Nom de l'appareil |
| `linking_code` | texte | Code en attente, vidé après l'échange |
| `linking_code_expires_at` | date | Vidé après l'échange |
| `token` | texte | Hash SHA-256 du token. **Ne pas cocher « Nonempty »** : la valeur est vide tant que le code n'est pas échangé |
| `is_disabled` | booléen | `true` désactive le token |

Index uniques partiels (obligatoires, ils garantissent l'unicité malgré les valeurs vides) :

```sql
CREATE UNIQUE INDEX idx_identity_token ON discord_identity_token (token) WHERE token != ''
CREATE UNIQUE INDEX idx_identity_linking_code ON discord_identity_token (linking_code) WHERE linking_code != ''
```

Aucune règle d'accès publique n'est nécessaire : OAPI accède à la collection avec son compte administrateur.

## 8. Limites connues

- **Échanges simultanés** : PocketBase n'offre pas de mise à jour conditionnelle, deux échanges du même code peuvent donc écrire chacun leur hash. OAPI relit l'enregistrement après l'écriture : seul l'appelant dont le hash est resté reçoit le token, l'autre reçoit `404` et doit relancer la liaison. Cela couvre le cas courant (écritures qui se chevauchent), pas tous : si la seconde écriture arrive après la relecture de la première, le premier token reçu devient inutilisable (`401`) ; relancer alors la liaison avec un nouveau code.
- **Force brute du code** : il y a 9⁶ = 531 441 codes possibles. Le quota par IP (10 requêtes/min) ne suffit pas contre un attaquant disposant de nombreuses IP ; le budget global (30 échecs / 60 s) plafonne donc le débit total à environ 43 000 essais par jour. Avec un seul code en attente, la probabilité de le deviner pendant ses 15 minutes de validité est d'environ 450 / 531 441 ≈ 0,08 % ; elle augmente proportionnellement au nombre de codes en attente au même moment. Le risque n'est pas nul : un code ne doit être généré qu'au moment de lier l'appareil.
- **Déni de service** : le budget étant global, un attaquant qui envoie des codes faux en continu peut bloquer (`429`) toutes les liaisons légitimes. Les tokens déjà émis et le reste de l'API ne sont pas touchés. Le budget est par processus : plusieurs instances d'OAPI ont chacune le leur.

## 9. Exemples

```bash
# Site : générer un code (session)
curl -X POST -b "auth_token=<jwt>" "https://<host>/api/auth/rom/code?label=Ma%20console"

# Jeu : échanger le code contre un token (sans authentification)
curl -X POST -H "Content-Type: application/json" \
  -d '{"code":"483915"}' https://<host>/api/auth/rom/token

# Jeu : lire sa liste d'amis
curl -H "Authorization: Bearer rom_<64 hex>" https://<host>/api/friends/me

# Site : lister puis révoquer un appareil
curl -b "auth_token=<jwt>" https://<host>/api/auth/rom/devices
curl -X POST -b "auth_token=<jwt>" https://<host>/api/auth/rom/devices/<device_id>/revoke
```

Depuis le site Astroloutre (navigateur) :

```js
const res = await fetch(`${OAPI}/api/auth/rom/code?label=${encodeURIComponent(label)}`, {
  method: "POST",
  credentials: "include",
});
const { code, expires_at } = await res.json();
```
