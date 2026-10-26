# Liaison d'une sauvegarde Mario Kart DS (`POST /saves/mkds/link`)

Le site (ADL ROM Patcher) envoie une sauvegarde Mario Kart DS ; OAPI renvoie une **copie** contenant un identifiant d'appareil ADLN enregistré dans `adln_devices` pour le joueur **DS Games** du membre connecté. La progression n'est pas touchée.

Ce n'est pas une preuve que la connexion du jeu fonctionne : la lecture de l'identifiant par la ROM et la connexion authentifiée ADLN/2 sont des travaux séparés.

## 1. Requête

- `POST /api/saves/mkds/link`, `multipart/form-data`, un seul champ `save` (le fichier). Tout autre champ (`player_id`, `discord_id`…) est refusé (`400`) : l'identité vient uniquement de la session.
- Session OAPI (cookie `auth_token`) obligatoire. Le token de service est refusé (`403`), tout comme un token `rom_` (`401`).
- En-tête `Origin` **obligatoire** et égal au portail OAPI ou à une origine de `site_auth` (`403` sinon). La requête `multipart` est une requête simple : aucun preflight CORS n'est nécessaire.
- Corps limité à une sauvegarde (256 Kio + 16 Kio d'enveloppe). Quota habituel : 10 requêtes par minute.

Réponse `200` : `application/octet-stream`, `Content-Disposition: attachment`, `Cache-Control: no-store`, `X-ADLN-Link: created` (nouvel appareil) ou `reused` (sauvegarde déjà liée). Rien n'est publié à une URL : le fichier n'existe que dans cette réponse.

| Code | Cause |
|---|---|
| `400` | Pas une sauvegarde brute (taille ≠ 262 144 octets, magie ≠ `MKDSSV10`, état d'émulateur, `.dsv`, octets ajoutés), ou formulaire invalide |
| `401` | Pas de session |
| `403` | Origine absente ou étrangère, token de service, ou sauvegarde liée à **un autre compte** |
| `409` | Copies contradictoires, appareil révoqué, appareil inconnu, ou enregistrement qui ne correspond pas (joueur, jeu, secret) |
| `422` | Données inconnues dans la zone réservée, ou deux copies endommagées |
| `502` | PocketBase ou le journal indisponible : réessayer **avec le même fichier** |
| `503` | `ADLN_DEVICE_WRAPPING_KEY` absente ou invalide |

## 2. Création automatique du compte

À chaque connexion Discord (`/auth/callback`) puis, par sécurité, à chaque liaison :

1. `discord_users` est cherché par `discord_id` (chaîne). S'il manque, il est créé avec `discord_id`, `username` (nom affiché Discord, sinon nom d'utilisateur), `discord_tag` (nom d'utilisateur Discord) et `avatar_url`. Un enregistrement existant n'est jamais modifié (rôles, `is_admin`… préservés). **Conséquence : tout compte Discord peut désormais se connecter à OAPI** ; il obtient le rôle `Normal`.
2. La plateforme dont `name` vaut exactement `DS Games` est résolue (une seule doit exister).
3. Le joueur est cherché par `discord_user` **et** `platform`. S'il manque, il est créé avec `account_id = "NA"` (identifiant ADL des joueurs DS Games) et `playername` = nom affiché. Un joueur existant est réutilisé tel quel, `account_id` compris. `first_connected_at` et `last_connected_at` ne sont jamais écrits : utiliser le site n'est pas une connexion de jeu.

Un échec de création du joueur ne bloque pas la connexion. Les créations sont sérialisées par compte dans le processus, puis relues : si un autre service a créé la même ligne en parallèle, la plus ancienne est conservée et celle qu'OAPI vient de créer est supprimée. Index uniques recommandés (ils rendent ce cas impossible, OAPI réutilise alors la ligne existante) :

```sql
CREATE UNIQUE INDEX idx_discord_users_discord_id ON discord_users (discord_id)
CREATE UNIQUE INDEX idx_players_discord_platform ON players (discord_user, platform) WHERE discord_user != ''
```

Ne **pas** créer d'index unique sur `(platform, account_id)` : tous les joueurs DS Games partagent `account_id = "NA"`.

## 3. Identifiant d'appareil

Nouvel appareil : `id` de 15 caractères `[a-z0-9]` et secret de 32 octets, tirés du générateur de l'OS. Enregistrement `adln_devices` :

| Champ | Valeur |
|---|---|
| `id` | identifiant public de l'appareil |
| `player` | joueur DS Games de la session |
| `secret_ciphertext` | `"v1." + base64url_sans_padding(nonce[12] ‖ chiffré[32] ‖ tag[16])`, 83 caractères |
| `revoked` | `false` |
| `games` | `["MKDS"]` |

Chiffrement AES-256-GCM avec la clé `ADLN_DEVICE_WRAPPING_KEY` du serveur de jeu, nonce aléatoire par chiffrement, données associées `UTF8("ADLN/device-secret/1") ‖ 0x00 ‖ id ‖ 0x00 ‖ player` (sans NUL final). La sauvegarde contient le secret brut ; PocketBase uniquement sa forme chiffrée. La clé et le compte PocketBase ne quittent jamais OAPI et ne sont jamais journalisés.

Règles d'accès de `adln_devices` : **toutes vides (administrateurs uniquement)**. Aucune règle publique : OAPI y accède avec son compte administrateur.

## 4. Format dans la sauvegarde

Sauvegarde EUR brute, exactement `0x40000` octets commençant par `MKDSSV10`. Seuls les octets `[0x3E000, 0x3E200)` changent : page A `0x3E000`, page B `0x3E100`, identiques pour un nouvel identifiant. Le journal TLS (`0x3F000`/`0x3F100`), les données Wi-Fi et les sections du jeu sont recopiés tels quels ; aucune somme de contrôle du jeu n'est recalculée.

Page de 256 octets, little-endian : `ADLC`, version `1`, taille `128`, `MKDS`, drapeaux `0`, génération (`1`), compteur de connexions (`0`), ID d'appareil (15 + NUL), ID du joueur (15 + NUL), secret (32), réservé (28 × 0), CRC32 `zlib` des octets `[0x00, 0x7C)`, puis 128 octets nuls.

Avant de répondre, OAPI relit les deux pages, vérifie la longueur et compare chaque octet hors zone avec l'envoi.

## 5. Sauvegarde déjà liée

- Page entièrement `0x00` ou `0xFF` : vide. Données inconnues ou version non prise en charge : refus.
- Une page `ADLC` v1 dont le CRC (ou le remplissage) est faux est endommagée et restaurée depuis sa copie valide ; une copie vide à côté d'une copie valide aussi. Deux copies inutilisables : refus.
- Deux copies valides doivent avoir le même appareil, joueur et secret ; la génération la plus haute l'emporte, une égalité doit être identique.
- Le joueur de la sauvegarde doit être **celui de la session** (sinon `403`), l'appareil exister, appartenir à ce joueur, autoriser `MKDS`, ne pas être révoqué, et son secret déchiffré doit être identique. L'identifiant est alors réutilisé avec ses compteurs, sans nouvel appareil.
- Pas de transfert de propriété, pas de réactivation d'un appareil révoqué, pas d'enregistrement d'un identifiant inconnu fourni par une sauvegarde.

## 6. Reprise et idempotence

Avant de créer l'appareil, OAPI écrit son `id` dans un journal privé (`ADLN_LINK_JOURNAL_DIR`, `data/adln_links` par défaut, volume `adln_links` en Docker). Le nom du fichier est un HMAC (clé dérivée de la clé d'enveloppe) du `discord_id`, du joueur et du SHA-256 de la sauvegarde envoyée ; il ne contient aucun secret. Les entrées sont conservées 30 jours.

- Réponse PocketBase perdue ou délai dépassé : l'appareil est relu ; s'il existe, il est utilisé, sinon `502` et le prochain envoi **du même fichier** crée le même `id`.
- Téléchargement échoué : renvoyer le même fichier rend le même appareil et les mêmes octets.
- Le secret est toujours celui stocké dans PocketBase, déchiffré : le journal n'en a pas besoin.

Limites : verrous et journal sont propres à une instance (ne pas lancer plusieurs instances d'OAPI sur des journaux différents). Un appareil créé dont le fichier n'est jamais téléchargé reste enregistré pour ce joueur ; l'administrateur peut le révoquer.
