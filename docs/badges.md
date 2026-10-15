# Attribution des badges

`badges.yaml` définit les badges attribuables et leurs conditions. Chaque entrée
référence un badge existant dans la collection PocketBase `badges`, avec
`is_enabled = true`. Sans `rule`, le badge est manuel. Avec `rule`, il est
attribuable uniquement après vérification des statistiques par l'API.

Chaque entrée indique `kind: player` ou `kind: discord_user` et un `id` PocketBase
à remplir. Le nom du badge reste en commentaire pour identifier le palier.
Une entrée avec `id: ""` est inactive ; les autres entrées remplies fonctionnent
normalement. Les badges doivent déjà exister dans PocketBase. Le champ `kind`
du YAML distingue les badges Minecraft et Discord.
Le fichier est relu à chaque demande. Les sélecteurs dupliqués, champs inconnus et
règles invalides sont rejetés avant les écritures d'une évaluation.

Paliers configurés :

| Série | Paliers |
| --- | --- |
| Message I–V | 100, 1 000, 3 000, 8 000, 15 000 messages |
| Vocal I | Première présence en vocal attestée par les stats |
| Vocal II–V | 100, 500, 1 000, 2 000 heures |
| Temps de jeu Minecraft | 1, 50, 500, 1 000, 2 000 heures |
| Bienvenue dans l'Antre | `joined_at` renseigné sur le compte Discord |

Tous les paliers atteints sont attribués, même lors d'une première évaluation
tardive. Le badge « Une loutre qui ne prend pas de douche » est exclu.
« Aventure sans fin » reste manuel en attendant l'identification du serveur Vanilla.

## Appels

### Rafraîchissement par joueur

Sur la page d'accueil, la section **Mes badges** propose **Rafraîchir mes badges**
après connexion Discord. `POST /api/badges/me/refresh` retrouve les comptes depuis
la session : compte Discord et joueurs Minecraft dont `discord_user` référence
l'identifiant Discord ou son record PocketBase. Aucun ID n'est saisi côté navigateur.
La réponse contient `new_badges` (nom, description, image et compte concerné),
`accounts_checked` et `failed_accounts`. Les erreurs par compte sont signalées
sans masquer les badges obtenus sur les autres comptes.

Un membre connecté peut demander le recalcul des badges de ses propres comptes :

- `POST /api/badges/players/{player}/refresh` pour Minecraft ;
- `POST /api/badges/discord/{user}/refresh` pour Discord.

Les exemples utilisent la base `/api`, à adapter à la configuration. Les IDs sont
ceux des records PocketBase. Aucun corps de requête n'est nécessaire. La session
`auth_token` est requise. L'API vérifie la propriété du compte ; un administrateur
peut rafraîchir n'importe quel joueur. Minecraft accepte les liens `discord_user`
stockés comme identifiant Discord ou comme relation vers le record Discord.

Chaque appel relit les statistiques et les règles YAML, puis enregistre les badges
éligibles manquants. La réponse est une liste `{ "badge": "ID", "created": true }` ;
`created: false` indique un badge déjà obtenu. Les IDs vides et les badges désactivés
ne sont pas attribués. Le rafraîchissement ne retire pas les badges acquis.
Les badges déjà acquis sont chargés en une seule lecture et ignorés avant la
résolution de leur définition et le calcul de leurs conditions. Un badge acquis
supprimé du catalogue PocketBase ne bloque donc pas les autres attributions.
La correspondance par ID ne dépend pas du libellé `plateform` ; `kind` dans le YAML
détermine le type du destinataire. Les UUID Minecraft avec ou sans tirets sont acceptés.

### Administration

Les routes sont préfixées par la base API configurée et exigent une session Admin
existante (`auth_token`). Après enregistrement des statistiques, le service appelant
doit demander `POST /admin/badges/players/{player}/evaluate`, où `player` est l'ID
PocketBase du joueur. Le corps est vide. L'API lit elle-même `players_stats` et
retourne les badges éligibles avec `created: true` ou `false` si déjà acquis.
Les statistiques reçues du client ne servent jamais de preuve d'obtention.

Pour Discord, demander `POST /admin/badges/discord/{user}/evaluate`, où `user`
est l'ID PocketBase du compte. L'API lit `discord_user_stats` avec la même
agrégation que les graphiques : somme de `message_count` et `vocal_time`.
Vocal I nécessite un `vocal_time > 0` ou une liste `voice_channels` non vide ; une
connexion jamais enregistrée dans les statistiques ne peut pas être reconstituée.

Attribution manuelle : `POST /admin/badges/award` avec un corps JSON :

```json
{"kind":"player","recipient":"PLAYER_RECORD_ID","badge":"BADGE_RECORD_ID"}
```

Utiliser `kind: "discord_user"` pour un compte Discord. Les destinataires sont
toujours des IDs de records PocketBase. Le badge doit correspondre au type du
destinataire. Une attribution acquise n'est pas retirée si les stats baissent.

## Règles

`minimum` est un seuil positif inclusif. Les valeurs sont additionnées entre les
records du joueur, ou filtrées par `server` si précisé. Les unités sont celles
stockées dans PocketBase : `playtime` en ticks (72 000 ticks/heure), `vocal_time`
en heures décimales (nombre ou chaîne numérique). L'agrégation correspond à celle
des graphiques existants : les records doivent représenter des contributions
additionnables, pas des snapshots cumulatifs du même compteur.

Champs : `playtime`, `deaths`, `mob_kills`, `player_kills`, `blocks_mined`,
`blocks_placed`, `total_distance`, `distance_walked`, `distance_elytra`,
`distance_fligth` (orthographe existante). Les compteurs JSON acceptent une clé :
`mobs_killed.minecraft:zombie`, `items_crafted.minecraft:diamond`,
`items_broken.minecraft:diamond_pickaxe`.
Discord accepte aussi `message_count`, `vocal_time` et les preuves calculées
`voice_joined` et `membership`.

## Configuration PocketBase obligatoire avant mise en service

L'API écrit dans `badges_earned`, avec les champs existants `badge` et `player`
ou `discord_user`. Aucun nouveau champ de collection n'est requis.

Pour garantir l'absence de doublons entre plusieurs instances API, ajouter les
index uniques suivants après nettoyage des doublons existants :

```sql
CREATE UNIQUE INDEX idx_badges_player ON badges_earned (player, badge)
WHERE player != '';
CREATE UNIQUE INDEX idx_badges_discord ON badges_earned (discord_user, badge)
WHERE discord_user != '';
```

Verrouiller les règles PocketBase Create/Update/Delete de `badges_earned` (`null`,
réservé aux superusers). Retirer aux anciens clients toute écriture directe et
tout identifiant superuser. Seule OAPI doit disposer de ces identifiants.
Cela est nécessaire pour imposer le passage par l'API ; une route seule ne peut
pas interdire les écritures directes dans une base laissée accessible.

Les attributions utilisent les IDs de records canoniques ; la recherche reconnaît
aussi les anciennes attributions utilisant un UUID ou un identifiant Discord.
Une évaluation peut être partiellement enregistrée en cas de panne PocketBase :
elle peut être relancée pour reprendre les attributions manquantes.

Déploiement : inclure `badges.yaml` dans le répertoire de travail du serveur.
