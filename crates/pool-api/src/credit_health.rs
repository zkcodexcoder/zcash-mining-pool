//! Read-only projection of the pool process's PPS admission heartbeat.
use pool_core::pps_credit_health::{decode_credit_health, PpsCreditHealth, CREDIT_HEALTH_KEY};
use pool_db::PoolDb;

pub(crate) const SCRIPT: &str = include_str!("credit_health.js");

pub fn present(enabled: bool, raw: Option<&str>, now_unix: i64) -> Option<PpsCreditHealth> {
    enabled.then(|| decode_credit_health(raw, now_unix))
}

/// One metadata read, never a wallet/node RPC or a financial authorization.
pub(crate) async fn read(db: &PoolDb, enabled: bool) -> Option<PpsCreditHealth> {
    if !enabled {
        return None;
    }
    let stored = db.get_pool_status(CREDIT_HEALTH_KEY).await.ok().flatten();
    present(
        true,
        stored.as_ref().map(|(raw, _)| raw.as_str()),
        chrono::Utc::now().timestamp(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pool_core::pps_credit_health::CreditAdmissionState;
    use serde_json::{json, Value};

    fn heartbeat() -> Value {
        json!({"version":2,"sampled_at_unix":1000,"state":"ready","category":"ok",
            "refresh_in_progress":false,"last_attempt_unix":995,"last_success_unix":999,
            "last_refresh_stage":"complete","last_refresh_result":"ok",
            "funding_checked_at_unix":980,"funding_expires_at_unix":1040,
            "funding_expiry_valid":true,"generation_matches":true,
            "chain_expires_at_unix":1060,"chain_expiry_valid":true,
            "quote_required":true,"quote_checked_at_unix":1000,"quote_expires_at_unix":1015,
            "current_quote_fits":true,"budget_low":false,
            "denial_count":0,"last_denial_category":null})
    }

    #[test]
    fn credit_health_is_independent_of_a_successful_idle_payout() {
        let payout = json!({"consecutive_payout_failures":0,"last_payout_error":"",
            "pps_payout_cycle":{"outcome":"no_payout_due","funding_check":"not_checked_no_payout_due"}});
        let mut h = heartbeat();
        h["state"] = json!("paused");
        h["category"] = json!("generation_changed");
        h["generation_matches"] = json!(false);
        let credit = present(true, Some(&h.to_string()), 1001).unwrap();
        assert_eq!(payout["consecutive_payout_failures"], 0);
        assert_eq!(credit.state, CreditAdmissionState::Paused);
        assert_eq!(credit.category, "generation_changed");
    }

    #[test]
    fn missing_stale_future_and_malformed_metrics_are_unknown_not_green() {
        assert_eq!(
            present(true, None, 1001).unwrap().state,
            CreditAdmissionState::Unknown
        );
        for (raw, now) in [
            (heartbeat().to_string(), 1016),
            (heartbeat().to_string(), 999),
            ("{invalid".into(), 1001),
            ("x".repeat(8193), 1001),
        ] {
            assert_eq!(
                present(true, Some(&raw), now).unwrap().state,
                CreditAdmissionState::Unknown
            );
        }
        let mut private = heartbeat();
        private["category"] = json!("synthetic private detail");
        let out = serde_json::to_string(&present(true, Some(&private.to_string()), 1001)).unwrap();
        assert!(!out.contains("synthetic private detail"));
        assert!(present(false, Some(&heartbeat().to_string()), 1001).is_none());
    }

    #[test]
    fn genuinely_current_gates_are_ready_but_original_deadlines_still_expire() {
        assert_eq!(
            present(true, Some(&heartbeat().to_string()), 1001)
                .unwrap()
                .state,
            CreditAdmissionState::Ready
        );
        // A lapsed funding lease only degrades (shares still credit; sends may
        // hold); chain agreement is a warning only, so a lapsed proof degrades too.
        for (field, expected) in [
            ("funding_expires_at_unix", CreditAdmissionState::Degraded),
            ("chain_expires_at_unix", CreditAdmissionState::Degraded),
        ] {
            let mut h = heartbeat();
            h[field] = json!(1002);
            assert_eq!(
                present(true, Some(&h.to_string()), 1002).unwrap().state,
                expected,
                "{field}"
            );
        }
    }

    #[tokio::test]
    async fn database_reader_uses_only_existing_metadata_and_rejects_missing_rows() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let db = PoolDb::new(pool);
        db.run_migrations().await.unwrap();
        assert_eq!(
            read(&db, true).await.unwrap().state,
            CreditAdmissionState::Unknown
        );
        assert!(read(&db, false).await.is_none());
        let now = chrono::Utc::now().timestamp();
        let mut h = heartbeat();
        for (field, delta) in [
            ("sampled_at_unix", 0),
            ("last_attempt_unix", -5),
            ("last_success_unix", -1),
            ("funding_checked_at_unix", -20),
            ("funding_expires_at_unix", 40),
            ("chain_expires_at_unix", 60),
            ("quote_checked_at_unix", 0),
            ("quote_expires_at_unix", 15),
        ] {
            h[field] = json!(now + delta);
        }
        db.set_pool_status(CREDIT_HEALTH_KEY, &h.to_string())
            .await
            .unwrap();
        assert_eq!(
            read(&db, true).await.unwrap().state,
            CreditAdmissionState::Ready
        );
    }
    #[test]
    fn quote_capacity_expiry_and_schema_migration_cannot_report_green() {
        let mut h=heartbeat(); h["current_quote_fits"]=json!(false);
        let projected=present(true,Some(&h.to_string()),1001).unwrap();
        assert_eq!(projected.state,CreditAdmissionState::Paused);
        assert_eq!(projected.category,"current_quote_insufficient");
        h["state"]=json!("paused"); h["category"]=json!("current_quote_insufficient");
        h["sampled_at_unix"]=json!(1014);
        // Once that quote lapses its verdict is withheld (fits=None), but the
        // sampler's recorded hard pause stands until it resamples — the reader
        // never invents "unknown" from a mere quote-clock expiry.
        let expired=present(true,Some(&h.to_string()),1015).unwrap();
        assert_eq!(expired.state,CreditAdmissionState::Paused);
        assert_eq!(expired.category,"current_quote_insufficient");
        assert!(expired.current_quote_fits.is_none());
        assert_eq!(expired.quote_expires_at_unix,Some(1015));
        assert!(expired.funding_expiry_valid); // separate evidence is still current
        // Schema-required flags and a half-present quote pair are malformed.
        for field in ["quote_required","quote_checked_at_unix","quote_expires_at_unix","budget_low"] {
            let mut h=heartbeat(); h.as_object_mut().unwrap().remove(field);
            assert_eq!(present(true,Some(&h.to_string()),1001).unwrap().state,CreditAdmissionState::Unknown, "{field}");
        }
        // A quote with its fit verdict withheld is merely "no verdict yet" — still healthy.
        let mut h=heartbeat(); h.as_object_mut().unwrap().remove("current_quote_fits");
        assert_eq!(present(true,Some(&h.to_string()),1001).unwrap().state,CreditAdmissionState::Ready);
        let mut old=heartbeat(); old["version"]=json!(1);
        assert_eq!(present(true,Some(&old.to_string()),1001).unwrap().state,CreditAdmissionState::Unknown);
        let mut healthy=heartbeat(); healthy["budget_low"]=json!(true);
        let projected=present(true,Some(&healthy.to_string()),1001).unwrap();
        assert_eq!(projected.state,CreditAdmissionState::Ready); assert!(projected.budget_low);
    }
}
