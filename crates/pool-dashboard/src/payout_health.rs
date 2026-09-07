//! Observation only: a successful idle payout cycle is not a funding proof.
use std::cell::Cell;

#[derive(Clone, Copy, Default)]
struct Observation {
    no_payout_due: bool,
    funding_check_started: bool,
}

tokio::task_local! {
    static OBSERVATION: Cell<Observation>;
}

/// Called only at the actual below-minimum early return. No extra DB/RPC read.
pub(crate) fn note_no_payout_due() {
    let _ = OBSERVATION.try_with(|cell| {
        let mut value = cell.get();
        value.no_payout_due = true;
        cell.set(value);
    });
}

pub(crate) fn note_funding_check() {
    let _ = OBSERVATION.try_with(|cell| {
        let mut value = cell.get();
        value.funding_check_started = true;
        cell.set(value);
    });
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct PpsPayoutCycleReport {
    pub outcome: &'static str,
    /// Describes this payout cycle only, never the separate credit-admission gate.
    pub funding_check: &'static str,
}

pub(crate) async fn observe<F>(future: F) -> (anyhow::Result<usize>, PpsPayoutCycleReport)
where
    F: std::future::Future<Output = anyhow::Result<usize>>,
{
    OBSERVATION
        .scope(Cell::new(Observation::default()), async {
            let result = future.await;
            let observation = OBSERVATION.with(Cell::get);
            let outcome = match &result {
                Err(_) => "held",
                Ok(0) if observation.no_payout_due => "no_payout_due",
                Ok(0) => "no_payout_processed",
                Ok(_) => "payouts_processed",
            };
            let funding_check = if observation.funding_check_started {
                "attempted"
            } else if observation.no_payout_due {
                "not_checked_no_payout_due"
            } else {
                "not_observed"
            };
            (
                result,
                PpsPayoutCycleReport {
                    outcome,
                    funding_check,
                },
            )
        })
        .await
}

/// Keep the successful-zero-cycle reset: idle is not a payout failure.
pub(crate) fn clear_successful_cycle(failures: &mut u32, error: &mut String) {
    *failures = 0;
    error.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn below_minimum_is_successful_but_does_not_claim_funding_was_checked() {
        let (result, report) = observe(async {
            note_no_payout_due();
            Ok(0)
        })
        .await;
        assert_eq!(result.unwrap(), 0);
        assert_eq!(report.outcome, "no_payout_due");
        assert_eq!(report.funding_check, "not_checked_no_payout_due");
        let (mut failures, mut error) = (3, "synthetic prior hold".to_owned());
        clear_successful_cycle(&mut failures, &mut error);
        assert_eq!((failures, error.as_str()), (0, ""));
    }

    #[tokio::test]
    async fn observed_funding_is_never_hidden_by_a_zero_result_or_error() {
        let (result, report) = observe(async {
            note_funding_check();
            Ok(0)
        })
        .await;
        assert_eq!(result.unwrap(), 0);
        assert_eq!(report.outcome, "no_payout_processed");
        assert_eq!(report.funding_check, "attempted");
        let (result, report) = observe(async {
            note_funding_check();
            anyhow::bail!("synthetic private detail")
        })
        .await;
        assert!(result.is_err());
        assert_eq!(
            serde_json::to_value(report).unwrap(),
            serde_json::json!({
                "outcome":"held", "funding_check":"attempted"
            })
        );
    }

    #[tokio::test]
    async fn observations_are_task_local_and_success_is_not_relabelled_idle() {
        let (idle, paid) = tokio::join!(
            observe(async {
                note_no_payout_due();
                tokio::task::yield_now().await;
                Ok(0)
            }),
            observe(async {
                note_funding_check();
                tokio::task::yield_now().await;
                Ok(1)
            })
        );
        assert_eq!(idle.1.outcome, "no_payout_due");
        assert_eq!(paid.1.outcome, "payouts_processed");
        assert_eq!(paid.1.funding_check, "attempted");
        assert!(OBSERVATION.try_with(Cell::get).is_err());
    }
}
