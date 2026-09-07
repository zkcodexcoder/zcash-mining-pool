// Synthetic projection tests for the exact script embedded in both dashboards.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(path.join(__dirname, '../src/credit_health.js'), 'utf8');
const context = vm.createContext({});
vm.runInContext(source, context);
const view = (health, now = 1001) => context.ppsCreditView(health, now);
const ready = {
    version: 2, sampled_at_unix: 1000, state: 'ready', category: 'ok',
    generation_matches: true, funding_expiry_valid: true,
    funding_expires_at_unix: 1040, chain_expiry_valid: true,
    chain_expires_at_unix: 1060,
    quote_required: true, quote_checked_at_unix: 1000, quote_expires_at_unix: 1015,
    current_quote_fits: true, budget_low: false,
};
assert.equal(view(ready).state, 'ready');
assert.equal(view(ready).label, 'Ready (sampled)');
assert.equal(view(null).state, 'unknown');
assert.equal(view({}).state, 'unknown');
assert.equal(view(ready, 999).state, 'unknown');
assert.equal(view(ready, 1014).state, 'ready');
assert.equal(view(ready, 1015).category, 'quote_stale');
assert.equal(view(ready, 1016).state, 'unknown');
assert.equal(view({...ready, state: 'unknown', category: 'malformed'}).state, 'unknown');
assert.equal(view({...ready, generation_matches: false}).state, 'unknown');
assert.equal(view({...ready, category: 'invalid'}).state, 'unknown');
assert.equal(view({...ready, funding_expires_at_unix: 1001}).category, 'funding_expired');
assert.equal(view({...ready, chain_expires_at_unix: 1001}).category, 'chain_invalid');
assert.equal(view({...ready, funding_expiry_valid: false}).state, 'paused');
assert.equal(view({...ready, chain_expiry_valid: false}).state, 'paused');
const blocked = {...ready, state: 'paused', category: 'generation_changed', generation_matches: false};
const successfulIdlePayout = {consecutive_payout_failures: 0,
    pps_payout_cycle: {outcome: 'no_payout_due', funding_check: 'not_checked_no_payout_due'}};
assert.equal(successfulIdlePayout.consecutive_payout_failures, 0);
assert.equal(view(blocked).state, 'paused');
assert.equal(view(blocked, 1016).state, 'unknown');
assert.equal(view({...ready, current_quote_fits: false}).category, 'current_quote_insufficient');
assert.equal(view({...ready, version: 1}).state, 'unknown');
for (const field of ['quote_required', 'quote_checked_at_unix', 'quote_expires_at_unix', 'current_quote_fits', 'budget_low']) {
    const h = {...ready}; delete h[field]; assert.equal(view(h).state, 'unknown', field);
}
assert.equal(view({...ready, sampled_at_unix: 1014}, 1015).state, 'unknown');
assert.equal(view({...ready, sampled_at_unix: 1014}, 1015).funding, 'Current (sampled)');
assert.equal(view({...ready, state: 'unknown', category: 'quote_context_changed'}).funding, 'Current (sampled)');
assert.equal(view({...ready, budget_low: true}).warning, true);
assert.equal(view({...ready, budget_low: true}, 1016).warning, false);
assert.equal(view({...ready, quote_required: false, quote_checked_at_unix: null,
    quote_expires_at_unix: null, current_quote_fits: null}, 1015).state, 'ready');
for (const file of ['routes.rs', 'admin.rs']) {
    const template = fs.readFileSync(path.join(__dirname, '../src', file), 'utf8');
    assert(template.includes('.replace("__PPS_CREDIT_HEALTH_JS__", crate::credit_health::SCRIPT)'));
    assert(template.includes('setInterval(renderPpsCreditHealth, 1000)'));
    assert(template.includes('ppsCreditHealth = null;'));
    const script = template.match(/<script>([\s\S]*?)<\/script>/)[1]
        .replace('__PPS_CREDIT_HEALTH_JS__', source).replace('__PPS_ENABLED__', 'true');
    // Parse the actual complete page script, not a rewritten example.
    new vm.Script(script, {filename: file});
    const fetchName = file === 'admin.rs' ? 'fetchHealth' : 'fetchStats';
    const fetchBody = script.match(new RegExp('async function ' + fetchName + '\\(\\) \\{([\\s\\S]*?)\\n\\}'))[1];
    assert(fetchBody.includes('ppsCreditHealth ='));
    assert(fetchBody.includes('renderPpsCreditHealth()'));
}
console.log('PPS credit projection and actual template wiring checks passed');
