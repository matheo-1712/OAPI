# Statistiques Discord

Routes sous le préfixe configuré (`/api` par défaut) :

- `GET /discord/stats/me` : historique personnel, identité issue exclusivement du JWT `auth_token`. Tous les rôles connectés disposent du même accès personnel. Aucun identifiant de membre ne peut être fourni pour changer de cible.
- `GET /admin/discord/stats` : agrégation de tous les enregistrements Discord et statistiques complètes de chaque joueur, réservée au rôle `Admin`. `Normal` et `LoutreInvesti` n'y ont pas accès.

La réponse globale conserve les champs cumulés à la racine et ajoute `players` : chaque entrée contient `id` (PocketBase), `discord_id`, `username` et `stats` (mêmes champs que la réponse personnelle, historique journalier compris). Les références PocketBase et Discord d'un même joueur sont regroupées. Les joueurs sans activité sont inclus avec des totaux nuls. Les références à des comptes supprimés restent incluses avec leur référence stockée dans `id` et une identité `null`. La liste est triée par `id` croissant et partage le cache global de 60 secondes.

La réponse JSON contient `total_messages`, `total_vocal_hours`, `record_messages_day`, `record_vocal_day`, `favorite_text_channel`, `favorite_voice_channel`, `channel_counters_complete`, `daily` (dates triées) et `fetched_at` (UTC). Les durées sont des heures décimales. Les journées utilisent la date métier `date_stats` déjà enregistrée ; aucune conversion de fuseau n'est appliquée à ces agrégats journaliers. Le total global mesure des heures-utilisateurs en vocal, pas la durée d'occupation unique du serveur.

Les compteurs d'une même date sont additionnés avant de calculer les records. Les salons sont regroupés par ID, avec somme de `nb_message` pour le texte et de `vocal_time` pour le vocal. À égalité, priorité à la date la plus ancienne ou au plus petit ID de salon. Sans activité, les records et favoris sont `null`, les totaux sont zéro et `daily` est vide. Un compte absent de PocketBase produit une indisponibilité, pas un faux historique vide.

Les enregistrements sans compteur par salon donnent `channel_counters_complete: false`. Les favoris sont calculés avec les compteurs disponibles, sans approximation par nombre d'occurrences, et restent `null` si aucun compteur positif n'est exploitable. Les noms de salons enregistrés sont renvoyés au propriétaire de l'activité ; il n'y a pas de revalidation des permissions Discord actuelles des salons.

## Protection contre les appels répétés

- Quota global uniforme de 10 requêtes par minute et par route, par sujet JWT connecté ou IP anonyme, y compris pour les administrateurs. Il couvre toute l'application, Swagger et les fichiers statiques inclus. Les identifiants dynamiques appartiennent au même quota de route.
- Réponse `429` avec `Retry-After` lorsque le quota de la route est dépassé.
- Cache interne de 60 secondes, isolé par compte avec une entrée globale distincte ; au plus 16 entrées. Les lectures identiques concurrentes réutilisent le même résultat. Le cache ne contourne jamais l'authentification ou le quota global.
- Collecte des statistiques limitée à 15 secondes, pagination bornée à 200 pages de 500 lignes. Un historique trop grand ou invalide renvoie une erreur, jamais un résultat tronqué.
- Réponses privées `Cache-Control: no-store` et `X-Content-Type-Options: nosniff`. Les détails techniques des erreurs restent dans les logs.

Les quotas et le cache sont locaux au processus et sont réinitialisés au redémarrage. Avec plusieurs instances, prévoir également un quota partagé au reverse proxy. La borne de pagination implique une adaptation future si la collection dépasse 100 000 enregistrements.

## Génération d'image

L'action de génération obtient le même snapshot typé que la route personnelle JSON, via l'action interne partagée `discord_activity::snapshot`. Elle réutilise le cache sans requête HTTP vers elle-même et sans transmettre le cookie utilisateur à un autre service. Les totaux et favoris affichés proviennent de `DiscordActivity`. Le hash de l'image inclut ces données, sans l'horodatage de récupération, afin d'invalider les anciennes images dont les favoris étaient calculés par fréquence d'apparition.

## Validation

`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`.

## Dates historiques absentes ou invalides

Les lignes sans `date_stats` exploitable restent incluses dans les totaux et les favoris de salons. Elles ne sont attribuées à aucune journée. La réponse expose `undated_records`, `undated_messages`, `undated_vocal_hours` et `daily_complete`. Si `daily_complete` est faux, `daily` et les records journaliers utilisent uniquement les journées connues : ces records ne garantissent donc pas un maximum sur les lignes sans date. Les dates PocketBase, ISO et les espaces autour sont pris en charge. Les compteurs négatifs, non finis ou dépassant les limites restent rejetés.

La date métier `2025-01-01` est réservée au cumul importé de l'ancien système 2025–2026 : ses messages et heures vocales restent inclus dans les totaux, mais cette ligne est exclue de `daily`, des records journaliers et des favoris de salons. Ses compteurs par salon manquants ne rendent pas les données modernes incomplètes.


## Compagnon vocal

`top_voice_companion` expose `{id, username, records_together}` pour la personne avec laquelle le membre est le plus régulièrement en vocal : celle qui apparaît dans le plus de lignes `vocal_with`, une seule fois par ligne. Il fonctionne avec le format existant `{id, username}`, sans demander de durée partagée.

Le classement regroupe par ID, ignore le cumul de migration `2025-01-01` et départage les égalités par ID croissant. Sans compagnon enregistré, `top_voice_companion` est `null`. Sur la route globale admin, le classement agrège les compagnons mentionnés par tous les comptes.

L'image réutilise ce même compagnon le plus régulier. Les règles d'accès et le quota des routes existantes s'appliquent à ces champs.


## Image personnelle

`POST /api/discord-summary/me` (préfixe et chemin adaptés à la configuration) génère l'image du sujet JWT connecté et retourne le JSON `ImageResponse` contenant son URL protégée. Aucun ID ni corps de requête n'est nécessaire. La route partage l'authentification, le quota global et le cache des statistiques et de la génération par ID. Swagger décrit cette route avec la session `auth_token` et la réponse `429` avec `Retry-After`.



## Bornes des images et du cache

Les téléchargements d'images sont limités à 5 secondes et 4 Mio, sans redirection. Les sources autorisées sont les fichiers de l'origine PocketBase configurée (`/api/files/`), `https://cdn.discordapp.com`, `https://media.discordapp.net` et `https://mc-heads.net`. Les URLs avec identifiants sont rejetées. Le décodage est borné à 4096 pixels par dimension et 32 Mio d'allocation. Une image externe refusée ou invalide est omise de la carte. Les badges hébergés sur une autre origine doivent être déplacés vers PocketBase ou une source autorisée.

Le cache conserve au plus 16 entrées et regroupe les requêtes simultanées d'un même compte. Chaque entrée dispose de son propre verrou ; une récupération lente ne verrouille pas les autres comptes. Une entrée en cours d'utilisation n'est pas évincée. À saturation des 16 entrées actives, les nouvelles demandes échouent sans agrandir le cache.
