# Contrat de l'API Amis

Système d'amitié entre comptes Discord, stocké dans la collection PocketBase `discord_friendships`. Toutes les routes sont sous le préfixe configuré (`/api` par défaut) et nécessitent une authentification.

## 1. Authentification

Une requête doit présenter **l'une** de ces identités, sinon `401` :

| Identité | Comment | Usage |
|---|---|---|
| Session OAPI | Cookie `auth_token` (connexion Discord via `/auth/login` ou `/auth/site/login`) | Utilisateur connecté, portail ou site Astroloutre |
| Token de service | `Authorization: Bearer <OAPI_SERVICE_API_KEY>` | Bot, serveur Astro |

Règles :

- Un en-tête `Authorization` présent prend toujours le pas sur le cookie. S'il est invalide, la requête est refusée, le cookie n'est pas essayé.
- Le token de service a les droits `Admin` mais **ne peut pas utiliser `me`** : `400`. Il doit fournir un ID de compte explicite.
- Les requêtes `POST` portant un en-tête `Origin` doivent venir du portail OAPI ou de `site_auth.url`, sinon `403 Cross-origin request rejected.`. Les appels avec token de service n'ont pas cette vérification.
- Le site `site_auth.url` peut lire les réponses avec ses cookies (CORS avec `credentials`). Les autres origines restent bloquées par le navigateur.

## 2. Identifier un compte

`{account}` désigne le compte qui agit :

- `me` : le compte Discord de la session (résolu via `discord_id` du JWT). `404` si aucun enregistrement `discord_users` ne correspond.
- un **ID d'enregistrement PocketBase** `discord_users` (alphanumérique, 64 caractères max). Un utilisateur normal ne peut viser que son propre compte (`403` sinon). `Admin` et token de service peuvent viser n'importe quel compte.

`{user}` est toujours l'**ID d'enregistrement PocketBase** `discord_users` de l'autre membre, jamais un `discord_id` Discord.

## 3. Modèle de données

Collection `discord_friendships` :

| Champ | Description |
|---|---|
| `user_a`, `user_b` | Les deux membres. **`user_a` est toujours l'ID le plus petit** (ordre alphabétique) : l'index unique `(user_a, user_b)` couvre donc les deux sens. |
| `requested_by` | Membre qui a envoyé la demande |
| `status` | `pending` ou `accepted` |
| `accepted_at` | Date d'acceptation (vide tant que `pending`) |
| `created`, `updated` | Gérés par PocketBase |

États d'une relation :

```
(aucune) --request--> pending --accept--> accepted
   ^                     |                    |
   +------ remove -------+-------- remove -----+
```

Il n'existe pas d'état « refusé » : refuser ou annuler supprime l'enregistrement.

## 4. Routes

### `GET /friends/{account}`

Liste les relations du compte.

Réponse `200` :

```json
{
  "friends":  [ { "friendship_id": "…", "user": { … }, "status": "accepted", "since": "2026-10-24 12:00:00.000Z" } ],
  "incoming": [ { "friendship_id": "…", "user": { … }, "status": "pending",  "since": "…" } ],
  "outgoing": [ { "friendship_id": "…", "user": { … }, "status": "pending",  "since": "…" } ]
}
```

- `friends` : relations acceptées. `since` = date d'acceptation.
- `incoming` : demandes reçues, en attente de la réponse de ce compte. `since` = date de la demande.
- `outgoing` : demandes envoyées par ce compte.
- `user` est le profil public de **l'autre** membre : `id`, `username`, `discord_id`, `discord_tag`, `avatar_url`.
- Si l'autre compte a été supprimé, `user` ne contient que `id` (les autres champs sont vides) : la relation reste visible et peut être retirée.
- Tri : mises à jour les plus récentes d'abord. Pagination interne bornée à 20 pages de 500.

### `POST /friends/{account}/{user}/request`

Envoie une demande d'ami. Pas de corps.

| Situation | Résultat |
|---|---|
| Aucune relation | `200`, crée une relation `pending`, `requested_by = account` |
| `{user}` avait déjà envoyé une demande à `{account}` | `200`, la demande est **acceptée directement** (`status: accepted`) |
| Demande déjà envoyée par `{account}` | `409` |
| Déjà amis | `409` |
| `{user}` = `{account}` | `400` |
| `{user}` inexistant | `404` |

Réponse `200` : `{ "friendship_id": "…", "status": "pending" | "accepted" }`.

### `POST /friends/{account}/{user}/accept`

Accepte une demande **reçue** de `{user}`. Pas de corps. Remplit `accepted_at`.

Réponse `200` : `{ "friendship_id": "…", "status": "accepted" }`.
`404` s'il n'existe pas de demande en attente envoyée par `{user}` (y compris si c'est `{account}` qui l'a envoyée : on ne peut pas accepter sa propre demande, ni une relation déjà acceptée).

### `POST /friends/{account}/{user}/remove`

Supprime la relation avec `{user}`, quel que soit son état : retirer un ami, refuser une demande reçue, annuler une demande envoyée. Pas de corps.

Réponse `204` sans contenu. `404` si aucune relation n'existe.

> Les mutations sont des `POST` sans corps (pas de `DELETE`, pas de JSON) afin que le site Astroloutre puisse les appeler sans requête CORS préalable (preflight).

## 5. Codes d'erreur

| Code | Cause |
|---|---|
| `400` | ID invalide, auto-ajout, ou token de service utilisant `me` |
| `401` | Pas de session valide ni de token de service |
| `403` | Compte appartenant à un autre utilisateur, ou requête `POST` cross-origin |
| `404` | Compte, utilisateur, ou relation / demande introuvable |
| `409` | Déjà amis, ou demande déjà envoyée |
| `429` | Quota dépassé : 10 requêtes par minute et par route, `Retry-After` en secondes |
| `502` | PocketBase indisponible (détails dans les logs, jamais dans la réponse) |

Les réponses sont privées : `Cache-Control: no-store` et `X-Content-Type-Options: nosniff`.

## 6. Garanties

- L'identité de l'appelant vient exclusivement du cookie ou du token de service, jamais du corps ou de paramètres libres. Un utilisateur ne peut agir qu'en son nom.
- Un compte ne peut accepter qu'une demande qu'il a **reçue**.
- Une paire de comptes a au plus une relation, quel que soit le sens de la demande.
- Les opérations ne sont pas transactionnelles : deux `request` simultanées en sens opposés peuvent faire échouer l'une d'elles sur l'index unique (`502`) ; relancer l'appel donne alors le résultat attendu.

## 7. Exemples

```bash
# Liste de mes amis (session)
curl -b "auth_token=<jwt>" https://<host>/api/friends/me

# Envoyer une demande
curl -X POST -b "auth_token=<jwt>" https://<host>/api/friends/me/<id_autre>/request

# Le bot agit pour un compte précis
curl -X POST -H "Authorization: Bearer $OAPI_SERVICE_API_KEY" \
  https://<host>/api/friends/<id_compte>/<id_autre>/accept
```

Depuis le site Astroloutre (navigateur) :

```js
await fetch(`${OAPI}/api/friends/me/${userId}/request`, {
  method: "POST",
  credentials: "include",
});
```
