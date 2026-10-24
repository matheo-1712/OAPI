# Tâches quotidiennes OAPI — conception

## Intention

OAPI doit exécuter seule, chaque jour, des tâches de maintenance, sans action
d'un membre ou d'un admin. Deux tâches pour commencer :

1. **Purge des comptes Discord à supprimer** : quand la date `delete_at` d'un
   compte `discord_users` est dépassée, effacer définitivement le compte et ses
   données Discord.
2. **Recalcul des badges** de tous les comptes, pour que les badges mérités
   soient attribués même si personne ne clique sur « Rafraîchir mes badges ».

D'autres tâches viendront s'ajouter plus tard : le mécanisme doit permettre
d'en ajouter une sans le retoucher.

## Objectifs

- Une boucle de fond, lancée au démarrage d'OAPI, qui exécute la liste des
  tâches une fois par jour à une heure configurable.
- La purge respecte le périmètre et l'ordre décrits ci-dessous.
- Le recalcul des badges réutilise la logique d'évaluation existante.
- Chaque exécution laisse dans les logs un résumé par tâche (comptes traités,
  supprimés, badges créés, erreurs).

## Hors périmètre

- Notification des membres : les nouveaux badges sont attribués en silence et
  vus sur le site.
- Suppression des comptes Minecraft liés et de leurs stats (gardés pour l'instant).
- Rattrapage d'une exécution manquée si OAPI était arrêté à l'heure prévue.
- Déploiement multi-instance (la production prévoit une seule instance, voir
  `docs/production.md`).

## Anti-objectifs

- Supprimer un compte dont `delete_at` est vide ou dans le futur.
- Supprimer le compte `discord_users` avant ses données liées, ce qui laisserait
  des lignes orphelines impossibles à rattacher.
- Retirer des badges déjà acquis.
- Qu'une erreur sur un compte arrête le traitement des autres, ou fasse planter
  le serveur HTTP.

## Contraintes

- Single instance en production : une boucle interne ne s'exécute qu'une fois.
- PocketBase est la seule source de données ; client admin `PocketbaseClient`
  (`login`, `list_all_records`, `update_record`, `delete_record`).
- Les liens `discord_user` peuvent contenir l'ID du record PocketBase **ou** le
  snowflake Discord (anciennes données, cf. `badges.md` et
  `badge_service.rs:72-80`). La purge doit couvrir les deux formes.
- L'image Docker (`debian:bookworm-slim`) n'embarque pas `tzdata`.

## Approches envisagées

- **A. Boucle intégrée à OAPI (choisie).** Tâche tokio lancée dans `main`,
  qui dort jusqu'à l'heure prévue puis exécute la liste des tâches. Pas
  d'authentification machine à créer, accès direct au client PocketBase admin.
  Limite acceptée : une exécution manquée n'est pas rattrapée, mais le passage
  du lendemain rattrape tout (purge et badges sont idempotents).
- **B. Crate `tokio-cron-scheduler`.** Expressions cron plus souples, mais une
  dépendance de plus pour un simple rythme quotidien.
- **C. Cron externe appelant des routes admin.** Garde le serveur sans tâche de
  fond, mais exige un nouveau mode d'authentification machine pour une action
  destructrice.

Choix de l'utilisateur : **A**.

## Décisions

### Planificateur

- Nouveau module (ex. `src/services/scheduler.rs`) : une liste ordonnée de
  tâches, chacune avec un nom et une fonction async qui reçoit un
  `PocketbaseClient` connecté et renvoie un résumé ou une erreur.
- Ordre d'exécution : **purge puis badges**, pour ne pas évaluer un compte sur
  le point d'être supprimé.
- Chaque tâche s'exécute même si la précédente a échoué ; l'échec est loggé.
- Une seule connexion PocketBase (`login`) par exécution ; si elle échoue,
  l'exécution est abandonnée et loggée, la suivante a lieu le lendemain.
- La boucle tourne dans `tokio::spawn` ; une panique ou une erreur ne doit pas
  affecter le serveur HTTP.
- Configuration dans `default_config.yaml` / `config.yaml` :

  ```yaml
  scheduler:
    enabled: true
    run_at: "04:00"        # heure locale de Paris
  ```

  `enabled: false` désactive la boucle. Section absente → valeurs par défaut.

### Purge

Sélection : `discord_users` avec `delete_at != '' && delete_at <= <maintenant UTC>`.

Pour chaque compte (ID record `R`, snowflake `S`), dans cet ordre :

| Étape | Collection | Action |
| --- | --- | --- |
| 1 | `discord_user_stats` | supprimer les lignes `discord_user = R` ou `= S` |
| 2 | `badges_earned` | supprimer les lignes `discord_user = R` ou `= S` |
| 3 | `discord_friendships` | supprimer les lignes `user_a = R` ou `user_b = R`, tout statut |
| 4 | `discord_identity_token` | supprimer les lignes `discord_user = R` |
| 5 | `players` | **ne pas supprimer** : mettre `discord_user` à `""` pour les joueurs liés à `R` ou `S` |
| 6 | `discord_users` | supprimer le record `R` |

- Les `players_stats` et badges Minecraft (`badges_earned.player`) sont conservés.
- Une erreur sur une étape arrête le traitement **de ce compte** (le record
  `discord_users` n'est pas supprimé) ; le compte sera repris le lendemain.
  Les autres comptes continuent.
- Résumé loggé : comptes purgés, comptes en échec (ID record seulement, pas de
  données personnelles dans les logs).

### Badges

- Pour chaque record `discord_users` : `badge_service::evaluate_discord`.
- Pour chaque record `players` : `badge_service::evaluate`.
- On n'utilise pas `refresh_discord_account` pour éviter d'évaluer deux fois
  les joueurs liés.
- Les définitions `badges.yaml` sont lues comme aujourd'hui ; une erreur de
  configuration YAML fait échouer la tâche badges (loggée), pas la purge.
- Résumé loggé : comptes évalués, badges créés, comptes en échec.

## Mes choix (à vérifier)

- **Fuseau horaire** : pour respecter « 04:00 heure de Paris » toute l'année
  (heure d'été comprise) sans installer `tzdata` dans l'image, ajout de la
  crate `chrono-tz` (`Europe/Paris`). Contrairement à ce qui était annoncé,
  c'est donc une petite dépendance en plus. Alternative : heure fixe en UTC
  (`03:00` UTC = 04:00 en hiver, 05:00 en été), sans dépendance.
- **Pas de route admin de déclenchement manuel** pour l'instant : la purge est
  destructrice et l'ouvrir via HTTP élargit la surface. Les tests se font avec
  des mocks PocketBase (`mockito`) et en réglant `run_at` en local.
- Les comptes sont traités séquentiellement (pas de parallélisme) pour ménager
  PocketBase.

## Laissé au développeur

- Nom exact du module et des types, format du résumé de log.
- Découpage des fonctions de suppression par collection.
- Tests : sélection par `delete_at` (passée / future / vide), ordre des
  suppressions, arrêt sur erreur sans supprimer le compte, liens `players`
  vidés et non supprimés, double forme `R`/`S`, calcul du prochain réveil.
- Documentation : `docs/daily_tasks.md` et mise à jour de `docs/configuration.md`.
