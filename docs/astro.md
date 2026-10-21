# Intégration Astroloutre (Astro)

La branche `develop` du dépôt Astroloutre consultée utilise Astro 5, sans adaptateur
serveur ni `output: 'server'`. Son utilitaire PocketBase expose déjà des méthodes
de connexion, mais cela ne suffit pas à établir une session serveur vérifiée.

## Clé de service OAPI

Générer un secret avec `openssl rand -hex 32` puis définir la même valeur
`OAPI_SERVICE_API_KEY` dans l'environnement d'OAPI et du serveur Astro.
Utiliser une valeur distincte de `JWT_SECRET`. Redémarrer les deux services après
rotation. Une valeur vide désactive cette authentification.

Les appels utilisent `Authorization: Bearer <clé>`. La clé donne les droits
administrateur à l'application serveur : statistiques globales, badges, profils
Discord/Minecraft par identifiant et téléchargement des images protégées.
Elle ne représente aucun utilisateur Discord. Pour les routes `/me`, employer
la route par identifiant correspondante. Les routes `/auth/*` restent destinées
aux sessions navigateur Discord.

Un en-tête Authorization invalide est rejeté sur les routes protégées même si une
session cookie est présente. Les cookies existants continuent de fonctionner sans
en-tête Authorization. Le service partage un quota de 10 appels par minute et par
route, y compris entre différents comptes cibles ; gérer les réponses 429 et
`Retry-After` côté site.

## Serveur Astro

Pour les actions à la demande, installer un adaptateur compatible Astro 5, par
exemple `@astrojs/node` 9, et compléter la configuration existante :

```js
import node from '@astrojs/node';

// Ajouter à l'objet defineConfig existant en conservant les intégrations :
output: 'server',
adapter: node({ mode: 'standalone' }),
```

Un hébergement de fichiers statiques seul ne peut pas exécuter ces actions. Une
page statique peut effectuer des lectures à la compilation, mais ne doit jamais
embarquer de données privées ou de clé de service dans le HTML produit.

Exemple de fonction réservée au serveur Node, dans `src/server/oapi.ts` :

```ts
export async function investorStats() {
  const base = process.env.OAPI_URL;
  const key = process.env.OAPI_SERVICE_API_KEY;
  if (!base || !key) throw new Error('Configuration OAPI manquante');

  const response = await fetch(new URL('/api/investor/stats', base), {
    headers: { Authorization: `Bearer ${key}` },
    signal: AbortSignal.timeout(30_000),
    redirect: 'error',
    cache: 'no-store',
  });
  if (!response.ok) throw new Error(`OAPI: ${response.status}`);
  return response.json();
}
```

Définir `OAPI_URL=https://votre-api` (sans préfixe `/api`). Adapter le chemin si le
préfixe d'OAPI est personnalisé. Appeler cette fonction uniquement depuis les pages
rendues à la demande ou endpoints serveur, après vérification des droits.
Ne pas importer ce module dans un composant React hydraté ou un script navigateur.
Ne jamais préfixer la clé par `PUBLIC_`.

Le navigateur appelle des routes précises de votre site. Chaque route vérifie la
session PocketBase côté serveur, les droits et la propriété du compte cible ; les
écritures vérifient également l'origine pour prévenir les requêtes intersites.
Le serveur ajoute ensuite la clé pour appeler OAPI. Ne pas créer de proxy public
acceptant n'importe quel chemin OAPI : cela exposerait tous les droits administrateur.

Pour les images, le serveur Astro télécharge le PNG avec la clé et le transmet
avec `Content-Type: image/png` et `Cache-Control: no-store`, après autorisation.
Un simple `<img src="https://oapi/...">` ne transmet pas la clé de service.

Cette préparation modifie OAPI uniquement. La migration serveur, la validation des
sessions et les routes de façade restent à implémenter dans le dépôt Astroloutre.

Références :
- Configuration du site : https://github.com/L-Antre-des-Loutres/Astroloutre/blob/develop/astro.config.mjs
- Sessions existantes : https://github.com/L-Antre-des-Loutres/Astroloutre/blob/develop/src/utils/API/apiFetcher.ts
- Rendu à la demande : https://docs.astro.build/en/guides/on-demand-rendering/
- Variables privées : https://docs.astro.build/en/guides/environment-variables/
