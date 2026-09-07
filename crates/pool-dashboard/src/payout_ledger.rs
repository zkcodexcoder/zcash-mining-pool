//! One payout saga, two isolated liabilities. Legacy block accounting never
//! receives authority over PPS accounts or their confirmed payout records.
use pool_db::{PendingPayout, PoolDb};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PayoutLedger {
    Legacy,
    Pps,
}

impl PayoutLedger {
    pub async fn pending(
        self,
        db: &PoolDb,
        min: i64,
        immature: bool,
        cooldown: i64,
        override_at: i64,
    ) -> anyhow::Result<Vec<PendingPayout>> {
        Ok(match self {
            Self::Legacy => {
                db.get_pending_payouts(min, immature, cooldown, override_at)
                    .await?
            }
            Self::Pps => db.get_pending_pps_payouts(min).await?,
        })
    }

    pub async fn reserve(
        self,
        db: &PoolDb,
        attempt: i64,
        items: &[(i64, i64)],
    ) -> anyhow::Result<Vec<(i64, i64)>> {
        Ok(match self {
            Self::Legacy => db.reserve_payout(attempt, items).await?,
            Self::Pps => {
                anyhow::bail!("PPS reservation requires verified funding and fixed PCZT fee")
            }
        })
    }

    pub async fn confirm(self, db: &PoolDb, attempt: i64, txid: &str) -> anyhow::Result<i64> {
        Ok(match self {
            Self::Legacy => db.confirm_payout(attempt, txid).await?,
            Self::Pps => db.confirm_pps_payout(attempt, txid).await?,
        })
    }

    pub async fn refund(self, db: &PoolDb, attempt: i64) -> anyhow::Result<i64> {
        Ok(match self {
            Self::Legacy => db.refund_payout(attempt).await?,
            Self::Pps => db.refund_pps_payout(attempt).await?,
        })
    }
}
