use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

declare_id!("FGjLaVo5zLGdzCxMo9gu9tXr1kzTToKd8C8K7YS5hNM1");

/*
TODO: 
- Handle transferring of the funds on-chain in the programs
- Implement odds VERIFICATION on-chain
- Implement 'wager-dispute' features
- See other TODOs in the code
*/

#[program]
pub mod the_wager_program {
    use super::*;

    pub fn create_wager(
        ctx: Context<CreateWager>,
        wager_id: u64,
        opponent: Option<Pubkey>,
        judge: Pubkey,
        amount: u64,
        expiration_date: i64,
        end_date: i64,
        odds_numerator: u16,
        odds_denominator: u16,
    ) -> Result<()> {
        let wager = &mut ctx.accounts.wager;
        let user = &ctx.accounts.user;

        require!(expiration_date > Clock::get()?.unix_timestamp, ErrorCode::InvalidExpirationDate);
        require!(end_date > expiration_date, ErrorCode::InvalidEndDate);
        require!(odds_numerator >= 1 && odds_denominator >= 1, ErrorCode::InvalidOdds);
        require!(odds_denominator <= 1000 && odds_denominator <= 1000, ErrorCode::InvalidOdds);

        // Odds must be in a range (1:100 to 100:1)
        let odds_ratio = (odds_numerator as f64) / (odds_denominator as f64);
        require!(odds_ratio >= 0.01 && odds_ratio <= 100.0, ErrorCode::InvalidOdds);

        wager.wager_initiator = user.key();
        wager.id = wager_id;
        wager.opponent = opponent;
        wager.judge = judge;
        wager.amount = amount;
        wager.expiration_date = expiration_date;
        wager.end_date = end_date;
        wager.odds_numerator = odds_numerator;
        wager.odds_denominator = odds_denominator;

        Ok(())
    }

    pub fn accept_wager(ctx: Context<UpdateWager>) -> Result<()> {
        let user = &mut ctx.accounts.user;
        let wager = &mut ctx.accounts.wager;

        require!(Clock::get()?.unix_timestamp <= wager.expiration_date, ErrorCode::WagerExpired);

        // Check if the wager has a specified opponent
        if let Some(opponent) = wager.opponent {
            require!(user.key() == opponent, ErrorCode::InvalidOpponent);
        }
        
        wager.opponent_accepted = true;
        wager.opponent = Some(user.key());

        Ok(())
    }

    pub fn accept_judging(ctx: Context<UpdateWager>) -> Result<()> {
        let user = &mut ctx.accounts.user;
        let wager = &mut ctx.accounts.wager;

        require!(user.key() == wager.judge, ErrorCode::InvalidCaller);
        wager.judge_accepted = true;

        Ok(())
    }

    pub fn cancel_wager(ctx: Context<UpdateWager>) -> Result<()> { // TODO: Implement this on frontend
        let user = &ctx.accounts.user;
        let wager = &mut ctx.accounts.wager;

        require!(user.key() == wager.wager_initiator, ErrorCode::InvalidCaller);
        require!(!wager.opponent_accepted, ErrorCode::WagerAlreadyAccepted);

        // Transfer funds back to the wager initiator
        let balance = ctx.accounts.wager.to_account_info().lamports();
        **ctx.accounts.wager.to_account_info().try_borrow_mut_lamports()? = 0;
        **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? += balance;

        Ok(())
    }

    pub fn declare_winner(
        ctx: Context<Create>,
        start_time: u64,
        net_amount_deposited: u64,
        period: u64,
        amount_per_period: u64,
        cliff: u64,
        cliff_amount: u64,
        cancelable_by_sender: bool,
        cancelable_by_recipient: bool,
        automatic_withdrawal: bool,
        transferable_by_sender: bool,
        transferable_by_recipient: bool,
        can_topup: bool,
        stream_name: [u8; 64],
        withdraw_frequency: u64,
        pausable: Option<bool>,
        can_update_rate: Option<bool>,
        winner: Pubkey,
    ) -> Result<()> {
        let wager = &mut ctx.accounts.wager;

        require!(ctx.accounts.user.key() == wager.judge, ErrorCode::InvalidCaller);
        require!(wager.opponent_accepted && wager.judge_accepted, ErrorCode::WagerNotReady);

        wager.winner = Some(winner);

        // STREAMFLOW

        msg!("Got create");
        // initializing accounts struct for cross-program invoke
        let accs = CpiCreate {
            sender: ctx.accounts.sender.to_account_info(),
            sender_tokens: ctx.accounts.sender_tokens.to_account_info(),
            recipient: ctx.accounts.recipient.to_account_info(),
            recipient_tokens: ctx.accounts.recipient_tokens.to_account_info(),
            metadata: ctx.accounts.metadata.to_account_info(),
            escrow_tokens: ctx.accounts.escrow_tokens.to_account_info(),
            streamflow_treasury: ctx.accounts.streamflow_treasury.to_account_info(),
            streamflow_treasury_tokens: ctx.accounts.streamflow_treasury_tokens.to_account_info(),
            withdrawor: ctx.accounts.withdrawor.to_account_info(),
            partner: ctx.accounts.partner.to_account_info(),
            partner_tokens: ctx.accounts.partner_tokens.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            fee_oracle: ctx.accounts.fee_oracle.to_account_info(),
            rent: ctx.accounts.rent.to_account_info(),
            timelock_program: ctx.accounts.streamflow_program.to_account_info(),
            token_program: ctx.accounts.token_program.to_account_info(),
            associated_token_program: ctx.accounts.associated_token_program.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };

        let cpi_ctx = CpiContext::new(ctx.accounts.streamflow_program.to_account_info(), accs);

        // Transfer funds to winner account
        let balance = ctx.accounts.wager.to_account_info().lamports();
        **ctx.accounts.wager.to_account_info().try_borrow_mut_lamports()? = 0;
        **ctx.accounts.winner.try_borrow_mut_lamports()? += balance;

        Ok(())
    }

    pub fn refund_wager(ctx: Context<RefundWager>) -> Result<()> {
        let wager = &ctx.accounts.wager;
        let user = &ctx.accounts.user;

        require!(user.key() == wager.wager_initiator, ErrorCode::InvalidCaller);
        require!(!wager.opponent_accepted, ErrorCode::WagerAlreadyAccepted);
        require!(Clock::get()?.unix_timestamp > wager.expiration_date, ErrorCode::WagerNotExpired);

        // Transfer funds back to the wager initiator
        let balance = ctx.accounts.wager.to_account_info().lamports();
        **ctx.accounts.wager.to_account_info().try_borrow_mut_lamports()? = 0;
        **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? += balance;

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(wager_id: u64)]
pub struct CreateWager<'info> {
    #[account(
        init,
        payer = user,
        space = 8 + 8 + 32 + 33 + 32 + 8 + 8 + 8 + 1 + 1 + 33 + 2 + 2,
        seeds = [b"wager", user.key().as_ref(), wager_id.to_le_bytes().as_ref()],
        bump
    )]
    pub wager: Account<'info, Wager>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateWager<'info> {
    #[account(mut)]
    pub wager: Account<'info, Wager>,
    #[account(mut)]
    pub user: Signer<'info>,
}

#[derive(Accounts)]
pub struct EndWager<'info> {
    #[account(mut, close = user)]
    pub wager: Account<'info, Wager>,
    #[account(mut)]
    pub user: Signer<'info>,
    /// CHECK: This is the account that will receive the funds
    #[account(mut)]
    pub winner: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Create<'info> {
    #[account(mut)]
    pub sender: Signer<'info>,
    #[account(
        associated_token::mint = mint,
        associated_token::authority = sender,
    )]
    pub sender_tokens: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    /// CHECK: Wallet address of the recipient.
    pub recipient: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = mint,
        associated_token::authority = recipient,
    )]
    pub recipient_tokens: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub metadata: Signer<'info>,
    #[account(
        mut,
        seeds = [b"strm", metadata.key().to_bytes().as_ref()],
        bump,
        seeds::program = streamflow_program
    )]
    /// CHECK: The escrow account holding the funds, expects empty (non-initialized) account.
    pub escrow_tokens: AccountInfo<'info>,
    #[account(mut)]
    /// CHECK: Streamflow treasury account.
    pub streamflow_treasury: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = mint,
        associated_token::authority = streamflow_treasury,
    )]
    /// CHECK: Associated token account address of `streamflow_treasury`.
    pub streamflow_treasury_tokens: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    /// CHECK: Delegate account for automatically withdrawing contracts.
    pub withdrawor: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Partner treasury account.
    pub partner: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = sender,
        associated_token::mint = mint,
        associated_token::authority = partner,
    )]
    pub partner_tokens: Box<Account<'info, TokenAccount>>,
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut, close = user)]
    pub wager: Account<'info, Wager>,
    #[account(mut)]
    pub user: Signer<'info>,
    /// CHECK: This is the account that will receive the funds
    #[account(mut)]
    pub winner: AccountInfo<'info>,
    /// CHECK: Internal program that handles fees for specified partners.
    pub fee_oracle: UncheckedAccount<'info>,
    pub rent: Sysvar<'info, Rent>,
    /// CHECK: Streamflow protocol (alias timelock) program account.
    pub streamflow_program: UncheckedAccount<'info>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}


#[derive(Accounts)]
pub struct RefundWager<'info> {
    #[account(mut, close = user)]
    pub wager: Account<'info, Wager>,
    #[account(mut)]
    pub user: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[account]
pub struct Wager {
    pub id: u64,
    pub wager_initiator: Pubkey,
    pub opponent: Option<Pubkey>,
    pub judge: Pubkey,
    pub amount: u64,
    pub expiration_date: i64,
    pub end_date: i64,
    pub opponent_accepted: bool,
    pub judge_accepted: bool,
    pub winner: Option<Pubkey>,
    pub odds_numerator: u16,
    pub odds_denominator: u16,
}

#[error_code]
pub enum ErrorCode {
    #[msg("No access")]
    InvalidCaller,
    #[msg("Wager not ready")]
    WagerNotReady,
    #[msg("Wager has expired")]
    WagerExpired,
    #[msg("Wager has not ended yet")]
    WagerNotEnded,
    #[msg("Invalid expiration date")]
    InvalidExpirationDate,
    #[msg("Invalid end date")]
    InvalidEndDate,
    #[msg("Wager has already been accepted")]
    WagerAlreadyAccepted,
    #[msg("Wager has not expired yet")]
    WagerNotExpired,
    #[msg("Invalid odds")]
    InvalidOdds,
    #[msg("Insufficient deposit")]
    InsufficientDeposit,
    #[msg("Calculation error")]
    CalculationError,
    #[msg("Invalid opponent")]
    InvalidOpponent,
}
