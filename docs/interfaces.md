# Interfaces OAPI

Toutes les pages applicatives utilisent `public/theme.css` et une navigation commune.

- `/` : accueil, connexion et rafraîchissement des badges.
- `/tools.html` : génération des cartes Discord et Minecraft, aperçu des images protégées et activité Discord personnelle.
- `/explorer.html` : catalogue filtrable issu de `/api-docs/openapi.json`, paramètres, réponses et consultation manuelle des routes GET fixes.
- `/monitoring.html` : états groupés, actualisés toutes les 30 secondes.

L’atelier découvre les routes de génération depuis les `operationId` du schéma OpenAPI, afin de respecter les chemins configurés côté serveur. Les actions utilisent le cookie de session ; aucune clé de service n’est demandée dans l’interface. Les aperçus de cartes acceptent uniquement les URL de même origine sous `/generated_images/`.

L’explorateur n’exécute pas les routes POST ni les navigations OAuth. Les autorisations restent contrôlées par le serveur pour chaque appel. Swagger et son schéma restent publics comme dans la configuration existante ; ils décrivent les contrats des routes et ne contiennent pas les valeurs des secrets d’environnement.

Validation : `node --test tests/*.test.cjs` et `cargo check --locked`.
