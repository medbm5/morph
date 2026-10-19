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
    'toast.install': 'INSTALLER LE COMPOSANT',
    'dock.drop': 'DÉPOSER', 'dock.files': 'FICHIERS ICI',
    'about.title': 'À PROPOS',
    's.title': 'RÉGLAGES', 's.saved': 'Enregistré',
    's.intro': 'Metamorphosis fonctionne en arrière-plan (cherchez son icône dans la barre des tâches). Commencez à glisser un fichier, touchez une touche, puis déposez le fichier sur l’anneau.',
    's.keys': 'TOUCHES', 's.keysIntro': 'Pendant que vous glissez un fichier, touchez une de ces touches pour ouvrir un anneau. Touchez-la, ne la maintenez pas. Chaque anneau a sa propre touche.',
    's.keyConvert': 'Convertir (changer le format)', 's.keyTools': 'Outils (compresser, recadrer, retoucher…)', 's.keyActions': 'Actions (copier, déplacer, partager…)',
    's.look': 'APPARENCE', 's.colour': 'Couleur', 's.colourHint': 'La couleur de mise en avant de l’anneau.', 's.customColour': 'Choisir une couleur',
    's.c.amber': 'Ambre', 's.c.cyan': 'Cyan', 's.c.pink': 'Rose', 's.c.lime': 'Vert',
    's.lowContrast': 'Cette couleur est difficile à lire sur l’anneau. Essayez une couleur plus claire.',
    's.motion': 'Animations', 's.motionHint': 'Réduites : pas de rotation ni de scintillement, juste un fondu rapide.',
    's.motionSystem': 'Comme mon ordinateur', 's.motionFull': 'Activées', 's.motionReduced': 'Réduites',
    's.language': 'Langue', 's.langAuto': 'Comme mon ordinateur',
    's.files': 'NOUVEAUX FICHIERS', 's.filesIntro': 'Les résultats sont enregistrés à côté de l’original. L’original n’est jamais modifié.',
    's.converted': 'Nom des fichiers convertis', 's.edited': 'Nom des fichiers modifiés',
    's.nameSame': 'Même nom : photo.png', 's.nameDate': 'Même nom + date : photo 2026-10-19.png',
    's.nameOp': 'Ajouter l’action : photo Cropped.jpg', 's.nameOpDate': 'Ajouter l’action + date : photo Cropped 2026-10-19.jpg', 's.nameCustom': 'Personnalisé…',
    's.customHint': 'Un nom personnalisé peut utiliser {stem} (nom d’origine), {op} (l’action), {ext} (extension) et {date}.',
    's.taken': 'Si un fichier porte déjà ce nom', 's.takenNumber': 'Garder les deux, ajouter un numéro (photo 2.png)', 's.takenTime': 'Garder les deux, ajouter l’heure', 's.takenSkip': 'Ne pas enregistrer le nouveau fichier',
    's.example': 'Exemple', 's.ifTaken': 'si le nom est pris', 's.notSaved': 'pas enregistré',
    's.ring': 'CE QUE MONTRE L’ANNEAU', 's.ringIntro': 'Masquez ce que vous n’utilisez jamais, ou placez vos favoris en premier. Choisissez un type de fichier et un anneau :',
    's.fileType': 'Type de fichier', 's.whichRing': 'Anneau', 's.ringName.convert': 'Anneau Convertir', 's.ringName.tools': 'Anneau Outils', 's.ringName.actions': 'Anneau Actions',
    's.first': 'toujours en premier', 's.hidden': 'masqué', 's.up': 'Monter', 's.down': 'Descendre', 's.pin': 'Toujours en premier', 's.unpin': 'Détacher',
    's.hide': 'Masquer', 's.show': 'Afficher', 's.reset': 'Revenir par défaut', 's.nothing': 'Rien à organiser pour cette combinaison.',
    's.watch': 'DOSSIERS SURVEILLÉS', 's.watchIntro': 'Les fichiers placés dans un dossier surveillé sont traités automatiquement. Par exemple, chaque photo enregistrée dans « Inbox » est compressée.',
    's.noWatch': 'Aucun dossier pour l’instant.', 's.addFolder': 'Ajouter un dossier…', 's.remove': 'Retirer',
    's.doWhat': 'Que faire des nouveaux fichiers', 's.putWhere': 'Mettre les résultats dans ce sous-dossier', 's.watchActive': 'Surveillance', 's.convertTo': 'Convertir en',
    's.general': 'GÉNÉRAL', 's.autostart': 'Démarrer à l’ouverture de session', 's.autostartHint': 'Metamorphosis attend discrètement dans la barre des tâches jusqu’à ce que vous glissiez un fichier.',
    's.history': 'Garder l’historique', 's.historyHint': 'Nécessaire pour Annuler et Relancer.', 's.openHistory': 'Ouvrir l’historique', 's.on': 'Activé',
    's.advanced': 'Avancé', 's.portable': 'Mode portable', 's.portableHint': 'Garder les réglages et l’historique à côté de l’application ({dir}), par exemple pour l’utiliser depuis une clé USB. Redémarrez ensuite.',
    's.restart': 'Redémarrez Metamorphosis pour appliquer.',
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
    Normalize: 'Normaliser', Visualizer: 'Visualiseur', Resize: 'Redimensionner', Rotate: 'Pivoter', Mute: 'Couper le son',
    Protect: 'Protéger', Unlock: 'Déverrouiller', 'Page numbers': 'Numéroter les pages', Watermark: 'Filigrane', Move: 'Déplacer', Share: 'Partager', Float: 'Étagère', 'Copy path': 'Copier le chemin', Terminal: 'Terminal',
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
