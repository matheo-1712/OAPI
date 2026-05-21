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
api:
  base_url: "http://votre-url-locale/api"
```

*Note : Si un champ est vide ou absent de `config.yaml`, la valeur de `default_config.yaml` est utilisée automatiquement.*

---

## 📡 Récupération de Données (Fetcher API)

Le projet inclut un utilitaire robuste pour interagir avec des APIs externes (`src/utils/api_fetch.rs`) :

- **Santé de l'API** : Un test de santé automatique (`Health Check`) est effectué sur la `base_url` avant chaque requête.
- **Généricité** : Un fetcher générique permet de désérialiser n'importe quelle réponse API respectant le format standard `{ data: T }`.
- **Logging** : Toutes les erreurs et réponses brutes sont logguées via `tracing` pour faciliter le debugging.

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

### Vérifier le code
```bash
cargo check
```

## 🎨 Technologies utilisées
- **Axum** : Framework web asynchrone.
- **Config** : Gestion hiérarchique des configurations.
- **Serde YAML** : Support du format YAML pour la config.
- **Utoipa** : Documentation OpenAPI automatique.
- **Image / RustType** : Génération d'images et rendu de texte.
- **Reqwest** : Client HTTP asynchrone.
