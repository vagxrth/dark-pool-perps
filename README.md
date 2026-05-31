# darkpool_perps

A confidential Solana app built with Arcium: an Anchor program queues computations, and Arcis instructions define the confidential logic.

## Quickstart

```bash
arcium build
arcium test
```

## Layout

| Path | Purpose |
|------|---------|
| `programs/darkpool_perps/` | Anchor program: queues computations, handles callbacks |
| `encrypted-ixs/` | Arcis confidential instructions |
| `tests/darkpool_perps.ts` | TypeScript integration tests |
| `Arcium.toml` | Localnet and cluster configuration |

## Docs

<https://docs.arcium.com/developers>
