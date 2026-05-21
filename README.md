# OAPI - Otterly API Image Generator

OAPI est une API Rust performante basée sur **Axum**, conçue pour automatiser la génération d'images de profil et de résumés de statistiques pour la communauté des Loutres.

## 🚀 Architecture du Projet

Le projet suit une architecture modulaire et scalable, séparant strictement les responsabilités pour faciliter la maintenance et l'évolution.

### 📂 Structure des Dossiers

- **`src/models/`** : Définit les structures de données (DTOs). C'est le "langage" commun de l'application.
- **`src/routes/`** : Définit le plan de l'API (URLs). C'est ici que l'on décide quels chemins sont exposés.
- **`src/handlers/`** : Point d'entrée technique des requêtes. Il extrait les données HTTP et délègue le travail aux Actions.
- **`src/actions/`** : **Le cœur de l'orchestration (Use Cases).** C'est ici que les données sont récupérées de l'extérieur et structurées avant traitement.
- **`src/services/`** : Contient la logique métier pure et lourde (calculs, génération d'images).
- **`src/main.rs`** : Initialisation du serveur, du logging et des middlewares.

---

## 🛠 À quoi servent les "Actions" ?

La couche **Actions** est un pont stratégique entre les Handlers et les Services. Ses rôles principaux sont :

1.  **Orchestration** : Elle coordonne plusieurs services ou appels externes. Par exemple, l'Action Discord récupère d'abord les infos du joueur, puis ses stats, avant de demander au service d'image de générer le visuel.
2.  **Filtrage & Sécurité** : Elle décide quelles données sont acceptables. Si une API externe renvoie 50 champs mais que nous n'en avons besoin que de 5, l'Action fait le tri.
3.  **Indépendance technique** : L'Action ne sait pas qu'elle est appelée par du HTTP. On pourrait l'appeler depuis une commande terminal ou un bot Discord sans rien changer à sa logique.

---

## 📡 Endpoints Principaux

- `POST /api/images` : Génère une image de test à partir d'un prompt.
- `POST /api/discord-summary/{discord_id}` : Récupère les données réelles via l'API Otterly et génère une image de synthèse complète du joueur.

---

## 📖 Documentation API (Swagger)

L'API est auto-documentée grâce à **Utoipa**.
Une fois le serveur lancé, accédez à :
👉 `http://localhost:3000/swagger-ui`

---

## ⚙️ Installation et Lancement

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
- **Axum** : Framework web rapide et modulaire.
- **Tokio** : Runtime asynchrone.
- **Utoipa** : Génération automatique de Swagger/OpenAPI.
- **Image** : Manipulation et génération d'images en natif Rust.
- **Reqwest** : Client HTTP pour récupérer les données externes.
- **Tracing** : Logging structuré pour le monitoring.
