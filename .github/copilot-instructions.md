# Règles d'exécution des commandes Terminal (RTK)

Chaque fois que tu dois me suggérer une commande shell/terminal ou utiliser un outil CLI (comme git, npm, cargo, pytest, docker, ls, cat, etc.), tu DOIS obligatoirement préfixer cette commande par `rtk `.

- Mauvais : `git diff`
- Bon : `rtk git diff`
- Mauvais : `npm run test`
- Bon : `rtk npm run test`

Le but est d'utiliser le proxy Rust Token Killer (RTK) pour compresser la sortie du terminal et économiser tes tokens de contexte.