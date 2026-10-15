# Documentation Authentification & Autorisations

Ce document décrit le système d'authentification basé sur OAuth2 et les JWT (JSON Web Tokens) implémenté dans OAPI.

## Vue d'ensemble
OAPI utilise Discord OAuth2 pour authentifier les utilisateurs. Plutôt que de stocker les sessions sur le serveur (ce qui consommerait de la mémoire et des requêtes base de données), nous générons un jeton "stateless" (JWT) stocké dans un cookie `auth_token`.

## 1. Le flux OAuth2
1. **Route de Connexion (`/api/auth/login`)** : Quand un utilisateur clique sur "Se connecter", cette route inclut son URL de départ dans un état OAuth signé et crée un cookie temporaire `oauth_state`, puis le redirige vers la page d'autorisation de Discord.
2. **Route de Retour (`/api/auth/callback`)** : Une fois que Discord a autorisé l'utilisateur, il est redirigé ici avec un `code` temporaire. Le serveur l'échange contre un jeton d'accès, récupère le profil Discord et ses rôles, vérifie l'existence de l'utilisateur dans PocketBase, génère un JWT, crée le cookie `auth_token`, et redirige l'utilisateur vers son URL de retour incluse dans le state signé.
3. **Route de Déconnexion (`/api/auth/logout`)** : Supprime le cookie `auth_token` et redirige l'utilisateur vers la page d'accueil ou l'URL de retour.

## 2. JWT et Cookies
Le JWT généré contient les informations (claims) suivantes :
- `sub` : ID de l'utilisateur Discord
- `username` : Nom d'utilisateur Discord
- `role` : Le rôle le plus élevé attribué à l'utilisateur (`Admin`, `LoutreInvesti`, ou `Normal`).
- `exp` : Date d'expiration (7 jours)

Le cookie de session est `HttpOnly`, `SameSite=Lax` et `Secure` en HTTPS ou en production, avec une durée de sept jours. Le frontend consulte `/api/auth/me` pour connaître la connexion. `JWT_SECRET` doit contenir au moins 32 octets hors espaces de bord ; une valeur absente ou trop courte bloque le démarrage.

## 3. Middlewares
Nous avons implémenté trois middlewares de sécurité en cascade dans `src/handlers/middleware.rs` :
- **`require_auth`** : Valide la signature du JWT. Rejette les requêtes non authentifiées avec une erreur HTTP 401. Autorise tous les rôles valides.
- **`require_investor`** : Valide la signature du JWT et vérifie si l'utilisateur possède le rôle `LoutreInvesti` ou `Admin`. Rejette avec HTTP 403.
- **`require_admin`** : Valide la signature du JWT et exige strictement le rôle `Admin`.

Pour protéger une route, il suffit de l'envelopper via `.route_layer()` :
```rust
Router::new()
    .route("/admin/users", get(get_all_users))
    .route_layer(axum::middleware::from_fn(middleware::require_admin))
```

## 4. Bouton d'Arrêt d'Urgence (Kill-Switch)
Si vous soupçonnez que des cookies ou clés ont été compromis, vous pouvez déconnecter instantanément tous les utilisateurs actifs en modifiant la variable `JWT_SECRET` dans le fichier `.env` et en redémarrant le serveur. Les JWT étant sans état, modifier la clé de signature invalide immédiatement tous les jetons émis précédemment.

## Protection du retour OAuth et des images

Chaque connexion utilise un état signé et aléatoire, valable dix minutes, lié au cookie HttpOnly du navigateur. Le retour Discord doit présenter le même état ; une tentative invalide est rejetée avant tout échange de code. La destination de retour est incluse dans cet état signé et vérifiée comme chemin local.

Les images sont enregistrées hors du dossier public. Leur lecture exige une session et le compte propriétaire, ou un administrateur. Les comptes Minecraft sont liés via le champ discord_user de PocketBase. Les réponses API et images portent Cache-Control: no-store. Les anciennes images du dossier public ne sont plus servies ; elles seront régénérées à la demande.

En production, définir OAPI_ENV=production, utiliser une URL de retour Discord HTTPS et terminer TLS sur le serveur ou le proxy.
