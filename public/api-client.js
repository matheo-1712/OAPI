'use strict';
const OAPI = (() => {
    let specification;
    async function request(path, method = 'GET') {
        if (!path.startsWith('/') || path.startsWith('//') || /[{}\\]/.test(path)) throw new Error('Route invalide.');
        let response;
        try {
            response = await fetch(path, { method, credentials: 'same-origin', cache: 'no-store', signal: AbortSignal.timeout(30000) });
        } catch (error) {
            throw new Error(error.name === 'TimeoutError' ? 'Le serveur met trop de temps à répondre. Réessayez.' : 'Impossible de joindre l’API. Réessayez dans un instant.');
        }
        if (!response.ok) {
            const errors = { 401: 'Votre session a expiré. Connectez-vous avec Discord.', 403: 'Votre compte ne possède pas les droits nécessaires.', 429: 'Quota atteint. Réessayez dans ' + (response.headers.get('retry-after') || '60') + ' s.' };
            const error = new Error(errors[response.status] || 'Service temporairement indisponible (HTTP ' + response.status + ').');
            error.status = response.status;
            throw error;
        }
        return response.status === 204 ? null : response.json();
    }
    async function spec() {
        if (!specification) specification = request('/api-docs/openapi.json').catch(error => { specification = null; throw error; });
        const data = await specification;
        if (!data || !data.paths || typeof data.paths !== 'object') { specification = null; throw new Error('Catalogue API invalide.'); }
        return data;
    }
    function entries(data) {
        return Object.entries(data.paths).flatMap(([path, item]) => ['get','post','put','patch','delete'].filter(method => item[method]).map(method => ({ path, method, ...item[method], parameters: [...(item.parameters || []), ...(item[method].parameters || [])] })));
    }
    async function route(operation, params = {}) {
        const entry = entries(await spec()).find(item => item.operationId === operation);
        if (!entry) throw new Error('Cette fonction n’est pas disponible sur ce serveur.');
        return entry.path.replace(/\{([^}]+)\}/g, (_, name) => {
            if (!params[name]) throw new Error('Paramètre manquant : ' + name);
            return encodeURIComponent(params[name]);
        });
    }
    function imageURL(value, origin) {
        const url = new URL(value, origin);
        if (url.origin !== origin || !url.pathname.startsWith('/generated_images/') || url.username || url.password) throw new Error('URL d’image invalide.');
        return url.href;
    }
    return { request, spec, entries, route, imageURL };
})();
if (typeof module !== 'undefined') module.exports = OAPI;
