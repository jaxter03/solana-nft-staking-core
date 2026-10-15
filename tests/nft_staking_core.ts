import * as anchor from "@anchor-lang/core";
import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  getAccount,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";
import {
  keypairIdentity,
  publicKey as umiPk,
} from "@metaplex-foundation/umi";
import {
  AssetV1,
  CollectionV1,
  MPL_CORE_PROGRAM_ID,
  fetchAsset,
  fetchCollection,
  mplCore,
  transfer,
} from "@metaplex-foundation/mpl-core";
import { assert } from "chai";
import { NftStakingCore } from "../target/types/nft_staking_core";

// umi reads at "confirmed", so the Anchor provider must confirm at least that far.
const envProvider = anchor.AnchorProvider.env();
const provider = new anchor.AnchorProvider(
  new Connection(envProvider.connection.rpcEndpoint, "confirmed"),
  envProvider.wallet,
  { commitment: "confirmed", preflightCommitment: "confirmed" }
);
anchor.setProvider(provider);

const program = anchor.workspace.nftStakingCore as anchor.Program<NftStakingCore>;
const connection = provider.connection;
const admin = (provider.wallet as anchor.Wallet).payer;
const isDevnet = connection.rpcEndpoint.includes("devnet");

const CORE_PROGRAM_ID = new PublicKey(MPL_CORE_PROGRAM_ID);

/** 1 reward token (6 decimals) per second per staked NFT. */
const REWARDS_PER_SECOND = 1_000_000n;
const REWARDS_PER_DAY = new anchor.BN((REWARDS_PER_SECOND * 86_400n).toString());
/** Minimum stake time before `unstake` is allowed. */
const FREEZE_PERIOD = 10;
/** One-time bonus for burning a staked NFT: 1,000 tokens. */
const BURN_BONUS = 1_000_000_000n;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const signatures: { step: string; sig: string }[] = [];
function record(step: string, sig: string) {
  signatures.push({ step, sig });
  const suffix = isDevnet ? "?cluster=devnet" : "";
  console.log(`      ${step}: https://explorer.solana.com/tx/${sig}${suffix}`);
}

async function expectError(promise: Promise<unknown>, code: string) {
  try {
    await promise;
  } catch (err: any) {
    if (err?.error?.errorCode?.code === code) return;
    const haystack = [
      err?.message,
      err?.toString?.(),
      ...(err?.logs ?? []),
      ...(err?.transactionLogs ?? []),
    ]
      .filter(Boolean)
      .join("\n");
    if (haystack.includes(code)) return;
    throw new Error(`expected error "${code}" but got:\n${haystack}`);
  }
  throw new Error(`expected error "${code}" but the transaction succeeded`);
}

function attr(list: { key: string; value: string }[] | undefined, key: string) {
  return list?.find((a) => a.key === key)?.value;
}

describe("nft_staking_core", () => {
  const umi = createUmi(connection.rpcEndpoint, "confirmed").use(mplCore());

  const user = Keypair.generate();
  const stranger = Keypair.generate();
  const collection = Keypair.generate();
  const nftA = Keypair.generate(); // staked → claimed → unstaked
  const nftB = Keypair.generate(); // staked → burned

  const [updateAuthority] = PublicKey.findProgramAddressSync(
    [Buffer.from("update_authority"), collection.publicKey.toBuffer()],
    program.programId
  );
  const [config] = PublicKey.findProgramAddressSync(
    [Buffer.from("config"), collection.publicKey.toBuffer()],
    program.programId
  );
  const [rewardsMint] = PublicKey.findProgramAddressSync(
    [Buffer.from("rewards"), config.toBuffer()],
    program.programId
  );
  const userAta = getAssociatedTokenAddressSync(rewardsMint, user.publicKey);

  const fetchA = () => fetchAsset(umi, umiPk(nftA.publicKey));
  const fetchCol = () => fetchCollection(umi, umiPk(collection.publicKey));
  const totalStaked = async () =>
    Number(attr((await fetchCol()).attributes?.attributeList, "total_staked"));
  const rewardBalance = async () => {
    try {
      return (await getAccount(connection, userAta)).amount;
    } catch {
      return 0n;
    }
  };

  const stakeAccounts = (asset: PublicKey, owner = user.publicKey) => ({
    user: owner,
    config,
    updateAuthority,
    asset,
    collection: collection.publicKey,
    coreProgram: CORE_PROGRAM_ID,
    systemProgram: SystemProgram.programId,
  });

  const rewardAccounts = (asset: PublicKey, owner = user.publicKey) => ({
    ...stakeAccounts(asset, owner),
    rewardsMint,
    userRewardsAta: getAssociatedTokenAddressSync(rewardsMint, owner),
    tokenProgram: TOKEN_PROGRAM_ID,
    associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
  });

  before(async () => {
    // Fund test wallets from the provider wallet (devnet airdrops are rate-limited).
    const tx = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: user.publicKey,
        lamports: 0.1 * LAMPORTS_PER_SOL,
      }),
      SystemProgram.transfer({
        fromPubkey: admin.publicKey,
        toPubkey: stranger.publicKey,
        lamports: 0.02 * LAMPORTS_PER_SOL,
      })
    );
    await provider.sendAndConfirm(tx);
    console.log(`      program:    ${program.programId.toBase58()}`);
    console.log(`      collection: ${collection.publicKey.toBase58()}`);
    console.log(`      nft A:      ${nftA.publicKey.toBase58()}`);
    console.log(`      nft B:      ${nftB.publicKey.toBase58()}`);
  });

  after(() => {
    console.log("\n      signatures:");
    for (const { step, sig } of signatures) console.log(`        ${step}: ${sig}`);
  });

  describe("setup", () => {
    it("creates a collection with total_staked = 0", async () => {
      const sig = await program.methods
        .createCollection("Turbin3 Stakers", "https://example.com/collection.json")
        .accountsPartial({
          payer: admin.publicKey,
          collection: collection.publicKey,
          updateAuthority,
          coreProgram: CORE_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([collection])
        .rpc();
      record("create_collection", sig);

      const col: CollectionV1 = await fetchCol();
      assert.equal(col.updateAuthority.toString(), updateAuthority.toBase58());
      assert.equal(await totalStaked(), 0);
    });

    it("initializes the staking config and reward mint", async () => {
      const sig = await program.methods
        .initializeConfig(REWARDS_PER_DAY, new anchor.BN(FREEZE_PERIOD), new anchor.BN(BURN_BONUS.toString()))
        .accountsPartial({
          admin: admin.publicKey,
          collection: collection.publicKey,
          updateAuthority,
          config,
          rewardsMint,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .rpc();
      record("initialize_config", sig);

      const cfg = await program.account.config.fetch(config);
      assert.equal(cfg.collection.toBase58(), collection.publicKey.toBase58());
      assert.equal(cfg.freezePeriod.toNumber(), FREEZE_PERIOD);
      assert.equal(cfg.burnBonus.toString(), BURN_BONUS.toString());
    });

    it("mints two NFTs into the collection", async () => {
      for (const [name, nft] of [
        ["A", nftA],
        ["B", nftB],
      ] as const) {
        const sig = await program.methods
          .mintNft(`Staker #${name}`, `https://example.com/${name}.json`)
          .accountsPartial({
            user: user.publicKey,
            asset: nft.publicKey,
            collection: collection.publicKey,
            updateAuthority,
            coreProgram: CORE_PROGRAM_ID,
            systemProgram: SystemProgram.programId,
          })
          .signers([user, nft])
          .rpc();
        record(`mint_nft ${name}`, sig);
        const asset = await fetchAsset(umi, umiPk(nft.publicKey));
        assert.equal(asset.owner.toString(), user.publicKey.toBase58());
      }
    });
  });

  describe("stake + collection stats", () => {
    it("stakes NFT A: freezes it, delegates burn, increments total_staked", async () => {
      const sig = await program.methods
        .stake()
        .accountsPartial(stakeAccounts(nftA.publicKey))
        .signers([user])
        .rpc();
      record("stake A", sig);

      const asset: AssetV1 = await fetchA();
      assert.isTrue(asset.freezeDelegate?.frozen);
      assert.equal(asset.freezeDelegate?.authority.address?.toString(), updateAuthority.toBase58());
      assert.equal(asset.burnDelegate?.authority.address?.toString(), updateAuthority.toBase58());
      assert.equal(attr(asset.attributes?.attributeList, "staked"), "true");
      assert.equal(await totalStaked(), 1);
    });

    it("stakes NFT B: total_staked = 2", async () => {
      const sig = await program.methods
        .stake()
        .accountsPartial(stakeAccounts(nftB.publicKey))
        .signers([user])
        .rpc();
      record("stake B", sig);
      assert.equal(await totalStaked(), 2);
    });

    it("rejects staking an already staked NFT", async () => {
      await expectError(
        program.methods.stake().accountsPartial(stakeAccounts(nftA.publicKey)).signers([user]).rpc(),
        "AlreadyStaked"
      );
    });

    it("a staked NFT cannot be transferred (FreezeDelegate)", async () => {
      const userUmi = createUmi(connection.rpcEndpoint, "confirmed")
        .use(mplCore())
        .use(keypairIdentity(umi.eddsa.createKeypairFromSecretKey(user.secretKey)));
      let failed = false;
      try {
        await transfer(userUmi, {
          asset: await fetchA(),
          collection: await fetchCol(),
          newOwner: umiPk(stranger.publicKey),
        }).sendAndConfirm(userUmi);
      } catch {
        failed = true;
      }
      assert.isTrue(failed, "transfer of a frozen asset should fail");
      assert.equal((await fetchA()).owner.toString(), user.publicKey.toBase58());
    });

    it("rejects unstaking before the freeze period", async () => {
      await expectError(
        program.methods.unstake().accountsPartial(rewardAccounts(nftA.publicKey)).signers([user]).rpc(),
        "FreezePeriodNotElapsed"
      );
    });
  });

  describe("claim_rewards", () => {
    it("rejects a claim by someone who does not own the NFT", async () => {
      await expectError(
        program.methods
          .claimRewards()
          .accountsPartial(rewardAccounts(nftA.publicKey, stranger.publicKey))
          .signers([stranger])
          .rpc(),
        "NotOwner"
      );
    });

    it("mints accrued rewards and keeps the NFT staked and frozen", async () => {
      await sleep(3_000);
      const before = BigInt(attr((await fetchA()).attributes?.attributeList, "last_claimed")!);

      const sig = await program.methods
        .claimRewards()
        .accountsPartial(rewardAccounts(nftA.publicKey))
        .signers([user])
        .rpc();
      record("claim_rewards A", sig);

      const asset = await fetchA();
      const after = BigInt(attr(asset.attributes?.attributeList, "last_claimed")!);
      const balance = await rewardBalance();

      assert.isTrue(after > before, "last_claimed advances");
      assert.equal(balance, (after - before) * REWARDS_PER_SECOND);
      assert.isTrue(asset.freezeDelegate?.frozen, "still frozen");
      assert.equal(attr(asset.attributes?.attributeList, "staked"), "true");
      assert.equal(asset.owner.toString(), user.publicKey.toBase58());
      assert.equal(await totalStaked(), 2, "claiming does not change total_staked");
    });
  });

  describe("burn_staked_nft", () => {
    it("burns staked NFT B for accrued rewards + burn bonus, decrements total_staked", async () => {
      const lastClaimed = BigInt(
        attr((await fetchAsset(umi, umiPk(nftB.publicKey))).attributes?.attributeList, "last_claimed")!
      );
      const before = await rewardBalance();

      const sig = await program.methods
        .burnStakedNft()
        .accountsPartial(rewardAccounts(nftB.publicKey))
        .signers([user])
        .rpc();
      record("burn_staked_nft B", sig);

      const paid = (await rewardBalance()) - before;
      const accrued = paid - BURN_BONUS;
      assert.isTrue(accrued > 0n, "bonus plus some accrued rewards");
      assert.equal(accrued % REWARDS_PER_SECOND, 0n, "accrued is a whole number of seconds");
      const blockTime = await connection.getBlockTime(
        (await connection.getTransaction(sig, { commitment: "confirmed", maxSupportedTransactionVersion: 0 }))!.slot
      );
      assert.approximately(Number(accrued / REWARDS_PER_SECOND), blockTime! - Number(lastClaimed), 5);

      const info = await connection.getAccountInfo(nftB.publicKey);
      assert.isTrue(info === null || info.data.length <= 1, "asset account is burned");
      assert.equal(await totalStaked(), 1);
    });
  });

  describe("unstake", () => {
    it("unstakes NFT A after the freeze period: pays rewards, thaws, decrements total_staked", async () => {
      const stakedAt = Number(attr((await fetchA()).attributes?.attributeList, "staked_at"));
      const wait = stakedAt + FREEZE_PERIOD + 2 - Math.floor(Date.now() / 1000);
      if (wait > 0) await sleep(wait * 1000);

      const before = await rewardBalance();
      const sig = await program.methods
        .unstake()
        .accountsPartial(rewardAccounts(nftA.publicKey))
        .signers([user])
        .rpc();
      record("unstake A", sig);

      const paid = (await rewardBalance()) - before;
      assert.isTrue(paid > 0n);
      assert.equal(paid % REWARDS_PER_SECOND, 0n);

      const asset = await fetchA();
      assert.isUndefined(asset.freezeDelegate, "FreezeDelegate removed");
      assert.isUndefined(asset.burnDelegate, "BurnDelegate removed");
      assert.equal(attr(asset.attributes?.attributeList, "staked"), "false");
      assert.equal(await totalStaked(), 0);
    });

    it("rejects claiming on an unstaked NFT", async () => {
      await expectError(
        program.methods.claimRewards().accountsPartial(rewardAccounts(nftA.publicKey)).signers([user]).rpc(),
        "NotStaked"
      );
    });

    it("an unstaked NFT can be re-staked", async () => {
      const sig = await program.methods
        .stake()
        .accountsPartial(stakeAccounts(nftA.publicKey))
        .signers([user])
        .rpc();
      record("re-stake A", sig);
      assert.isTrue((await fetchA()).freezeDelegate?.frozen);
      assert.equal(await totalStaked(), 1);
    });
  });
});
