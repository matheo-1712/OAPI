# Documentation Authentification & Autorisations

Ce document décrit le système d'authentification basé sur OAuth2 et les JWT (JSON Web Tokens) implémenté dans OAPI.

## Vue d'ensemble
OAPI utilise Discord OAuth2 pour authentifier les utilisateurs. Plutôt que de stocker les sessions sur le serveur (ce qui consommerait de la mémoire et des requêtes base de données), nous générons un jeton "stateless" (JWT) stocké dans un cookie `auth_token`.

## 1. Le flux OAuth2
1. **Route de Connexion (`/api/auth/login`)** : Quand un utilisateur clique sur "Se connecter", cette route inclut son URL de départ dans un état OAuth signé et crée un cookie temporaire `oauth_state`, puis le redirige vers la page d'autorisation de Discord.
2. **Route de Retour (`/api/auth/callback`)** : Une fois que Discord a autorisé l'utilisateur, il est redirigé ici avec un `code` temporaire. Le serveur l'échange contre un jeton d'accès, récupère le profil Discord et ses rôles, retrouve l'utilisateur dans PocketBase ou le crée (avec son joueur DS Games, voir [save_link.md](save_link.md)), génère un JWT, crée le cookie `auth_token`, et redirige l'utilisateur vers son URL de retour incluse dans le state signé.
3. **Route de Déconnexion (`/api/auth/logout`)** : Supprime le cookie `auth_token` et redirige l'utilisateur vers la page d'accueil ou l'URL de retour.

### Connexion depuis le site (Astroloutre)
Le site est sur un autre domaine que le portail : il utilise des routes dédiées, actives seulement si `site_auth.url` est renseigné (origine seule, ex. `https://antredesloutres.fr`, en HTTPS obligatoire en production).
- **`/api/auth/site/login?return_to=/chemin`** : même flux Discord, mais le retour se fait sur `site_auth.url` + le chemin (toujours un chemin local, jamais une URL fournie par le client).
- **`/api/auth/site/me`** : profil public de la session (`username`, `discord_id`, `discord_tag`, `avatar_url`, `role`). Réponse lisible par `site_auth.url` uniquement (CORS avec `credentials`), les autres origines restent bloquées par le navigateur.
- **`/api/auth/site/logout?return_to=/chemin`** : supprime le cookie et renvoie vers le site.

Le cookie reste celui d'OAPI (`SameSite=Lax`, sans `Domain`) : le site doit être sur le même site enregistrable (ex. `antredesloutres.fr` et `oapi.antredesloutres.fr`) pour que le navigateur l'envoie. En local, lancer OAPI et le site tous deux sur `localhost` (ports différents) ; la valeur par défaut est `http://localhost:4321`. Le callback Discord reste `/api/auth/callback` : aucune modification de l'application Discord n'est nécessaire.

### Sites supplémentaires (autre domaine)
`site_auth.extra_urls` liste d'autres sites autorisés, par exemple `https://rom-patcher.ect-adl.fr` (origine seule, HTTPS obligatoire en production). Ils ont les mêmes droits que `site_auth.url` : routes `/auth/site/*`, lecture CORS avec `credentials` des réponses, appels `POST` acceptés par la vérification d'origine (donc les routes `/friends`, `/badges/me/refresh`, etc.).

- Le site choisit sa cible avec `origin` : `/api/auth/site/login?origin=https://rom-patcher.ect-adl.fr&return_to=/chemin` et de même pour `/api/auth/site/logout`. Sans `origin`, la cible est `site_auth.url`. Une origine absente de la configuration renvoie `400`. L'origine est conservée dans l'état OAuth signé.
- Comme `ect-adl.fr` n'est pas le même site enregistrable que l'API, le cookie `auth_token` émis pour une origine de `extra_urls` est `SameSite=None; Secure` (sinon le navigateur ne l'enverrait pas aux `fetch` du site). Il reste `HttpOnly`. En HTTP local, il reste `Lax` : ce mode ne fonctionne qu'en HTTPS.
- Le cookie est unique par hôte d'API : se connecter depuis un site de `extra_urls` le rend `SameSite=None` pour tous les sites. Les mutations `POST` portant un `Origin` étranger restent refusées (`403`).
- Côté site, tous les appels doivent utiliser `credentials: "include"`. Les navigateurs qui bloquent les cookies tiers (Safari, Firefox en mode strict) empêcheront la session de fonctionner ; dans ce cas, héberger l'API sous un sous-domaine du site (même site enregistrable) reste la solution fiable.

## 2. JWT et Cookies
Le JWT généré contient les informations (claims) suivantes :
- `sub` : ID de l'utilisateur Discord
- `username` : Nom d'utilisateur Discord
- `role` : Le rôle le plus élevé attribué à l'utilisateur (`Admin`, `LoutreInvesti`, ou `Normal`).
- `exp` : Date d'expiration (7 jours)
- `profile` : `discord_tag` (PocketBase) et `avatar_url` (Discord) affichés par le site ; absent des jetons émis avant son ajout

Le cookie de session est `HttpOnly`, `SameSite=Lax` et `Secure` en HTTPS ou en production, avec une durée de sept jours. Le frontend consulte `/api/auth/me` pour connaître la connexion. `JWT_SECRET` doit contenir au moins 32 octets hors espaces de bord ; une valeur absente ou trop courte bloque le démarrage.

**Révocation.** La déconnexion (`/api/auth/logout`, `/api/auth/site/logout`) invalide aussi le jeton côté serveur jusqu'à son expiration ; cette liste est en mémoire et se perd au redémarrage. Sur les routes protégées, le compte est revérifié dans PocketBase (résultat gardé 60 s) : un compte supprimé perd sa session et un administrateur dont `is_admin` est retiré redevient `Normal`. Si PocketBase est injoignable, seuls les droits administrateur sont retirés. Le rôle `LoutreInvesti`, issu de Discord, reste celui du jeton.

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

Un jeu (ROM) peut aussi s'authentifier avec un token `rom_…` obtenu en liant un code à 6 chiffres à un compte Discord. Ce token n'ouvre que la lecture de la liste d'amis (`GET /friends/{account}`, via `require_friends_reader`) ; voir [rom_auth.md](rom_auth.md).

## 4. Bouton d'Arrêt d'Urgence (Kill-Switch)
Si vous soupçonnez que des cookies ou clés ont été compromis, vous pouvez déconnecter instantanément tous les utilisateurs actifs en modifiant la variable `JWT_SECRET` dans le fichier `.env` et en redémarrant le serveur. Les JWT étant sans état, modifier la clé de signature invalide immédiatement tous les jetons émis précédemment.

## Protection du retour OAuth et des images

Chaque connexion utilise un état signé et aléatoire, valable dix minutes, lié au cookie HttpOnly du navigateur. Le retour Discord doit présenter le même état ; une tentative invalide est rejetée avant tout échange de code. La destination de retour est incluse dans cet état signé et vérifiée comme chemin local.

Les images sont enregistrées hors du dossier public. Leur lecture exige une session et le compte propriétaire, ou un administrateur. Les comptes Minecraft sont liés via le champ discord_user de PocketBase. Les réponses API et images portent Cache-Control: no-store. Les anciennes images du dossier public ne sont plus servies ; elles seront régénérées à la demande.

En production, définir OAPI_ENV=production, utiliser une URL de retour Discord HTTPS et terminer TLS sur le serveur ou le proxy.

## Quota global

Le middleware unique `handlers::rate_limit::limit_requests` s'applique à toute l'application : API, authentification, monitoring, images, Swagger, fichiers statiques et chemins inconnus. Chaque compte connecté (sujet du JWT vérifié), ou IP anonyme, dispose de **10 requêtes par minute par route**. Les paramètres dynamiques partagent le même quota : changer l'ID dans un chemin ne le réinitialise pas. Les méthodes HTTP d'un même chemin partagent ce quota. Les chemins inconnus utilisent un quota commun.

La onzième requête renvoie `429 Too Many Requests`, `Retry-After` en secondes et `Cache-Control: no-store`. La fenêtre expire 60 secondes après son premier appel. Aucun rôle n'est exempté. Le cache de statistiques n'exempte pas du quota. L'ancien quota spécifique à Discord a été supprimé pour éviter un double comptage. Les entrées mémoire expirent après une minute et leur nombre est borné à 10 000 ; à saturation, les nouvelles identités/routes reçoivent également `429`.

L'IP vient de la connexion TCP. Par défaut, les en-têtes transférés sont ignorés. Derrière un reverse proxy, renseigner ses IP ou plages CIDR dans `server.trusted_proxies` (par exemple `["127.0.0.1"]` si le proxy local est seul à pouvoir contacter le serveur, ou `["172.16.0.0/12"]` pour un proxy en conteneur Docker). Les visiteurs anonymes IPv6 partagent un quota par préfixe /64 ; quand la table des quotas est pleine, la fenêtre la plus ancienne est évincée plutôt que de refuser les nouveaux clients. Le proxy doit renseigner ou compléter correctement `X-Forwarded-For`. La chaîne est lue depuis la droite en ne traversant que les proxys approuvés ; une valeur invalide fournie par un proxy approuvé est rejetée. Sans cette configuration, les visiteurs anonymes derrière le même proxy partagent son quota IP. Ne pas ajouter d'IP de visiteurs à la liste des proxys approuvés.

Les quotas sont locaux au processus et réinitialisés au redémarrage. Plusieurs instances nécessitent une limite partagée au proxy pour garantir un quota commun. Les routes Swagger et les ressources statiques respectent volontairement la même limite de 10 appels par minute : des rechargements répétés peuvent donc recevoir `429`.
