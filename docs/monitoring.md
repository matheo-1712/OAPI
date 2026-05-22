# Système de Monitoring (Health Check)

Ce module permet de surveiller en temps réel l'état de différents services externes (Bots Discord, Serveurs Minecraft, APIs génériques) et d'agréger leurs statuts dans un seul endpoint API.

## Configuration

La configuration des services s'effectue dans le fichier `config.yaml` (ou `default_config.yaml`) sous la section `monitoring`.

### Types de services supportés

#### 1. Discord (`type: "discord"`)
Conçu spécifiquement pour les bots Discord de la communauté. Il interroge un endpoint de santé du bot et extrait des métadonnées riches.
- **Configuration** :
  ```yaml
  - type: "discord"
    name: "Nom du Bot"
    url: "https://url-du-bot.fr/healthcheck"
  ```
- **Métadonnées extraites** : Version, Uptime (secondes et format humain), Ping Discord, URL de l'Avatar.

#### 2. Minecraft (`type: "minecraft"`)
Vérifie l'état d'un serveur Minecraft (Java ou Bedrock supportant le protocole de statut).
- **Configuration** :
  ```yaml
  - type: "minecraft"
    name: "Mon Serveur"
    host: "mc.exemple.fr"
    port: 25565
  ```
- **Métadonnées extraites** : Nombre de joueurs en ligne, Nombre maximum de joueurs.

#### 3. Site Web (`type: "site"`)
Effectue une simple requête GET pour vérifier la disponibilité d'un site web.
- **Configuration** :
  ```yaml
  - type: "site"
    name: "Mon Portfolio"
    url: "https://mon-site.fr"
  ```

#### 4. API (`type: "api"`)
Vérifie la disponibilité d'un point de terminaison API.
- **Configuration** :
  ```yaml
  - type: "api"
    name: "Otterly API"
    url: "https://api.loutres.fr/health"
  ```

#### 5. Self-Hosted (`type: "self-hosted"`)
Pour les services auto-hébergés (tableaux de bord, outils internes).
- **Configuration** :
  ```yaml
  - type: "self-hosted"
    name: "Proxmox"
    url: "https://pve.local"
  ```

#### 6. HTTP Générique (`type: "http"`)
Effectue une simple requête GET et vérifie si le code de réponse est un succès (2xx).
- **Configuration** :
  ```yaml
  - type: "http"
    name: "Site Web"
    url: "https://google.fr"
  ```

## Endpoint API

L'endpoint est accessible via la route configurée (par défaut `/api/monitoring`).

### Format de réponse (JSON)

L'API renvoie un objet contenant une liste de `services`. Chaque service a la structure suivante :

```json
{
  "name": "Nom du service",
  "type_name": "discord | minecraft | http",
  "status": "UP | DOWN",
  "response_time_ms": 123,
  "metadata": { ... }, // Optionnel, dépend du type
  "error": "Message d'erreur" // Présent uniquement si status est DOWN
}
```

## Performance

- **Concurrence** : Tous les tests de santé sont effectués en parallèle. Le temps de réponse total de l'API est égal au temps de réponse du service le plus lent (borné par un timeout de 5 secondes).
- **Asynchrone** : Utilise `tokio` pour ne pas bloquer les autres requêtes de l'API.

## Documentation OpenAPI

Vous pouvez tester l'endpoint et voir les schémas détaillés sur l'interface Swagger : `/swagger-ui`.
