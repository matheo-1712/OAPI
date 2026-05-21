# OAPI - Otter API

OAPI est une API Rust qui contient l'ensemble de la logique de l'Antre des loutres

## 🚀 Architecture du Projet

Le projet suit une architecture modulaire et scalable, séparant strictement les responsabilités :

- **`src/config.rs`** : Gestion de la configuration hiérarchique (YAML).
- **`src/models/`** : Structures de données (DTOs).
- **`src/routes/`** : Routage de l'API.
- **`src/handlers/`** : Extraction des données HTTP et délégation aux Actions.
- **`src/actions/`** : Orchestration des cas d'usage (récupération de données externes, appels aux services).
- **`src/services/`** : Logique métier pure (génération d'images, calculs).
- **`src/utils/`** : Utilitaires (fetcher API générique, formateurs, constantes).

---

## ⚙️ Configuration

OAPI utilise un système de configuration flexible basé sur des fichiers YAML.

1.  **`default_config.yaml`** : Contient les valeurs par défaut. **Ce fichier est suivi par Git** et ne doit pas être modifié pour des réglages personnels.
2.  **`config.yaml`** : Utilisé pour vos surcharges locales (URL de test, ports spécifiques, etc.). **Ce fichier est ignoré par Git.**

### Personnalisation
Au premier lancement, un fichier `config.yaml` est généré automatiquement s'il n'existe pas. Pour modifier une configuration (ex: l'URL de l'API Otterly), ajoutez-la simplement dans ce fichier :

```yaml
external_apis:
  discord_user: "https://otterlyapi.antredesloutres.fr/api/utilisateurs_discord"
  discord_stats: "https://otterlyapi.antredesloutres.fr/api/utilisateurs_discord/stats"
```

*Note : Si un champ est vide ou absent de `config.yaml`, la valeur de `default_config.yaml` est utilisée automatiquement.*

---

## 📡 Récupération de Données (Fetcher API)

Le projet inclut un utilitaire robuste pour interagir avec des APIs externes (`src/utils/api_fetch.rs`) :

- **Santé de l'API** : Un test de santé automatique (`Health Check`) est effectué sur l'URL de santé configurée avant chaque requête.
- **Généricité** : Un fetcher générique permet de désérialiser n'importe quelle réponse API respectant le format standard `{ data: T }`.
- **Logging** : Toutes les erreurs et réponses brutes sont logguées via `tracing` pour faciliter le debugging.

---

## 📚 Documentation Détaillée

Pour plus de détails sur le fonctionnement interne, consultez les guides suivants dans le dossier `docs/` :

- [**Architecture**](./docs/architecture.md) : Comprendre les couches et le flux de données.
- [**Système de Configuration**](./docs/configuration.md) : Maîtriser les surcharges YAML et ENV.
- [**Génération d'Images**](./docs/generation_images.md) : Détails sur le moteur graphique et le cache.
- [**API Fetcher**](./docs/api_fetcher.md) : Utilisation du client HTTP générique.
- [**Ajouter une API Externe**](./docs/ajouter_api_externe.md) : Guide pas à pas pour l'extension.

---

## 📖 Documentation API (Swagger)

L'API est auto-documentée. Une fois le serveur lancé, accédez à :
👉 `http://localhost:3000/swagger-ui`

---

## 🛠 Installation et Lancement

### Prérequis
- Rust (dernière version stable)

### Lancer le serveur
```bash
cargo run
```

---

## 🛠 CI/CD et Qualité du Code

Ce projet utilise un pipeline **GitHub Actions** pour garantir la qualité du code à chaque modification.

### Vérifications automatisées
- **Formatage** : `cargo fmt --all -- --check`
- **Linting** : `cargo clippy --all-targets --all-features -- -D warnings`
- **Tests** : `cargo test --all-features`
- **Build** : `cargo build --release`

### Exécuter les vérifications localement
Il est recommandé de lancer ces commandes avant de pousser vos modifications :
```bash
# Formater le code
cargo fmt

# Vérifier les lints
cargo clippy --all-targets --all-features -- -D warnings

# Lancer les tests
cargo test
```

## 🎨 Technologies utilisées
- **Axum** : Framework web asynchrone.
- **Config** : Gestion hiérarchique des configurations.
- **Serde YAML** : Support du format YAML pour la config.
- **Utoipa** : Documentation OpenAPI automatique.
- **Image / RustType** : Génération d'images et rendu de texte.
- **Reqwest** : Client HTTP asynchrone.
