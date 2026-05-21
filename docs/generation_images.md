# Service de Génération d'Images

Le service de génération d'images (`src/services/image_service.rs`) est responsable de la création dynamique des visuels de profil Discord.

---

## Logique de Caching

Pour optimiser les performances et éviter de redessiner inutilement des images identiques, le service utilise un système de cache basé sur le contenu des données et l'identifiant utilisateur.

1.  **Organisation par Dossier** : Chaque utilisateur possède son propre dossier nommé selon son `discord_id` dans `public/generated_images/discord_summary/{discord_id}/`.
2.  **Calcul du Hash** : Un hash SHA-256 est calculé à partir de l'état actuel des données de l'utilisateur.
3.  **Vérification** : Si le fichier `{hash}.png` existe déjà dans le dossier de l'utilisateur, il est renvoyé immédiatement.
4.  **Génération & Nettoyage** : Si les données ont changé (hash différent), le service **vide le dossier de l'utilisateur** pour ne conserver qu'une seule image (la plus récente) avant de générer la nouvelle.

Cette approche garantit que l'image est toujours à jour tout en évitant l'accumulation de fichiers obsolètes pour un même utilisateur.

---

## Processus de Dessin

L'image est construite par empilement de couches en utilisant la crate `image` :

### 1. Fond et Structure
- Création d'une image vierge RGBA.
- Remplissage avec les couleurs de fond (Deep BG, Sidebar).
- Dessin de la barre d'accentuation supérieure.

### 2. Avatar Circulaire
- Téléchargement de l'avatar via `reqwest`.
- Redimensionnement et application d'un masque circulaire (calcul de distance par rapport au centre).
- Ajout d'une bordure blanche antialiasée.

### 3. Typographie
- Chargement de la police `Arial` incluse dans les binaires via `include_bytes!`.
- Rendu du texte avec `rusttype` (Centrage automatique, calcul des métriques).

### 4. Grille de Statistiques
- Dessin de "cartes" de fond pour chaque statistique.
- Ajout d'un trait d'accentuation coloré sous chaque carte.
- Affichage des labels et des valeurs.

### 5. Badges de Rôles (Pills)
- Filtrage des rôles pertinents (contenant "Loutre" ou "Rôle").
- Dessin de "pilules" à bords arrondis avec :
    - Fond sombre.
    - Bordure colorée (extraite de la couleur réelle du rôle Discord).
    - Texte du rôle.

---

## Optimisations Techniques

- **Antialiasing Manuel** : Le service calcule manuellement l'alpha des pixels sur les bords des cercles et des pilules pour un rendu lisse.
- **Dossiers Automatiques** : Le service s'assure que l'arborescence des dossiers de sortie existe avant d'enregistrer.
- **URLs Publiques** : Le chemin retourné est relatif à la racine du serveur statique d'Axum (`/generated_images/...`).
