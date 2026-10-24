# Authentification de la ROM par code de liaison

## Intention

Un joueur lie sa ROM (ou un émulateur, un autre jeu, un autre appareil) à son compte Discord sans saisir d'identifiants en jeu. Sur rom-patcher, connecté via Discord, il génère un code court ; il le saisit dans le jeu, qui reçoit un token durable. Ce token identifie le joueur auprès d'OAPI : aujourd'hui pour lire sa liste d'amis, plus tard pour ses statistiques et son classement.

## Objectifs

- Code de liaison de **6 chiffres de 1 à 9**, valable **15 minutes**, à usage unique.
- **Plusieurs appareils** liés en même temps : chaque échange de code crée un nouveau token, les autres restent valables.
- Le joueur voit ses appareils liés sur rom-patcher et peut en révoquer un.
- Un token `rom_…` donne accès en lecture à la liste d'amis du joueur, et à rien d'autre.

## Non-objectifs

- Statistiques et classement en jeu (viendront plus tard, en s'appuyant sur la même identité).
- Mutations d'amitié depuis le jeu (request / accept / remove).
- Expiration automatique des tokens.

## Anti-objectifs

- Un token `rom_` confondu avec le token de service, ou acceptant une autre route que la liste d'amis.
- Un token en clair dans PocketBase.
- Un code utilisable après 15 minutes ou deux fois.

## Contraintes

- Collection PocketBase `discord_identity_token` : `discord_user` (relation `discord_users`, Nonempty), `label`, `token`, `linking_code`, `is_disabled`, `linking_code_expires_at`, `created`, `updated`.
- Index à créer dans PocketBase : `token` unique partiel (`WHERE token != ''`), `linking_code` unique partiel (`WHERE linking_code != ''`). Pas d'index unique sur `discord_user`.
- rom-patcher est dans `site_auth.extra_urls` : session cookie `SameSite=None`, CORS avec credentials via `site_cors`, vérification d'origine des `POST`.
- Quota global : 10 requêtes par minute par route et par identité / IP.
- Le jeu n'est pas un navigateur : pas de CORS ni de cookie côté jeu, un corps JSON est possible.

## Approches

- **A. Une ligne par joueur** (code et token sur la même ligne) : la plus simple, mais un seul appareil lié. Écartée : on veut plusieurs jeux / appareils.
- **B. Une ligne par appareil** : retenue. La ligne naît à la génération du code (token vide), reçoit le hash du token à l'échange.
- **C. Token JWT signé sans base** : écartée, aucune révocation possible via `is_disabled` ou suppression.

## Fonctionnement

### Cycle d'une ligne

```
génération du code      -> ligne { linking_code, linking_code_expires_at = now + 15 min, token = '' }
échange du code (jeu)   -> ligne { token = sha256(token), linking_code = '', linking_code_expires_at = '' }
révocation (site)       -> ligne supprimée
is_disabled = true      -> token refusé (coupure manuelle par un admin)
```

### Routes

| Route | Appelant | Auth | Résultat |
|---|---|---|---|
| `POST /auth/rom/code?label=…` | rom-patcher | cookie de session (`me` implicite), `site_cors`, vérif. d'origine | `{ code, expires_at, device_id }` |
| `POST /auth/rom/token` corps `{ "code": "123456" }` | jeu | aucune | `{ token, device_id, label }` |
| `GET /auth/rom/devices` | rom-patcher | cookie de session | appareils liés (`id`, `label`, `created`, `updated`, `is_disabled`) |
| `POST /auth/rom/devices/{id}/revoke` | rom-patcher | cookie de session | `204`, ligne supprimée ; `404` si elle n'appartient pas au joueur |
| `GET /friends/me` (et `/friends/{son id}`) | jeu | `Authorization: Bearer rom_…` | liste d'amis existante |

- Générer un code supprime d'abord les lignes **sans token** du joueur (codes expirés ou en attente) : un seul code en attente à la fois, et pas de lignes orphelines.
- Le code est tiré aléatoirement parmi les 531 441 combinaisons ; en cas de collision avec un code actif (index unique), on retire, avec une limite d'essais.
- L'échange refuse un code inconnu ou expiré avec la même réponse (`404`), pour ne rien révéler.
- `label` : facultatif, texte court (≤ 32 caractères, nettoyé), saisi sur le site au moment de générer le code.
- Les routes de session refusent le token de service (pas de compte personnel).

### Token

- Format : `rom_` + 32 octets aléatoires encodés en hexadécimal.
- Stocké : `sha256` hexadécimal du token complet. Le token en clair n'est rendu qu'une fois, à l'échange.
- Vérification : un en-tête `Authorization: Bearer rom_…` est cherché par hash dans `discord_identity_token` (`is_disabled = false`), puis le `discord_users` lié donne le `discord_id`. Les claims produits ont `sub = discord_id`, rôle `Normal`.
- Seul `GET /friends/:account` accepte ces tokens. Partout ailleurs, un `Bearer rom_…` donne `401`, et il n'est jamais comparé au token de service.

## Décisions de Claude (à vérifier)

- Révoquer = supprimer la ligne ; `is_disabled` reste la coupure manuelle admin.
- Le label est saisi sur le site, pas par le jeu.
- Un seul code en attente par joueur à la fois.
- Pas de plafond sur le nombre d'appareils liés.
- Routes sous `/auth/rom/*`.

## Laissé au développeur

- Organisation interne (action `rom_auth_actions`, handler, modèles), messages d'erreur, documentation `docs/rom_auth.md`, tests.
