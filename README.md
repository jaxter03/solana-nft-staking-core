# NFT Staking on Metaplex Core

An Anchor program that stakes [Metaplex Core](https://developers.metaplex.com/core) NFTs
without moving them out of the owner's wallet. The NFT is frozen in place with Core plugins,
rewards accrue per second, and staking statistics live on the collection itself.

This builds on the Turbin3 Core staking program (`create_collection`, `mint_nft`,
`initialize_config`, `stake`, `unstake`) and adds the three mandatory Task 1 features:

| # | Feature | Instruction / change |
|---|---------|----------------------|
| 1 | Claim rewards without unstaking | `claim_rewards` |
| 2 | Burn-to-earn with `BurnDelegate` | `burn_staked_nft` |
| 3 | Collection-level `total_staked` Attribute | `create_collection`, `stake`, `unstake`, `burn_staked_nft` |

- **Program ID (devnet):** [`h9Ku7LMnUXYojrRPxc85kTdz8YUdKK33RFAWfrzaaxy`](https://explorer.solana.com/address/h9Ku7LMnUXYojrRPxc85kTdz8YUdKK33RFAWfrzaaxy?cluster=devnet)
- **Metaplex Core:** `CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d`
- **Toolchain:** Anchor 1.1.2 · `mpl-core` 0.12.1 · Solana CLI 3.1.10 · Rust 1.89
- **Tests:** 14 passing on localnet and on devnet

---

## Quick start

```bash
yarn install
anchor build
anchor test --validator legacy        # local validator, Core cloned from devnet
```

Run the same suite against the devnet deployment:

```bash
anchor test --skip-build --skip-deploy --skip-local-validator --provider.cluster devnet
```

The local validator runs on port `8999` (gossip `8100`) so it doesn't clash with another
validator on the default ports. It clones the real Core program from devnet, so every CPI
in the tests hits real Core code.

---

## Accounts

| Account | Derivation | Purpose |
|---------|------------|---------|
| `update_authority` | `["update_authority", collection]` | Program PDA. Update authority of the collection; authority of each staked NFT's `FreezeDelegate` and `BurnDelegate` |
| `config` | `["config", collection]` | Staking parameters; mint authority of the reward mint |
| `rewards_mint` | `["rewards", config]` | SPL reward token, 6 decimals |

```rust
pub struct Config {
    pub admin: Pubkey,
    pub collection: Pubkey,
    pub rewards_per_day: u64, // reward base units per NFT per day, pro-rated per second
    pub freeze_period: i64,   // seconds an NFT must stay staked before `unstake`
    pub burn_bonus: u64,      // one-time bonus paid by `burn_staked_nft`
    pub config_bump: u8,
    pub rewards_bump: u8,
}
```

### State stored on Core accounts

**Asset: Attributes plugin.** Authority-managed; the PDA signs as collection update authority.

| Key | Value |
|-----|-------|
| `staked` | `"true"` / `"false"` |
| `staked_at` | Unix timestamp of the stake; enforces `freeze_period` |
| `last_claimed` | Unix timestamp rewards were last paid up to |

**Asset: owner-managed plugins.** Added while staked, each delegated to the PDA.

- `FreezeDelegate { frozen: true }`: blocks transfers (and burns) while staked.
- `BurnDelegate`: lets the program burn the NFT in `burn_staked_nft`.

**Collection: Attributes plugin.**

| Key | Value |
|-----|-------|
| `total_staked` | Count of currently staked NFTs in the collection |

---

## Instructions

### `create_collection(name, uri)`
Creates a Core collection whose update authority is the `update_authority` PDA, with an
Attributes plugin initialised to `total_staked = "0"`.

### `mint_nft(name, uri)`
Mints a Core asset into the collection, owned by the caller. The PDA signs as the
collection authority.

### `initialize_config(rewards_per_day, freeze_period, burn_bonus)`
Creates the `config` PDA and the reward mint. Rejects collections whose update authority
is not this program's PDA.

### `stake`
Owner signs. The program checks the asset belongs to the signer and to the collection,
and that it isn't already staked, then:

1. Writes `staked = true`, `staked_at = now` and `last_claimed = now` to the asset's
   Attributes, adding the plugin on first stake or updating it on re-stake.
2. Adds `FreezeDelegate { frozen: true }`, with the PDA as authority.
3. Adds `BurnDelegate`, with the PDA as authority.
4. **Increments `total_staked`** on the collection.

### `claim_rewards` (Task 1.1)
Owner signs. The NFT must be staked.

1. `amount = (now - last_claimed) * rewards_per_day / 86_400`
2. Mints `amount` reward tokens to the user's ATA, creating it if needed. The `config` PDA
   signs as mint authority.
3. Sets `last_claimed = now`.

The FreezeDelegate is never touched, so **the NFT stays staked and frozen**. Core allows
updating the Attributes plugin while the asset is frozen.

### `burn_staked_nft` (Task 1.2)
Owner signs. The NFT must be staked.

1. Pays accrued rewards **plus `burn_bonus`** to the user's ATA.
2. Thaws the asset by setting `FreezeDelegate.frozen = false` through the PDA. Core's
   `FreezeDelegate` rejects `burn` while frozen, so this step must come first.
3. Burns the asset with `BurnV1`, **signed by the PDA as `BurnDelegate` authority**.
4. **Decrements `total_staked`** on the collection.

### `unstake`
Owner signs. The NFT must be staked, and `now - staked_at >= freeze_period`.

1. Pays accrued rewards since `last_claimed`.
2. Thaws the asset through the PDA. Core rejects removing *any* plugin while the asset is
   frozen.
3. Removes `FreezeDelegate` and `BurnDelegate`, with the owner as authority.
4. Resets the asset Attributes to `staked = false`.
5. **Decrements `total_staked`** on the collection.

### Errors

| Error | Raised when |
|-------|-------------|
| `NotOwner` | Signer is not the asset owner |
| `WrongCollection` | Asset isn't in the config's collection, or the collection isn't owned by the PDA |
| `AlreadyStaked` / `NotStaked` | Stake state doesn't match the instruction |
| `FreezePeriodNotElapsed` | `unstake` called before `freeze_period` |
| `MissingCollectionStats` | Collection has no `total_staked` attribute |
| `InvalidAttribute` / `InvalidCoreAccount` / `Overflow` | Malformed data or arithmetic overflow |

---

## Tests

`tests/nft_staking_core.ts` uses the Anchor client for the program and Umi with
`@metaplex-foundation/mpl-core` to read plugin state straight from the Core accounts.

Test parameters: 1 reward token per second, a 10 s freeze period and a 1,000-token burn
bonus.

```
setup
  ✔ creates a collection with total_staked = 0
  ✔ initializes the staking config and reward mint
  ✔ mints two NFTs into the collection
stake + collection stats
  ✔ stakes NFT A: freezes it, delegates burn, increments total_staked
  ✔ stakes NFT B: total_staked = 2
  ✔ rejects staking an already staked NFT
  ✔ a staked NFT cannot be transferred (FreezeDelegate)
  ✔ rejects unstaking before the freeze period
claim_rewards
  ✔ rejects a claim by someone who does not own the NFT
  ✔ mints accrued rewards and keeps the NFT staked and frozen
burn_staked_nft
  ✔ burns staked NFT B for accrued rewards + burn bonus, decrements total_staked
unstake
  ✔ unstakes NFT A after the freeze period: pays rewards, thaws, decrements total_staked
  ✔ rejects claiming on an unstaked NFT
  ✔ an unstaked NFT can be re-staked
```

Reward amounts are checked exactly, not just as non-zero. For a claim, the minted amount
must equal `(last_claimed_after - last_claimed_before) * rate`. For a burn, the amount
minted above `burn_bonus` must be a whole number of seconds of accrual and match the
transaction's block time.

---

## Devnet deployment

Deployed with `anchor deploy --provider.cluster devnet`; the IDL is published on-chain.

| | Address |
|---|---|
| Program | [`h9Ku7LMnUXYojrRPxc85kTdz8YUdKK33RFAWfrzaaxy`](https://explorer.solana.com/address/h9Ku7LMnUXYojrRPxc85kTdz8YUdKK33RFAWfrzaaxy?cluster=devnet) |
| Test collection | [`FxW7UhcSSNFYC2eysSJqnVuekRjYVtCQXcmD1K7UjqXy`](https://explorer.solana.com/address/FxW7UhcSSNFYC2eysSJqnVuekRjYVtCQXcmD1K7UjqXy?cluster=devnet) |
| NFT A (claimed, unstaked, re-staked) | [`A4P2z55Xn7Mngp21haPo8XfnzmKkAz5dUTEaKERmmkDR`](https://explorer.solana.com/address/A4P2z55Xn7Mngp21haPo8XfnzmKkAz5dUTEaKERmmkDR?cluster=devnet) |
| NFT B (burned) | `4WyxMBh1gcGwDbr6MNc4nM6jpxsjsKNKyUbFiFo7FKdT` |

Transactions from the devnet test run:

| Step | Transaction |
|------|-------------|
| `create_collection` | [26aQasi7…GdcZ6](https://explorer.solana.com/tx/26aQasi77srt4LwRb5PHjoTZ64ZrMfsaDmpix9nhwiXvL7VkcUuVVQUTy5ij5NbLcvimGAzj4ebvF3dN88qGdcZ6?cluster=devnet) |
| `initialize_config` | [3USqPXCa…K8Yw](https://explorer.solana.com/tx/3USqPXCaeRGkY3fjAqqzH2vwAsrRgdMTk4BjDvdNCxzpLZjcWcGDicaQzARPdFxHs6ZShxAwAoh9hGH9u4K1K8Yw?cluster=devnet) |
| `mint_nft` A | [aFJnzD1J…Ko62](https://explorer.solana.com/tx/aFJnzD1J8JcFpMHaY4fMhYLgPLkqpDNdGQFjSqhFxbn6wzAm6zKv4qszZxXTJGpxZxuG7HJUemirQ9dtBDKKo62?cluster=devnet) |
| `mint_nft` B | [3ZoY79P2…MBVG](https://explorer.solana.com/tx/3ZoY79P2MHea7iNtCXyHSY6YDzfeDBqk3swmCZYpRYLgzwNC3zU2wuCyL2oNEGhYVUcGCHRNmc8aVsrz1dHMBMVG?cluster=devnet) |
| `stake` A | [4fHpXevy…z6NS](https://explorer.solana.com/tx/4fHpXevyXRJAc1dcm4L898cuGG5gy63uba1FCgBi7EhRVJMQFhW53s4UMC1LaXYAhB59byou9QpX5UStNfuaz6NS?cluster=devnet) |
| `stake` B | [3K5EKYCW…h8iZ](https://explorer.solana.com/tx/3K5EKYCW53FB6f52PMQwMQXJCtApS6adAiksZ8FNzqsgePBaWGGbD7Y2SU3YX9eqRCTrh4RHUx9edwJyE1Pbh8iZ?cluster=devnet) |
| **`claim_rewards`** A | [4EnwNi79…GKcvS](https://explorer.solana.com/tx/4EnwNi79wZ2sZLMkA8a2zemkeHLL4Tv7XPMBv3AozRgbex7ckpcKUFV6ybpfCHYwusAqmsmh5CedJ17BD21GKcvS?cluster=devnet) |
| **`burn_staked_nft`** B | [x24nrex6…3vL](https://explorer.solana.com/tx/x24nrex6yjZyvU1SQooMWrhB1xM57kAnxegMcfuvD1qBTzroJ8nit8on1VgRw4rocex2pzahyuRSVyMmmo7W3vL?cluster=devnet) |
| `unstake` A | [4CCn8K1b…J99e](https://explorer.solana.com/tx/4CCn8K1bbvxPWVJcukovgaEy9gqUj7rCpKZp53fPz3BQUrYAWQqoUNvZPf3knmaLb7K2h4TTfkHCKyq19Qb6J99e?cluster=devnet) |
| re-`stake` A | [T5qr93ng…MHkc](https://explorer.solana.com/tx/T5qr93ngfP9djETTbwmbEJ9Esk6EgdrrS8TT2EQYSumw4JHW8y5GtgpibiouCYZTW79J75LZBFEkuP8ABTXMkHc?cluster=devnet) |

After the run, the collection's Attributes read `total_staked = 1`. The sequence was
0 → 1 → 2 (two stakes), 1 (burn), 0 (unstake), 1 (re-stake).

---

## Project layout

```
programs/nft_staking_core/src/
├── lib.rs                     # instruction entrypoints
├── constants.rs               # PDA seeds, attribute keys
├── error.rs
├── state.rs                   # Config
├── core_utils.rs              # Core plugin reads + CPI helpers, reward math
└── instructions/
    ├── create_collection.rs
    ├── mint_nft.rs
    ├── initialize_config.rs
    ├── stake.rs
    ├── claim_rewards.rs       # Task 1.1
    ├── burn_staked_nft.rs     # Task 1.2
    └── unstake.rs
tests/nft_staking_core.ts
```

## Notes

- `mpl-core` is used without its `anchor` feature, because that feature targets Anchor
  0.31/0.32. Core accounts are therefore `UncheckedAccount`s with
  `owner = mpl_core::ID`, and the handlers deserialize and validate them.
- `anchor build` prints "Stack offset exceeded" warnings for a few `mpl-core` helpers
  (`registry_records_to_plugin_list`, `Asset::deserialize`). This program doesn't call
  them; it reads plugins with `fetch_plugin`.
