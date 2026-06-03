# Utilisation de PocketBase (Client API)

Le projet utilise PocketBase comme source de données centrale. L'utilitaire `src/utils/pocketbase.rs` fournit un client robuste pour interagir avec l'API REST de PocketBase.

---

## Fonctionnalités du Client

### 1. Authentification Admin
Le client s'authentifie automatiquement comme administrateur lors de l'appel à `login()`. Cela permet de passer outre les règles de visibilité publiques et de lire toutes les collections nécessaires à la génération des stats.

### 2. Gestion de la Pagination (Exhaustivité)
Contrairement aux appels classiques qui sont limités à 100 résultats par PocketBase, la méthode `list_all_records` :
- Boucle sur toutes les pages disponibles.
- Utilise une taille de page de 500 pour optimiser les performances.
- Fusionne tous les résultats avant de les retourner.

C'est ce qui permet de calculer un temps de jeu total exact sur des milliers d'enregistrements.

---

## Exemple d'utilisation dans une Action

```rust
let mut pb = PocketbaseClient::new();
pb.login().await?;

// Récupérer TOUTES les stats d'un joueur (même s'il y a 5000 lignes)
let stats: Vec<MinecraftStats> = pb.list_all_records("players_stats", &filter).await?;
```

---

## Sécurité
- Le jeton d'authentification (`token`) est stocké localement dans l'instance du client.
- Les erreurs de requête sont logguées via `tracing` avec le corps de la réponse brute pour faciliter le diagnostic (ex: erreurs de filtre 400).
