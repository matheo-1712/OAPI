# Service de Génération d'Images

Le service de génération d'images (`src/services/image_service.rs`) est responsable de la création dynamique des visuels de profil Discord.

---

## Logique de Caching

Pour optimiser les performances et éviter de redessiner inutilement des images identiques, le service utilise un système de cache basé sur le contenu des données.

1.  **Calcul du Hash** : Avant toute génération, un hash SHA-256 est calculé à partir des données de l'utilisateur (pseudo, tag, avatar, statistiques agrégées, rôles).
2.  **Vérification** : Si un fichier nommé `{hash}.png` existe déjà dans le dossier `public/generated_images/discord_summary/`, le service renvoie immédiatement son URL.
3.  **Génération** : Si le fichier n'existe pas (données modifiées ou premier appel), le service dessine l'image.

Cette approche garantit que l'image est toujours à jour par rapport aux statistiques réelles du joueur tout en étant extrêmement rapide lors d'appels répétés.

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
