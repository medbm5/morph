/* Metamorphosis — settings that every window follows: accent colour, motion and
   language (EN/FR). Load before the page's own scripts.

   MorphLook.t('key')        → translated string (English when missing)
   MorphLook.label('Crop')   → translated operation label
   [data-i18n="key"]         → element text is translated in place
   MorphLook.ready           → resolves once settings are applied
   MorphLook.onChange(fn)    → called with { accent, reduced, language } on every change */
(function (global) {
  'use strict';
  const FR = {
    'ring.convert': 'CONVERTIR', 'ring.tools': 'OUTILS', 'ring.actions': 'ACTIONS',
    'ring.scanning': 'ANALYSE', 'toast.saved': 'ENREGISTRÉ', 'toast.failed': '✗ ÉCHEC',
    'toast.cancelled': 'ANNULÉ', 'toast.undo': 'ANNULER', 'toast.undone': 'ANNULÉ',
    'toast.trash': 'déplacé dans la corbeille', 'toast.details': 'DÉTAILS', 'error.title': 'ÉCHEC', 'error.close': 'FERMER',
    'watch.title': 'DOSSIERS SURVEILLÉS', 'watch.help': 'Chaque nouveau fichier déposé dans le dossier passe par l’opération choisie ; les résultats vont dans le sous-dossier indiqué.', 'watch.add': 'AJOUTER UN DOSSIER', 'watch.remove': 'RETIRER', 'watch.out': 'Sous-dossier des résultats', 'watch.on': 'Actif', 'watch.op': 'Opération', 'toast.install': 'INSTALLER LE COMPOSANT',
    'dock.drop': 'DÉPOSER', 'dock.files': 'FICHIERS ICI',
    'about.title': 'À PROPOS',
    'settings.title': 'RÉGLAGES', 'settings.keys': 'TOUCHES DE DÉCLENCHEMENT',
    'settings.keysHelp': 'Pendant un glisser, touchez cette touche pour ouvrir l’anneau.',
    'settings.accent': 'COULEUR D’ACCENT', 'settings.custom': 'Personnalisée',
    'settings.lowContrast': 'Contraste faible avec le centre de l’anneau ({r}:1, minimum 4,5:1).',
    'settings.contrastOk': 'Contraste {r}:1',
    'settings.motion': 'ANIMATIONS', 'motion.system': 'Comme le système', 'motion.full': 'Complètes', 'motion.reduced': 'Réduites',
    'settings.layout': 'DISPOSITION DE L’ANNEAU', 'settings.layoutHelp': 'Épinglez, réordonnez ou masquez les segments pour chaque type de fichier.',
    'settings.family': 'Type', 'settings.ring': 'Anneau', 'settings.pin': 'ÉPINGLER', 'settings.unpin': 'DÉSÉPINGLER',
    'settings.hide': 'MASQUER', 'settings.show': 'AFFICHER', 'settings.reset': 'RÉINITIALISER',
    'settings.naming': 'NOMS DE SORTIE', 'settings.convertName': 'Conversions', 'settings.toolName': 'Outils',
    'settings.namingHelp': 'Variables : {stem} nom d’origine, {op} opération, {ext} extension, {date} date du jour.',
    'settings.conflict': 'Si le nom existe déjà',
    'conflict.number': 'Garder les deux (photo 2.png)', 'conflict.timestamp': 'Garder les deux, horodaté', 'conflict.skip': 'Ignorer le nouveau fichier',
    'settings.preview': 'Aperçu',
    'settings.history': 'HISTORIQUE', 'settings.keepHistory': 'Enregistrer les tâches (Annuler, Relancer)',
    'settings.openHistory': 'OUVRIR L’HISTORIQUE',
    'settings.system': 'SYSTÈME', 'settings.autostart': 'Démarrer avec la session',
    'settings.language': 'Langue', 'lang.auto': 'Comme le système',
    'settings.portable': 'Mode portable', 'settings.portableHelp': 'Réglages, recettes et historique à côté de l’application ({dir}). Redémarrage nécessaire.',
    'settings.save': 'ENREGISTRER', 'settings.saved': 'Enregistré', 'settings.restart': 'Redémarrez Metamorphosis pour appliquer.',
    'family.image': 'Images', 'family.pdf': 'PDF', 'family.video': 'Vidéos', 'family.audio': 'Audio', 'family.document': 'Documents',
    'family.slides': 'Présentations', 'family.spreadsheet': 'Tableurs', 'family.subtitle': 'Sous-titres', 'family.archive': 'Archives', 'family.folder': 'Dossiers',
    'history.title': 'HISTORIQUE', 'history.recent': 'TÂCHES RÉCENTES', 'history.empty': 'Rien pour l’instant. Glissez un fichier, touchez Maj, déposez sur un segment.',
    'history.show': 'AFFICHER', 'history.again': 'RELANCER', 'history.undo': 'ANNULER', 'history.saveRecipe': 'ENREGISTRER COMME RECETTE',
    'components.title': 'COMPOSANTS',
    'wayland.title': 'MODE WAYLAND', 'wayland.ok': 'COMPRIS',
  };
  const LABELS_FR = {
    Crop: 'Recadrer', Compress: 'Compresser', Edit: 'Retoucher', Annotate: 'Annoter', 'Add BG': 'Ajouter un fond',
    Collage: 'Collage', Redact: 'Masquer', 'Remove BG': 'Détourer', Metadata: 'Métadonnées', 'Strip metadata': 'Effacer les métadonnées',
    'Copy text': 'Copier le texte', 'Searchable PDF': 'PDF interrogeable', Merge: 'Fusionner', Organize: 'Organiser',
    Extract: 'Extraire', Trim: 'Couper', Speed: 'Vitesse', Join: 'Assembler', Split: 'Scinder', 'Save frames': 'Extraire les images',
    Normalize: 'Normaliser', Visualizer: 'Visualiseur', Channels: 'Canaux', Bleep: 'Bip',
    Copy: 'Copier', Move: 'Déplacer', Share: 'Partager', Float: 'Étagère', 'Copy path': 'Copier le chemin', Terminal: 'Terminal',
  };

  let lang = 'en';
  const listeners = [];
  const fill = (s, vars) => (vars ? s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? vars[k] : m)) : s);

  function t(key, fallback, vars) {
    const s = lang === 'fr' && FR[key] ? FR[key] : (fallback !== undefined ? fallback : key);
    return fill(s, vars);
  }
  const label = (l) => (lang === 'fr' && LABELS_FR[l]) || l;

  function translateDom(root) {
    (root || document).querySelectorAll('[data-i18n]').forEach((el) => {
      if (!el.dataset.en) el.dataset.en = el.textContent;
      el.textContent = t(el.dataset.i18n, el.dataset.en);
    });
  }

  function apply(look) {
    if (!look) return;
    const root = document.documentElement;
    if (/^#[0-9a-f]{6}$/i.test(look.accent || '')) root.style.setProperty('--hud-accent', look.accent);
    root.classList.toggle('hud-reduced', !!look.reduced);
    lang = look.language === 'fr' ? 'fr' : 'en';
    root.lang = lang;
    translateDom();
    listeners.forEach((fn) => { try { fn(look); } catch (e) { console.error(e); } });
  }

  const T = global.__TAURI__;
  const ready = T
    ? T.core.invoke('get_look').then(apply).catch(() => {})
    : Promise.resolve();
  if (T) T.event.listen('settings://changed', ({ payload }) => apply(payload));
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', () => translateDom());

  global.MorphLook = {
    t, label, ready, translateDom,
    lang: () => lang,
    onChange(fn) { listeners.push(fn); },
  };
})(window);
