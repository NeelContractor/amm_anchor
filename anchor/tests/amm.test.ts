import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import { 
  TOKEN_PROGRAM_ID, 
  ASSOCIATED_TOKEN_PROGRAM_ID,
  createMint,
  createAssociatedTokenAccount,
  mintTo,
  getAssociatedTokenAddress
} from "@solana/spl-token";
import { Amm } from "../target/types/amm";

describe("AMM Program Tests", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Amm as Program<Amm>;

  let ammId: PublicKey;
  let admin: Keypair;
  let user: Keypair;
  let payer: Keypair;
  let mintA: PublicKey;
  let mintB: PublicKey;
  let userAccountA: PublicKey;
  let userAccountB: PublicKey;
  let ammPda: PublicKey;
  let poolPda: PublicKey;
  let poolAuthority: PublicKey;
  let mintLiquidity: PublicKey;
  let poolAccountA: PublicKey;
  let poolAccountB: PublicKey;

  beforeAll(async () => {
    // Generate test keypairs
    admin = Keypair.generate();
    user = Keypair.generate();
    payer = Keypair.generate();
    ammId = Keypair.generate().publicKey;

    // Calculate PDAs
    [ammPda] = PublicKey.findProgramAddressSync(
      [ammId.toBuffer()],
      program.programId
    );

    // Airdrop SOL to test accounts
    await provider.connection.requestAirdrop(admin.publicKey, 2 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.requestAirdrop(user.publicKey, 2 * anchor.web3.LAMPORTS_PER_SOL);
    await provider.connection.requestAirdrop(payer.publicKey, 2 * anchor.web3.LAMPORTS_PER_SOL);

    // Wait for airdrops to confirm
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Create test tokens
    mintA = await createMint(
      provider.connection,
      payer,
      payer.publicKey,
      null,
      6 // decimals
    );

    mintB = await createMint(
      provider.connection,
      payer,
      payer.publicKey,
      null,
      6 // decimals
    );

    // Calculate remaining PDAs after mints are created
    [poolPda] = PublicKey.findProgramAddressSync(
      [ammPda.toBuffer(), mintA.toBuffer(), mintB.toBuffer()],
      program.programId
    );

    [poolAuthority] = PublicKey.findProgramAddressSync(
      [ammPda.toBuffer(), mintA.toBuffer(), mintB.toBuffer(), Buffer.from("authority")],
      program.programId
    );

    [mintLiquidity] = PublicKey.findProgramAddressSync(
      [ammPda.toBuffer(), mintA.toBuffer(), mintB.toBuffer(), Buffer.from("liquidity")],
      program.programId
    );

    poolAccountA = await getAssociatedTokenAddress(mintA, poolAuthority, true);
    poolAccountB = await getAssociatedTokenAddress(mintB, poolAuthority, true);

    // Create user token accounts
    userAccountA = await createAssociatedTokenAccount(
      provider.connection,
      payer,
      mintA,
      user.publicKey
    );

    userAccountB = await createAssociatedTokenAccount(
      provider.connection,
      payer,
      mintB,
      user.publicKey
    );

    // Mint tokens to user
    await mintTo(
      provider.connection,
      payer,
      mintA,
      userAccountA,
      payer.publicKey,
      1000000000 // 1000 tokens with 6 decimals
    );

    await mintTo(
      provider.connection,
      payer,
      mintB,
      userAccountB,
      payer.publicKey,
      1000000000 // 1000 tokens with 6 decimals
    );
  });

  it("Creates an AMM", async () => {
    const fee = 300; // 3% fee (300 basis points)
    
    await program.methods
      .createAmm(ammId, fee)
      .accountsPartial({
        payer: payer.publicKey,
        admin: admin.publicKey,
        amm: ammPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([payer])
      .rpc();

    const ammAccount = await program.account.amm.fetch(ammPda);
    expect(ammAccount.id.toString()).toEqual(ammId.toString());
    expect(ammAccount.admin.toString()).toEqual(admin.publicKey.toString());
    expect(ammAccount.fee).toEqual(fee);
  });

  it("Creates a pool", async () => {
    await program.methods
      .createPool()
      .accountsPartial({
        amm: ammPda,
        pool: poolPda,
        poolAuthority: poolAuthority,
        mintLiquidity: mintLiquidity,
        mintA: mintA,
        mintB: mintB,
        poolAccountA: poolAccountA,
        poolAccountB: poolAccountB,
        payer: payer.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([payer])
      .rpc();

    const poolAccount = await program.account.pool.fetch(poolPda);
    expect(poolAccount.amm.toString()).toEqual(ammPda.toString());
    expect(poolAccount.mintA.toString()).toEqual(mintA.toString());
    expect(poolAccount.mintB.toString()).toEqual(mintB.toString());
  });

  it("Deposits liquidity", async () => {
    const depositorAccountLiquidity = await getAssociatedTokenAddress(mintLiquidity, user.publicKey, true);

    const amountA = 100000000; // 100 tokens with 6 decimals
    const amountB = 100000000; // 100 tokens with 6 decimals

    await program.methods
      .depositLiquidity(new anchor.BN(amountA), new anchor.BN(amountB))
      .accountsPartial({
        payer: payer.publicKey,
        pool: poolPda,
        poolAuthority: poolAuthority,
        depositor: user.publicKey,
        mintLiquidity: mintLiquidity,
        mintA: mintA,
        mintB: mintB,
        poolAccountA: poolAccountA,
        poolAccountB: poolAccountB,
        depositorAccountLiquidity: depositorAccountLiquidity,
        depositorAccountA: userAccountA,
        depositorAccountB: userAccountB,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([user, payer])
      .rpc();

    // Verify liquidity was deposited
    const poolABalance = await provider.connection.getTokenAccountBalance(poolAccountA);
    const poolBBalance = await provider.connection.getTokenAccountBalance(poolAccountB);
    
    expect(Number(poolABalance.value.amount)).toBeGreaterThan(0);
    expect(Number(poolBBalance.value.amount)).toBeGreaterThan(0);
  });

  it("Performs a swap", async () => {
    const swapAmount = 1000000; // 1 token with 6 decimals
    const minOutput = 1; // Minimum output amount

    // Get balances before swap
    const userBalanceABefore = await provider.connection.getTokenAccountBalance(userAccountA);
    const userBalanceBBefore = await provider.connection.getTokenAccountBalance(userAccountB);

    await program.methods
      .swapExactTokensForTokens(
        true, // swap_a (swap token A for token B)
        new anchor.BN(swapAmount),
        new anchor.BN(minOutput)
      )
      .accountsPartial({
        amm: ammPda,
        pool: poolPda,
        poolAuthority: poolAuthority,
        trader: user.publicKey,
        mintA: mintA,
        mintB: mintB,
        poolAccountA: poolAccountA,
        poolAccountB: poolAccountB,
        traderAccountA: userAccountA,
        traderAccountB: userAccountB,
        payer: payer.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([user, payer])
      .rpc();

    // Get balances after swap
    const userBalanceAAfter = await provider.connection.getTokenAccountBalance(userAccountA);
    const userBalanceBAfter = await provider.connection.getTokenAccountBalance(userAccountB);

    // Verify swap occurred
    expect(Number(userBalanceAAfter.value.amount)).toBeLessThan(Number(userBalanceABefore.value.amount));
    expect(Number(userBalanceBAfter.value.amount)).toBeGreaterThan(Number(userBalanceBBefore.value.amount));
  });

  // Helper function to wait for transaction confirmation
  async function confirmTransaction(connection: any, signature: string) {
    const latestBlockhash = await connection.getLatestBlockhash();
    await connection.confirmTransaction({
      signature,
      ...latestBlockhash,
    });
  }
});