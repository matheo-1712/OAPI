# Guide : Système de Configuration

Le projet OAPI utilise un système de configuration hiérarchique et flexible basé sur la crate `config`. Ce système permet de gérer des valeurs par défaut tout en autorisant des surcharges locales et par variables d'environnement.

---

## Hiérarchie de chargement

Les sources de configuration sont chargées dans l'ordre suivant (les dernières écrasent les premières) :

1.  **`default_config.yaml`** : Valeurs par défaut du projet. **Ce fichier est suivi par Git.**
2.  **`config.yaml`** : Surcharges locales spécifiques à l'environnement. **Ce fichier est ignoré par Git.**
3.  **Variables d'environnement** : Surcharges dynamiques via le système (ex: Docker).

---

## 1. Valeurs par Défaut (`default_config.yaml`)

Ce fichier contient la configuration de base nécessaire au fonctionnement de l'application :
- Paramètres du serveur (host, port).
- Définition des routes internes de l'API.
- URLs des APIs externes.

## 2. Surcharges Locales (`config.yaml`)

Au premier lancement de l'application, un fichier `config.yaml` est généré automatiquement s'il n'existe pas. Il contient l'ensemble des champs de `default_config.yaml`, mais tous commentés.

### Comment surcharger une valeur ?
Ouvrez `config.yaml` et décommentez la section et le champ souhaités. 

**Exemple : Modifier le port du serveur**
```yaml
server:
  port: 4000
```

## 3. Variables d'Environnement

Toutes les configurations peuvent être surchargées via des variables d'environnement avec le préfixe `OAPI_` et le séparateur `__` (double underscore).

**Exemples :**
- `OAPI_SERVER__PORT=5000`
- `OAPI_EXTERNAL_APIS__DISCORD_USER="https://mon-api-de-test.com/user"`

---

## Structure des Données

La configuration est structurée en deux blocs principaux :

### `server`
- `host` : Adresse IP sur laquelle le serveur écoute.
- `port` : Port TCP.
- `routes` :
    - `base` : Préfixe de toutes les routes API (ex: `/api`).
    - `discord_summary` : Chemin de l'endpoint de résumé Discord.

### `external_apis`
- `discord_user` : URL complète pour récupérer les infos utilisateur.
- `discord_stats` : URL complète pour récupérer les statistiques.
- `health_check` : URL utilisée par le fetcher pour vérifier la disponibilité de l'API externe.
