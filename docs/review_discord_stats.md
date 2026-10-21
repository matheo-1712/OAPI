# Revue avant PR — statistiques Discord et quota global

Revue du 21 octobre 2026 sur `feat/discord-stats`, comparée à `origin/master`. La branche contient également les trois commits de badges précédents : ces changements appartiennent au périmètre de la future PR.

## Corrections appliquées pendant la revue

- Cache : verrou par compte, réutilisation des snapshots via `Arc`, 16 entrées maximum et aucune éviction d'une entrée en cours d'utilisation. Une requête PocketBase lente ne garde plus un verrou global sur les autres comptes.
- Images : seuls les fichiers de l'origine PocketBase configurée, le CDN Discord et mc-heads sont autorisés. Les redirections et les identifiants dans les URLs sont refusés. Téléchargements limités à 5 secondes et 4 Mio ; décodage limité à 4096 pixels par dimension et 32 Mio. Une ressource refusée est omise de la carte.
- Couleurs : validation ASCII hexadécimale avant découpage, pour éviter les paniques UTF-8.
- Journaux : traces HTTP limitées au chemin sans query string, notamment sur le retour OAuth. Suppression de la journalisation d'un enregistrement de badge brut en erreur.
- Pagination : rejet d'un numéro de page incohérent et vérification du plafond sur le nombre effectivement accumulé.

## Contrôles examinés

Authentification JWT et cookies, contrôles propriétaire/admin, routes `/me`, validation des identifiants avant les filtres PocketBase, protections des fichiers générés, quota global et regroupement des chemins dynamiques, proxys approuvés, isolation du cache, gestion des erreurs, rendu des données externes dans les pages et couverture Swagger. Les secrets locaux `.env` et `config.yaml` ne sont pas suivis par Git.

## Validation locale

- `cargo fmt --all -- --check` : réussi.
- `cargo clippy --all-targets --all-features -- -D warnings` : réussi.
- `cargo test --all-features` : 63 tests réussis.
- `node --test tests/investor_page.test.cjs tests/badges_page.test.cjs` : 10 tests réussis.
- `git diff --check` : réussi.

Les tests Rust utilisant HTTP local et des fichiers temporaires ont été exécutés hors du bac à sable. Les correctifs du cache, des couleurs, des URLs d'images et de la pagination sont couverts par des tests de régression.

## Conditions et limites avant déploiement

- Les index uniques PocketBase des badges, décrits dans `docs/badges.md`, doivent être appliqués pour garantir l'absence de doublons entre plusieurs instances. L'état de la base de production n'a pas été vérifié ni modifié.
- Déclarer uniquement les IP des reverse proxys maîtrisés dans `server.trusted_proxies`. Sans cela, les visiteurs anonymes derrière un proxy partagent son quota.
- Les quotas sont locaux au processus : 10 requêtes/minute/route/compte connecté ou IP anonyme. Plusieurs instances nécessitent une limite partagée au proxy.
- Un historique supérieur à 100 000 lignes est rejeté explicitement. Les champs d'incomplétude doivent être pris en compte pour interpréter les records et favoris.
- Les images hébergées ailleurs que sur les sources autorisées doivent être déplacées vers PocketBase ou une source autorisée.
- Cette revue et les tests locaux ne constituent pas un audit de la production, de PocketBase ou de toutes les dépendances ; la CI, dont Semgrep et le build release, reste à exécuter sur la PR.

## Proposition de PR

Titre : `Add protected Discord activity statistics, personal image generation and global rate limiting`

La PR expose les statistiques personnelles Discord, les totaux réservés aux administrateurs et la génération d'image `/me`. Elle partage le calcul des statistiques avec les images, traite le cumul de migration du 2025-01-01 uniquement dans les totaux et expose le compagnon vocal le plus régulier. Toutes les routes utilisent le même quota global et sont décrites dans Swagger. La branche inclut également les endpoints d'attribution et de rafraîchissement des badges, avec leurs contrôles de propriété et leur interface.

Validation : 63 tests Rust, 10 tests web, formatage et Clippy réussis. Préconditions : index uniques PocketBase pour les badges, proxys approuvés et limites locales au processus décrits ci-dessus.
