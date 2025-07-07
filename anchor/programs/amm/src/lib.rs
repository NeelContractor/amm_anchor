#![allow(clippy::result_large_err)]
#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use anchor_spl::{associated_token::AssociatedToken, token::{self, Mint, Token, TokenAccount, Burn, MintTo, Transfer}};
use fixed::types::I64F64;

declare_id!("BM1qcSpGfkGwiv32VcydapQmNMVRv1h13UrmsNtZhubV");

#[constant]
pub const MINIMUM_LIQUIDITY: u64 = 100;

#[constant]
pub const AUTHORITY_SEED: &[u8] = b"authority";

#[constant]
pub const LIQUIDITY_SEED: &[u8] = b"liquidity";

#[program]
pub mod amm {
    use super::*;

    pub fn create_amm(ctx: Context<CreateAmm>, id: Pubkey, fee: u16) -> Result<()> {
        let amm = &mut ctx.accounts.amm;
        amm.id = id;
        amm.admin = ctx.accounts.admin.key();
        amm.fee = fee;
        Ok(())
    }

    pub fn create_pool(ctx: Context<CreatePool>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.amm = ctx.accounts.amm.key();
        pool.mint_a = ctx.accounts.mint_a.key();
        pool.mint_b = ctx.accounts.mint_b.key();
        Ok(())
    }

    pub fn deposit_liquidity(ctx: Context<DepositLiquidity>, amount_a: u64, amount_b: u64) -> Result<()> {
        let mut amount_a = if amount_a > ctx.accounts.depositor_account_a.amount {
            ctx.accounts.depositor_account_a.amount
        } else {
            amount_a
        };
        let mut amount_b = if amount_b > ctx.accounts.depositor_account_b.amount {
            ctx.accounts.depositor_account_b.amount
        } else {
            amount_b
        };

        let pool_a = &ctx.accounts.pool_account_a;
        let pool_b = &ctx.accounts.pool_account_b;

        let pool_creation = pool_a.amount == 0 && pool_b.amount == 0;
        (amount_a, amount_b) =  if pool_creation {
            // Add as is if there is no liquidity
            (amount_a, amount_b)
        } else {
            let ratio = I64F64::from_num(pool_a.amount)
                .checked_mul(I64F64::from_num(pool_b.amount))
                .unwrap();
            if pool_a.amount > pool_b.amount {
                (
                    I64F64::from_num(amount_b)
                        .checked_mul(ratio)
                        .unwrap()
                        .to_num::<u64>(),
                    amount_b,
                )
            } else {
                (
                    amount_a,
                    I64F64::from_num(amount_a)
                        .checked_div(ratio)
                        .unwrap()
                        .to_num::<u64>(),
                )
            }
        };

        //computing the amount of liquidity about to be deposited
        let mut liquidity = I64F64::from_num(amount_a)
            .checked_mul(I64F64::from_num(amount_b))
            .unwrap()
            .sqrt()
            .to_num::<u64>();

        // Lock some minimum liquidity on the first deposit
        if pool_creation {
            if liquidity < MINIMUM_LIQUIDITY {
                return err!(TutorialError::DepositTooSmall);
            }

            liquidity -= MINIMUM_LIQUIDITY;
        }

        // transfer tokens to the pool
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.depositor_account_a.to_account_info(),
                    to: ctx.accounts.pool_account_a.to_account_info(),
                    authority: ctx.accounts.depositor.to_account_info()
                },
            ),
            amount_a
        )?;
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.depositor_account_b.to_account_info(),
                    to: ctx.accounts.pool_account_b.to_account_info(),
                    authority: ctx.accounts.depositor.to_account_info()
                },
            ),
            amount_b
        )?;

        //Mint the liquidity to user
        let authority_bump = ctx.bumps.pool_authority;
        let authority_seeds = &[
            &ctx.accounts.pool.amm.to_bytes(),
            &ctx.accounts.mint_a.key().to_bytes(),
            &ctx.accounts.mint_b.key().to_bytes(),
            AUTHORITY_SEED,
            &[authority_bump],
        ];

        let signer_seeds = &[&authority_seeds[..]];
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.mint_liquidity.to_account_info(),
                    to: ctx.accounts.depositor_account_liquidity.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info()
                },
                signer_seeds
            ),
            liquidity
        )?;
        Ok(())
    }

    pub fn withdraw_liquidity(ctx: Context<WithdrawLiquidity>, amount: u64) -> Result<()> {
        let authority_bump = ctx.bumps.pool_authority;
        let authority_seeds = &[
            &ctx.accounts.pool.amm.to_bytes(),
            &ctx.accounts.mint_a.key().to_bytes(),
            &ctx.accounts.mint_b.key().to_bytes(),
            AUTHORITY_SEED,
            &[authority_bump],
        ];
        let signer_seeds = &[&authority_seeds[..]];

        //transfer tokens from the pool 
        let amount_a = I64F64::from_num(amount)
            .checked_mul(I64F64::from_num(ctx.accounts.pool_account_a.amount))
            .unwrap()
            .checked_div(I64F64::from_num(ctx.accounts.mint_liquidity.supply + MINIMUM_LIQUIDITY))
            .unwrap()
            .floor().to_num::<u64>();
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.pool_account_a.to_account_info(),
                    to: ctx.accounts.depositer_account_a.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info()
                },
                signer_seeds
            ),
            amount_a
        )?;

        let amount_b = I64F64::from_num(amount)
            .checked_div(I64F64::from_num(ctx.accounts.pool_account_b.amount))
            .unwrap()
            .checked_div(I64F64::from_num(ctx.accounts.mint_liquidity.supply + MINIMUM_LIQUIDITY))
            .unwrap().floor().to_num::<u64>();
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.pool_account_b.to_account_info(),
                    to: ctx.accounts.depositer_account_b.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info()
                },
                signer_seeds
            ),
            amount_b
        )?;

        //burn the liquidity tokens 
        //it will fail if the amount is invalid
        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.mint_liquidity.to_account_info(),
                    from: ctx.accounts.depositor_account_liquidity.to_account_info(),
                    authority: ctx.accounts.depositor.to_account_info()
                },
            ),
            amount,
        )?;
        Ok(())
    }

    pub fn swap_exact_tokens_for_tokens(ctx: Context<SwapExactTokensForTokens>, swap_a: bool, input_amount: u64, min_output_amount: u64) -> Result<()> {
        let input = if swap_a && input_amount > ctx.accounts.trader_account_a.amount {
            ctx.accounts.trader_account_a.amount
        } else if !swap_a && input_amount > ctx.accounts.trader_account_b.amount {
            ctx.accounts.trader_account_b.amount
        } else {
            input_amount
        };

        // Apply trading fee, used to compute the output
        let amm= &ctx.accounts.amm;
        let taxed_input = input - input * amm.fee as u64 / 10000;

        let pool_a = &ctx.accounts.pool_account_a;
        let pool_b = &ctx.accounts.pool_account_b;
        let output = if swap_a {
            I64F64::from_num(taxed_input)
                .checked_mul(I64F64::from_num(pool_b.amount))
                .unwrap()
                .checked_div(
                    I64F64::from_num(pool_a.amount)
                        .checked_add(I64F64::from_num(taxed_input))
                        .unwrap(),
                )
                .unwrap()
        } else {
            I64F64::from_num(taxed_input)
                .checked_mul(I64F64::from_num(pool_a.amount))
                .unwrap()
                .checked_div(
                    I64F64::from_num(pool_b.amount)
                        .checked_add(I64F64::from_num(taxed_input))
                        .unwrap(),
                )
                .unwrap()
        }
        .to_num::<u64>();

    if output < min_output_amount {
        return err!(TutorialError::OutputTooSmall);
    }

    // COmpute the invariant before the trade
    let invariant = pool_a.amount * pool_b.amount;

    // transfer tokens to the pool
    let authority_bump = ctx.bumps.pool_authority;
    let authority_seeds = &[
        &ctx.accounts.pool.amm.to_bytes(),
        &ctx.accounts.mint_a.key().to_bytes(),
        &ctx.accounts.mint_b.key().to_bytes(),
        AUTHORITY_SEED,
        &[authority_bump],
    ];
    let signer_seeds = &[&authority_seeds[..]];
    if swap_a {
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.trader_account_a.to_account_info(),
                    to: ctx.accounts.pool_account_a.to_account_info(),
                    authority: ctx.accounts.trader.to_account_info()
                },
            ),
            input,
        )?;
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.pool_account_b.to_account_info(),
                    to: ctx.accounts.trader_account_b.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info()
                },
                signer_seeds
            ),
            output,
        )?;
    } else {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.pool_account_a.to_account_info(),
                    to: ctx.accounts.trader_account_a.to_account_info(),
                    authority: ctx.accounts.pool_authority.to_account_info()
                },
                signer_seeds
            ),
            input,
        )?;
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.trader_account_b.to_account_info(),
                    to: ctx.accounts.pool_account_b.to_account_info(),
                    authority: ctx.accounts.trader.to_account_info()
                },
            ),
            output,
        )?;
    }

    msg!("Traded {} tokens ({} after fees) for {}", input, taxed_input, output);

    // verify the invariant still holds
    // reload accounts because of the CPIs
    // we tolerate if the new invariant is higher because it means a roundeing error for LPs
    ctx.accounts.pool_account_a.reload()?;
    ctx.accounts.pool_account_b.reload()?;
    if invariant > ctx.accounts.pool_account_a.amount * ctx.accounts.pool_account_a.amount {
        return err!(TutorialError::InvariantViolated);
    }

        Ok(())
    }

}

#[derive(Accounts)]
#[instruction(id: Pubkey, fee: u16)]
pub struct CreateAmm<'info> {
    /// the account paying for all rents
    #[account(mut)]
    pub payer: Signer<'info>,

    /// the admin fo the AMM
    /// CHECK: Read only, delegatable creation
    pub admin: AccountInfo<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + Amm::INIT_SPACE,
        seeds = [id.as_ref()],
        bump,
        constraint = fee < 10000 @ TutorialError::InvalidFee,
    )]
    pub amm: Account<'info, Amm>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreatePool<'info> {
    #[account(
        seeds = [amm.id.as_ref()],
        bump
    )]
    pub amm: Box<Account<'info, Amm>>,

    #[account(
        init,
        payer = payer,
        space = Pool::INIT_SPACE,
        seeds = [amm.key().as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref()],
        bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// CHECK: Read only authority
    #[account(
        seeds = [amm.key().as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref(), AUTHORITY_SEED],
        bump
    )]
    pub pool_authority: AccountInfo<'info>,

    #[account(
        init,
        payer = payer,
        seeds = [amm.key().as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref(), LIQUIDITY_SEED],
        bump,
        mint::decimals = 6,
        mint::authority = pool_authority,
    )]
    pub mint_liquidity: Box<Account<'info, Mint>>,

    pub mint_a: Box<Account<'info, Mint>>,
    pub mint_b: Box<Account<'info, Mint>>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
    )]
    pub pool_account_a: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_b,
        associated_token::authority = pool_authority,
    )]
    pub pool_account_b: Box<Account<'info, TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DepositLiquidity<'info> {
    /// THe account paying for all rents
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        seeds = [pool.amm.as_ref(), pool.mint_a.key().as_ref(), pool.mint_b.key().as_ref()],
        bump,
        has_one = mint_a,
        has_one = mint_b,
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// CHECK: Read only authority
    #[account(
        seeds = [pool.amm.as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref(), AUTHORITY_SEED],
        bump
    )]
    pub pool_authority: AccountInfo<'info>,

    /// The account paying for all rent
    pub depositor: Signer<'info>,
    #[account(
        mut,
        seeds = [pool.amm.as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref(), LIQUIDITY_SEED],
        bump
    )]
    pub mint_liquidity: Box<Account<'info, Mint>>,

    pub mint_a: Box<Account<'info, Mint>>,

    pub mint_b: Box<Account<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
    )]
    pub pool_account_a: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = pool_authority,
    )]
    pub pool_account_b: Box<Account<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_liquidity,
        associated_token::authority = depositor,
    )]
    pub depositor_account_liquidity: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = depositor,
    )]
    pub depositor_account_a: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = depositor,
    )]
    pub depositor_account_b: Box<Account<'info, TokenAccount>>,

    /// Solana ecosystem accounts
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WithdrawLiquidity<'info> {
    #[account(
        seeds = [amm.id.as_ref()],
        bump
    )]
    pub amm: Account<'info, Amm>,
    #[account(
        seeds = [pool.amm.as_ref(), pool.mint_a.as_ref(), pool.mint_b.as_ref()],
        bump,
        has_one = mint_a,
        has_one = mint_b
    )]
    pub pool: Account<'info, Pool>,
    /// CHECK: Read only authority
    #[account(
        seeds = [pool.amm.as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref(), AUTHORITY_SEED],
        bump,
    )]
    pub pool_authority: AccountInfo<'info>,

    ///The account paying for all rents
    pub depositor: Signer<'info>,

    #[account(
        mut,
        seeds = [pool.amm.as_ref(), mint_a.key().as_ref(), mint_b.key().as_ref(), LIQUIDITY_SEED],
        bump,
    )]
    pub mint_liquidity: Box<Account<'info, Mint>>,

    #[account(mut)]
    pub mint_a: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub mint_b: Box<Account<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
    )]
    pub pool_account_a: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = pool_authority,
    )]
    pub pool_account_b: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_liquidity,
        associated_token::authority = depositor,
    )]
    pub depositor_account_liquidity: Box<Account<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = depositor
    )]
    pub depositer_account_a: Box<Account<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_b,
        associated_token::authority = depositor
    )]
    pub depositer_account_b: Box<Account<'info, TokenAccount>>,

    /// The account paying for all rents
    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SwapExactTokensForTokens<'info> {
    #[account(
        seeds = [amm.id.as_ref()],
        bump
    )]
    pub amm: Account<'info, Amm>,

    #[account(
        seeds = [pool.amm.as_ref(), pool.mint_a.key().as_ref(), pool.mint_b.key().as_ref()],
        bump,
        has_one = amm,
        has_one = mint_a,
        has_one = mint_b
    )]
    pub pool: Account<'info, Pool>,

    /// CHECK: Read only authority
    #[account(
        seeds = [
            pool.amm.as_ref(),
            mint_a.key().as_ref(),
            mint_b.key().as_ref(),
            AUTHORITY_SEED,
        ],
        bump
    )]
    pub pool_authority: AccountInfo<'info>,

    /// The account doing the swap
    pub trader: Signer<'info>,
    pub mint_a: Box<Account<'info, Mint>>,
    pub mint_b: Box<Account<'info, Mint>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority
    )]
    pub pool_account_a: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = pool_authority
    )]
    pub pool_account_b: Box<Account<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = trader
    )]
    pub trader_account_a: Box<Account<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_b,
        associated_token::authority = trader
    )]
    pub trader_account_b: Box<Account<'info, TokenAccount>>,

    /// The account paying fo rall rents
    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[account]
#[derive(InitSpace)]
pub struct Amm {
    pub id: Pubkey,
    pub admin: Pubkey,
    pub fee: u16
}

#[account]
#[derive(InitSpace)]
pub struct Pool {
    pub amm: Pubkey,
    pub mint_a: Pubkey,
    pub mint_b: Pubkey
}

#[error_code]
pub enum TutorialError {
    #[msg("Invalid fee value")]
    InvalidFee,
    #[msg("Invalid mint for the pool")]
    InvalidMint,
    #[msg("Depositing too little liquidity")]
    DepositTooSmall,
    #[msg("Output is below the minimum expected")]
    OutputTooSmall,
    #[msg("Invariant does not hold")]
    InvariantViolated,
}