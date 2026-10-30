# Architecture Decision Records

Records follow the [MADR](https://adr.github.io/madr/) format. Start from [template.md](template.md), name records `NNNN-kebab-title.md`, and number them sequentially. A proposed record describes a decision under discussion, not an implemented contract.

| ADR                                  | Status   | Summary                                                                                  |
| ------------------------------------ | -------- | ---------------------------------------------------------------------------------------- |
| [0001](0001-initial-architecture.md) | accepted | Local web app and Neovim plugin for code and document review, with CLI access for agents |
| [0002](0002-cli-design/ADR.md)       | accepted | Local LGTM forge snapshots, local vs. remote forge review flows, and CLI design          |

## Future topics

Future decisions must define managed Git storage, review ownership, explicit forge publication and remote instance access; the current hello-review skeleton has no SQL persistence or synchronization contract.

## References

- [Architecture Decision Records](https://github.com/architecture-decision-record/architecture-decision-record)
- [MADR template](https://github.com/architecture-decision-record/architecture-decision-record/tree/main/locales/en/templates/decision-record-template-of-the-madr-project)
