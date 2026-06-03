# Architecture du Projet

OAPI suit une architecture en couches (Layered Architecture) pour garantir une séparation claire des responsabilités, faciliter les tests et permettre une évolution modulaire.

---

## Les Couches de l'Application

### 1. Couche de Configuration (`src/config.rs`)
Responsable du chargement et de la fusion des sources de configuration (YAML, ENV). Elle fournit un accès global et thread-safe aux paramètres de l'application via un `OnceLock`.

### 2. Couche des Modèles (`src/models/`)
Définit les structures de données (DTOs - Data Transfer Objects). Ces structures sont utilisées pour :
- Désérialiser les réponses de PocketBase.
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
- Appels à PocketBase via le client utilitaire.
- Agrégation, filtrage et calculs sur les données brutes.
- Appel aux **Services** pour les traitements lourds (génération d'images).
- C'est la couche qui contient la "recette" d'un endpoint.

### 6. Couche des Services (`src/services/`)
Contient la logique métier pure et isolée :
- Traitement d'images (dessin, calculs géométriques, redimensionnement).
- Calculs mathématiques complexes.
- Cette couche ne sait rien du HTTP ou de la provenance des données.

### 7. Couche des Utilitaires (`src/utils/`)
Fonctions transverses réutilisables :
- `pocketbase.rs` : Client PocketBase avec authentification admin et gestion de la pagination automatique.
- `formatters.rs` : Formatage de dates, textes, nombres (séparateurs de milliers) et durées.
- `constants.rs` : Labels UI et noms des collections de base de données.

---

## Flux d'une requête (Exemple Minecraft)

1.  **Client** envoie un `POST` sur `/api/minecraft-summary/{uuid}`.
2.  **Routeur** redirige vers `minecraft_handler::create_minecraft_summary_by_id`.
3.  **Handler** extrait l'UUID et appelle `minecraft_actions::get_minecraft_summary_action`.
4.  **Action** :
    - S'authentifie sur PocketBase via le `PocketbaseClient`.
    - Récupère les infos du joueur, ses statistiques (via toutes les pages) et les infos des serveurs.
    - Agrège les temps de jeu et identifie les serveurs préférés.
    - Passe les données à `image_service::generate_minecraft_profile`.
5.  **Service** :
    - Calcule le hash des données pour le cache.
    - Si l'image n'est pas en cache :
        - Télécharge la tête du joueur.
        - Dessine l'image (statistiques, badges, serveurs).
        - Sauvegarde le fichier PNG.
    - Retourne l'URL de l'image.
6.  **Action** & **Handler** remontent le résultat jusqu'au client au format JSON.
