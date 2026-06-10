# Instructions globales — Guillaume / Mintsys

## Philosophie de développement
- Le code doit être lisible et compréhensible par un humain qui ne connaît pas le projet
- Privilégier la clarté sur la concision ; un nom de variable explicite vaut mieux qu'un commentaire
- Chaque fichier, fonction et module doit avoir une responsabilité unique et claire
- Aucune incohérence tolérée : nommage, structure, style doivent être uniformes dans tout le projet

## Dépendances
- Toujours utiliser les versions stables et maintenues des paquets
- Vérifier l'activité du package avant de l'ajouter (dernière release, nb de mainteneurs, issues ouvertes)
- Ne pas ajouter une dépendance si la stdlib suffit
- Toujours préciser les versions dans les fichiers de dépendances (`package.json`, `pyproject.toml`, etc.)
- Pas de dépendances abandonnées ou dépréciées

## Nommage
- Variables, fonctions, classes : noms en anglais, descriptifs, sans abréviation opaque
  - ✅ `userSessionToken`, `fetchActiveClients()`, `ConnectionError`
  - ❌ `tok`, `getData()`, `Err`
- Fichiers et dossiers : kebab-case pour les projets web, snake_case pour Python
- Constantes : SCREAMING_SNAKE_CASE
- Éviter les préfixes inutiles (`data_`, `info_`, `my_`)

## Structure du code
- Une fonction = une responsabilité ; si elle fait plus de 40 lignes, la découper
- Pas de logique métier dans les contrôleurs/routes — déléguer aux services
- Séparer clairement : configuration, domaine métier, infrastructure (I/O, DB, API externes)
- Les fichiers de configuration ne contiennent pas de secrets (utiliser `.env` + variables d'environnement)

## Logs
- Utiliser un logger structuré (ex: `pino`, `winston` pour Node ; `structlog` ou `logging` configuré en JSON pour Python)
- Chaque log doit répondre à : "Qu'est-ce qui s'est passé ? Où ? Avec quel contexte ?"
- Niveaux stricts :
  - `DEBUG` : détails de débogage, jamais en production
  - `INFO` : événements normaux du cycle de vie (`Server started on port 3000`, `Job completed: 42 records processed`)
  - `WARN` : situation anormale mais récupérée (`Retry 2/3 for endpoint /api/x`)
  - `ERROR` : erreur qui impacte l'utilisateur ou le service, avec stack trace
- Pas de `console.log` laissés dans le code final
- Les messages de log sont en anglais, complets, sans jargon cryptique
  - ✅ `"Failed to connect to database after 3 retries — host: db.internal, port: 5432"`
  - ❌ `"db err"`, `"failed"`, `"error!!!"`
- Inclure le contexte pertinent dans le log (userId, requestId, nom du job, etc.)

## Gestion des erreurs
- Ne jamais swallower silencieusement une exception (`except: pass`, `catch(e) {}`)
- Les erreurs inattendues sont toujours loguées avec stack trace
- Distinguer les erreurs utilisateur (4xx) des erreurs système (5xx) dans les APIs
- Utiliser des classes d'erreur personnalisées avec des messages explicites

## Commentaires et documentation
- Le code doit s'expliquer lui-même ; les commentaires expliquent le **pourquoi**, pas le **quoi**
- Documenter les fonctions publiques avec JSDoc / docstrings Python
- Un `README.md` à la racine de chaque projet avec : description, prérequis, installation, usage, variables d'environnement

## Tests
- Tout nouveau code métier doit avoir des tests unitaires
- Les noms de tests décrivent le comportement attendu : `"should return 404 when user does not exist"`
- Pas de logique dans les tests, un test = un cas

## Git
- Messages de commit au format Conventional Commits : `feat:`, `fix:`, `chore:`, `docs:`, `refactor:`
- Un commit = un changement cohérent et atomique
- Pas de fichiers générés ou de secrets commités (`.gitignore` soigné)

## Sécurité
- Jamais de credentials, tokens ou clés en dur dans le code
- Valider et sanitizer toutes les entrées externes
- Les dépendances sont auditées régulièrement (`npm audit`, `pip-audit`)