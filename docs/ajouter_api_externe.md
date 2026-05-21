# Guide : Ajouter une nouvelle route API externe

Ce guide explique étape par étape comment ajouter l'intégration d'une nouvelle API externe ou d'un nouvel endpoint dans le projet OAPI.

---

## Étape 1 : Mettre à jour la Configuration

Toutes les URLs externes doivent être configurées dans les fichiers YAML pour permettre une modification sans recompilation.

1.  Ouvrez `src/config.rs` et ajoutez le nouveau champ dans la structure `ExternalApis` :
    ```rust
    pub struct ExternalApis {
        // ... vos routes existantes
        pub nouveau_service: String, // Ajoutez ceci
    }
    ```

2.  Mettez à jour `default_config.yaml` avec l'URL par défaut :
    ```yaml
    external_apis:
      # ...
      nouveau_service: "https://api.externe.com/v1/data"
    ```

3.  Mettez à jour le template dans la fonction `init()` de `src/config.rs` pour que les futurs fichiers `config.yaml` locaux soient à jour.

---

## Étape 2 : Créer l'Alias d'URL

Pour faciliter l'accès à l'URL dans le code, ajoutez une fonction d'alias dans `src/utils/api_endpoints.rs`.

```rust
/// Retourne l'URL complète pour le nouveau service.
pub fn nouveau_service_url() -> &'static str {
    &Config::global().external_apis.nouveau_service
}
```

---

## Étape 3 : Définir le Modèle de Données (DTO)

Créez la structure qui recevra les données de l'API dans `src/models/`.

```rust
#[derive(Debug, serde::Deserialize)]
pub struct NouveauModele {
    pub id: i32,
    pub nom: String,
    // ...
}
```

---

## Étape 4 : Utiliser le Fetcher dans une Action

Dans votre couche `src/actions/`, utilisez le `fetch_api_data` générique pour récupérer les données.

```rust
use crate::utils::api_fetch::fetch_api_data;
use crate::utils::api_endpoints::nouveau_service_url;

pub async fn ma_nouvelle_action() -> Result<MonResultat, String> {
    // Le fetcher s'occupe du Health Check, de l'appel HTTP et du parsing JSON
    let data: NouveauModele = fetch_api_data(nouveau_service_url(), "nouveau service").await?;
    
    // Continuez avec votre logique métier...
}
```

---

## Résumé du flux
1. **Config** (`src/config.rs` + `.yaml`) : Déclaration de l'URL.
2. **Endpoint** (`src/utils/api_endpoints.rs`) : Création de l'accès typé.
3. **Model** (`src/models/`) : Définition de la structure de réponse.
4. **Action** (`src/actions/`) : Appel au fetcher et traitement.
