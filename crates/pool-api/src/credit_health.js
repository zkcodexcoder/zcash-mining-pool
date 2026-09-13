// Sampled metadata only. Recheck age/expiry between HTTP refreshes so a lost
// connection cannot leave a previously green credit indicator green forever.
//
// States mirror pool-core's CreditAdmissionState:
//   ready    - shares credited, all proofs current
//   degraded - shares STILL credited; a warning (chain agreement) or a stale
//              send-side proof (payouts may hold)
//   paused   - a hard gate is rejecting valid shares (cap, accounting)
//   unknown  - telemetry missing, malformed or stale
// An idle gap between priced shares is not unknown.
function ppsCreditView(health, now = Math.floor(Date.now() / 1000)) {
    let funding = 'Unknown', warning = false;
    const result = (state, label, category) => ({state, label, category, funding, warning});
    const unknown = category => result('unknown', 'Unknown', category);
    const degraded = (category, label = 'Crediting (payouts may hold)') => result('degraded', label, category);
    if (!health) return unknown('missing');
    if (health.version !== 2 || typeof health.quote_required !== 'boolean'
        || typeof health.budget_low !== 'boolean' || !Number.isSafeInteger(health.sampled_at_unix)
        || health.sampled_at_unix < 0 || health.sampled_at_unix > now
        || now - health.sampled_at_unix > 15) return unknown('stale');
    // A half-present quote pair is malformed, exactly as the server decoder rules.
    if (Number.isSafeInteger(health.quote_checked_at_unix) !== Number.isSafeInteger(health.quote_expires_at_unix))
        return unknown('malformed');
    warning = health.budget_low;
    const fundingCurrent = health.funding_expiry_valid === true
        && Number.isSafeInteger(health.funding_expires_at_unix) && health.funding_expires_at_unix > now;
    const chainCurrent = health.chain_expiry_valid === true
        && Number.isSafeInteger(health.chain_expires_at_unix) && health.chain_expires_at_unix > now;
    if (fundingCurrent && health.generation_matches === true) funding = 'Current (sampled)';
    if (health.state === 'unknown') return unknown(health.category);
    if (!['ready', 'degraded', 'paused'].includes(health.state)) return unknown('malformed');
    // Only a FRESH quote says anything about the next share.
    const quoteFresh = health.quote_required === true
        && Number.isSafeInteger(health.quote_checked_at_unix) && health.quote_checked_at_unix <= now
        && Number.isSafeInteger(health.quote_expires_at_unix) && health.quote_expires_at_unix > now;
    if (quoteFresh && health.current_quote_fits === false)
        return result('paused', 'Paused', 'current_quote_insufficient');
    if (health.state === 'paused') return result('paused', 'Paused', health.category);
    // Chain agreement is a warning only: a lapsed proof degrades, never pauses,
    // and is reported ahead of other warnings, as the server ranks it.
    if (!chainCurrent) return degraded('chain_invalid', 'Crediting (chain warning)');
    if (health.state === 'degraded') return degraded(health.category);
    if (health.category !== 'ok' || health.generation_matches !== true) return unknown('malformed');
    if (!fundingCurrent) return degraded('funding_expired');
    return result('ready', 'Ready (sampled)', 'ok');
}
