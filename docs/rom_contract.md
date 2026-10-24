# Contrat d'intégration ROM / OAPI

Ce document fixe ce que chaque partie s'engage à faire dans la liaison d'une ROM à un compte Discord : **OAPI**, le **site rom-patcher** et le **jeu (ROM)**. Le détail technique des routes (champs, codes d'erreur, configuration PocketBase) est dans [rom_auth.md](rom_auth.md) ; les routes d'amis sont dans [friends.md](friends.md). En cas de divergence, le code et ces deux documents priment.

## 1. Parties et rôles

| Partie | Rôle | S'identifie par |
|---|---|---|
| **Membre** | Propriétaire du compte Discord ; lie, utilise et révoque ses appareils | Connexion Discord sur le site |
| **Site rom-patcher** | Génère les codes, liste et révoque les appareils | Cookie de session OAPI (`auth_token`) |
| **Jeu (ROM)** | Échange le code, lit la liste d'amis | Token `rom_…` |
| **OAPI** | Émet, vérifie et révoque les tokens | — |

## 2. Cycle de vie d'une liaison

1. Le membre se connecte sur le site, qui appelle `POST /auth/rom/code` (avec un `label` optionnel).
2. Le membre recopie le code (6 chiffres de 1 à 9) dans le jeu **dans les 15 minutes**.
3. Le jeu appelle `POST /auth/rom/token` ; OAPI lui rend un token `rom_…`, **une seule fois**.
4. Le jeu conserve le token et l'envoie en `Authorization: Bearer rom_…`.
5. Le membre peut à tout moment voir ses appareils et en révoquer un depuis le site ; le token cesse alors de fonctionner immédiatement.

Une liaison = un appareil = un token. Un membre peut avoir plusieurs appareils liés en même temps ; en lier un nouveau ne déconnecte pas les autres.

## 3. Engagements d'OAPI

- Un code est valable **15 minutes**, **à usage unique**, et ne peut être deviné que par essais : au plus **30 échanges échoués par minute au total** (puis `429` + `Retry-After`), en plus du quota de 10 requêtes par minute et par route.
- Le token est tiré de 32 octets aléatoires ; OAPI ne stocke que son **hash SHA-256**. Il n'est jamais renvoyé, journalisé ni listé : le perdre impose de relier l'appareil.
- Un token `rom_` ne donne accès qu'à **`GET /friends/{account}`**, en lecture seule, sur **son propre compte**. Toute autre route le refuse (`401`) ; viser un autre compte renvoie `403`.
- Un token révoqué (ligne supprimée) ou désactivé (`is_disabled = true`) est refusé aussitôt (`401`).
- Les réponses sont privées (`Cache-Control: no-store`). Une panne PocketBase donne `502`, sans détail interne.
- Le token de service et les tokens `rom_` ne sont pas interchangeables : un token `rom_` n'est jamais comparé à la clé de service.

## 4. Engagements du site rom-patcher

- N'appeler les routes `/auth/rom/code`, `/auth/rom/devices` et `/auth/rom/devices/{id}/revoke` qu'avec la session du membre (`credentials: "include"`), jamais avec le token de service.
- Être déclaré dans `site_auth.url` ou `site_auth.extra_urls` (HTTPS en production), sinon les `POST` sont rejetés (`403`).
- **Ne générer un code qu'à la demande explicite du membre**, au moment où il lie l'appareil : chaque code en attente augmente légèrement la probabilité qu'un attaquant tombe dessus.
- Afficher le code avec sa date d'expiration et dire clairement qu'il ne sert qu'une fois ; ne pas le stocker ni le renvoyer à un tiers.
- Proposer au membre la liste de ses appareils (`label`, date de liaison) avec un bouton de révocation.
- Gérer les refus : `401` (session expirée → reconnexion), `429` (attendre `Retry-After`), `502` (réessayer plus tard).

## 5. Engagements du jeu (ROM)

- Appeler `POST /auth/rom/token` **sans cookie**, avec le seul corps `{ "code": "…" }`, au moment où le membre saisit le code.
- **Stocker le token de façon persistante** dès réception : il n'est plus récupérable. Le traiter comme un secret (pas de journal, pas d'affichage, pas de partage de sauvegarde).
- Envoyer le token uniquement à OAPI, via HTTPS, dans `Authorization: Bearer rom_…`.
- Ne jamais réessayer en boucle un code refusé : `404` = code faux, expiré ou déjà utilisé ; inviter le membre à en demander un nouveau sur le site. Respecter `Retry-After` sur `429`.
- Sur `401` avec un token déjà stocké : l'appareil a été révoqué ou désactivé. Effacer le token local et proposer une nouvelle liaison, sans insister.
- Ne pas supposer plus de droits que la lecture de sa propre liste d'amis ; les autres fonctions (statistiques, classement) feront l'objet d'une évolution distincte.

## 6. Ce qui n'est pas garanti

- **Pas d'expiration automatique des tokens** : un appareil reste lié jusqu'à révocation.
- **Pas de plafond** sur le nombre d'appareils par membre.
- **Échanges simultanés d'un même code** : l'un des deux appelants peut recevoir un token inutilisable (`401` ensuite) ; il suffit de relier l'appareil avec un nouveau code.
- **Force brute** : le risque n'est pas nul (environ 0,08 % pour un code en attente pendant ses 15 minutes, plus avec plusieurs codes simultanés) ; il est borné par le budget global d'échecs.
- **Déni de service** : un attaquant peut épuiser le budget d'échecs et faire refuser (`429`) les liaisons légitimes pendant au plus une minute à la fois ; les tokens déjà émis et le reste de l'API ne sont pas touchés. Le budget est par instance d'OAPI.
- **Perte du token** : irrécupérable, la seule issue est de relier l'appareil.

## 7. Responsabilités du membre

- Ne communiquer son code qu'au jeu qu'il veut lier, et le saisir sans attendre.
- Révoquer depuis le site tout appareil perdu, vendu ou qu'il ne reconnaît pas dans la liste (un appareil inconnu peut signaler un code deviné).
- Savoir qu'un token donne accès à sa liste d'amis : il doit être traité comme un mot de passe de lecture.

## 8. Évolution du contrat

- Ajouter des routes accessibles aux tokens `rom_` (statistiques, classement…) est un **changement de ce contrat** : il faut mettre à jour §3, la documentation de la route et prévenir le site et le jeu.
- Les formats actuels (`rom_` + 64 hexadécimaux, code de 6 chiffres de 1 à 9, 15 minutes) sont stables : toute modification doit rester acceptée par les jeux déjà liés (un token émis reste valable tant qu'il n'est pas révoqué).

## 9. Liste de contrôle d'intégration

- [ ] Collection PocketBase `discord_identity_token` créée, `token` sans « Nonempty », deux index uniques partiels en place ([rom_auth.md §7](rom_auth.md)).
- [ ] Site rom-patcher présent dans `site_auth.extra_urls`, HTTPS en production.
- [ ] Site : génération de code à la demande, affichage de l'expiration, liste et révocation des appareils.
- [ ] Jeu : échange du code, stockage persistant du token, gestion de `404`, `429`, `401`.
- [ ] Test de bout en bout : code → token → `GET /friends/me` → révocation → `401`.
