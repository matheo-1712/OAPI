# Guide : Récupération de Données (API Fetcher)

Le projet OAPI propose un utilitaire générique pour interagir avec des APIs externes de manière sécurisée et uniforme (`src/utils/api_fetch.rs`).

---

## Fonctionnement du Fetcher

Le `fetch_api_data` est une fonction asynchrone générique qui suit ce processus :

### 1. Test de Santé (Health Check)
Avant chaque requête, le fetcher appelle `check_api_health()`. 
- Il effectue un `GET` sur l'URL de base configurée (`external_apis.health_check`).
- Si l'API ne répond pas (ou répond avec une erreur), un `warn!` est enregistré.
- Cela permet de diagnostiquer rapidement si une erreur vient de nos paramètres ou de la disponibilité du service distant.

### 2. Requête HTTP
- Utilisation du client `reqwest`.
- Gestion des erreurs réseau.
- Vérification du code de statut HTTP (doit être `2xx`).

### 3. Désérialisation Générique
Le fetcher s'attend à ce que l'API externe renvoie un objet JSON contenant un champ `data`.
Structure attendue :
```json
{
  "data": { ... vos données ... }
}
```
Il désérialise automatiquement le contenu de `data` vers la structure Rust que vous lui passez en paramètre générique.

---

## Utilisation dans le Code

### Signature
```rust
pub async fn fetch_api_data<T>(url: &str, description: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
```

### Exemple d'appel
```rust
let stats: Vec<DiscordStats> = fetch_api_data("https://api.com/stats", "statistiques utilisateur").await?;
```

---

## Pourquoi utiliser le Fetcher ?

1.  **Centralisation** : Si vous devez ajouter un header d'authentification ou changer de client HTTP, vous ne le faites qu'à un seul endroit.
2.  **Robustesse** : La gestion des erreurs (parsing JSON, timeout, codes 500) est déjà implémentée et testée.
3.  **Observabilité** : Le fetcher logue automatiquement les URLs appelées et les erreurs rencontrées via `tracing`.
