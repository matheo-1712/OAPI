'use strict';
const byId = id => document.getElementById(id);
const controls = ['discord-generate', 'minecraft-generate', 'activity-load'];
function sessionExpired(error) {
    if (error.status === 401) {
        byId('tool-login').hidden = false;
        byId('tool-session').textContent = error.message;
        controls.forEach(id => { byId(id).disabled = true; });
    }
}
async function loadToolSession() {
    try {
        const path = await OAPI.route('me');
        const session = await OAPI.request(path);
        byId('tool-session').textContent = session.authenticated ? 'Connecté en tant que ' + session.username + '.' : 'Connectez-vous pour utiliser les outils personnels.';
        byId('tool-login').hidden = Boolean(session.authenticated);
        controls.forEach(id => { byId(id).disabled = !session.authenticated; });
    } catch (error) { byId('tool-session').textContent = 'Connexion indisponible. ' + error.message; }
}
async function generate(kind, operation, params = {}) {
    const button = byId(kind + '-generate');
    const status = byId(kind + '-status');
    const output = byId(kind + '-result');
    button.disabled = true;
    status.textContent = 'Génération en cours…';
    output.replaceChildren();
    let expired = false;
    try {
        const result = await OAPI.request(await OAPI.route(operation, params), 'POST');
        const url = OAPI.imageURL(result.url, location.origin);
        const image = document.createElement('img');
        image.alt = 'Carte de profil ' + (kind === 'discord' ? 'Discord' : 'Minecraft');
        image.addEventListener('error', () => { status.textContent = 'Carte générée, mais aperçu indisponible. Essayez le lien ci-dessous.'; });
        image.src = url;
        const link = document.createElement('a');
        link.href = url;
        link.textContent = 'Ouvrir l’image';
        output.append(image, document.createElement('br'), link);
        status.textContent = 'Votre carte est prête.';
    } catch (error) { status.textContent = error.message; expired = error.status === 401; sessionExpired(error); }
    finally { if (!expired) button.disabled = false; }
}
byId('discord-generate').addEventListener('click', () => generate('discord', 'create_my_discord_summary'));
byId('minecraft-form').addEventListener('submit', event => {
    event.preventDefault();
    const uuid = byId('minecraft-uuid').value.trim();
    if (!/^(?:[a-f0-9]{32}|[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})$/i.test(uuid)) {
        byId('minecraft-status').textContent = 'Saisissez un UUID Minecraft valide.';
        return;
    }
    generate('minecraft', 'create_minecraft_summary_by_id', { uuid });
});
byId('activity-load').addEventListener('click', async () => {
    const button = byId('activity-load');
    const status = byId('activity-status');
    button.disabled = true;
    byId('activity-summary').hidden = true;
    byId('activity-details').hidden = true;
    status.textContent = 'Chargement de votre activité…';
    let expired = false;
    try {
        const activity = await OAPI.request(await OAPI.route('personal'));
        if (!Number.isFinite(activity.total_messages) || !Number.isFinite(activity.total_vocal_hours)) throw new Error('Réponse d’activité invalide.');
        const summary = byId('activity-summary');
        summary.replaceChildren();
        for (const [label, value] of [
            ['Messages enregistrés', new Intl.NumberFormat('fr-FR').format(activity.total_messages)],
            ['Heures en vocal', new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 1 }).format(activity.total_vocal_hours)],
            ['Salon texte favori', activity.favorite_text_channel?.name || 'Aucun salon enregistré'],
            ['Salon vocal favori', activity.favorite_voice_channel?.name || 'Aucun salon enregistré']
        ]) {
            const box = document.createElement('div'); box.className = 'stat-box';
            const number = document.createElement('div'); number.className = 'stat-value'; number.textContent = value;
            const title = document.createElement('div'); title.className = 'stat-label'; title.textContent = label;
            box.append(number, title); summary.append(box);
        }
        summary.hidden = false;
        byId('activity-json').textContent = JSON.stringify(activity, null, 2);
        byId('activity-details').hidden = false;
        status.textContent = 'Activité sur l’ensemble de l’historique enregistré.' + (activity.daily_complete === false ? ' Certaines dates sont manquantes ; les totaux les incluent.' : '') + (activity.channel_counters_complete === false ? ' Les salons favoris reposent sur des compteurs incomplets.' : '');
    } catch (error) { status.textContent = error.message; expired = error.status === 401; sessionExpired(error); }
    finally { if (!expired) button.disabled = false; }
});
loadToolSession();
