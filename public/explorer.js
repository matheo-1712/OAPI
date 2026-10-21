'use strict';
const search = document.getElementById('route-search');
const method = document.getElementById('route-method');
const list = document.getElementById('endpoint-list');
const catalogStatus = document.getElementById('catalog-status');
let endpoints = [];
function node(tag, text, className) {
    const element = document.createElement(tag);
    element.textContent = text;
    if (className) element.className = className;
    return element;
}
function renderCatalogue() {
    const term = search.value.toLocaleLowerCase('fr');
    const selected = endpoints.filter(entry => (!method.value || entry.method.toUpperCase() === method.value) && [entry.path, entry.summary, entry.description, ...(entry.tags || [])].join(' ').toLocaleLowerCase('fr').includes(term));
    list.replaceChildren();
    catalogStatus.textContent = selected.length + ' route(s) affichée(s) sur ' + endpoints.length + '.';
    for (const entry of selected) {
        const article = document.createElement('article');
        article.className = 'endpoint';
        article.append(node('span', entry.method.toUpperCase(), 'method ' + entry.method), node('span', (entry.tags || []).join(' · '), 'pill'), node('code', entry.path), node('h3', entry.summary || entry.operationId || 'Fonction API'), node('p', entry.description || 'Consultez les paramètres et les réponses ci-dessous.'));
        const details = document.createElement('details');
        details.append(node('summary', 'Paramètres et réponses'));
        const params = entry.parameters.map(p => p.name + ' (' + p.in + (p.required ? ', requis' : ', facultatif') + ')' + (p.description ? ' : ' + p.description : '')).join('\n');
        const body = entry.requestBody ? '\nCorps de requête : ' + JSON.stringify(entry.requestBody, null, 2) : '';
        const responses = Object.entries(entry.responses || {}).map(([code, response]) => code + ' : ' + (response.description || response.$ref || '')).join('\n');
        details.append(node('pre', (params || 'Aucun paramètre.') + body + '\n\nRéponses\n' + responses));
        article.append(details);
        // Only fixed read operations can be tried here. OAuth navigation stays in sign-in links.
        if (entry.method === 'get' && !/[{}]/.test(entry.path) && !/\/auth\/(login|callback|logout)(\/|$)/.test(entry.path)) {
            const button = node('button', 'Consulter la réponse');
            button.type = 'button';
            button.className = 'btn';
            const output = node('pre', '');
            output.hidden = true;
            output.setAttribute('role', 'status');
            button.addEventListener('click', async () => {
                button.disabled = true;
                output.hidden = false;
                output.textContent = 'Chargement…';
                try { output.textContent = JSON.stringify(await OAPI.request(entry.path), null, 2) ?? 'Requête réussie (aucun contenu).'; }
                catch (error) { output.textContent = error.message; }
                finally { button.disabled = false; }
            });
            article.append(button, output);
        }
        list.append(article);
    }
}
async function loadCatalogue() {
    const reload = document.getElementById('reload-catalog');
    reload.disabled = true;
    catalogStatus.textContent = 'Chargement…';
    try { endpoints = OAPI.entries(await OAPI.spec()); renderCatalogue(); }
    catch (error) { catalogStatus.textContent = 'Catalogue indisponible. ' + error.message; }
    finally { reload.disabled = false; }
}
search.addEventListener('input', renderCatalogue);
method.addEventListener('change', renderCatalogue);
document.getElementById('reload-catalog').addEventListener('click', loadCatalogue);
loadCatalogue();
