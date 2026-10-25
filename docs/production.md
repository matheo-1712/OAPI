# Mise en production

Cette procédure prévoit un serveur Linux avec Docker Compose et un reverse proxy
sur l'hôte qui termine HTTPS. PocketBase reste un service externe.

## Configuration

1. Copier `.env.example` vers `.env` et renseigner les identifiants PocketBase et Discord.
   Générer `JWT_SECRET` avec `openssl rand -hex 32`. Protéger `.env` avec `chmod 600 .env`.
2. Définir `OAPI_ENV=production` et `RUST_LOG=info`.
3. Définir `DISCORD_REDIRECT_URL=https://votre-domaine/api/auth/callback` et enregistrer
   exactement cette URL dans l'application Discord.
4. OAPI rejoint le réseau Docker externe `pocketbase_network` par défaut et utilise
   `PB_URL=http://pocketbase:8090`. Définir `POCKETBASE_NETWORK` dans `.env` avec
   le nom du réseau existant de PocketBase s’il diffère. Le conteneur PocketBase
   doit être joignable sous le nom ou alias `pocketbase` et écouter sur `0.0.0.0:8090`.
   Vérifier son réseau avec `docker inspect pocketbase --format '{{json .NetworkSettings.Networks}}'`.
   Si aucun réseau commun ne convient, créer `docker network create pocketbase_network`,
   puis y connecter PocketBase avec `docker network connect pocketbase_network pocketbase`.
   `PB_URL` est transmis au build et au lancement ; une ancienne valeur dans `.env`
   doit donc être remplacée. Sans Compose, l’image utilise aussi cette URL par défaut,
   modifiable avec `docker build --build-arg PB_URL=http://pocketbase:8090 .`
   ou avec `docker run --network pocketbase_network -e PB_URL=http://pocketbase:8090 ...`.
   Pour un accès distant, utiliser HTTPS.
5. Adapter `deploy/config.production.yaml` : rôles Discord, services surveillés et
   éventuels paramètres de routes. Les valeurs absentes viennent de `default_config.yaml`.
6. Configurer le reverse proxy vers `http://127.0.0.1:9003` s'il tourne sur l'hôte,
   ou `http://IP_DU_SERVEUR:9003` s'il tourne sur une autre machine, avec un certificat TLS
   valide et redirection HTTP vers HTTPS. Docker publie le port 9003 sur les interfaces
   réseau de l'hôte (`9003:9003`), vers le port 9003 de l'application.
   Configurer `trusted_proxies` avec les IP ou plages CIDR des pairs vus par l'application
   (`172.16.0.0/12` par défaut dans `deploy/config.production.yaml`, pour un proxy en conteneur Docker)
   et faire écraser `X-Forwarded-For` par le proxy. Sans cela, les visiteurs anonymes
   derrière un même proxy partagent le quota de requêtes.

## Validation et lancement

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
node --test tests/*.test.cjs
docker compose config --quiet
docker compose build --pull
docker compose up -d
docker compose ps
curl --fail -i http://127.0.0.1:9003/healthz
```

Le port de production est défini à 9003 dans `deploy/config.production.yaml`.
Pour le changer, aligner `server.port`, la publication dans `compose.yaml`,
`EXPOSE` et l'URL du `HEALTHCHECK` dans `Dockerfile`, puis reconstruire et recréer
le conteneur avec `docker compose up -d --build`.

`/healthz` répond 204 sans interroger les dépendances. Le middleware global de quota
peut répondre 429 en cas de sondes trop fréquentes ; conserver l'intervalle de 30 secondes.
Vérifier ensuite via HTTPS la connexion Discord, les rôles administrateur/investisseur,
la génération et l'accès aux images, ainsi que le monitoring de PocketBase.
Ces vérifications nécessitent les services et identifiants réels.

## Exploitation

Les images persistent dans le volume `generated_images`. Sauvegarder ce volume,
la configuration et, séparément, les données PocketBase. Vérifier la restauration
avant l'ouverture au public. Prévoir une politique de rétention des images.
Les quotas sont en mémoire et propres à chaque instance ; ce déploiement prévoit
une seule instance applicative.

Consulter les logs avec `docker compose logs --tail=100 oapi`. Un SIGTERM déclenche
le drainage des requêtes ; Docker impose une limite de 45 secondes avant arrêt forcé.
Une sonde unhealthy seule ne redémarre pas le conteneur : prévoir une alerte externe.
Pour une mise à jour, conserver l'image précédente et une sauvegarde PocketBase,
puis reconstruire et relancer. En cas d'échec, redéployer l'image précédente avec
sa configuration compatible.

Le conteneur s'exécute sans privilèges, avec un système de fichiers en lecture seule,
hors images et fichiers temporaires. Ne pas ajouter `.env` à l'image ou au dépôt.
