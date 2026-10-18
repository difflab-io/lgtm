# Architecture Decision Records

Records follow the [MADR](https://adr.github.io/madr/) format. Start from [template.md](template.md), name records `NNNN-kebab-title.md`, and number them sequentially. A proposed record describes a decision under discussion, not an implemented contract.

| ADR                                  | Status   | Summary                                                                                  |
| ------------------------------------ | -------- | ---------------------------------------------------------------------------------------- |
| [0001](0001-initial-architecture.md) | accepted | Local web app and Neovim plugin for code and document review, with CLI access for agents |

## Future topics

These topics need discussion before a decision is recorded:

- Local/remote sync: synchronizing local SQLite data with a remote service backed by PostgreSQL.
- Desktop app database migrations: upgrading installed local databases across app versions, including interrupted upgrades and recovery.

## References

- [Architecture Decision Records](https://github.com/architecture-decision-record/architecture-decision-record)
- [MADR template](https://github.com/architecture-decision-record/architecture-decision-record/tree/main/locales/en/templates/decision-record-template-of-the-madr-project)
