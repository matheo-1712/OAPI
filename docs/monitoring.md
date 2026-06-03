# Système de Monitoring (Health Check)

Ce module permet de surveiller en temps réel l'état de différents services externes et d'agréger leurs statuts dans un seul endpoint API.

## Configuration

La configuration s'effectue dans `default_config.yaml` ou `config.yaml` :

```yaml
monitoring:
  discord:
    - name: "Mateloutre"
      url: "https://mateloutre.fr/health"
  minecraft:
    - name: "Vanilla S4"
      host: "mc.antredesloutres.fr"
      port: 25565
  api: []
  site: []
```

### Types de services

1.  **Discord** : Vérifie un bot et extrait des métadonnées (Uptime, Version, Ping).
2.  **Minecraft** : Pinge le serveur via le protocole Server List Ping (SLP) pour obtenir le nombre de joueurs.
3.  **HTTP (Site, API, Self-hosted)** : Effectue un simple `GET` et vérifie le succès (2xx).

---

## Points de terminaison

### 1. Statut Global
- **Route** : `/api/monitoring`
- **Action** : Lance tous les tests en parallèle. Le temps de réponse est limité par le service le plus lent (timeout 5s).

### 2. Test Individuel
- **Route** : `/api/monitoring/check/{type}/{name}`
- **Usage** : Permet de rafraîchir un seul service spécifique sur le dashboard frontend sans tout re-tester.

## Intégration Frontend
Le projet fournit un dashboard minimaliste sombre accessible à la racine de l'URL (`/monitoring.html`) qui consomme ces endpoints.
