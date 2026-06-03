# Guide : Système de Configuration

Le projet OAPI utilise un système de configuration hiérarchique et flexible basé sur la crate `config`. Ce système permet de gérer des valeurs par défaut tout en autorisant des surcharges locales et des secrets via l'environnement.

---

## Hiérarchie de chargement

Les sources de configuration sont chargées dans l'ordre suivant (les dernières écrasent les premières) :

1.  **`default_config.yaml`** : Valeurs par défaut du projet. **Ce fichier est suivi par Git.**
2.  **`config.yaml`** : Surcharges locales spécifiques à l'environnement. **Ce fichier est ignoré par Git.**
3.  **Variables d'Environnement** : Secrets et paramètres sensibles (ex: `PB_PASSWORD`).

---

## 1. Valeurs par Défaut (`default_config.yaml`)

Ce fichier contient la structure obligatoire et les valeurs de base :
- Paramètres du serveur (host, port).
- Définition des routes internes de l'API.
- Configuration du monitoring (liste des services à surveiller).

## 2. Surcharges Locales (`config.yaml`)

Au premier lancement, un fichier `config.yaml` est généré automatiquement. Vous pouvez y décommenter des lignes pour modifier la configuration sans toucher au code.

## 3. Secrets (.env)

Les informations sensibles ne sont **jamais** stockées dans les fichiers YAML. Elles sont lues depuis l'environnement (ou un fichier `.env`) :
- `PB_EMAIL` : Email de l'administrateur PocketBase.
- `PB_PASSWORD` : Mot de passe de l'administrateur.
- `PB_URL` : URL de l'instance PocketBase.

---

## Structure des Données

La configuration est structurée en trois blocs principaux :

### `server`
Gère le serveur Axum et le routage interne.
- `host` : IP d'écoute.
- `port` : Port TCP.
- `routes` : Chemins dynamiques pour les endpoints (ex: `minecraft_summary: "/minecraft-summary/:id"`).

### `monitoring`
Liste des services à surveiller.
- `discord` : Bots avec endpoint healthcheck.
- `minecraft` : Serveurs avec host et port.
- `api`, `site`, `self_hosted` : Services HTTP simples.

### `auth` (Interne)
Regroupe les accès PocketBase chargés via l'environnement.
