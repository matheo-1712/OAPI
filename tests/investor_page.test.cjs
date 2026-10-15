// Run with: node --test tests/investor_page.test.cjs
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const html = fs.readFileSync(path.join(__dirname, '../public/investor.html'), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];

async function render(statusCode, data, reject = false) {
    const nodes = new Map();
    const document = {
        getElementById(id) {
            if (!nodes.has(id)) nodes.set(id, {
                hidden: false, textContent: '', disabled: false,
                setAttribute() {}, addEventListener() {}
            });
            return nodes.get(id);
        }
    };
    let request;
    const context = vm.createContext({
        document, Intl, Date,
        fetch: async (url, options) => {
            request = { url, options };
            if (reject) throw new Error('Network failure');
            return { status: statusCode, ok: statusCode === 200, json: async () => data };
        }
    });
    await vm.runInContext(script, context);
    assert.equal(request.url, '/api/investor/stats');
    assert.equal(request.options.cache, 'no-store');
    assert.equal(nodes.get('refresh-stats').disabled, false);
    return nodes;
}

test('shows actual counts, including zero', async () => {
    const nodes = await render(200, {
        discord_users: 1234, minecraft_players: 17, minecraft_servers: 0,
        badges_awarded: 51, fetched_at: '2026-10-15T10:00:00Z'
    });
    assert.equal(nodes.get('stats-grid').hidden, false);
    assert.equal(nodes.get('minecraft-servers').textContent, '0');
    assert.equal(nodes.get('badges-awarded').textContent, '51');
});

test('unauthenticated visitors see a login link', async () => {
    const nodes = await render(401);
    assert.equal(nodes.get('login-link').hidden, false);
    assert.equal(nodes.get('stats-grid').hidden, true);
});

test('forbidden access is distinguished from service failure', async () => {
    const forbidden = await render(403);
    assert.match(forbidden.get('stats-status').textContent, /réservé/);
    for (const nodes of [await render(503), await render(200, null, true)]) {
        assert.match(nodes.get('stats-status').textContent, /indisponibles/);
        assert.equal(nodes.get('stats-grid').hidden, true);
        assert.equal(nodes.get('login-link').hidden, true);
    }
});

test('malformed data never becomes a misleading zero', async () => {
    for (const data of [null, {}, {
        discord_users: '<img src=x onerror=alert(1)>', minecraft_players: 1,
        minecraft_servers: 2, badges_awarded: 3, fetched_at: '2026-10-15T10:00:00Z'
    }]) {
        const nodes = await render(200, data);
        assert.equal(nodes.get('stats-grid').hidden, true);
        assert.match(nodes.get('stats-status').textContent, /indisponibles/);
    }
});

test('administrator view escapes external profile data', () => {
    const admin = fs.readFileSync(path.join(__dirname, '../public/admin.html'), 'utf8');
    const source = admin.match(/<script>([\s\S]*?)<\/script>/)[1];
    const context = vm.createContext({ document: { addEventListener() {} } });
    vm.runInContext(source, context);
    const escaped = vm.runInContext('escapeHtml(`<img src=x onerror="alert(1)">&`)', context);
    assert.equal(escaped, '&lt;img src=x onerror=&quot;alert(1)&quot;&gt;&amp;');
});
