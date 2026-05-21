# Architecture du Projet

OAPI suit une architecture en couches (Layered Architecture) pour garantir une séparation claire des responsabilités, faciliter les tests et permettre une évolution modulaire.

---

## Les Couches de l'Application

### 1. Couche de Configuration (`src/config.rs`)
Responsable du chargement et de la fusion des sources de configuration (YAML, ENV). Elle fournit un accès global et thread-safe aux paramètres de l'application via un `OnceLock`.

### 2. Couche des Modèles (`src/models/`)
Définit les structures de données (DTOs - Data Transfer Objects). Ces structures sont utilisées pour :
- Désérialiser les réponses des APIs externes.
- Sérialiser les réponses de notre propre API.
- Générer automatiquement la documentation OpenAPI (Swagger).

### 3. Couche de Routage (`src/routes/`)
Définit les points d'entrée de notre API. Elle utilise les chemins définis dans la configuration pour construire l'arbre de routage `Axum`.

### 4. Couche des Handlers (`src/handlers/`)
Gère l'interface HTTP :
- Extraction des paramètres de chemin (`Path`), de requête (`Query`) ou du corps JSON.
- Validation technique basique.
- Délégation de la logique métier à la couche **Actions**.
- Retour des codes de statut HTTP appropriés.

### 5. Couche des Actions (`src/actions/`)
**Le cœur de l'orchestration (Cas d'utilisation).** Une action coordonne le flux de données pour répondre à un besoin spécifique :
- Appels successifs à plusieurs APIs externes via le fetcher.
- Agrégation et filtrage des données brutes.
- Appel aux **Services** pour les traitements lourds.
- C'est la couche qui contient la "recette" d'un endpoint.

### 6. Couche des Services (`src/services/`)
Contient la logique métier pure et isolée :
- Traitement d'images (dessin, calculs géométriques).
- Calculs mathématiques complexes.
- Cette couche ne sait rien du HTTP ou de la provenance des données.

### 7. Couche des Utilitaires (`src/utils/`)
Fonctions transverses réutilisables :
- `api_fetch.rs` : Client HTTP générique avec Health Check.
- `api_endpoints.rs` : Gestionnaire dynamique d'URLs.
- `formatters.rs` : Formatage de dates, textes et durées.

---

## Flux d'une requête (Exemple)

1.  **Client** envoie un `POST` sur `/api/discord-summary/123`.
2.  **Routeur** redirige vers `discord_handler::create_discord_summary_by_id`.
3.  **Handler** extrait l'ID `123` et appelle `discord_actions::get_discord_summary_action`.
4.  **Action** :
    - Récupère l'URL configurée via `api_endpoints`.
    - Appelle le `fetch_api_data` pour obtenir les infos utilisateur.
    - Appelle le `fetch_api_data` pour obtenir les stats.
    - Passe les données agrégées à `image_service::generate_discord_profile`.
5.  **Service** :
    - Calcule le hash des données pour le cache.
    - Dessine l'image (si non présente en cache).
    - Retourne l'URL de l'image.
6.  **Action** & **Handler** remontent le résultat jusqu'au client au format JSON.
