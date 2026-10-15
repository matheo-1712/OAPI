const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

const html = fs.readFileSync(path.join(__dirname, '../public/index.html'), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];

async function page(session, result, status = 200) {
    const nodes = new Map();
    const requests = [];
    function element() {
        return {
            hidden: false, disabled: false, textContent: '', children: [],
            append(...children) { this.children.push(...children); },
            replaceChildren(...children) { this.children = children; },
            setAttribute() {}, addEventListener() {}
        };
    }
    const document = {
        getElementById(id) {
            if (!nodes.has(id)) nodes.set(id, element());
            return nodes.get(id);
        },
        createElement: element
    };
    const context = vm.createContext({
        document, URL, location: { origin: 'https://oapi.example' },
        fetch: async (url, options) => {
            requests.push({ url, options });
            if (url === '/api/auth/me') return { ok: true, json: async () => session };
            if (result instanceof Error) throw result;
            return { status, ok: status === 200, json: async () => result };
        }
    });
    await vm.runInContext(script, context);
    return { nodes, requests, refresh: () => vm.runInContext('refreshBadges()', context) };
}

test('visitors see login and members can refresh without entering account IDs', async () => {
    const visitor = await page({ authenticated: false });
    assert.equal(visitor.nodes.get('badge-login').hidden, false);
    assert.equal(visitor.nodes.get('badge-controls').hidden, true);
    assert.equal(visitor.nodes.get('refresh-badges').disabled, true);
    const member = await page({ authenticated: true, username: 'Loutre' }, {
        new_badges: [], accounts_checked: 2, failed_accounts: []
    });
    assert.equal(member.nodes.get('badge-controls').hidden, false);
    await member.refresh();
    assert.equal(member.requests[1].url, '/api/badges/me/refresh');
    assert.equal(member.requests[1].options.method, 'POST');
    assert.equal(member.requests[1].options.credentials, 'same-origin');
    assert.match(member.nodes.get('badge-status').textContent, /à jour/);
});

test('new badges show names, descriptions and account with external content as text', async () => {
    const member = await page({ authenticated: true, username: 'Loutre' }, {
        new_badges: [{ name: '<img onerror=alert(1)>', description: '100 messages', platform: 'Discord', account: 'Loutre', image_url: 'javascript:alert(1)' }],
        accounts_checked: 1, failed_accounts: []
    });
    await member.refresh();
    const cards = member.nodes.get('new-badges').children;
    assert.equal(cards.length, 1);
    assert.equal(cards[0].children.length, 3); // Unsafe image URL is omitted.
    assert.equal(cards[0].children[0].textContent, '<img onerror=alert(1)>');
    assert.equal(cards[0].children[1].textContent, '100 messages');
    assert.equal(cards[0].children[2].textContent, 'Discord · Loutre');
    assert.match(member.nodes.get('badge-status').textContent, /1 nouveau/);
    assert.equal(member.nodes.get('refresh-badges').disabled, false);
});

test('session expiration offers reconnection', async () => {
    const member = await page({ authenticated: true }, null, 401);
    await member.refresh();
    assert.equal(member.nodes.get('badge-login').hidden, false);
    assert.equal(member.nodes.get('badge-controls').hidden, true);
    assert.match(member.nodes.get('badge-status').textContent, /expiré/);
});

test('partial failure preserves new awards and identifies accounts to retry', async () => {
    const member = await page({ authenticated: true }, {
        new_badges: [{ name: 'Message I', description: '', platform: 'Discord', account: 'Loutre' }],
        accounts_checked: 1, failed_accounts: ['Minecraft']
    });
    await member.refresh();
    assert.equal(member.nodes.get('new-badges').children.length, 1);
    assert.match(member.nodes.get('badge-status').textContent, /Impossible de vérifier : Minecraft/);
});

test('network failures allow retry and never claim badges are up to date', async () => {
    const member = await page({ authenticated: true }, new Error('offline'));
    await member.refresh();
    assert.match(member.nodes.get('badge-status').textContent, /indisponible/);
    assert.equal(member.nodes.get('refresh-badges').disabled, false);
});
