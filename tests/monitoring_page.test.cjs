const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const script = fs.readFileSync('public/monitoring.html', 'utf8').match(/<script>([\s\S]*?)<\/script>/)[1].split('        refreshDashboard().then')[0];
function dashboard() {
    const nodes = new Map();
    const updates = [];
    const calls = [];
    let response;
    const context = vm.createContext({
        document: { getElementById(id) {
            if (!nodes.has(id)) nodes.set(id, { textContent: '', innerHTML: '', classList: { add() {}, remove() {} } });
            return nodes.get(id);
        } },
        fetch: async (url) => { calls.push(url); return response; },
        record: result => updates.push(result),
    });
    vm.runInContext(script + '\nupdateUI = (card, badge, latency, meta, result) => record(result);', context);
    return { nodes, updates, calls, context, async refresh(value) {
        response = value;
        await vm.runInContext('refreshDashboard()', context);
    } };
}
test('one aggregate request renders more services than the individual route quota allows', async () => {
    const page = dashboard();
    const services = Array.from({ length: 15 }, (_, i) => ({ name: 'Service ' + i, type_name: 'http', status: i ? 'UP' : 'DOWN', error: i ? undefined : 'HTTP Error: 503' }));
    await page.refresh({ ok: true, json: async () => ({ services }) });
    assert.deepEqual(page.calls, ['/api/monitoring']);
    assert.deepEqual(page.updates, services);
    await page.refresh({ ok: true, json: async () => ({ services }) });
    assert.equal(page.calls.length, 2);
});
test('monitoring failures preserve service results and recover on the next refresh', async () => {
    const page = dashboard();
    const result = { name: 'Bot', type_name: 'discord', status: 'UP' };
    await page.refresh({ ok: true, json: async () => ({ services: [result] }) });
    await page.refresh({ ok: false, status: 429 });
    assert.equal(page.updates.length, 1);
    assert.match(page.nodes.get('last-update').textContent, /Monitoring indisponible.*429/);
    await page.refresh({ ok: true, json: async () => ({ services: [result] }) });
    assert.equal(page.updates.length, 2);
    assert.match(page.nodes.get('last-update').textContent, /Mise à jour/);
});
test('initial API failure reports monitoring unavailability without marking services down', async () => {
    const page = dashboard();
    await page.refresh({ ok: false, status: 503 });
    assert.equal(page.updates.length, 0);
    assert.match(page.nodes.get('status-grid').textContent, /Impossible de récupérer/);
});
test('card identifiers distinguish service types and punctuation', () => {
    const page = dashboard();
    const ids = vm.runInContext("['site:Bot', 'discord:Bot', 'site:A B', 'site:A-B'].map(safeId)", page.context);
    assert.equal(new Set(ids).size, 4);
});
