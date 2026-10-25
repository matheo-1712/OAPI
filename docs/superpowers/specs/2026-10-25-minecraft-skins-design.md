# Skins Minecraft dans PocketBase — conception

## Intention

Le site Astroloutre affiche la tête et le corps des joueurs Minecraft en appelant
`mc-heads.net` à chaque rendu (liste des joueurs d'un serveur, classements, page
joueur, profil Discord). Ce service limite le nombre de requêtes, et le site en
fait beaucoup au build.

La collection PocketBase `players` a reçu deux champs fichier, `head` et `skin`.
OAPI doit les remplir lui-même à partir de la texture officielle Mojang. Le site
les lit déjà (branche Astroloutre `feat/no-rate-limit-mcheads`) et ne retombe sur
`mc-heads.net` que tant qu'un champ est vide.

## Objectifs

- Une troisième tâche quotidienne `skins`, après `purge` et `badges`, qui pour
  chaque compte Minecraft :
  1. récupère la texture du skin auprès de Mojang ;
  2. si elle a changé depuis le dernier passage, génère la tête et le corps de
     face, puis les envoie dans `players.head` et `players.skin`.
- `head` : la tête vue de face, avec la couche « chapeau ».
- `skin` : le corps entier vu de face (tête, torse, bras, jambes, avec les
  couches extérieures), affichable tel quel à la place de `mc-heads.net/body`.
- L'image de résumé Minecraft d'OAPI (`image_service.rs`) utilise `head` depuis
  PocketBase quand il est rempli, au lieu de `mc-heads.net`.
- Un résumé de la tâche dans les logs, comme les autres tâches.

## Hors périmètre

- Mise à jour à la connexion d'un joueur (route appelée par le plugin du serveur).
  Un changement de skin apparaît donc au plus tard le lendemain, puis au
  prochain build du site, qui est statique.
- Rendu 3D ou isométrique, vue de dos, capes.
- Stocker la texture brute 64×64.
- Changements côté site : déjà faits.

## Anti-objectifs

- Dépasser la limite de requêtes de Mojang : on remplacerait un service limité
  par un autre.
- Renvoyer chaque jour les images de tous les joueurs alors que rien n'a changé.
- Écraser une image existante par une image vide ou cassée quand Mojang ne répond
  pas ou ne connaît pas le joueur.
- Traiter les comptes Steam de la collection `players`.
- Qu'un joueur en échec arrête le traitement des autres.

## Contraintes

- Le planificateur existe (`services/scheduler.rs`) : une tâche est une entrée
  de `DAILY_TASKS` qui renvoie un `TaskSummary`. Une seule instance en production.
- `PocketbaseClient` n'envoie que du JSON (`create_record`, `update_record`).
  Remplir un champ fichier demande une requête `multipart/form-data`, donc la
  fonctionnalité `multipart` de `reqwest`.
- `players` mélange comptes Minecraft et Steam. Un compte Minecraft a un
  `account_id` au format UUID (`image_service::valid_uuid`) ; un compte Steam a un
  SteamID numérique (`badge_service::is_steam_id`). On se base sur l'`account_id`,
  pas sur le libellé de plateforme (même choix que `badge_service`).
- Plusieurs records `players` peuvent partager le même `account_id` (doublons
  connus, cf. le dédoublonnement côté site).
- `image_assets::download` n'autorise qu'une liste fermée d'hôtes. La texture
  vient de `textures.minecraft.net`.
- Des joueurs peuvent avoir un UUID hors-ligne (non premium) : Mojang ne les
  connaît pas.

## Approches envisagées

- **A. Tâche quotidienne `skins` (choisie).** Réutilise le planificateur, ne
  demande rien à un autre dépôt. Limite acceptée : délai d'un jour au plus, sans
  conséquence puisque le site est généré à l'avance.
- **B. À la connexion du joueur.** Route OAPI appelée par le plugin Minecraft.
  Plus réactif, mais demande de modifier le plugin, et un joueur qui ne revient
  pas n'a jamais d'images. Écartée.
- **C. Les deux.** Deux chemins à maintenir pour un gain faible tant que le site
  est statique. Écartée ; la route pourra être ajoutée plus tard en réutilisant le
  même service.

## Conception

### Nouveau champ PocketBase

À créer à la main dans `players`, comme `head` et `skin` :

| Champ | Type | Rôle |
| --- | --- | --- |
| `skin_hash` | texte | identifiant de la texture déjà traitée |

Valeur : `<hash>:<modèle>`, où `<hash>` est le dernier segment de l'URL de
texture Mojang (`http://textures.minecraft.net/texture/<hash>`) et `<modèle>`
vaut `classic` ou `slim`. Le modèle fait partie de la valeur parce qu'il change
le rendu des bras sans changer la texture.

### Récupération chez Mojang

1. `GET https://sessionserver.mojang.com/session/minecraft/profile/<uuid sans tirets>`.
   - `200` : lire `properties[name = "textures"].value`, décoder en base64 puis
     en JSON, et lire `textures.SKIN.url` et `textures.SKIN.metadata.model`
     (`"slim"` si présent, sinon `classic`).
   - `204` ou `404` : joueur inconnu de Mojang (UUID hors-ligne). Ignoré.
   - `429` : limite atteinte. On arrête la tâche pour ce passage ; les joueurs
     restants sont repris le lendemain.
   - Autre erreur ou réponse illisible : échec pour ce joueur.
   - Pas de `SKIN` dans les textures (skin par défaut) : ignoré.
2. Si `<hash>:<modèle>` est égal au `skin_hash` du record et que `head` et
   `skin` sont remplis : rien à faire.
3. Sinon, télécharger la texture depuis `textures.minecraft.net`, avec les mêmes
   garde-fous que `image_assets::download` (taille, dimensions, pas de
   redirection). Ajouter l'hôte à la liste autorisée, en `https` (réécrire le
   schéma `http` renvoyé par Mojang).

Une requête Mojang toutes les **1,5 seconde** au plus (constante dans le service),
soit 40 par minute, nettement sous les limites observées de Mojang (non
documentées officiellement). Avec quelques centaines de joueurs, la tâche dure
une dizaine de minutes en fond, sans gêner le serveur HTTP. Un seul appel par
`account_id` et par passage, même s'il y a plusieurs records : le résultat est
réutilisé pour les doublons.

### Rendu

La texture fait 64×64 (format actuel) ou 64×32 (ancien format, avant 1.8).
Les deux images sont générées en pixels nets (redimensionnement « plus proche
voisin »), au format PNG transparent.

**`head`** : face de la tête `(8,8)–(16,16)`, puis couche chapeau
`(40,8)–(48,16)` par-dessus (transparence respectée). Agrandie de 8×8 à
**128×128**.

**`skin`** : assemblage de face sur un canevas de 16×32 pixels, agrandi à
**128×256** (×8). Coordonnées dans la texture (x, y, largeur, hauteur) et
position sur le canevas :

| Partie | Base | Couche extérieure | Position |
| --- | --- | --- | --- |
| Tête | (8,8,8,8) | (40,8,8,8) | (4,0) |
| Torse | (20,20,8,12) | (20,36,8,12) | (4,8) |
| Bras droit (à gauche à l'écran) | (44,20,L,12) | (44,36,L,12) | (4−L,8) |
| Bras gauche | (36,52,L,12) | (52,52,L,12) | (12,8) |
| Jambe droite | (4,20,4,12) | (4,36,4,12) | (4,20) |
| Jambe gauche | (20,52,4,12) | (4,52,4,12) | (8,20) |

`L` = 4 pour le modèle `classic`, 3 pour `slim`.

Ancien format 64×32 : pas de couches extérieures sauf le chapeau, et le bras
et la jambe gauches sont le miroir horizontal des droits.

Les couches extérieures sont posées en respectant leur transparence. Pour
l'ancien format, si la zone du chapeau n'a aucun pixel transparent, elle est
ignorée : beaucoup de textures anciennes la remplissent d'une couleur unie et
opaque, et Minecraft l'ignore aussi.

### Envoi dans PocketBase

Nouvelle méthode `PocketbaseClient::update_record_multipart(collection, id,
fields, files)` : `PATCH` en `multipart/form-data` avec `head` (`head.png`),
`skin` (`skin.png`) et `skin_hash` dans une seule requête, pour que les trois
champs restent cohérents. PocketBase remplace l'ancien fichier d'un champ
fichier simple (« Single ») et supprime l'ancien.

### Tâche `skins`

- Nouveau `services/skin_service.rs` avec `refresh_all(pb) -> Result<TaskSummary, String>`.
- Ajouté en dernier dans `DAILY_TASKS` (`purge`, `badges`, `skins`).
- Liste tous les `players`, garde ceux dont l'`account_id` est un UUID valide.
- `TaskSummary` : `processed` = comptes Minecraft examinés ; `succeeded` =
  à jour (inchangés, mis à jour ou ignorés car inconnus de Mojang / skin par
  défaut) ; `failed` = IDs de records en échec. Le log précise en plus combien
  ont été mis à jour.
- Un `429` arrête la boucle : la tâche renvoie le résumé partiel et logge un
  avertissement.

### Image de résumé Minecraft

`MinecraftPlayer` gagne `collectionId` et `head` (optionnels). Dans
`image_service.rs`, si `head` est rempli, l'avatar est téléchargé depuis
`<pb_url>/api/files/<collectionId>/<id>/<head>` (déjà autorisé par
`image_assets`) ; sinon on garde `mc-heads.net`.

### Documentation

- `docs/daily_tasks.md` : la troisième tâche, ce qu'elle compte, son
  comportement face à Mojang.
- `docs/pocketbase.md` (et `docs/en/pocketbase.md`) : les champs `head`, `skin`,
  `skin_hash` de `players`.

## Tests

- Rendu : textures de test construites en mémoire (64×64 classique, 64×64 slim,
  64×32 ancien format) ; vérifier dimensions de sortie et quelques pixels
  attendus par partie, couche extérieure appliquée, miroir de l'ancien format.
- Décodage de la réponse Mojang : `200` classique et slim, sans `SKIN`, `204`,
  `429`, JSON invalide (avec `mockito`, déjà en dépendance de dev).
- Tâche : un joueur inchangé n'est pas renvoyé ; un échec n'arrête pas les
  autres ; un `429` arrête la boucle ; les comptes Steam sont ignorés ; un
  `account_id` en doublon ne fait qu'un appel Mojang.
- Multipart : la requête envoyée contient les deux fichiers et `skin_hash`.

## Laissé au choix de l'implémentation

- Découpage interne du service (client Mojang, rendu, tâche).
- Forme exacte des structures de décodage de la réponse Mojang.
- Messages de log.
