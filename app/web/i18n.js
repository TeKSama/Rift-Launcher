'use strict';

const I18N_LANGS = [
  { code: 'auto', name: 'Automatique (langue de Windows)' },
  { code: 'fr', name: 'Français', locale: 'fr-FR' },
  { code: 'en', name: 'English', locale: 'en-US' },
  { code: 'es', name: 'Español', locale: 'es-ES' },
  { code: 'zh', name: '中文（简体）', locale: 'zh-CN' },
  { code: 'hi', name: 'हिन्दी', locale: 'hi-IN' },
  { code: 'ar', name: 'العربية', locale: 'ar', rtl: true },
];

const i18n = (() => {
  let choice = 'auto';
  try { choice = localStorage.getItem('rift-lang') || 'auto'; } catch (e) {}
  let lang = 'fr';
  let dict = null;
  let patterns = [];

  const norm = (s) => s.replace(/\s+/g, ' ').trim();
  const fill = (s, vars) => (vars ? s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m)) : s);

  function resolve(c) {
    if (c && c !== 'auto') return c;
    const nav = (navigator.languages && navigator.languages[0]) || navigator.language || 'fr';
    const short = nav.toLowerCase().split('-')[0];
    return I18N_LANGS.some((l) => l.code === short) ? short : 'en';
  }

  function build() {
    dict = lang === 'fr' ? null : ((window.I18N_DICT || {})[lang] || {});
    patterns = [];
    if (!dict) return;
    for (const key of Object.keys(dict)) {
      if (!/\{\w+\}/.test(key)) continue;
      const names = [];
      const rx = key.replace(/[.*+?^$()|[\]\\]/g, '\\$&').replace(/\\?\{(\w+)\\?\}/g, (m, n) => { names.push(n); return '(.+?)'; });
      patterns.push({ re: new RegExp('^' + rx + '$'), names, key });
    }
    for (const key of Object.keys(dict)) if (/ : $/.test(key)) patterns.push({ prefix: key });
  }

  function t(fr, vars) {
    if (typeof fr !== 'string') return fr;
    if (!dict) return fill(fr, vars);
    const k = norm(fr);
    const tr = dict[k];
    return fill(tr != null ? tr : fr, vars);
  }

  function tMsg(msg) {
    if (!dict || !msg) return msg;
    const k = norm(msg);
    if (dict[k] != null) return dict[k];
    for (const p of patterns) {
      if (p.prefix) { if (k.startsWith(norm(p.prefix))) return dict[p.prefix].trimEnd() + ' ' + k.slice(norm(p.prefix).length); continue; }
      const m = k.match(p.re);
      if (m) { const vars = {}; p.names.forEach((n, i) => { vars[n] = m[i + 1]; }); return fill(dict[p.key], vars); }
    }
    return msg;
  }

  const originals = new WeakMap();
  const SKIP = new Set(['SCRIPT', 'STYLE', 'CODE', 'TEXTAREA']);
  const ATTRS = ['title', 'placeholder', 'aria-label'];

  function translatePage(root) {
    root = root || document.body;
    root.querySelectorAll('[data-i18n="html"]').forEach((el) => {
      if (!originals.has(el)) originals.set(el, el.innerHTML);
      const fr = originals.get(el);
      const tr = dict ? dict[norm(fr)] : null;
      el.innerHTML = tr != null ? tr : fr;
    });
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode(n) {
        for (let p = n.parentElement; p; p = p.parentElement) {
          if (SKIP.has(p.tagName) || p.dataset.i18n === 'html' || p.dataset.i18n === 'no') return NodeFilter.FILTER_REJECT;
          if (p === root) break;
        }
        return /[A-Za-zÀ-ÿ]/.test(n.nodeValue) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
      },
    });
    const nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);
    for (const n of nodes) {
      if (!originals.has(n)) originals.set(n, n.nodeValue);
      const fr = originals.get(n);
      const k = norm(fr);
      const tr = dict ? dict[k] : null;
      if (tr == null) { n.nodeValue = fr; continue; }
      const lead = fr.match(/^\s*/)[0], trail = fr.match(/\s*$/)[0];
      n.nodeValue = lead + tr + trail;
    }
    root.querySelectorAll('[title], [placeholder], [aria-label]').forEach((el) => {
      if (el.closest('[data-i18n="no"]')) return;
      let saved = originals.get(el);
      if (!saved) { saved = {}; ATTRS.forEach((a) => { if (el.hasAttribute(a)) saved[a] = el.getAttribute(a); }); originals.set(el, saved); }
      for (const a of Object.keys(saved)) {
        const tr = dict ? dict[norm(saved[a])] : null;
        el.setAttribute(a, tr != null ? tr : saved[a]);
      }
    });
  }

  function apply(c, { save = true } = {}) {
    choice = c || 'auto';
    lang = resolve(choice);
    build();
    const info = I18N_LANGS.find((l) => l.code === lang) || I18N_LANGS[1];
    document.documentElement.lang = lang;
    document.documentElement.dir = info.rtl ? 'rtl' : 'ltr';
    if (save) { try { localStorage.setItem('rift-lang', choice); } catch (e) {} }
    translatePage(document.body);
  }

  return {
    t, tMsg, apply, translatePage,
    get lang() { return lang; },
    get choice() { return choice; },
    get locale() { return (I18N_LANGS.find((l) => l.code === lang) || {}).locale || 'fr-FR'; },
    init() { apply(choice, { save: false }); },
  };
})();
const t = i18n.t;
const tMsg = i18n.tMsg;
