# Tâches quotidiennes

OAPI exécute chaque jour deux tâches de maintenance en arrière-plan, dans cet ordre :

1. **purge** : suppression des comptes Discord dont la date de suppression est passée ;
2. **badges** : recalcul des badges de tous les comptes Discord et Minecraft.

Une tâche en échec n'empêche pas la suivante de s'exécuter.

## Horaire et configuration

Les tâches se lancent une fois par jour à l'heure `scheduler.run_at` (format `HH:MM`,
heure de Paris, `04:00` par défaut). Le bloc `scheduler` se règle dans `config.yaml`
(voir [configuration](./configuration.md)) :

```yaml
scheduler:
  enabled: true
  run_at: "04:00"
```

Avec `enabled: false`, aucune tâche n'est planifiée. Une valeur `run_at` invalide
empêche le démarrage d'OAPI. Au démarrage et après chaque passage, le log indique la
prochaine exécution : `Next daily tasks run at …`.

## Purge des comptes Discord

Sont purgés les `discord_users` dont `delete_at` est renseigné et déjà dépassé.
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

## Badges

La tâche réévalue les badges de chaque compte Discord puis de chaque joueur Minecraft,
avec les règles de [`badges.yaml`](./badges.md). Elle est silencieuse : aucune
notification n'est envoyée. Un `badges.yaml` invalide fait échouer cette tâche
(loggée) sans affecter la purge.

## Absence de rattrapage

Si OAPI est arrêté à l'heure prévue, le passage manqué n'est pas rattrapé au
redémarrage : les tâches s'exécutent à la prochaine échéance. Une panique pendant un
passage est loggée sans arrêter le serveur ni la boucle.

## Logs

Chaque tâche logge ses compteurs (traités, réussis, en échec) et les ID de records en
échec. Aucune donnée personnelle n'est écrite dans les logs.
