'use strict';
i18n.init();   // langue choisie (ou celle de Windows) avant tout affichage

// ---------- Lien avec le script PowerShell (serveur local) ----------
const TOKEN = new URLSearchParams(location.search).get('t') || '';
let offline = false;

function fix(data) {
  // PowerShell peut renvoyer un objet seul au lieu d'une liste.
  return {
    games: [].concat(data.games || []), running: [].concat(data.running || []),
    version: data.version || '', update: data.update || null, capture: data.capture || null,
  };
}

async function call(name, body) {
  try {
    const r = await fetch('/api/' + name, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Token': TOKEN },
      body: JSON.stringify(body || {}),
    });
    const data = await r.json();
    offline = false;
    return data;
  } catch (e) {
    if (!offline) toast(t('Rift Launcher ne répond plus. Ferme cette fenêtre et relance Rift Launcher.'));
    offline = true;
    throw e;
  }
}

const api = {
  getLibrary: () => call('library').then(fix),
  saveGame: (g) => call('save', g).then((d) => (d.error ? d : fix(d))),
  deleteGame: (id) => call('delete', { id }).then((d) => (d.error ? d : fix(d))),
  play: (id) => call('play', { id }),
  stop: (id) => call('stop', { id }),
  backup: (id) => call('backup', { id }),
  openBackups: (id) => call('openBackups', { id }),
  openFolder: (id) => call('openFolder', { id }),
  openRepo: (id) => call('openRepo', { id }),
  pickExe: () => call('pick', { kind: 'exe' }).then((d) => d.path || null),
  pickDir: (kind) => call('pick', { kind: kind || 'dir' }).then((d) => d.path || null),
  listMods: (id) => call('mods', { id }),
  setMod: (id, name, enabled) => call('modSet', { id, name, enabled }),
  openMods: (id) => call('openMods', { id }),
  coverPick: (id) => call('coverPick', { id }),
  soundPick: (id) => call('soundPick', { id }),
  assetClear: (id, kind) => call('assetClear', { id, kind }),
  coverSearch: (term, sgdbId) => call('coverSearch', { term, sgdbId }),
  coverDownload: (id, url) => call('coverDownload', { id, url }),
  settings: () => call('settings'),
  settingsSave: (s) => call('settingsSave', s),
  openAssets: (kind) => call('openAssets', { kind }),
  prefsSave: (prefs) => call('prefsSave', { prefs }),
  report: () => call('report'),
  updateCheck: () => call('updateCheck'),
  updateInstall: () => call('updateInstall'),
  recompCheck: () => call('recompCheck'),
  recompDone: (id) => call('recompDone', { id }),
  recompOpen: (id) => call('recompOpen', { id }),
  toggleFocus: () => call('toggleFocus'),
  fullscreen: (on) => call('fullscreen', { on }),
  capture: () => call('capture'),
  openCaptures: (id) => call('openCaptures', { id }),
  openLogs: () => call('openLogs'),
  log: (level, message, stack) => call('log', { level, message, stack }).catch(() => {}),
  onChange: (fn) => setInterval(() => api.getLibrary().then(fn).catch(() => {}), 2000),
  onToast: () => {},
};
// Toute erreur de l'interface est écrite dans le journal de Rift Launcher.
addEventListener('error', (e) => api.log('ERROR', `${e.message} (${(e.filename || '').split('/').pop()}:${e.lineno}:${e.colno})`, e.error && e.error.stack));
addEventListener('unhandledrejection', (e) => {
  const r = e.reason || {};
  if (offline) return; // le serveur est arrêté : rien à écrire
  api.log('ERROR', `Promesse rejetée : ${r.message || r}`, r.stack);
});
addEventListener('pagehide', () => navigator.sendBeacon('/api/bye?t=' + encodeURIComponent(TOKEN), '{}'));

let state = { games: [], running: [] };
let editingId = null;

// ---------- Sons (synthétisés, aucun fichier audio) ----------
const sfx = (() => {
  // Direction sonore : petits clics doux (façon menus de console) et notes de « cloche »
  // feutrées. Tout est synthétisé, aucun fichier audio.
  let ctx = null;
  let master = null;
  let uiBus = null;     // sons de l'interface (clics, intro)
  let gameBus = null;   // sons des jeux (licences, sons perso)
  let bus = null;       // bus utilisé par les voix en ce moment
  let noiseBuf = null;
  let revBus = null;    // envoi vers la réverbération
  let introTheme = 'd';   // mélange B + C choisi par TeK_Sama
  let prefs = { volume: 80, uiVolume: 70, gameVolume: 100, hoverSounds: true };
  let muted = false;
  try { muted = localStorage.getItem('tek-muted') === '1'; } catch (e) { /* stockage indisponible */ }

  function ready() {
    if (!ctx) {
      const AC = window.AudioContext || window.webkitAudioContext;
      if (!AC) return null;
      ctx = new AC();
      master = ctx.createGain();
      // Filtre doux + léger compresseur : rien d'agressif dans les aigus.
      const soft = ctx.createBiquadFilter();
      soft.type = 'lowpass'; soft.frequency.value = 9000; soft.Q.value = 0.4;
      const comp = ctx.createDynamicsCompressor();
      comp.threshold.value = -16; comp.ratio.value = 3; comp.attack.value = 0.003; comp.release.value = 0.2;
      master.connect(soft); soft.connect(comp); comp.connect(ctx.destination);
      uiBus = ctx.createGain(); uiBus.connect(master);
      gameBus = ctx.createGain(); gameBus.connect(master);
      bus = uiBus;
      // Réverbération (réponse synthétique de ~2,4 s) : donne de l'espace à l'intro.
      const rev = ctx.createConvolver();
      const len = Math.floor(ctx.sampleRate * 2.4);
      const ir = ctx.createBuffer(2, len, ctx.sampleRate);
      for (let c = 0; c < 2; c++) {
        const d = ir.getChannelData(c);
        for (let i = 0; i < len; i++) d[i] = (Math.random() * 2 - 1) * Math.pow(1 - i / len, 3.2);
      }
      rev.buffer = ir;
      revBus = ctx.createGain(); revBus.gain.value = 0.35;
      revBus.connect(rev); rev.connect(uiBus);
      noiseBuf = ctx.createBuffer(1, Math.floor(ctx.sampleRate * 0.5), ctx.sampleRate);
      const d = noiseBuf.getChannelData(0);
      for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1;
      applyVolumes();
    }
    if (ctx.state === 'suspended') ctx.resume().catch(() => {});
    return ctx;
  }

  function applyVolumes() {
    if (!master) return;
    master.gain.value = 0.95 * (prefs.volume / 100);
    uiBus.gain.value = prefs.uiVolume / 100;
    gameBus.gain.value = prefs.gameVolume / 100;
  }

  // Note pure (sinus / triangle) avec enveloppe douce.
  function tone({ f = 600, f2 = 0, type = 'sine', t = 0, dur = 0.15, vol = 0.3, attack = 0.006 }) {
    if (muted) return;
    const c = ready();
    if (!c) return;
    const now = c.currentTime + t;
    const o = c.createOscillator();
    const g = c.createGain();
    o.type = type === 'square' || type === 'sawtooth' ? 'triangle' : type;   // jamais de son « 8 bits »
    o.frequency.setValueAtTime(f, now);
    if (f2) o.frequency.exponentialRampToValueAtTime(f2, now + dur * 0.7);
    g.gain.setValueAtTime(0.0001, now);
    g.gain.exponentialRampToValueAtTime(vol, now + attack);
    g.gain.exponentialRampToValueAtTime(0.0001, now + dur);
    o.connect(g);
    g.connect(bus);
    o.start(now);
    o.stop(now + dur + 0.03);
  }

  // « Cloche » feutrée : fondamentale + harmoniques qui s'éteignent vite (piano / célesta doux).
  function bell({ f, t = 0, dur = 0.9, vol = 0.18, wet = 0.2 }) {
    if (muted) return;
    const c = ready();
    if (!c) return;
    const now = c.currentTime + t;
    [[1, 1, 1], [2, 0.28, 0.55], [3, 0.08, 0.35], [4.02, 0.04, 0.25]].forEach(([mult, amp, len]) => {
      const o = c.createOscillator();
      const g = c.createGain();
      o.type = 'sine';
      o.frequency.setValueAtTime(f * mult, now);
      g.gain.setValueAtTime(0.0001, now);
      g.gain.exponentialRampToValueAtTime(vol * amp, now + 0.006);
      g.gain.exponentialRampToValueAtTime(0.0001, now + dur * len);
      o.connect(g); out(g, wet);
      o.start(now); o.stop(now + dur * len + 0.05);
    });
  }

  // Clic : bruit très court filtré + petit corps tonal (comme un bouton physique).
  function click({ t = 0, f = 3200, q = 2.2, body = 0, vol = 0.22, dur = 0.022 } = {}) {
    if (muted) return;
    const c = ready();
    if (!c) return;
    const now = c.currentTime + t;
    const src = c.createBufferSource();
    src.buffer = noiseBuf;
    const bp = c.createBiquadFilter();
    bp.type = 'bandpass'; bp.frequency.value = f; bp.Q.value = q;
    const g = c.createGain();
    g.gain.setValueAtTime(0.0001, now);
    g.gain.exponentialRampToValueAtTime(vol * 2.6, now + 0.0015);   // le filtre étroit mange le volume
    g.gain.exponentialRampToValueAtTime(0.0001, now + dur);
    src.connect(bp); bp.connect(g); g.connect(bus);
    src.start(now, Math.random() * 0.3); src.stop(now + dur + 0.02);
    if (body) tone({ f: body, f2: body * 0.82, t, dur: dur * 2.2, vol: vol * 0.5, attack: 0.002 });
  }

  // Souffle filtré : l'ouverture de la faille.
  function whoosh({ t = 0, dur = 0.9, from = 250, to = 2600, vol = 0.22 } = {}) {
    if (muted) return;
    const c = ready();
    if (!c) return;
    const now = c.currentTime + t;
    const src = c.createBufferSource();
    src.buffer = noiseBuf;
    src.loop = true;
    const bp = c.createBiquadFilter();
    bp.type = 'bandpass';
    bp.Q.value = 1.1;
    bp.frequency.setValueAtTime(from, now);
    bp.frequency.exponentialRampToValueAtTime(to, now + dur * 0.75);
    const g = c.createGain();
    g.gain.setValueAtTime(0.0001, now);
    g.gain.exponentialRampToValueAtTime(vol, now + dur * 0.4);
    g.gain.exponentialRampToValueAtTime(0.0001, now + dur);
    src.connect(bp); bp.connect(g); g.connect(bus);
    src.start(now);
    src.stop(now + dur + 0.05);
  }

  // Nappe chaude (accord tenu) sous la faille.
  function pad(freqs, { t = 0, dur = 2.4, vol = 0.05 } = {}) {
    if (muted) return;
    const c = ready();
    if (!c) return;
    const now = c.currentTime + t;
    freqs.forEach((f, i) => {
      const o = c.createOscillator();
      const g = c.createGain();
      o.type = 'sine';
      o.frequency.setValueAtTime(f, now);
      o.detune.setValueAtTime(i % 2 ? 5 : -5, now);
      g.gain.setValueAtTime(0.0001, now);
      g.gain.exponentialRampToValueAtTime(vol, now + dur * 0.35);
      g.gain.exponentialRampToValueAtTime(0.0001, now + dur);
      o.connect(g); g.connect(bus);
      o.start(now); o.stop(now + dur + 0.05);
    });
  }


  // Envoie une voix vers le bus courant, avec un peu de réverbération si wet > 0.
  function out(node, wet) {
    node.connect(bus);
    if (wet && revBus) { const s = ctx.createGain(); s.gain.value = wet; node.connect(s); s.connect(revBus); }
  }

  // Piano électrique (FM douce, façon Rhodes).
  function epiano({ f, t = 0, dur = 1.6, vol = 0.12, wet = 0.3 }) {
    if (muted) return;
    const c = ready();
    if (!c) return;
    const now = c.currentTime + t;
    const car = c.createOscillator(), mod = c.createOscillator();
    const mg = c.createGain(), g = c.createGain();
    car.type = 'sine'; mod.type = 'sine';
    car.frequency.setValueAtTime(f, now);
    mod.frequency.setValueAtTime(f, now);
    mg.gain.setValueAtTime(f * 1.8, now);
    mg.gain.exponentialRampToValueAtTime(f * 0.05, now + 0.5);
    mod.connect(mg); mg.connect(car.frequency);
    g.gain.setValueAtTime(0.0001, now);
    g.gain.exponentialRampToValueAtTime(vol, now + 0.008);
    g.gain.exponentialRampToValueAtTime(vol * 0.35, now + 0.4);
    g.gain.exponentialRampToValueAtTime(0.0001, now + dur);
    car.connect(g); out(g, wet);
    car.start(now); mod.start(now); car.stop(now + dur + 0.05); mod.stop(now + dur + 0.05);
  }

  // Cloche métallique (partiels non harmoniques) pour l'ambiance « faille ».
  function metal({ f, t = 0, dur = 1.8, vol = 0.1, wet = 0.45 }) {
    if (muted) return;
    const c = ready();
    if (!c) return;
    const now = c.currentTime + t;
    [[1, 1, 1], [2.76, 0.45, 0.6], [5.4, 0.22, 0.35], [8.93, 0.1, 0.2]].forEach(([m, a, l]) => {
      const o = c.createOscillator(), g = c.createGain();
      o.type = 'sine'; o.frequency.setValueAtTime(f * m, now);
      g.gain.setValueAtTime(0.0001, now);
      g.gain.exponentialRampToValueAtTime(vol * a, now + 0.004);
      g.gain.exponentialRampToValueAtTime(0.0001, now + dur * l);
      o.connect(g); out(g, wet);
      o.start(now); o.stop(now + dur * l + 0.05);
    });
  }

  // Impact grave (« boum » de cinéma) : sinus qui plonge + souffle étouffé.
  function boom({ t = 0, vol = 0.5, wet = 0.5 } = {}) {
    if (muted) return;
    const c = ready();
    if (!c) return;
    const now = c.currentTime + t;
    const o = c.createOscillator(), g = c.createGain();
    o.type = 'sine';
    o.frequency.setValueAtTime(95, now);
    o.frequency.exponentialRampToValueAtTime(32, now + 1.2);
    g.gain.setValueAtTime(0.0001, now);
    g.gain.exponentialRampToValueAtTime(vol, now + 0.01);
    g.gain.exponentialRampToValueAtTime(0.0001, now + 1.8);
    o.connect(g); out(g, wet);
    o.start(now); o.stop(now + 1.9);
    const src = c.createBufferSource(); src.buffer = noiseBuf;
    const lp = c.createBiquadFilter(); lp.type = 'lowpass'; lp.frequency.value = 700;
    const ng = c.createGain();
    ng.gain.setValueAtTime(0.0001, now);
    ng.gain.exponentialRampToValueAtTime(vol * 0.5, now + 0.005);
    ng.gain.exponentialRampToValueAtTime(0.0001, now + 0.5);
    src.connect(lp); lp.connect(ng); out(ng, wet);
    src.start(now); src.stop(now + 0.55);
  }

  // Nappe de cuivres doux (scie filtrée qui s'ouvre lentement) : accord final épique.
  function brass(freqs, { t = 0, dur = 2.6, vol = 0.05, wet = 0.5 } = {}) {
    if (muted) return;
    const c = ready();
    if (!c) return;
    const now = c.currentTime + t;
    const lp = c.createBiquadFilter(); lp.type = 'lowpass'; lp.Q.value = 0.7;
    lp.frequency.setValueAtTime(300, now);
    lp.frequency.exponentialRampToValueAtTime(2400, now + dur * 0.45);
    lp.frequency.exponentialRampToValueAtTime(600, now + dur);
    const g = c.createGain();
    g.gain.setValueAtTime(0.0001, now);
    g.gain.exponentialRampToValueAtTime(vol, now + dur * 0.3);
    g.gain.exponentialRampToValueAtTime(0.0001, now + dur);
    lp.connect(g); out(g, wet);
    freqs.forEach((f, i) => {
      [-7, 7].forEach((det) => {
        const o = c.createOscillator();
        o.type = 'sawtooth'; o.frequency.setValueAtTime(f, now); o.detune.setValueAtTime(det + i, now);
        o.connect(lp); o.start(now); o.stop(now + dur + 0.05);
      });
    });
  }

  // Montée de souffle avant l'ouverture (« riser »).
  function riser({ t = 0, dur = 1.3, vol = 0.14 } = {}) {
    whoosh({ t, dur, from: 150, to: 6000, vol });
    tone({ f: 110, f2: 440, t, dur, vol: vol * 0.4, attack: dur * 0.8 });
  }

  // ---------------- Les intros : a = douce (console), b = chill (piano électrique), c = épique (faille), d = mélange B + C (celle utilisée)
  const IN = {
    a: {
      type: () => click({ f: 2400 + Math.random() * 600, q: 1.6, vol: 0.12, dur: 0.018 }),
      pop: (i) => { click({ f: 1600 + i * 90, q: 1.4, vol: 0.16, body: 480 + i * 22, dur: 0.03 }); bell({ f: 1760 + i * 110, dur: 0.35, vol: 0.025 }); },
      sig: () => { bell({ f: 880, dur: 1.6, vol: 0.12 }); bell({ f: 1174.66, t: 0.16, dur: 2.2, vol: 0.12 }); },
      rift: () => { pad([146.83, 220, 329.63, 369.99], { dur: 3, vol: 0.04 }); whoosh({ dur: 1.4, from: 200, to: 2200, vol: 0.1 }); },
      name: (i) => bell({ f: [2349.32, 2637.02, 2959.96, 3520][i % 4], dur: 0.5, vol: 0.035 }),
      end: () => [587.33, 739.99, 880, 1318.51].forEach((f, i) => bell({ f, t: i * 0.03, dur: 2.6, vol: 0.08 })),
    },
    b: {
      type: () => click({ f: 1800 + Math.random() * 400, q: 1.2, vol: 0.1, dur: 0.02 }),
      pop: (i) => epiano({ f: [698.46, 880, 1046.5, 1174.66, 1046.5, 880, 783.99, 698.46][i % 8], dur: 1.4, vol: 0.1 }),
      sig: () => [174.61, 220, 329.63, 392, 523.25].forEach((f, i) => epiano({ f, t: i * 0.04, dur: 2.4, vol: 0.07 })),
      rift: () => { tone({ f: 87.31, f2: 65.41, dur: 2.2, vol: 0.18, attack: 0.05 }); whoosh({ dur: 1.2, from: 150, to: 900, vol: 0.08 }); },
      name: (i) => epiano({ f: [1046.5, 1396.91, 1760, 2093][i % 4], dur: 1, vol: 0.06 }),
      end: () => [233.08, 293.66, 349.23, 440, 523.25, 587.33].forEach((f, i) => epiano({ f, t: i * 0.05, dur: 2.8, vol: 0.06 })),
    },
    c: {
      type: () => click({ f: 1300 + Math.random() * 300, q: 1.2, vol: 0.14, body: 220, dur: 0.03 }),
      pop: (i) => { metal({ f: [293.66, 349.23, 440, 587.33, 659.25, 587.33, 440, 349.23][i % 8], dur: 1.6, vol: 0.07 }); if (i === 0 || i === 4) boom({ vol: 0.22 }); },
      sig: () => riser({ dur: 1.3 }),
      rift: () => { boom({ vol: 0.55 }); pad([73.42, 110, 146.83], { dur: 3.2, vol: 0.05 }); },
      name: (i) => metal({ f: [880, 1174.66, 1318.51, 1760][i % 4], dur: 1.2, vol: 0.045 }),
      end: () => { brass([146.83, 220, 293.66, 349.23, 440], { dur: 3, vol: 0.045 }); metal({ f: 587.33, dur: 2.4, vol: 0.06 }); },
    },
    // d = mélange B + C : le piano électrique de la B, porté par les impacts et les cuivres de la C.
    d: {
      type: () => click({ f: 1500 + Math.random() * 350, q: 1.2, vol: 0.12, body: 260, dur: 0.026 }),
      pop: (i) => {
        epiano({ f: [698.46, 880, 1046.5, 1174.66, 1046.5, 880, 783.99, 698.46][i % 8], dur: 1.5, vol: 0.1, wet: 0.4 });
        metal({ f: [349.23, 440, 523.25, 587.33, 523.25, 440, 392, 349.23][i % 8], dur: 1.2, vol: 0.025 });
        if (i === 0 || i === 4) boom({ vol: 0.16, wet: 0.35 });   // « TeK » et « Sama » frappent
      },
      sig: () => {
        [174.61, 220, 329.63, 392, 523.25].forEach((f, i) => epiano({ f, t: i * 0.04, dur: 2.4, vol: 0.065, wet: 0.4 }));
        riser({ t: 0.35, dur: 1.0, vol: 0.12 });
      },
      rift: () => {
        boom({ vol: 0.5 });
        tone({ f: 87.31, f2: 65.41, dur: 2.4, vol: 0.14, attack: 0.05 });
        pad([87.31, 130.81, 174.61], { dur: 3, vol: 0.04 });
      },
      name: (i) => { epiano({ f: [1046.5, 1396.91, 1760, 2093][i % 4], dur: 1, vol: 0.055 }); metal({ f: [1046.5, 1396.91, 1760, 2093][i % 4], dur: 0.9, vol: 0.02 }); },
      end: () => {
        [233.08, 293.66, 349.23, 440, 523.25, 587.33].forEach((f, i) => epiano({ f, t: i * 0.05, dur: 3, vol: 0.055, wet: 0.45 }));
        brass([116.54, 174.61, 233.08, 293.66, 349.23], { dur: 3.2, vol: 0.035 });
      },
    },
  };
  const intro = (k, ...a) => { const th = IN[introTheme] || IN.d; th[k](...a); };

  // Notes utiles (ré majeur) pour les sons de l'interface.
  const N = { D5: 587.33, E5: 659.25, Fs5: 739.99, G5: 783.99, A5: 880, B5: 987.77, Cs6: 1108.73, D6: 1174.66, E6: 1318.51, Fs6: 1479.98, A6: 1760 };

  const fileCache = new Map();
  let current = null;

  return {
    get muted() { return muted; },
    toggle() {
      muted = !muted;
      try { localStorage.setItem('tek-muted', muted ? '1' : '0'); } catch (e) { /* rien */ }
      if (!muted) this.select();
      return muted;
    },
    unlock: ready,
    // Intro (3 ambiances au choix)
    setIntroTheme(t) { if (IN[t]) introTheme = t; },
    get introThemes() { return Object.keys(IN); },
    type() { intro('type'); },
    pop(i) { intro('pop', i); },
    introSig() { intro('sig'); },
    rift() { intro('rift'); },
    name(i) { intro('name', i); },
    introEnd() { intro('end'); },
    // Petit carillon (annonces : mise à jour dispo, nouvelle version d'un portage…).
    sparkle(t0 = 0) {
      [N.D5, N.Fs5, N.A5, N.D6].forEach((f, i) => bell({ f, t: t0 + i * 0.045, dur: 1.6, vol: 0.09 }));
    },
    // Petits sons de licence (sélection = court, lancement = plus riche), en notes douces.
    franchise(key, kind) {
      const big = kind === 'launch';
      const SOUNDS = {
        zelda: () => {            // fée scintillante
          const notes = big ? [1568, 2093, 2349, 2637, 3136, 2637, 3136, 3520] : [2093, 2637, 3136];
          notes.forEach((n, i) => bell({ f: n, t: i * 0.055, dur: 0.5, vol: 0.06 }));
        },
        mario: () => {            // « boing » rebondissant
          tone({ f: 260, f2: 720, dur: 0.16, vol: 0.12 });
          if (big) { tone({ f: 330, f2: 900, t: 0.18, dur: 0.16, vol: 0.12 }); bell({ f: 990, t: 0.36, dur: 0.5, vol: 0.1 }); }
        },
        mariokart: () => {        // moteur qui monte
          tone({ f: 90, f2: big ? 420 : 220, dur: big ? 0.8 : 0.3, vol: 0.12 });
          tone({ f: 92, f2: big ? 430 : 225, type: 'triangle', dur: big ? 0.8 : 0.3, vol: 0.06 });
        },
        drmario: () => { [880, 1175, big ? 1568 : 0].forEach((n, i) => n && bell({ f: n, t: i * 0.07, dur: 0.4, vol: 0.1 })); },
        splatoon: () => {         // « splotch » d'encre
          whoosh({ dur: big ? 0.45 : 0.25, from: 1800, to: 250, vol: 0.22 });
          tone({ f: 420, f2: 110, dur: big ? 0.35 : 0.2, vol: 0.22 });
          if (big) tone({ f: 380, f2: 95, t: 0.22, dur: 0.3, vol: 0.18 });
        },
        metroid: () => {          // nappe spatiale sombre
          pad([110, 164.8], { dur: big ? 1.4 : 0.7, vol: 0.06 });
        },
        kirby: () => { [784, 988, 1175, big ? 1568 : 0].forEach((n, i) => n && bell({ f: n, t: i * 0.08, dur: 0.45, vol: 0.12 })); },
        starfox: () => {          // tir laser (adouci)
          tone({ f: 2000, f2: 400, dur: 0.14, vol: 0.08 });
          if (big) { tone({ f: 2000, f2: 400, t: 0.16, dur: 0.14, vol: 0.08 }); whoosh({ t: 0.3, dur: 0.6, from: 400, to: 3000, vol: 0.1 }); }
        },
        sonic: () => { tone({ f: 300, f2: big ? 2600 : 1600, type: 'triangle', dur: big ? 0.45 : 0.22, vol: 0.12 }); },
        banjo: () => { [392, 494, 587, big ? 784 : 0].forEach((n, i) => n && bell({ f: n, t: i * 0.09, dur: 0.35, vol: 0.14 })); },
        perfectdark: () => { click({ f: 1800, vol: 0.2 }); tone({ f: 150, f2: 60, t: 0.04, dur: big ? 0.4 : 0.15, vol: 0.2 }); },
        goemon: () => { [0, 0.12, big ? 0.24 : -1].forEach((d) => d >= 0 && whoosh({ t: d, dur: 0.07, from: 2500, to: 1800, vol: 0.3 })); },
      };
      const fn = SOUNDS[key];
      if (!fn) return false;
      if (!ready()) return true;
      bus = gameBus;            // sons de licence = volume « jeux »
      try { fn(); } finally { bus = uiBus; }
      return true;
    },
    // Son personnalisé (fichier) : décodé une fois puis gardé en mémoire.
    async file(url) {
      if (muted || !url) return;
      const c = ready();
      if (!c) return;
      try {
        if (!fileCache.has(url)) {
          const buf = await (await fetch(url)).arrayBuffer();
          fileCache.set(url, await c.decodeAudioData(buf));
        }
        if (current) { try { current.stop(); } catch (e) { /* déjà fini */ } }
        const src = c.createBufferSource();
        src.buffer = fileCache.get(url);
        src.connect(gameBus);
        src.start();
        current = src;
      } catch (e) { /* fichier illisible : on reste silencieux */ }
    },
    // Interface : clics de menu.
    setPrefs(p) { prefs = { ...prefs, ...p }; applyVolumes(); },
    hover() { if (!prefs.hoverSounds) return; click({ f: 3400, q: 3, vol: 0.13, dur: 0.016 }); },
    move() { if (!prefs.hoverSounds) return; click({ f: 3000, q: 2.6, vol: 0.16, dur: 0.018 }); },
    select() { click({ f: 2600, q: 2, vol: 0.2, body: 1250, dur: 0.024 }); },
    back() { click({ f: 1900, q: 2, vol: 0.18, body: 820, dur: 0.026 }); },
    modOn() { click({ f: 2800, vol: 0.18, body: 1400 }); click({ t: 0.06, f: 3600, vol: 0.12, dur: 0.016 }); },
    modOff() { click({ f: 2200, vol: 0.18, body: 900 }); },
    launch() {
      // Lancement : petit accord qui monte, comme l'ouverture d'un jeu sur console.
      click({ f: 2600, vol: 0.18, body: 1250 });
      [N.D5, N.A5, N.D6, N.Fs6].forEach((f, i) => bell({ f, t: 0.05 + i * 0.07, dur: 1.2, vol: 0.12 }));
      whoosh({ t: 0.05, dur: 0.6, from: 600, to: 3500, vol: 0.06 });
    },
    stop() { click({ f: 2000, vol: 0.18, body: 760 }); bell({ f: N.A5, t: 0.04, dur: 0.6, vol: 0.09 }); bell({ f: N.D5, t: 0.13, dur: 0.8, vol: 0.09 }); },
    save() { click({ f: 2800, vol: 0.16, body: 1300 }); bell({ f: N.A5, t: 0.05, dur: 0.7, vol: 0.1 }); bell({ f: N.E6, t: 0.12, dur: 0.9, vol: 0.08 }); },
    error() { click({ f: 1500, q: 1.4, vol: 0.2, body: 300, dur: 0.04 }); click({ t: 0.1, f: 1300, q: 1.4, vol: 0.18, body: 240, dur: 0.05 }); },
  };
})();

// ---------- Outils ----------
const ICONS = {
  play: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z"/></svg>',
  stop: '<svg viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2"/></svg>',
  save: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v12"/><path d="m7 10 5 5 5-5"/><path d="M5 21h14"/></svg>',
  folder: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg>',
  update: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-3-6.7"/><path d="M21 4v5h-5"/></svg>',
  sound: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5 6 9H3v6h3l5 4z"/><path d="M15.5 8.5a5 5 0 0 1 0 7"/><path d="M18.5 5.5a9 9 0 0 1 0 13"/></svg>',
  mute: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5 6 9H3v6h3l5 4z"/><path d="m16 9 6 6"/><path d="m22 9-6 6"/></svg>',
  mods: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M10 3h4v3a2 2 0 1 0 4 0h0V3h3v7h-3a2 2 0 1 0 0 4h3v7h-7v-3a2 2 0 1 0-4 0v3H3v-7h3a2 2 0 1 0 0-4H3V3z"/></svg>',
  gear: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/></svg>',
  camera: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 8h3l2-3h6l2 3h3a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z"/><circle cx="12" cy="13" r="3.5"/></svg>',
  big: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="6" width="20" height="13" rx="6.5"/><path d="M7 10.5v4M5 12.5h4"/><circle cx="16" cy="11" r="1" fill="currentColor"/><circle cx="18" cy="14" r="1" fill="currentColor"/></svg>',
};

function h(tag, props = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === 'class') el.className = v;
    else if (k === 'icon') el.insertAdjacentHTML('afterbegin', ICONS[v]); // icônes constantes uniquement
    else if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'style') el.style.cssText = v;
    else if (k === 'title' || k === 'aria-label' || k === 'placeholder') el.setAttribute(k, t(v));
    else el.setAttribute(k, v);
  }
  // Les textes fixes de l'interface sont traduits automatiquement (voir i18n.js).
  for (const c of children.flat()) if (c != null && c !== false) el.append(typeof c === 'string' ? t(c) : c);
  return el;
}

function hue(text) {
  let n = 0;
  for (const ch of text) n = (n * 31 + ch.codePointAt(0)) >>> 0;
  return n % 360;
}

function initials(name) {
  const small = new Set(['the', 'of', 'and', 'de', 'du', 'des', 'la', 'le', 'les', 'et']);
  const words = name.replace(/[^\p{L}\p{N} ]/gu, ' ').split(/\s+/)
    .filter((w) => w && !small.has(w.toLowerCase()) && (w.length > 1 || /\d/.test(w)));
  return (words.length ? words : [name]).slice(-2).map((w) => w[0]).join('').toUpperCase();
}

function formatTime(seconds) {
  if (seconds < 60) return seconds > 0 ? t('< 1 min') : t('{n} min', { n: 0 });
  const hrs = Math.floor(seconds / 3600);
  const min = Math.floor((seconds % 3600) / 60);
  return hrs ? t('{h} h {m}', { h: hrs, m: String(min).padStart(2, '0') }) : t('{n} min', { n: min });
}

function formatAgo(ts) {
  if (!ts) return t('Jamais');
  const s = Math.round((Date.now() - ts) / 1000);
  if (s < 90) return t("À l'instant");
  if (s < 3600) return t('Il y a {n} min', { n: Math.round(s / 60) });
  if (s < 86400) return t('Il y a {n} h', { n: Math.round(s / 3600) });
  const d = Math.round(s / 86400);
  if (d === 1) return t('Hier');
  if (d < 30) return t('Il y a {n} j', { n: d });
  return new Date(ts).toLocaleDateString(i18n.locale);
}

let toastTimer = null;
function toast(msg, kind) {
  if (kind === 'error') sfx.error();
  const el = document.getElementById('toast');
  el.textContent = tMsg(String(msg));
  el.classList.toggle('is-error', kind === 'error');
  el.classList.remove('leaving');
  el.hidden = false;
  el.style.animation = 'none'; void el.offsetWidth; el.style.animation = '';   // rejoue l'entrée
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    el.classList.add('leaving');
    toastTimer = setTimeout(() => { el.hidden = true; el.classList.remove('leaving'); }, 260);
  }, 4200);
}

// ---------- Actions ----------
async function playOrStop(game) {
  if (state.running.includes(game.id)) {
    sfx.stop();
    await api.stop(game.id);
    return;
  }
  const r = await api.play(game.id);
  if (r && r.error) toast(r.error, 'error');
  else gameSound(game, 'launch');
}

// Son d'un jeu : fichier perso / dossier « sons », sinon son intégré de sa licence.
const lastPick = {};
function gameSound(game, kind) {
  const urls = [].concat(game.soundUrls || (game.soundUrl ? [game.soundUrl] : []));
  if (urls.length) {
    // Au hasard, sans rejouer deux fois de suite le même son.
    let i = Math.floor(Math.random() * urls.length);
    if (urls.length > 1 && i === lastPick[game.id]) i = (i + 1 + Math.floor(Math.random() * (urls.length - 1))) % urls.length;
    lastPick[game.id] = i;
    sfx.file(urls[i]);
    return;
  }
  if (sfx.franchise(game.franchiseKey, kind)) return;
  if (kind === 'launch') sfx.launch(); else sfx.select();
}

async function doBackup(game) {
  const r = await api.backup(game.id);
  if (r.error) toast(r.error, 'error');
  else { sfx.save(); toast(t('Sauvegarde de {name} faite.', { name: game.name })); }
}

function playButton(game) {
  const live = state.running.includes(game.id);
  return h('button', {
    'data-nav': 'hero-play',
    class: `btn btn-play ${live ? 'btn-stop' : 'btn-primary'}`,
    icon: live ? 'stop' : 'play',
    onclick: () => playOrStop(game),
  }, live ? 'Arrêter' : 'Jouer');
}

// compact = version des cartes (moins de place) : sans « dossier » ni « mises à jour ».
function toolButtons(game, compact) {
  const btns = [
    h('button', { class: 'icon-btn', icon: 'save', title: 'Sauvegarder les saves (clic droit : ouvrir les copies)',
      onclick: () => doBackup(game), oncontextmenu: (e) => { e.preventDefault(); api.openBackups(game.id); } }),
    compact ? null : h('button', { class: 'icon-btn', icon: 'folder', title: 'Ouvrir le dossier du jeu', onclick: () => { sfx.select(); api.openFolder(game.id); } }),
  ];
  if (game.modsDir) btns.push(h('button', { class: 'icon-btn', icon: 'mods', title: 'Mods', onclick: () => openMods(game) }));
  if (game.repoUrl && !compact) btns.push(h('button', { class: 'icon-btn', icon: 'update', title: 'Voir les mises à jour du portage', onclick: () => { sfx.select(); api.openRepo(game.id); } }));
  if (!compact) btns.push(h('button', { class: 'icon-btn', icon: 'camera', title: "Captures d'écran de ce jeu (bouton capture de la manette)", onclick: () => { sfx.select(); api.openCaptures(game.id); } }));
  btns.push(h('button', { class: 'icon-btn', icon: 'gear', title: 'Paramètres', onclick: () => openEditor(game) }));
  return btns.filter(Boolean).map((b, i) => { b.dataset.nav = `tool:${i}`; return b; });
}

// ---------- Rendu ----------
let selectedId = null;
try { selectedId = localStorage.getItem('rift-selected'); } catch (e) { /* rien */ }
let lastHover = null;
document.getElementById('grid').addEventListener('mouseleave', () => { lastHover = null; });

function selectedGame() {
  const byId = state.games.find((g) => g.id === selectedId);
  if (byId) return byId;
  return [...state.games].sort((a, b) => (b.lastPlayed || 0) - (a.lastPlayed || 0))[0] || null;
}

function selectGame(game) {
  if (selectedId === game.id) return;
  selectedId = game.id;
  try { localStorage.setItem('rift-selected', game.id); } catch (e) { /* rien */ }
  gameSound(game, 'select');
  render();
}

// Affiche portrait : jaquette, ou affiche générée (couleur + titre) s'il n'y en a pas.
function posterArt(game, cls) {
  if (game.coverUrl) return h('img', { class: cls, src: game.coverUrl, alt: '', loading: 'lazy', draggable: 'false' });
  return h('div', { class: `${cls} gen`, style: `--hue:${hue(game.name)}` },
    h('span', { class: 'gen-initials' }, initials(game.name)),
    h('span', { class: 'gen-title' }, game.name));
}

let heroShownId = null;
function renderHero() {
  const hero = document.getElementById('hero');
  const g = selectedGame();
  hero.replaceChildren();
  if (!g) { hero.hidden = true; return; }
  const live = state.running.includes(g.id);
  hero.hidden = false;
  hero.style.setProperty('--hue', hue(g.name));
  // Petite animation seulement quand on change de jeu (pas à chaque rafraîchissement).
  if (heroShownId !== null && heroShownId !== g.id) { hero.classList.remove('swap'); void hero.offsetWidth; hero.classList.add('swap'); }
  heroShownId = g.id;
  const bg = h('div', { class: 'hero-bg' });
  if (g.coverUrl) bg.style.backgroundImage = `url("${g.coverUrl}")`;
  const side = g.coverUrl ? h('div', { class: 'hero-side', style: `background-image:url("${g.coverUrl}")`, 'aria-hidden': 'true' }) : null;
  const eyebrow = [t(live ? 'En cours' : g.lastPlayed ? 'Reprendre' : 'Sélectionné'), g.franchiseLabel].filter(Boolean).join(' · ');
  const stat = (label, value, title) => h('span', { class: 'stat-chip', title: title || '' }, h('b', {}, value), label ? t(label) : '');
  hero.append(
    bg,
    side,
    h('div', { class: 'hero-poster' }, posterArt(g, 'hero-art'), live ? h('span', { class: 'hero-live' }, 'En cours') : null),
    h('div', { class: 'hero-info' },
      h('p', { class: 'eyebrow' }, live ? h('span', { class: 'live-dot', 'aria-hidden': 'true' }) : null, document.createTextNode(eyebrow)),
      h('h2', {}, document.createTextNode(g.name)),
      h('div', { class: 'hero-stats' },
        stat('de jeu', formatTime(g.playSeconds), g.playSource === 'jeu' ? 'Temps compté par le jeu lui-même (seulement quand sa fenêtre est active).' : 'Temps compté par Rift Launcher.'),
        stat('', formatAgo(g.lastPlayed), 'Dernière partie'),
        g.sessions ? stat(g.sessions > 1 ? 'sessions' : 'session', String(g.sessions)) : null,
        g.version ? stat('', 'v' + g.version, 'Version du portage') : null),
      h('div', { class: 'hero-actions' },
        g.exePath ? playButton(g) : h('button', { class: 'btn btn-play btn-primary', 'data-nav': 'hero-play', onclick: () => openEditor(g) }, 'Configurer'),
        h('div', { class: 'tool-group' }, ...toolButtons(g))),
      recompNote(g)),
  );
}

function renderPoster(game) {
  const live = state.running.includes(game.id);
  const ready = Boolean(game.exePath);
  const badge = live ? h('span', { class: 'pill live' }, 'En cours')
    : !ready ? h('span', { class: 'pill warn' }, 'À configurer')
    : game.recomp && game.recomp.isNew ? h('span', { class: 'pill new', title: t('Version {v} disponible', { v: game.recomp.latest }) }, 'Mise à jour') : null;
  return h('article', {
    class: `poster${live ? ' is-running' : ''}${game.id === (selectedGame() || {}).id ? ' is-selected' : ''}`,
    tabindex: '0',
    'data-nav': `poster:${game.id}`,
    'data-id': game.id,
    title: `${game.name}\n${t('Clic : sélectionner · Double-clic : jouer')}`,
    onfocus: () => { if (pad.active) selectGame(game); },
    onmouseenter: () => { if (lastHover !== game.id) { lastHover = game.id; sfx.hover(); } },
    onclick: () => selectGame(game),
    ondblclick: () => (ready ? playOrStop(game) : openEditor(game)),
    onkeydown: (e) => { if (e.key === 'Enter') { if (ready) playOrStop(game); else openEditor(game); } },
  },
    h('div', { class: 'poster-frame' },
      posterArt(game, 'poster-art'),
      badge,
      h('button', {
        class: 'poster-play', icon: live ? 'stop' : 'play',
        title: live ? 'Arrêter' : ready ? 'Jouer' : 'Configurer',
        'aria-label': live ? 'Arrêter' : 'Jouer',
        onclick: (e) => { e.stopPropagation(); if (ready || live) playOrStop(game); else openEditor(game); },
      })),
    h('div', { class: 'poster-meta' },
      h('strong', {}, document.createTextNode(game.name)),
      h('span', {}, document.createTextNode([formatTime(game.playSeconds), game.franchiseLabel].filter(Boolean).join(' · ')))));
}

// ---------- Nouvelles versions des portages (recomps) ----------
function recompNote(g) {
  const r = g.recomp;
  if (!r) return null;
  const when = r.date && Date.parse(r.date) ? ' · ' + t('sortie {ago}', { ago: formatAgo(Date.parse(r.date)).toLowerCase() }) : '';
  if (r.isNew) {
    return h('div', { class: 'recomp-note is-new' },
      h('span', { class: 'recomp-dot', 'aria-hidden': 'true' }),
      h('div', { class: 'recomp-text' },
        h('strong', {}, t('Nouvelle version du portage : {v}', { v: r.latest })),
        h('span', {}, t('Tu as la v{v}', { v: g.version }) + when)),
      h('div', { class: 'recomp-btns' },
        h('button', { class: 'btn btn-small btn-primary', 'data-nav': 'recomp-open', onclick: () => { sfx.select(); api.recompOpen(g.id); } }, 'Voir la mise à jour'),
        h('button', { class: 'btn btn-small btn-ghost', 'data-nav': 'recomp-done', title: t('La version du jeu passe à {v}', { v: r.latest }), onclick: () => markRecompDone(g) }, "C'est fait, je l'ai")));
  }
  if (!g.version) {
    return h('div', { class: 'recomp-note' },
      h('div', { class: 'recomp-text' },
        h('strong', {}, t('Dernière version du portage : {v}', { v: r.latest }) + when),
        h('span', {}, 'Indique ta version pour être prévenu des prochaines.')),
      h('div', { class: 'recomp-btns' },
        h('button', { class: 'btn btn-small btn-ghost', 'data-nav': 'recomp-done', onclick: () => markRecompDone(g) }, "J'ai celle-là")));
  }
  return null;
}

async function markRecompDone(g) {
  const r = await api.recompDone(g.id);
  if (r.error) { toast(r.error, 'error'); return; }
  sfx.save();
  await refreshLibrary();
}

// Nouvelle capture d'écran (bouton capture de la manette).
let captureSeen = null;
function announceCapture() {
  const c = state.capture;
  if (!c) return;
  if (captureSeen === null) { captureSeen = c.count; return; }
  if (c.count > captureSeen) {
    captureSeen = c.count;
    toast('📸 ' + t('Capture enregistrée : {file}', { file: c.last }));
    sfx.save();
  }
}

// Prévient une fois par session quand un portage a une nouvelle version.
const recompAnnounced = new Set();
function announceRecomps() {
  const fresh = state.games.filter((g) => g.recomp && g.recomp.isNew && !recompAnnounced.has(g.id + g.recomp.latest));
  if (!fresh.length) return;
  fresh.forEach((g) => recompAnnounced.add(g.id + g.recomp.latest));
  if (document.getElementById('splash')) { setTimeout(announceRecomps, 1500); fresh.forEach((g) => recompAnnounced.delete(g.id + g.recomp.latest)); return; }
  toast(fresh.length === 1 ? t('Nouvelle version du portage de {name} : {v}', { name: fresh[0].name, v: fresh[0].recomp.latest })
    : t('{n} portages ont une nouvelle version.', { n: fresh.length }));
  sfx.sparkle();
}

// ---------- Mises à jour ----------
let updateDismissed = '';
function renderUpdate() {
  const bar = document.getElementById('update-bar');
  const u = state.update;
  if (!u || updateDismissed === u.version || document.body.classList.contains('updating')) { bar.hidden = true; return; }
  if (!bar.hidden && bar.dataset.version === u.version) return;
  bar.dataset.version = u.version;
  bar.replaceChildren(
    h('div', { class: 'update-text' },
      h('strong', {}, t('Rift Launcher {v} est disponible', { v: u.version })),
      h('span', {}, u.notes ? document.createTextNode(u.notes) : t('Tu as la version {v}.', { v: state.version }))),
    h('div', { class: 'update-actions' },
      h('button', { class: 'btn btn-small btn-ghost', onclick: () => { updateDismissed = u.version; sfx.select(); renderUpdate(); } }, 'Plus tard'),
      h('button', { class: 'btn btn-small btn-primary', onclick: () => startUpdate(u) }, 'Mettre à jour')));
  bar.hidden = false;
  sfx.sparkle();
}

async function startUpdate(u) {
  const overlay = document.getElementById('update-overlay');
  document.getElementById('update-title').textContent = t('Mise à jour vers {v}…', { v: u.version });
  document.getElementById('update-msg').textContent = t('Téléchargement en cours. Rift Launcher va redémarrer tout seul, ne ferme pas cette fenêtre.');
  overlay.hidden = false;
  document.body.classList.add('updating');
  sfx.select();
  const r = await api.updateInstall().catch((e) => ({ error: String(e) }));
  if (r.error) {
    overlay.hidden = true;
    document.body.classList.remove('updating');
    toast(r.error, 'error');
    return;
  }
  document.getElementById('update-msg').textContent = t('Installation… La fenêtre se recharge dès que la nouvelle version est prête.');
  // On attend que le nouveau Rift Launcher réponde, puis on recharge la page.
  const started = Date.now();
  const timer = setInterval(async () => {
    try {
      const res = await fetch('/api/version', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Token': TOKEN }, body: '{}' });
      const v = await res.json();
      if (v.version === u.version) { clearInterval(timer); location.reload(); }
    } catch (e) { /* redémarrage en cours */ }
    if (Date.now() - started > 120000) {
      clearInterval(timer);
      document.getElementById('update-msg').textContent = t('La mise à jour prend du temps. Ferme cette fenêtre et relance Rift Launcher.');
    }
  }, 1500);
}

function render() {
  // Garde le focus (manette) sur le même élément après la reconstruction.
  const active = document.activeElement;
  const navKey = active && !active.closest('dialog') && active.dataset ? active.dataset.nav : '';
  renderUpdate();
  const total = state.games.reduce((s, g) => s + g.playSeconds, 0);
  const n = state.games.length;
  document.getElementById('total').textContent = t(n > 1 ? '{n} jeux · {time} de jeu au total' : '{n} jeu · {time} de jeu au total', { n, time: formatTime(total) });
  renderHero();
  document.getElementById('grid').replaceChildren(...state.games.map(renderPoster));
  document.getElementById('empty').hidden = n > 0;
  document.getElementById('library-title').hidden = n === 0;
  if (dlg.open && editingId) paintMedia();
  if (navKey) {
    const el = [...document.querySelectorAll('[data-nav]')].find((x) => x.dataset.nav === navKey && !x.closest('dialog'));
    if (el && el !== document.activeElement) el.focus({ preventScroll: true });
  }
  announceRecomps();
  announceCapture();
  pad.paintHints();
}

// ---------- Édition ----------
const dlg = document.getElementById('editor');
const form = document.getElementById('form');
const FIELDS = ['name', 'exePath', 'saveDir', 'modsDir', 'version', 'repoUrl', 'args', 'env', 'franchise'];
let franchiseList = [];

function openEditor(game) {
  editingId = game ? game.id : null;
  document.getElementById('editor-title').textContent = game ? t('Paramètres — {name}', { name: game.name }) : t('Ajouter un jeu');
  document.getElementById('delete').hidden = !game;
  document.getElementById('form-error').hidden = true;
  for (const f of FIELDS) form.elements[f].value = game ? game[f] : '';
  form.querySelector('details').open = Boolean(game && (game.args || game.env));
  document.getElementById('media').hidden = !game;
  document.getElementById('media-new').hidden = Boolean(game);
  paintMedia();
  sfx.select();
  dlg.showModal();
  form.elements[game && !game.exePath ? 'exePath' : 'name'].focus();
}

form.addEventListener('click', async (e) => {
  const field = e.target.dataset && e.target.dataset.pick;
  if (!field) return;
  const picked = field === 'exePath' ? await api.pickExe() : await api.pickDir(field === 'modsDir' ? 'mods' : 'dir');
  if (!picked) return;
  form.elements[field].value = picked;
  // Propose un nom à partir du dossier si le champ est vide.
  if (field === 'exePath' && !form.elements.name.value.trim()) {
    const parts = picked.split(/[\\/]/);
    form.elements.name.value = (parts.length > 1 ? parts[parts.length - 2] : parts[0]).replace(/[-_]+/g, ' ');
  }
});

form.addEventListener('submit', async (e) => {
  e.preventDefault();
  const data = { id: editingId };
  for (const f of FIELDS) data[f] = form.elements[f].value.trim();
  const err = document.getElementById('form-error');
  if (!data.name) { sfx.error(); err.textContent = t('Donne un nom au jeu.'); err.hidden = false; return; }
  if (data.repoUrl && !/^https:\/\//i.test(data.repoUrl)) { err.textContent = t('La page du projet doit commencer par https://'); err.hidden = false; return; }
  const r = await api.saveGame(data);
  if (r.error) { err.textContent = tMsg(r.error); err.hidden = false; return; }
  state = r;
  sfx.save();
  dlg.close();
  render();
});

document.getElementById('cancel').addEventListener('click', () => dlg.close());

const SOURCE_TEXT = {
  perso: 'Choisie pour ce jeu.',
  dossier: 'Trouvée dans ton dossier « jaquettes » (nom du jeu ou de sa licence).',
  integree: 'Jaquette intégrée à Rift Launcher.',
  '': 'Aucune jaquette : une affiche est générée avec le nom du jeu.',
};
const SOUND_TEXT = {
  perso: 'Son choisi pour ce jeu.',
  aleatoire: 'Un son au hasard parmi ceux du dossier de sa licence (dossier « sons »).',
  dossier: 'Trouvé dans ton dossier « sons » (nom du jeu ou de sa licence).',
  '': 'Son intégré de la licence (ou son par défaut).',
};

function paintMedia() {
  const g = state.games.find((x) => x.id === editingId);
  if (!g) return;
  document.getElementById('cover-preview').replaceChildren(posterArt(g, 'preview-art'));
  document.getElementById('cover-source').textContent = t(SOURCE_TEXT[g.coverSource || '']);
  document.getElementById('sound-source').textContent = t(SOUND_TEXT[g.soundSource || '']);
  const auto = document.querySelector('#franchise-select option[value=""]');
  if (auto) auto.textContent = g.franchiseLabel && !g.franchise ? t('Automatique ({name})', { name: g.franchiseLabel }) : t('Automatique (d’après le nom)');
}

async function refreshLibrary() {
  state = await api.getLibrary();
  lastJson = JSON.stringify(state);
  render();
}

document.getElementById('media').addEventListener('click', async (e) => {
  const act = e.target.closest('[data-act]') && e.target.closest('[data-act]').dataset.act;
  if (!act || !editingId) return;
  const g = state.games.find((x) => x.id === editingId);
  let r = null;
  if (act === 'cover-pick') r = await api.coverPick(editingId);
  else if (act === 'cover-clear') r = await api.assetClear(editingId, 'cover');
  else if (act === 'cover-search') { openCoverSearch(g); return; }
  else if (act === 'sound-pick') r = await api.soundPick(editingId);
  else if (act === 'sound-clear') r = await api.assetClear(editingId, 'sound');
  else if (act === 'sound-test') { gameSound(g, 'launch'); return; }
  if (r && r.error) { toast(r.error, 'error'); return; }
  if (r && r.warning) toast(r.warning);
  if (r && r.ok) sfx.save();
  await refreshLibrary();
});

// ---------- Recherche de jaquettes en ligne (SteamGridDB) ----------
const coversDlg = document.getElementById('covers');
const coversGrid = document.getElementById('covers-grid');
const coversStatus = document.getElementById('covers-status');
let coversGame = null;

function openCoverSearch(game) {
  coversGame = game;
  document.getElementById('covers-title').textContent = t('Jaquettes — {name}', { name: game.name });
  document.getElementById('covers-term').value = game.name.replace(/^(the legend of )?zelda\s*[:\-–]\s*/i, '');
  coversDlg.showModal();
  runCoverSearch();
}

async function runCoverSearch(sgdbId) {
  const term = document.getElementById('covers-term').value.trim();
  document.getElementById('covers-key').hidden = true;
  coversStatus.textContent = t('Recherche…');
  coversGrid.replaceChildren();
  document.getElementById('covers-games').replaceChildren();
  const r = await api.coverSearch(term, sgdbId || '');
  if (r.error === 'NOKEY') {
    coversStatus.textContent = '';
    document.getElementById('covers-key').hidden = false;
    return;
  }
  if (r.error) { coversStatus.textContent = tMsg(r.error); sfx.error(); return; }
  const games = [].concat(r.games || []);
  document.getElementById('covers-games').replaceChildren(...games.map((x) =>
    h('button', { class: `chip${x.id === r.gameId ? ' on' : ''}`, onclick: () => runCoverSearch(x.id) }, document.createTextNode(x.name))));
  const grids = [].concat(r.grids || []);
  coversStatus.textContent = grids.length ? t(grids.length > 1 ? '{n} jaquettes — clique pour la choisir.' : '{n} jaquette — clique pour la choisir.', { n: grids.length }) : t('Aucune jaquette portrait pour ce jeu. Essaie un autre nom.');
  coversGrid.replaceChildren(...grids.map((x) => h('button', {
    class: 'cover-pick', title: 'Choisir cette jaquette',
    onclick: async (e) => {
      const btn = e.currentTarget;
      btn.classList.add('loading');
      const d = await api.coverDownload(coversGame.id, x.url);
      btn.classList.remove('loading');
      if (d.error) { toast(d.error, 'error'); return; }
      sfx.save();
      coversDlg.close();
      await refreshLibrary();
    },
  }, h('img', { src: x.thumb, alt: '', loading: 'lazy' }))));
}

document.getElementById('covers-go').addEventListener('click', () => runCoverSearch());
document.getElementById('covers-term').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); runCoverSearch(); } });
document.getElementById('covers-close').addEventListener('click', () => coversDlg.close());
document.getElementById('covers-key-save').addEventListener('click', async () => {
  const key = document.getElementById('covers-key-input').value.trim();
  if (!key) return;
  await api.settingsSave({ sgdbKey: key });
  sfx.save();
  runCoverSearch();
});

// ---------- Réglages ----------
const settingsDlg = document.getElementById('settings');
async function openSettings() {
  const s = await api.settings();
  franchiseList = [].concat(s.franchises || []);
  document.getElementById('settings-key-state').textContent = t(s.hasKey ? 'Clé enregistrée ✓' : 'Aucune clé enregistrée.');
  document.getElementById('settings-key').value = '';
  document.getElementById('settings-cover-dir').textContent = s.coverDir;
  document.getElementById('settings-sound-dir').textContent = s.soundDir;
  document.getElementById('settings-keys').replaceChildren(...franchiseList.map((f) => h('code', { title: f.label }, f.key)));
  paintPrefs(s.prefs || {});
  document.getElementById('settings-version').textContent = `Rift Launcher ${s.appVersion || ''} · made by TeK_Sama`;
  document.getElementById('pref-lang').value = i18n.choice;
  document.getElementById('settings-current').textContent = s.appVersion || '';
  document.getElementById('settings-log-dir').textContent = s.logDir || '';
  document.getElementById('discord-id').value = s.discordId || '';
  document.getElementById('discord-id').placeholder = s.discordBuiltinId ? t('ID intégré : {id} (rien à faire)', { id: s.discordBuiltinId }) : t('Application ID (ex. 1234567890123456789)');
  document.getElementById('discord-state').textContent = t(s.discordConnected ? 'Connecté à Discord ✓'
    : (s.discordId || s.discordBuiltin) ? 'Discord affiche « Navigue dans les menus » quand Rift Launcher est ouvert, puis le jeu lancé. Discord doit être ouvert.'
    : "Il manque l'ID de l'application Discord (voir ci-dessous).");
  document.getElementById('discord-adv').open = !s.discordBuiltin && !s.discordId;
  sfx.select();
  showSetTab(currentSetTab, true);
  if (!settingsDlg.open) settingsDlg.showModal();
  const tab = document.querySelector(`.set-tab[data-tab="${currentSetTab}"]`);
  if (tab) tab.focus();
}

// ---------- Langue ----------
{
  const sel = document.getElementById('pref-lang');
  sel.replaceChildren(...I18N_LANGS.map((l) => {
    const o = document.createElement('option');
    o.value = l.code;
    o.textContent = l.code === 'auto' ? t(l.name) : l.name;   // chaque langue écrite dans sa propre langue
    return o;
  }));
  sel.value = i18n.choice;
  sel.addEventListener('change', () => setLanguage(sel.value, true));
}
function setLanguage(code, fromSettings) {
  if (code === i18n.choice && !fromSettings) return;
  i18n.apply(code);
  const auto = document.querySelector('#pref-lang option[value="auto"]');
  if (auto) auto.textContent = t('Automatique (langue de Windows)');
  document.getElementById('pref-lang').value = i18n.choice;
  prefsState.lang = i18n.choice;
  if (fromSettings) { api.prefsSave(prefsState).catch(() => {}); sfx.select(); }
  // Tout ce qui est dessiné par le code est refait dans la nouvelle langue.
  lastJson = '';
  render();
  paintMute();
  if (settingsDlg.open) openSettings();
  if (dlg.open && editingId) paintMedia();
}

// ---------- Catégories des réglages ----------
const SET_TABS = [...document.querySelectorAll('.set-tab')].map((b) => b.dataset.tab);
let currentSetTab = 'general';
function showSetTab(name, quiet) {
  if (!SET_TABS.includes(name)) name = SET_TABS[0];
  currentSetTab = name;
  document.querySelectorAll('.set-tab').forEach((b) => b.classList.toggle('on', b.dataset.tab === name));
  document.querySelectorAll('.set-panel').forEach((s) => { s.hidden = s.dataset.tab !== name; });
  const btn = document.querySelector(`.set-tab[data-tab="${name}"]`);
  document.getElementById('set-title').textContent = btn && btn.lastChild ? btn.lastChild.textContent.trim() : '';
  if (name === 'manette') paintPadLegend();
  if (!quiet) sfx.move();
}
function shiftSetTab(d) {
  const i = SET_TABS.indexOf(currentSetTab);
  showSetTab(SET_TABS[(i + d + SET_TABS.length) % SET_TABS.length]);
  const tab = document.querySelector(`.set-tab[data-tab="${currentSetTab}"]`);
  if (tab) tab.focus({ preventScroll: true });
}
document.getElementById('set-nav').addEventListener('click', (e) => {
  const b = e.target.closest('.set-tab');
  if (b) showSetTab(b.dataset.tab);
});
// À la manette, passer sur un onglet suffit à l'ouvrir.
document.getElementById('set-nav').addEventListener('focusin', (e) => {
  const b = e.target.closest('.set-tab');
  if (b && pad.active && b.dataset.tab !== currentSetTab) showSetTab(b.dataset.tab);
});
function paintPadLegend() {
  const el = document.getElementById('pad-legend');
  const L = LAYOUTS[pad.layout] || LAYOUTS.xbox;
  const item = (k, t) => h('span', {}, h('b', {}, k), ' ', t);
  el.replaceChildren(
    h('span', { style: 'width:100%;color:var(--muted)' }, t('Manette détectée : {name}', { name: L.name })),
    item(L.face.ok[0], 'valider / jouer'), item(L.face.back[0], 'retour'),
    item(L.face.left[0], 'sauvegarder'), item(L.face.top[0], 'mods'),
    item(L.sh, 'changer de jeu / de catégorie'), item(L.menu, 'réglages'), item(L.view, 'Big Picture'),
    item(L.home, 'jeu ↔ launcher'), L.capLabel ? item(L.capLabel, 'capture') : null);
}

// ---------- Paramètres généraux ----------
let prefsState = {};
const PREF_SLIDERS = ['volume', 'uiVolume', 'gameVolume'];
const PREF_TOGGLES = ['hoverSounds', 'debugLog', 'discord', 'recompCheck', 'homeToggle', 'captureButton'];

function paintPrefs(p) {
  prefsState = { ...p };
  for (const k of PREF_SLIDERS) {
    const input = document.getElementById(`pref-${k}`);
    input.value = p[k];
    document.getElementById(`pref-${k}-val`).textContent = `${p[k]} %`;
  }
  for (const k of PREF_TOGGLES) document.getElementById(`pref-${k}`).setAttribute('aria-checked', String(Boolean(p[k])));
}

let prefsTimer = null;
function savePrefsSoon() {
  clearTimeout(prefsTimer);
  prefsTimer = setTimeout(() => api.prefsSave(prefsState).catch(() => {}), 350);
}

for (const k of PREF_SLIDERS) {
  const input = document.getElementById(`pref-${k}`);
  input.addEventListener('input', () => {
    prefsState[k] = Number(input.value);
    document.getElementById(`pref-${k}-val`).textContent = `${input.value} %`;
    sfx.setPrefs(prefsState);
    savePrefsSoon();
  });
  // Petit son d'essai au relâchement, sur le bon volume.
  input.addEventListener('change', () => { if (k === 'gameVolume') sfx.franchise('zelda', 'select'); else sfx.select(); });
}
for (const k of PREF_TOGGLES) {
  const sw = document.getElementById(`pref-${k}`);
  sw.addEventListener('click', () => {
    prefsState[k] = !prefsState[k];
    sw.setAttribute('aria-checked', String(prefsState[k]));
    sfx.setPrefs(prefsState);
    if (prefsState[k]) sfx.modOn(); else sfx.modOff();
    savePrefsSoon();
  });
}

document.getElementById('make-report').addEventListener('click', async (e) => {
  const btn = e.currentTarget;
  btn.disabled = true;
  btn.textContent = t('Création…');
  const r = await api.report();
  btn.disabled = false;
  btn.textContent = t('Créer un rapport de bug');
  if (r.error) { toast(r.error, 'error'); return; }
  sfx.save();
  toast('Rapport créé sur ton Bureau : envoie ce fichier .zip.');
});
document.getElementById('open-logs').addEventListener('click', () => { sfx.select(); api.openLogs(); });

document.getElementById('check-update').addEventListener('click', async (e) => {
  const btn = e.currentTarget;
  btn.disabled = true;
  btn.textContent = t('Vérification…');
  const r = await api.updateCheck().catch(() => ({ error: 'Vérification impossible.' }));
  btn.disabled = false;
  btn.textContent = t('Vérifier les mises à jour');
  if (r.error) { toast(r.error, 'error'); return; }
  if (r.update) {
    state.update = r.update;
    updateDismissed = '';
    settingsDlg.close();
    renderUpdate();
  } else {
    sfx.save();
    toast(t('Tu as la dernière version ({v}).', { v: r.current }));
  }
});
document.getElementById('check-recomps').addEventListener('click', async (e) => {
  const btn = e.currentTarget;
  btn.disabled = true;
  btn.textContent = t('Vérification…');
  const r = await api.recompCheck().catch(() => ({ error: 'Vérification impossible.' }));
  btn.disabled = false;
  btn.textContent = t('Vérifier les portages');
  if (r.error) { toast(r.error, 'error'); return; }
  await refreshLibrary();
  const ups = [].concat(r.updates || []);
  if (!r.checked) toast('Aucun jeu n\'a de page GitHub : ajoute-la dans les paramètres du jeu.');
  else if (!ups.length) { sfx.save(); toast(t(r.checked > 1 ? 'Tous tes portages sont à jour ({n} vérifiés).' : 'Tous tes portages sont à jour ({n} vérifié).', { n: r.checked })); }
});
document.getElementById('discord-save').addEventListener('click', async () => {
  const r = await api.settingsSave({ discordId: document.getElementById('discord-id').value });
  if (r.error) { toast(r.error, 'error'); return; }
  sfx.save();
  toast('ID Discord enregistré.');
});
document.getElementById('open-settings').addEventListener('click', openSettings);
document.getElementById('settings-close').addEventListener('click', () => settingsDlg.close());
document.getElementById('settings-save').addEventListener('click', async () => {
  const r = await api.settingsSave({ sgdbKey: document.getElementById('settings-key').value });
  document.getElementById('settings-key-state').textContent = t(r.hasKey ? 'Clé enregistrée ✓' : 'Aucune clé enregistrée.');
  sfx.save();
});
document.getElementById('open-covers-dir').addEventListener('click', () => { sfx.select(); api.openAssets('covers'); });
document.getElementById('open-sounds-dir').addEventListener('click', () => { sfx.select(); api.openAssets('sounds'); });

// Liste des licences pour le menu « Licence » des paramètres d'un jeu,
// et préférences (volumes, intro) appliquées dès le démarrage.
const settingsReady = api.settings();
settingsReady.then((s) => {
  if (s.prefs) { prefsState = { ...s.prefs }; sfx.setPrefs(s.prefs); }
  if (s.prefs && s.prefs.lang && s.prefs.lang !== i18n.choice) setLanguage(s.prefs.lang, false);
  franchiseList = [].concat(s.franchises || []);
  const sel = document.getElementById('franchise-select');
  sel.append(...franchiseList.map((f) => h('option', { value: f.key }, f.label)));
}).catch(() => {});
document.getElementById('delete').addEventListener('click', async () => {
  const game = state.games.find((g) => g.id === editingId);
  if (!game || !confirm(t('Retirer « {name} » du lanceur ? (les fichiers du jeu ne sont pas supprimés)', { name: game.name }))) return;
  const r = await api.deleteGame(editingId);
  if (r.error) { toast(r.error, 'error'); return; }
  state = r;
  dlg.close();
  render();
});
document.getElementById('add').addEventListener('click', () => openEditor(null));

const muteBtn = document.getElementById('mute');
function paintMute() {
  muteBtn.replaceChildren();
  muteBtn.insertAdjacentHTML('afterbegin', ICONS[sfx.muted ? 'mute' : 'sound']); // icônes constantes
  muteBtn.title = t(sfx.muted ? 'Remettre le son' : 'Couper le son');
  muteBtn.setAttribute('aria-label', muteBtn.title);
}
muteBtn.addEventListener('click', () => { sfx.toggle(); paintMute(); });
paintMute();

// ---------- Page des mods ----------
const modsDlg = document.getElementById('mods');
const modsList = document.getElementById('mods-list');
const modsSearch = document.getElementById('mods-search');
let modsGame = null;
let modsData = [];

async function openMods(game) {
  modsGame = game;
  modsSearch.value = '';
  document.getElementById('mods-title').textContent = t('Mods — {name}', { name: game.name });
  document.getElementById('mods-path').textContent = game.modsDir;
  modsList.replaceChildren();
  sfx.select();
  if (!modsDlg.open) modsDlg.showModal();
  await loadMods();
}

async function loadMods() {
  const r = await api.listMods(modsGame.id);
  const empty = document.getElementById('mods-empty');
  if (r.error) {
    modsData = [];
    empty.textContent = tMsg(r.error);
    empty.hidden = false;
    paintMods();
    return;
  }
  modsData = [].concat(r.mods || []);
  empty.textContent = t("Aucun mod. Décompresse un mod dans le dossier des mods (un dossier par mod), puis clique sur « Actualiser ».");
  empty.hidden = modsData.length > 0;
  paintMods();
}

function paintMods() {
  const q = modsSearch.value.trim().toLowerCase();
  const shown = modsData.filter((m) => !q || m.name.toLowerCase().includes(q));
  const on = modsData.filter((m) => m.enabled).length;
  document.getElementById('mods-count').textContent = modsData.length ? t(on > 1 ? '{on} actifs sur {n}' : '{on} actif sur {n}', { on, n: modsData.length }) : '';
  modsList.replaceChildren(...shown.map((m) => {
    const sw = h('button', {
      class: 'switch', role: 'switch', 'aria-checked': String(m.enabled),
      'aria-label': t(m.enabled ? 'Désactiver {name}' : 'Activer {name}', { name: m.name }),
      onclick: () => toggleMod(m, sw),
    }, h('span', { class: 'knob' }));
    return h('li', { class: `mod${m.enabled ? '' : ' is-off'}` },
      h('div', { class: 'mod-info' },
        h('strong', {}, document.createTextNode(m.name)),
        h('span', {}, m.isDir ? 'Dossier' : 'Fichier')),
      sw);
  }));
}

async function toggleMod(m, sw) {
  sw.disabled = true;
  const want = !m.enabled;
  const r = await api.setMod(modsGame.id, m.name, want);
  if (r.error) { toast(r.error, 'error'); sw.disabled = false; return; }
  if (want) sfx.modOn(); else sfx.modOff();
  m.enabled = want;
  paintMods();
}

modsSearch.addEventListener('input', paintMods);
document.getElementById('mods-close').addEventListener('click', () => modsDlg.close());
document.getElementById('mods-refresh').addEventListener('click', () => { sfx.select(); loadMods(); });
document.getElementById('mods-open').addEventListener('click', () => { sfx.select(); api.openMods(modsGame.id); });


// ---------- Manette (Xbox, PlayStation, Switch Pro…) et mode Big Picture ----------
// Boutons selon la manette. ok/back = index des boutons « valider » et « retour »
// (sur Switch, A est à droite et B en bas : on les inverse comme sur la console).
const LAYOUTS = {
  xbox: { name: 'Xbox', ok: 0, back: 1, viewBtn: 8, capBtn: 17, capLabel: '', face: { ok: ['A', 'xa'], back: ['B', 'xb'], left: ['X', 'xx'], top: ['Y', 'xy'] }, sh: 'LB RB', menu: '☰', view: '⧉', home: 'Ⓧ' },
  ps: { name: 'PlayStation', ok: 0, back: 1, viewBtn: 17, capBtn: 8, capLabel: 'SHARE', face: { ok: ['✕', 'px'], back: ['○', 'po'], left: ['□', 'pq'], top: ['△', 'pt'] }, sh: 'L1 R1', menu: 'OPTIONS', view: 'PAVÉ', home: 'PS' },
  switch: { name: 'Switch', ok: 1, back: 0, viewBtn: 8, capBtn: 17, capLabel: '◉', face: { ok: ['A', 'na'], back: ['B', 'nb'], left: ['Y', 'ny'], top: ['X', 'nx'] }, sh: 'L R', menu: '+', view: '−', home: '⌂' },
};
// Croix ou stick : se déplacer · A : valider / jouer · B : retour · X : sauvegarder · Y : mods
// LB / RB : jeu précédent / suivant · Menu : réglages · Vue (Select) : Big Picture.
// La manette est ignorée quand la fenêtre n'est pas au premier plan (pendant le jeu).
const pad = (() => {
  const FOCUSABLE = 'button:not([disabled]), input:not([type=hidden]):not([disabled]), select:not([disabled]), textarea, summary, a[href], [tabindex="0"]';
  const REPEAT = ['up', 'down', 'left', 'right', 'lb', 'rb'];
  let active = false;
  let running = false;
  let prev = {};
  let next = {};
  let wasAway = true;
  const hintsEl = document.getElementById('pad-hints');

  const isPoster = (el) => el && el.classList && el.classList.contains('poster');
  const visible = (el) => !el.closest('[hidden]') && el.getClientRects().length > 0 && el.getBoundingClientRect().width > 0;

  function scope() {
    if (!document.getElementById('update-overlay').hidden) return null;
    const open = [...document.querySelectorAll('dialog[open]')];
    return open.length ? open[open.length - 1] : document.body;
  }
  function items(root) {
    return [...root.querySelectorAll(FOCUSABLE)].filter((el) => visible(el) && (root !== document.body || !el.closest('dialog')));
  }
  function focus(el, quiet) {
    el.focus({ preventScroll: true });
    el.scrollIntoView({ block: isPoster(el) && document.body.classList.contains('big') ? 'center' : 'nearest', inline: 'nearest', behavior: 'smooth' });
    if (!quiet && !isPoster(el)) sfx.move();
  }
  function start(root, list) {
    if (root === document.body) {
      const sel = document.querySelector('.poster.is-selected') || list[0];
      if (sel) focus(sel, true);
    } else if (list[0]) focus(list[0]);
  }
  // Curseurs et menus déroulants : gauche/droite change la valeur.
  function adjust(el, d) {
    if (el.tagName === 'INPUT' && el.type === 'range') {
      const step = Number(el.step) || 1;
      const v = Math.min(Number(el.max), Math.max(Number(el.min), Number(el.value) + d * step));
      if (v === Number(el.value)) return true;
      el.value = v;
      el.dispatchEvent(new Event('input', { bubbles: true }));
      clearTimeout(el._padChange);
      el._padChange = setTimeout(() => el.dispatchEvent(new Event('change', { bubbles: true })), 250);
      return true;
    }
    if (el.tagName === 'SELECT') {
      const i = Math.min(el.options.length - 1, Math.max(0, el.selectedIndex + d));
      if (i !== el.selectedIndex) { el.selectedIndex = i; el.dispatchEvent(new Event('change', { bubbles: true })); sfx.move(); }
      return true;
    }
    return false;
  }
  function move(dir) {
    const root = scope();
    if (!root) return;
    const list = items(root);
    if (!list.length) return;
    const cur = document.activeElement;
    if (!cur || cur === document.body || !list.includes(cur)) { start(root, list); return; }
    // Depuis la croix « Fermer » d'une fenêtre, bas va au premier vrai réglage.
    if (dir === 'down' && cur.id && cur.id.endsWith('-close')) {
      const first = list.find((el) => el !== cur && !(el.id || '').endsWith('-close'));
      if (first) { focus(first); return; }
    }
    const horiz = dir === 'left' || dir === 'right';
    if (horiz && adjust(cur, dir === 'right' ? 1 : -1)) return;
    // Dans la bibliothèque, gauche/droite suit l'ordre des jeux (et passe à la ligne).
    if (horiz && isPoster(cur)) {
      const posters = list.filter(isPoster);
      const i = posters.indexOf(cur) + (dir === 'right' ? 1 : -1);
      if (posters[i]) focus(posters[i]);
      return;
    }
    const a = cur.getBoundingClientRect();
    const ax = a.left + a.width / 2;
    const ay = a.top + a.height / 2;
    let best = null;
    let bestScore = Infinity;
    for (const el of list) {
      if (el === cur) continue;
      const b = el.getBoundingClientRect();
      const dx = b.left + b.width / 2 - ax;
      const dy = b.top + b.height / 2 - ay;
      const main = { right: dx, left: -dx, down: dy, up: -dy }[dir];
      const side = horiz ? dy : dx;
      // Il faut vraiment être dans la direction (pas un voisin de la même ligne).
      const gap = horiz ? (dir === 'right' ? b.left - a.right : a.left - b.right) : (dir === 'down' ? b.top - a.bottom : a.top - b.bottom);
      if (main <= 4 || gap < -Math.min(a.width, a.height) / 2) continue;
      const score = Math.max(gap, 0) + Math.abs(side) * (horiz ? 3 : 1.4);
      if (score < bestScore) { bestScore = score; best = el; }
    }
    if (best) focus(best);
  }
  function confirm() {
    const splash = document.getElementById('splash');
    if (splash) { splash.click(); return; }
    const root = scope();
    if (!root) return;
    const el = document.activeElement;
    if (!el || el === document.body || !root.contains(el)) { start(root, items(root)); return; }
    if (isPoster(el)) { el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); return; }
    if (el.tagName === 'INPUT' && el.type !== 'checkbox' && el.type !== 'range') return; // champ texte : rien
    el.click();
  }
  function back() {
    const root = scope();
    if (root && root !== document.body) { root.close(); sfx.back(); return; }
    if (document.body.classList.contains('big')) setBig(false);
  }
  function game() { return selectedGame(); }
  function shift(d) {
    if (scope() !== document.body || !state.games.length) return;
    const posters = [...document.querySelectorAll('#grid .poster')];
    const cur = posters.findIndex((p) => p.dataset.id === (game() || {}).id);
    const el = posters[(cur + d + posters.length) % posters.length];
    if (el) focus(el);
  }
  function fire(k) {
    if (k === 'home') { homeToggle(); return; }
    if (k === 'cap') { if (prefsState.captureButton) api.capture().catch(() => {}); return; }
    if (!active) setActive(true);
    const inMain = scope() === document.body;
    switch (k) {
      case 'up': case 'down': case 'left': case 'right': move(k); break;
      case 'a': confirm(); break;
      case 'b': back(); break;
      case 'x': if (inMain && game()) doBackup(game()); break;
      case 'y': if (inMain && game() && game().modsDir) openMods(game()); break;
      case 'lb': if (settingsDlg.open) shiftSetTab(-1); else shift(-1); break;
      case 'rb': if (settingsDlg.open) shiftSetTab(1); else shift(1); break;
      case 'menu': if (settingsDlg.open) settingsDlg.close(); else if (inMain) openSettings(); break;
      case 'view': setBig(!document.body.classList.contains('big')); break;
      default: break;
    }
    paintHints();
  }

  // Bouton Home / Xbox / PS : passe du launcher au jeu (et inversement).
  // Pour les manettes Xbox, c'est Rift Launcher (côté Windows) qui le gère, même pendant le jeu.
  let homeBusy = false;
  function homeToggle() {
    if (layoutKey === 'xbox' || homeBusy || !prefsState.homeToggle) return;
    homeBusy = true;
    api.toggleFocus().catch(() => {}).finally(() => setTimeout(() => { homeBusy = false; }, 300));
  }

  // Type de manette d'après son nom (et l'identifiant du fabricant).
  function padType(gp) {
    const id = (gp.id || '').toLowerCase();
    if (/xbox|xinput|045e/.test(id)) return 'xbox';
    if (/054c|dualshock|dualsense|playstation|ps4|ps5|wireless controller/.test(id)) return 'ps';
    if (/057e|pro controller|joy-con|nintendo|switch/.test(id)) return 'switch';
    return 'xbox';
  }
  let layoutKey = 'xbox';
  document.body.dataset.pad = 'xbox';
  const L = () => LAYOUTS[layoutKey];

  function read() {
    const pads = navigator.getGamepads ? [...navigator.getGamepads()].filter(Boolean) : [];
    const st = { up: false, down: false, left: false, right: false, a: false, b: false, x: false, y: false, lb: false, rb: false, view: false, menu: false, home: false, cap: false };
    let touched = null;
    for (const p of pads) {
      const lay = LAYOUTS[padType(p)];
      const press = (i) => Boolean(p.buttons[i] && (p.buttons[i].pressed || p.buttons[i].value > 0.5));
      const ax = (i) => p.axes[i] || 0;
      const s = {
        up: press(12) || ax(1) < -0.55, down: press(13) || ax(1) > 0.55,
        left: press(14) || ax(0) < -0.55, right: press(15) || ax(0) > 0.55,
        // Switch : A (à droite) valide et B (en bas), retour, comme sur la console.
        a: press(lay.ok), b: press(lay.back), x: press(2), y: press(3),
        lb: press(4), rb: press(5), view: press(lay.viewBtn), menu: press(9), home: press(16), cap: press(lay.capBtn),
      };
      for (const k of Object.keys(s)) if (s[k]) { st[k] = true; touched = p; }
    }
    return { st, count: pads.length, touched };
  }
  let homePrev = false;
  let capPrev = false;
  // La surveillance ne s'arrête jamais : une manette Bluetooth qui se coupe puis revient
  // doit être reprise sans attendre l'événement « gamepadconnected » (Edge ne l'envoie pas toujours).
  const seenPads = new Set();
  let hadPads = false;
  function tick() {
    const { st, count, touched } = read();
    if (!count) {
      if (hadPads) { hadPads = false; api.log('INFO', 'Manette perdue par l\'interface (déconnectée ?), en attente de son retour'); }
      prev = {}; homePrev = false; capPrev = false;
      setTimeout(tick, 400);   // pas de manette : on vérifie 2 à 3 fois par seconde
      return;
    }
    if (!hadPads) {
      hadPads = true;
      for (const gp of navigator.getGamepads()) {
        if (!gp || seenPads.has(gp.id + gp.index)) continue;
        seenPads.add(gp.id + gp.index);
        api.log('INFO', `Manette vue par l'interface : « ${gp.id} » (disposition ${gp.mapping || 'non standard'}, ${gp.buttons.length} boutons) → ${padType(gp)}`);
      }
    }
    requestAnimationFrame(tick);
    if (touched) {
      const t = padType(touched);
      if (t !== layoutKey) { layoutKey = t; document.body.dataset.pad = t; paintHints(); }
    }
    // Le bouton Home marche aussi quand le jeu est devant (si Windows laisse la manette à la fenêtre).
    if (st.home && !homePrev) fire('home');
    homePrev = st.home;
    // Bouton capture (Share / Create / Capture) : capture d'écran, même en plein jeu.
    if (st.cap && !capPrev) fire('cap');
    capPrev = st.cap;
    if (!document.hasFocus() || document.hidden) { wasAway = true; return; }
    // Retour au premier plan (jeu fermé) : on ignore les boutons encore enfoncés.
    if (wasAway) { wasAway = false; prev = st; return; }
    const now = performance.now();
    for (const k of Object.keys(st)) {
      if (k === 'home' || k === 'cap') continue;
      if (st[k] && !prev[k]) { next[k] = now + 380; fire(k); }
      else if (st[k] && REPEAT.includes(k) && now >= next[k]) { next[k] = now + 105; fire(k); }
    }
    prev = st;
  }
  function run() { if (!running) { running = true; wasAway = !document.hasFocus(); tick(); } }

  function setActive(on) {
    active = on;
    document.body.classList.toggle('pad-mode', on);
    paintHints();
  }
  const chip = (cls, label, text) => h('span', { class: 'pad-hint' }, h('b', { class: `pad-key ${cls}` }, label), text);
  const face = (which, text) => { const [label, cls] = L().face[which]; return chip(`face ${cls}`, label, text); };
  function paintHints() {
    if (!active) { hintsEl.hidden = true; return; }
    const root = scope();
    // Les fenêtres (dialog) passent au-dessus de tout : la barre d'aide va dedans pour rester visible.
    const host = root && root !== document.body ? root : document.body;
    if (hintsEl.parentNode !== host) host.append(hintsEl);
    const g = game();
    const lay = L();
    const anyRunning = state.running.length > 0;
    hintsEl.replaceChildren(...(root === document.body ? [
      face('ok', !g ? 'Valider' : state.running.includes(g.id) ? 'Arrêter' : g.exePath ? 'Jouer' : 'Configurer'),
      face('left', 'Sauvegarder'),
      g && g.modsDir ? face('top', 'Mods') : null,
      chip('k-sh', lay.sh, 'Changer de jeu'),
      chip('k-sh', lay.menu, 'Réglages'),
      chip('k-sh', lay.view, document.body.classList.contains('big') ? 'Quitter Big Picture' : 'Big Picture'),
      anyRunning && prefsState.homeToggle ? chip('k-sh', lay.home, 'Retour au jeu') : null,
      lay.capLabel && prefsState.captureButton ? chip('k-sh', lay.capLabel, 'Capture') : null,
    ] : [
      face('ok', 'Valider'),
      face('back', 'Retour'),
      settingsDlg.open ? chip('k-sh', lay.sh, 'Catégorie') : null,
      chip('k-sh', '◀ ▶', 'Régler'),
    ]).filter(Boolean));
    hintsEl.hidden = false;
  }

  addEventListener('gamepadconnected', (e) => {
    const t = padType(e.gamepad);
    api.log('INFO', `Manette connectée dans l'interface : « ${e.gamepad.id} » (disposition ${e.gamepad.mapping || 'non standard'}, ${e.gamepad.buttons.length} boutons) → ${t}`);
    layoutKey = t;
    document.body.dataset.pad = t;
    toast('🎮 ' + i18n.t('Manette {name} connectée : appuie sur {key} pour le mode Big Picture.', { name: LAYOUTS[t].name, key: LAYOUTS[t].view }));
    run();
  });
  addEventListener('mousemove', (e) => { if (active && Math.abs(e.movementX) + Math.abs(e.movementY) > 4) setActive(false); });
  addEventListener('mousedown', () => { if (active) setActive(false); });
  // Les dialogues qui se ferment rendent la main à la bibliothèque.
  document.querySelectorAll('dialog').forEach((d) => d.addEventListener('close', () => setTimeout(paintHints, 0)));
  // Une fenêtre qui s'ouvre : la barre d'aide passe dedans.
  const watch = new MutationObserver(() => paintHints());
  document.querySelectorAll('dialog').forEach((d) => watch.observe(d, { attributes: true, attributeFilter: ['open'] }));
  run();
  return { get active() { return active; }, get layout() { return layoutKey; }, paintHints };
})();

// Plein écran : l'API du navigateur demande un clic. À la manette, Windows appuie sur F11 pour nous.
const isWindowFull = () => Math.abs(window.innerHeight - screen.height) < 4 && Math.abs(window.innerWidth - screen.width) < 4;
function setFull(on) {
  if (on) {
    if (document.fullscreenElement || isWindowFull()) return;
    document.documentElement.requestFullscreen().catch(() => api.fullscreen(true).catch(() => {}));
  } else if (document.fullscreenElement) {
    document.exitFullscreen().catch(() => {});
  } else if (isWindowFull()) {
    api.fullscreen(false).catch(() => {});
  }
}
function setBig(on) {
  document.body.classList.toggle('big', on);
  api.log('INFO', `Big Picture ${on ? 'activé' : 'désactivé'}`);
  setFull(on);
  const btn = document.getElementById('big-btn');
  btn.title = t(on ? 'Quitter le mode Big Picture' : 'Mode Big Picture (plein écran, pour la manette)');
  btn.setAttribute('aria-pressed', String(on));
  sfx.select();
  pad.paintHints();
  // Garde en vue l'élément choisi à la manette (sinon le haut de la page).
  const cur = document.activeElement;
  setTimeout(() => {
    if (cur && cur !== document.body && !cur.closest('dialog')) cur.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    else scrollTo({ top: 0, behavior: 'smooth' });
  }, 80);
}
let wasFull = false;
addEventListener('resize', () => {
  const full = isWindowFull() || Boolean(document.fullscreenElement);
  // Sortie du plein écran F11 (touche F11 ou Échap) : on quitte aussi Big Picture.
  if (wasFull && !full && document.body.classList.contains('big')) setBig(false);
  wasFull = full;
});
document.addEventListener('fullscreenchange', () => {
  // Échap quitte le plein écran : on quitte aussi Big Picture.
  if (!document.fullscreenElement && document.body.classList.contains('big')) setBig(false);
});
{
  const btn = document.getElementById('big-btn');
  btn.insertAdjacentHTML('afterbegin', ICONS.big); // icône constante
  btn.addEventListener('click', () => setBig(!document.body.classList.contains('big')));
}


// La barre du haut devient « verre dépoli » quand on fait défiler.
addEventListener('scroll', () => document.body.classList.toggle('scrolled', scrollY > 8), { passive: true });

// ---------- Démarrage ----------
var lastJson = '';
api.onChange((data) => {
  const json = JSON.stringify(data);
  if (json === lastJson) return;
  lastJson = json;
  state = data;
  render();
});
api.onToast(toast);
api.getLibrary().then((data) => { state = data; render(); });
setInterval(render, 60000); // rafraîchit les « Il y a … »


// ---------- Animation d'ouverture ----------
// Scène 1 : « made by » s'écrit, « TeK_Sama » surgit lettre par lettre.
// Scène 2 : la faille s'ouvre, l'icône s'anime et « Rift Launcher » apparaît.
function startSplash() {
  const el = document.getElementById('splash');
  const sceneSig = document.getElementById('scene-sig');
  const sceneRift = document.getElementById('scene-rift');
  const made = document.getElementById('made');
  const tek = document.getElementById('tek');
  const riftName = document.getElementById('rift-name');
  const sparks = document.getElementById('sparks');
  const rnd = (a, b) => a + Math.random() * (b - a);
  const timers = [];
  const at = (ms, fn) => timers.push(setTimeout(() => { if (!done) fn(); }, ms));
  let done = false;

  const caret = document.createElement('span');
  caret.className = 'caret';
  made.append(caret);
  sfx.unlock();

  // --- Scène 1
  const text = 'made by';
  [...text].forEach((ch, i) => at(250 + i * 95, () => { caret.before(ch); if (ch !== ' ') sfx.type(); }));
  const typed = 250 + text.length * 95;
  // Les lettres sont dessinées une fois dans une image (canvas), puis seule l'image bouge :
  // le texte n'est plus redessiné à chaque image de l'animation (fini les petits artefacts blancs).
  const fontSize = parseFloat(getComputedStyle(tek).fontSize) || 120;
  const font = `900 ${fontSize}px "Segoe UI Black", "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif`;
  const scale = Math.min(4, (window.devicePixelRatio || 1) * 1.5);
  const measure = document.createElement('canvas').getContext('2d');
  measure.font = font;
  const letterCanvas = (ch, sama) => {
    const pad = fontSize * 0.4;
    const adv = measure.measureText(ch).width - fontSize * 0.02;   // même espacement que le texte
    const w = adv + pad * 2, hgt = fontSize + pad * 2;
    const cv = document.createElement('canvas');
    cv.width = Math.ceil(w * scale); cv.height = Math.ceil(hgt * scale);
    cv.style.width = `${w}px`; cv.style.height = `${hgt}px`; cv.style.margin = `${-pad}px`;
    const c = cv.getContext('2d');
    c.scale(scale, scale);
    c.font = font; c.textBaseline = 'middle'; c.textAlign = 'left';
    const x = pad, y = pad + fontSize * 0.52;
    if (sama) {
      c.fillStyle = '#2f8cff';
      c.shadowColor = 'rgba(47, 140, 255, .55)'; c.shadowBlur = 28 * scale;
      c.fillText(ch, x, y);
      c.shadowColor = 'rgba(150, 200, 255, .6)'; c.shadowBlur = 2 * scale;
      c.fillText(ch, x, y);
    } else {
      c.fillStyle = '#ffffff';
      c.fillText(ch, x, y);
    }
    return cv;
  };
  [...'TeK_Sama'].forEach((ch, i) => {
    const s = document.createElement('span');
    s.className = 'l' + (i >= 4 ? ' sama' : '');
    s.append(letterCanvas(ch, i >= 4));
    s.style.setProperty('--delay', `${typed + 150 + i * 75}ms`);
    s.style.setProperty('--dur', `${rnd(1.4, 2.4).toFixed(2)}s`);
    s.style.setProperty('--rot', `${rnd(-24, 24).toFixed(1)}deg`);
    s.style.setProperty('--amp', `${rnd(0.03, 0.07).toFixed(3)}em`);
    tek.append(s);
    at(typed + 150 + i * 75 + 40, () => sfx.pop(i));
  });
  const sigEnd = typed + 150 + 8 * 75;
  at(sigEnd + 250, () => sfx.introSig());

  // --- Scène 2
  const riftStart = sigEnd + 1500;
  at(riftStart - 450, () => { sceneSig.classList.add('leave'); caret.remove(); });
  at(riftStart, () => {
    sceneRift.classList.add('show');
    sfx.rift();
    for (let i = 0; i < 14; i++) {
      const p = document.createElement('span');
      p.className = 'spark';
      p.style.setProperty('--x', `${rnd(-60, 60).toFixed(0)}px`);
      p.style.setProperty('--y', `${rnd(-90, -30).toFixed(0)}px`);
      p.style.setProperty('--d', `${rnd(0.2, 1.6).toFixed(2)}s`);
      p.style.setProperty('--t', `${rnd(1.6, 2.8).toFixed(2)}s`);
      p.style.setProperty('--s', `${rnd(2, 5).toFixed(1)}px`);
      sparks.append(p);
    }
  });
  const word = [['Rift', 'strong'], [' ', 'gap'], ['Launcher', 'light']];
  let k = 0;
  for (const [part, cls] of word) {
    const w = document.createElement('span');
    w.className = `w ${cls}`;
    for (const ch of part) {
      const s = document.createElement('span');
      s.className = 'c';
      s.textContent = ch;
      s.style.setProperty('--delay', `${riftStart + 420 + k * 45}ms`);
      const idx = k;
      if (cls === 'strong') at(riftStart + 420 + idx * 45, () => sfx.name(idx));
      w.append(s);
      k++;
    }
    riftName.append(w);
  }
  at(riftStart + 420 + k * 45 + 200, () => sfx.introEnd());

  // --- Fondu vers la bibliothèque
  const close = () => {
    if (done) return;
    done = true;
    timers.forEach(clearTimeout);
    el.classList.add('out');
    setTimeout(() => el.remove(), 750);
  };
  timers.push(setTimeout(close, riftStart + 2900));
  el.addEventListener('click', close);
  addEventListener('keydown', close, { once: true });
}
// L'intro se joue à chaque ouverture (un clic ou un bouton la passe).
startSplash();
