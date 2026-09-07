// Sampled metadata only. Recheck age/expiry between HTTP refreshes so a lost
// connection cannot leave a previously green credit indicator green forever.
function ppsCreditView(health, now = Math.floor(Date.now() / 1000)) {
    let funding = 'Unknown', warning = false;
    const result = (state, label, category) => ({state, label, category, funding, warning});
    const unknown = category => result('unknown', 'Unknown', category);
    if (!health) return unknown('missing');
    if (health.version !== 2 || typeof health.quote_required !== 'boolean'
        || typeof health.budget_low !== 'boolean' || !Number.isSafeInteger(health.sampled_at_unix)
        || health.sampled_at_unix < 0 || health.sampled_at_unix > now
        || now - health.sampled_at_unix > 15) return unknown('stale');
    warning = health.budget_low;
    if (health.funding_expiry_valid && health.chain_expiry_valid
        && Number.isSafeInteger(health.funding_expires_at_unix) && health.funding_expires_at_unix > now
        && Number.isSafeInteger(health.chain_expires_at_unix) && health.chain_expires_at_unix > now
        && health.generation_matches === true
        && ['ok', 'quote_missing', 'quote_stale', 'quote_context_changed', 'current_quote_insufficient'].includes(health.category))
        funding = 'Current (sampled)';
    if (health.state === 'unknown') return unknown(health.category);
    if (health.quote_required && (health.state === 'ready' || health.category === 'current_quote_insufficient')) {
        if (!Number.isSafeInteger(health.quote_checked_at_unix) || health.quote_checked_at_unix < 0
            || health.quote_checked_at_unix > now || !Number.isSafeInteger(health.quote_expires_at_unix)
            || health.quote_expires_at_unix - health.quote_checked_at_unix !== 15
            || typeof health.current_quote_fits !== 'boolean') return unknown('quote_missing');
        if (health.quote_expires_at_unix <= now) return unknown('quote_stale');
        if (!health.current_quote_fits) return result('paused', 'Paused', 'current_quote_insufficient');
        if (health.category === 'current_quote_insufficient') return unknown('malformed');
    }
    if (health.state === 'paused') return result('paused', 'Paused', health.category);
    if (health.state !== 'ready' || health.category !== 'ok'
        || health.generation_matches !== true) return unknown('malformed');
    if (!health.funding_expiry_valid || !Number.isSafeInteger(health.funding_expires_at_unix)
        || health.funding_expires_at_unix <= now)
        return result('paused', 'Paused', 'funding_expired');
    if (!health.chain_expiry_valid || !Number.isSafeInteger(health.chain_expires_at_unix)
        || health.chain_expires_at_unix <= now)
        return result('paused', 'Paused', 'chain_invalid');
    return result('ready', 'Ready (sampled)', 'ok');
}
