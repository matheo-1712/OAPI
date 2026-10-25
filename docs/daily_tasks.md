# Tâches quotidiennes

OAPI exécute chaque jour trois tâches de maintenance en arrière-plan, dans cet ordre :

1. **purge** : suppression des comptes Discord dont la date de suppression est passée ;
2. **badges** : recalcul des badges de tous les comptes Discord et Minecraft ;
3. **skins** : mise à jour de la tête et du skin des joueurs Minecraft.

Une tâche en échec n'empêche pas la suivante de s'exécuter.

## Horaire et configuration

Les tâches se lancent une première fois au démarrage d'OAPI, puis une fois par jour à
l'heure `scheduler.run_at` (format `HH:MM`, heure de Paris, `04:00` par défaut). Le
passage du démarrage se fait en arrière-plan : le serveur HTTP répond pendant ce temps.
Les deux passages ne se chevauchent jamais. Le scheduler est désactivé dans
`default_config.yaml` et activé en production par `deploy/config.production.yaml`. Le bloc
`scheduler` se règle dans `config.yaml` (voir [configuration](./configuration.md)) :

```yaml
scheduler:
  enabled: true
  run_at: "04:00"
```

Avec `enabled: false`, aucune tâche n'est lancée, ni au démarrage ni ensuite. Une valeur
`run_at` invalide empêche le démarrage d'OAPI. Le log indique `Running daily tasks at
startup`, puis, après chaque passage, la prochaine exécution : `Next daily tasks run at …`.

Si le scheduler est activé alors que `server.env` n'est pas `production`, OAPI logge un
avertissement au démarrage : la purge s'exécutera tout de suite, puis chaque jour, sur le
PocketBase configuré. Avec `scheduler.enabled: true` dans un `config.yaml` local, chaque
`cargo run` lance donc une purge.

## Purge des comptes Discord

Sont purgés les `discord_users` dont `delete_at` est renseigné et déjà dépassé.
`discord_users.delete_at` doit être un champ PocketBase de type `date`. OAPI revérifie la
date avant de purger : un compte dont `delete_at` est futur, vide ou illisible n'est
jamais supprimé (il est compté en échec et loggé par son ID).

**Plafond.** Au-delà de 100 comptes à purger dans un même passage (`MAX_PURGE_PER_RUN`
dans `purge_service.rs`), rien n'est purgé et la tâche logge une erreur
(`N accounts due for deletion, above the limit of 100; nothing purged`). Si ce volume est
légitime, relever la constante.
Pour chaque compte (record `R`, identifiant Discord `S`), dans cet ordre :

| Étape | Collection | Action |
| --- | --- | --- |
| 1 | `discord_user_stats` | supprimer les lignes `discord_user = R` ou `= S` |
| 2 | `badges_earned` | supprimer les lignes `discord_user = R` ou `= S` |
| 3 | `discord_friendships` | supprimer les lignes `user_a = R` ou `user_b = R`, tout statut |
| 4 | `discord_identity_token` | supprimer les lignes `discord_user = R` |
| 5 | `players` | ne pas supprimer : mettre `discord_user` à `""` pour les joueurs liés à `R` ou `S` |
| 6 | `discord_users` | supprimer le record `R` |

### Comptes Minecraft conservés

Les comptes Minecraft ne sont jamais supprimés : seul le lien `players.discord_user`
est vidé. Leurs statistiques (`players_stats`) et leurs badges (`badges_earned.player`)
sont conservés.

### Reprise en cas d'échec

Une erreur sur une étape arrête le traitement de ce compte seul : le record
`discord_users` n'est pas supprimé, et le compte est repris le lendemain. Les autres
comptes continuent. Si la tâche entière échoue (liste illisible, connexion PocketBase
impossible), le prochain passage a lieu le lendemain.

Un arrêt d'OAPI pendant un passage laisse au plus un compte partiellement purgé (le
record `discord_users` est supprimé en dernier) ; il est repris le lendemain.

## Badges

La tâche réévalue les badges de chaque compte Discord puis de chaque joueur Minecraft,
avec les règles de [`badges.yaml`](./badges.md). Elle est silencieuse : aucune
notification n'est envoyée. Un `badges.yaml` invalide fait échouer cette tâche
(loggée) sans affecter la purge.

## Skins Minecraft

Pour chaque joueur Minecraft (`players.account_id` est un UUID ; les comptes Steam sont
ignorés), la tâche lit le skin actuel chez Mojang et le compare à `players.skin_hash`
(`v2:<hash>:<modèle>`, par exemple `v2:abc123:classic`). Si le skin a changé, ou si `head` ou
`skin` est vide, elle génère la tête (128×128) et récupère la texture brute du skin (PNG 64×64, ou 64×32
pour les anciens skins), puis les envoie dans `players.head` et `players.skin` avec le
nouveau `skin_hash`. Un skin inchangé n'est pas renvoyé. Le passage au format `v2:`
régénère tous les joueurs au premier passage de la tâche.

**Fichiers remplacés.** Quand un skin change, PocketBase enregistre les nouveaux fichiers
sous un nouveau nom et supprime les anciens. Le site statique Astroloutre affiche donc une
image de tête ou de skin cassée pour ce joueur jusqu'à son prochain build.

**Ordre de déploiement du passage à `v2`.** Déployer OAPI d'abord et laisser un passage
complet de la tâche `skins` se terminer (1,5 s par joueur, arrêt sur un `429` Mojang puis
reprise le lendemain : cela peut donc dépasser une journée). Ensuite seulement, builder et
déployer le site. Sinon, chaque tête du site en ligne pointe vers des fichiers supprimés et
retombe sur mc-heads jusqu'au prochain build. Tant que ce passage n'a pas eu lieu, le build du
site logge `[skin3d] <joueur>: dimensions invalides (128x256)` pour chaque joueur : c'est
attendu, ce n'est pas une régression.

**Délai et limite Mojang.** Les requêtes à Mojang sont espacées de 1,5 s (`MOJANG_DELAY`
dans `skin_service.rs`), et un UUID présent sur plusieurs records n'est interrogé qu'une
fois. Si Mojang répond `429`, la tâche s'arrête, logge un avertissement et renvoie le
résumé partiel : les joueurs non traités sont repris le lendemain.

**Joueurs inconnus.** Un joueur inconnu de Mojang (profil supprimé ou inexistant) ou sans
skin personnalisé est ignoré : ses images existantes ne sont pas effacées. Il compte en
réussi.

**Compteurs.** `processed` est le nombre de records Minecraft examinés avant l'arrêt
éventuel, `succeeded` ceux à jour ou mis à jour, `failed` les ID de records en échec
(profil illisible, téléchargement, rendu ou envoi). Les records du groupe qui a reçu un
`429` comptent dans `processed` mais ni en réussi ni en échec. Le log indique en plus le
nombre de records mis à jour.

## Redémarrage et rattrapage

Si OAPI est arrêté à l'heure prévue, le passage du démarrage rattrape le passage manqué.
Chaque redémarrage (déploiement, crash) relance donc les tâches ; elles sont sans effet
sur ce qui est déjà fait (badges déjà acquis, comptes déjà purgés). Une panique pendant
un passage est loggée sans arrêter le serveur ni la boucle.

## Logs

Chaque tâche logge ses compteurs (traités, réussis, en échec) et les ID de records en
échec. Aucune donnée personnelle n'est écrite dans les logs.
