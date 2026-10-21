const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const clientScript = fs.readFileSync('public/api-client.js','utf8');
function client(fetch) {
    const context = vm.createContext({ fetch, URL, AbortSignal });
    return vm.runInContext(clientScript + '\nOAPI', context);
}
const schema = { paths: {
    '/custom/profiles/me': { post: { operationId: 'create_my_discord_summary' } },
    '/custom/minecraft/{uuid}': { parameters: [{ name:'uuid', in:'path' }], post: { operationId: 'create_minecraft_summary_by_id' } }
} };
test('tools discover configured routes from OpenAPI and encode path parameters', async () => {
    let calls = 0;
    const api = client(async path => { assert.equal(path, '/api-docs/openapi.json'); calls++; return { ok:true, json: async () => schema }; });
    assert.equal(await api.route('create_my_discord_summary'), '/custom/profiles/me');
    assert.equal(await api.route('create_minecraft_summary_by_id', { uuid:'abc/def' }), '/custom/minecraft/abc%2Fdef');
    assert.equal(calls,1);
    await assert.rejects(api.route('missing'), /pas disponible/);
    await assert.rejects(api.route('create_minecraft_summary_by_id'), /Paramètre manquant/);
});
test('HTTP errors distinguish authentication, forbidden access and retry delays', async () => {
    for (const [status, message] of [[401,/session/],[403,/droits/],[429,/17 s/],[503,/503/]]) {
        const api = client(async () => ({ ok:false, status, headers:{ get:()=> '17' } }));
        await assert.rejects(api.request('/api/test'), error => error.status===status && message.test(error.message));
    }
});
test('requests use the session and never follow arbitrary external paths', async () => {
    let calls=0;
    const api=client(async (path, options) => {
        calls++; assert.equal(options.method,'POST'); assert.equal(options.credentials,'same-origin'); assert.equal(options.cache,'no-store');
        return { ok:true, json:async()=>({url:'/generated_images/discord_summary/123/a.png'}) };
    });
    await api.request('/api/card','POST');
    for(const url of ['https://external.test','//external.test','/unresolved/{id}','/\\external.test']) await assert.rejects(api.request(url), /invalide/);
    assert.equal(calls,1);
});
test('failed catalogue loading can be retried', async () => {
    let calls=0;
    const api=client(async () => { if(calls++===0) throw new Error('offline'); return { ok:true,json:async()=>schema }; });
    await assert.rejects(api.spec(), /joindre/);
    assert.equal((await api.spec()).paths,schema.paths);
});
test('generated image previews are restricted to same-origin protected images', () => {
    const api=client(()=>{}), origin='https://oapi.example';
    assert.equal(api.imageURL('/generated_images/discord_summary/123/a.png',origin), origin+'/generated_images/discord_summary/123/a.png');
    for(const value of ['javascript:alert(1)','https://external.test/a.png','/api/auth/logout','//external.test/generated_images/a.png']) assert.throws(()=>api.imageURL(value,origin), /invalide/);
});
function tools(session, generateError) {
    const nodes = new Map(), requests=[];
    const element = () => ({ hidden:false,disabled:false,textContent:'',value:'',children:[],events:{},addEventListener(name,fn){this.events[name]=fn;},append(...items){this.children.push(...items);},replaceChildren(...items){this.children=items;} });
    const get=id=>{ if(!nodes.has(id)) nodes.set(id,element()); return nodes.get(id); };
    const context=vm.createContext({ document:{getElementById:get,createElement:element}, location:{origin:'https://oapi.example'}, Intl,
        OAPI:{ route:async name=>'/'+name, imageURL:value=>'https://oapi.example'+value,
            request:async (path,method)=>{ requests.push({path,method}); if(path==='/me') return session; if(generateError) throw generateError; return {url:'/generated_images/a.png'}; } }
    });
    vm.runInContext(fs.readFileSync('public/tools.js','utf8'), context);
    return {get,requests,context};
}
test('tool buttons require a session, then generate the connected user card', async () => {
    const guest=tools({authenticated:false});
    await new Promise(resolve=>setImmediate(resolve));
    assert.equal(guest.get('discord-generate').disabled,true);
    const member=tools({authenticated:true,username:'Loutre'});
    await new Promise(resolve=>setImmediate(resolve));
    assert.equal(member.get('discord-generate').disabled,false);
    await member.get('discord-generate').events.click();
    assert.equal(member.requests.at(-1).method,'POST');
    assert.equal(member.requests.at(-1).path,'/create_my_discord_summary');
    assert.equal(member.get('discord-result').children[0].alt,'Carte de profil Discord');
});
test('expired sessions disable tool actions and offer sign-in', async () => {
    const error=Object.assign(new Error('Session expirée'),{status:401});
    const member=tools({authenticated:true,username:'Loutre'},error);
    await new Promise(resolve=>setImmediate(resolve));
    await member.get('discord-generate').events.click();
    assert.equal(member.get('tool-login').hidden,false);
    assert.equal(member.get('discord-generate').disabled,true);
    assert.equal(member.get('activity-load').disabled,true);
});
test('invalid Minecraft IDs never issue generation requests', async () => {
    const member=tools({authenticated:true});
    await new Promise(resolve=>setImmediate(resolve));
    member.get('minecraft-uuid').value='invalid';
    member.get('minecraft-form').events.submit({preventDefault(){}});
    assert.equal(member.requests.length,1);
    assert.match(member.get('minecraft-status').textContent,/UUID Minecraft valide/);
});

test('API catalogue filters routes and never offers OAuth logout as a read action', async () => {
    const elements = new Map();
    const create = tag => ({ tag, textContent:'',value:'',children:[],events:{},addEventListener(name,fn){this.events[name]=fn;},append(...children){this.children.push(...children);},replaceChildren(){this.children=[];},setAttribute(){} });
    const get=id=>{if(!elements.has(id))elements.set(id,create('div'));return elements.get(id);};
    const entries=[
        {path:'/api/auth/logout',method:'get',operationId:'logout',parameters:[],responses:{}},
        {path:'/api/discord/stats/me',method:'get',operationId:'personal',summary:'<img onerror=alert(1)>',tags:['Discord'],parameters:[],responses:{200:{description:'OK'}}},
        {path:'/api/discord-summary/me',method:'post',operationId:'create_my_discord_summary',parameters:[],responses:{}}
    ];
    const context=vm.createContext({document:{getElementById:get,createElement:create}, OAPI:{spec:async()=>({}),entries:()=>entries, request:async()=>({messages:5})}});
    vm.runInContext(fs.readFileSync('public/explorer.js','utf8'),context);
    await new Promise(resolve=>setImmediate(resolve));
    const rows=get('endpoint-list').children;
    assert.equal(rows.length,3);
    assert.equal(rows[0].children.some(child=>child.tag==='button'),false);
    assert.equal(rows[2].children.some(child=>child.tag==='button'),false);
    assert.equal(rows[1].children.find(child=>child.tag==='h3').textContent,'<img onerror=alert(1)>');
    await rows[1].children.find(child=>child.tag==='button').events.click();
    assert.match(rows[1].children.at(-1).textContent,/"messages": 5/);
    get('route-search').value='Discord';
    get('route-method').value='GET';
    get('route-search').events.input();
    assert.equal(get('endpoint-list').children.length,1);
    assert.match(get('catalog-status').textContent,/1 route/);
});
