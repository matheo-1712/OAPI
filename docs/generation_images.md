# Service de Génération d'Images

Le service de génération d'images (`src/services/image_service.rs`) crée dynamiquement des visuels haute fidélité pour les profils Discord et Minecraft.

---

## Logique de Caching

Le service utilise un cache intelligent pour minimiser l'utilisation du CPU et de la bande passante.

1.  **Organisation** : Les images sont stockées dans `public/generated_images/` dans des sous-dossiers spécifiques (`discord_summary/` ou `minecraft_summary/`) par identifiant unique (`discord_id` ou `account_id`).
2.  **Hash de Contenu** : Un hash SHA-256 est généré à partir de **toutes les données** du profil (pseudo, statistiques, badges, serveurs).
3.  **Vérification** : Si le fichier `{hash}.png` existe, il est servi immédiatement.
4.  **Auto-nettoyage** : Lorsqu'une nouvelle image est générée (données modifiées), le dossier de l'utilisateur est vidé pour ne conserver que la version la plus récente.

---

## Fonctionnalités par Profil

### 1. Résumé Discord
- **Avatar** : Cercle antialiasé avec bordure.
- **Stats** : Messages envoyés et Temps vocal.
- **Spécificités** : Affiche le "Meilleur ami Loutre" (le joueur avec qui vous avez passé le plus de temps en vocal) et vos rôles préférés.

### 2. Résumé Minecraft
- **Avatar** : Tête 3D du joueur (récupérée via son UUID).
- **Stats** : Playtime, Distance (en blocs), Blocs minés/posés, Morts et Kills.
- **Spécificités** : Affiche les **3 serveurs préférés** sous forme de badges colorés selon la couleur du serveur dans la base de données.

---

## Détails Techniques du Dessin

- **Moteur** : Utilise les crates `image` (pixels) et `rusttype` (typographie).
- **Formatage** : Les grands nombres sont formatés avec des espaces (ex: `1 250 000`) pour une lecture optimale.
- **Pills (Badges)** : Les étiquettes de serveurs ou de rôles gèrent le retour à la ligne automatique pour ne pas dépasser de l'image.
- **Couleurs** : Support complet des codes HEX pour les bordures et accents.

---

## URLs Publiques
Les chemins retournés par l'API sont prêts à être utilisés dans un tag `<img>` :
`/generated_images/minecraft_summary/{uuid}/{hash}.png`
