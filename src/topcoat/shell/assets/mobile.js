/* Mobile owns only lificMobileNav drawer entries. Page routes are requested
 * through the shell's frozen lific:navigate event contract. */
(() => {
  'use strict';
  const panel = document.querySelector('[data-mobile-navigation]');
  if (!panel) return;
  window.lificMobileNavigation?.destroy();
  const root = panel.querySelector('[data-mobile-root]');
  const projectPane = panel.querySelector('[data-mobile-project]');
  const triggers = [...document.querySelectorAll('[data-mobile-open]')];
  const desktop = matchMedia('(min-width: 48rem)');
  const projectPattern = /^[A-Za-z][A-Za-z0-9_-]*$/;
  const sessionPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
  const publicProject = panel.dataset.mobilePublicProject || null;
  const activeProject = panel.dataset.mobileActiveProject || null;
  const activePage = panel.dataset.mobileActivePage || '';
  const removers = [];
  const listen = (target, name, listener, options) => {
    target.addEventListener(name, listener, options);
    removers.push(() => target.removeEventListener(name, listener, options));
  };
  function sessionId() {
    if (typeof crypto.randomUUID === 'function') return crypto.randomUUID();
    const bytes = crypto.getRandomValues(new Uint8Array(16));
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('');
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  }
  let pending = null;
  let traversing = false;
  function current() {
    const entry = history.state?.lificMobileNav;
    if (!entry || typeof entry !== 'object' || Array.isArray(entry) || entry.version !== 1 ||
        typeof entry.session !== 'string' || !sessionPattern.test(entry.session) ||
        entry.href !== location.href || ![0, 1, 2].includes(entry.depth)) return null;
    if (entry.depth === 2 ? typeof entry.project !== 'string' || !projectPattern.test(entry.project) : entry.project !== null) return null;
    // A restored private pane must never reveal an unrelated public project.
    if (publicProject && entry.project && entry.project.toLowerCase() !== publicProject.toLowerCase()) return null;
    return entry;
  }
  function write(entry, replace = false) {
    history[replace ? 'replaceState' : 'pushState']({...history.state, lificMobileNav: entry}, '', entry.href);
  }
  function openAt(identifier = null) {
    if (desktop.matches || traversing) return;
    if (identifier !== null && (typeof identifier !== 'string' || !projectPattern.test(identifier) ||
        (publicProject && identifier.toLowerCase() !== publicProject.toLowerCase()))) return;
    let entry = current();
    if (!identifier && entry?.depth === 2) { traversing = true; history.back(); return; }
    if (identifier && projectTrigger && projectTrigger.dataset.mobileProjectTrigger.toLowerCase() !== identifier.toLowerCase()) projectTrigger = null;
    if (!entry || entry.depth === 0) {
      entry = {version: 1, session: sessionId(), depth: 0, href: location.href, project: null};
      write(entry, true);
      entry = {...entry, depth: 1};
      write(entry);
    }
    if (identifier) {
      const replace = entry.depth === 2;
      entry = {...entry, depth: 2, project: identifier};
      write(entry, replace);
    }
    present(entry);
  }
  function back() {
    if (traversing) return;
    if (current()?.depth) { traversing = true; history.back(); }
    else present(null);
  }
  function close(action) {
    if (traversing) return;
    const entry = current();
    present(null);
    if (!entry?.depth) { action?.(); return; }
    pending = {session: entry.session, href: entry.href, action};
    traversing = true;
    history.go(-entry.depth);
  }
  function routeChanged() {
    pending = null;
    if (!current()?.depth) present(null);
  }
  listen(window, 'popstate', () => {
    traversing = false;
    const entry = current();
    if (pending) {
      const queued = pending;
      pending = null;
      present(null);
      if (entry?.session === queued.session && entry.depth === 0 && location.href === queued.href) queued.action?.();
      else present(entry?.depth ? entry : null);
    } else present(entry?.depth ? entry : null);
  });
  listen(window, 'hashchange', () => { if (!current()) routeChanged(); });

  let activeMenu = null;
  function dismissMenu() {
    if (!activeMenu) return;
    const {menu, trigger} = activeMenu;
    activeMenu = null;
    menu.remove();
    trigger.setAttribute('aria-expanded', 'false');
    if (trigger.isConnected && !trigger.closest('[inert]')) trigger.focus();
  }
  function showMenu(trigger, items) {
    dismissMenu();
    const menu = document.createElement('div');
    menu.className = 'tc-mobile__menu'; menu.setAttribute('role', 'menu');
    trigger.setAttribute('aria-expanded', 'true');
    for (const item of items) {
      const button = document.createElement('button');
      button.type = 'button'; button.setAttribute('role', 'menuitem');
      button.textContent = item.label; button.disabled = !!item.disabled;
      button.addEventListener('click', () => { dismissMenu(); item.run?.(); });
      menu.append(button);
    }
    document.body.append(menu);
    const rect = trigger.getBoundingClientRect();
    menu.style.left = `${Math.max(8, Math.min(rect.left, innerWidth - 216))}px`;
    menu.style.top = `${Math.max(8, Math.min(rect.bottom, innerHeight - menu.offsetHeight - 8))}px`;
    activeMenu = {menu, trigger};
    const enabled = () => [...menu.querySelectorAll('button:not(:disabled)')];
    (enabled()[0] || menu).focus();
    menu.addEventListener('keydown', event => {
      const rows = enabled(), index = rows.indexOf(document.activeElement);
      if (event.key === 'Escape' || event.key === 'Tab') {
        event.preventDefault(); event.stopPropagation(); dismissMenu();
      } else if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
        event.preventDefault(); event.stopPropagation();
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? rows.length - 1
          : (index + (event.key === 'ArrowDown' ? 1 : -1) + rows.length) % rows.length;
        rows[next]?.focus();
      }
    });
  }
  listen(document, 'pointerdown', event => {
    if (activeMenu && !activeMenu.menu.contains(event.target) && !activeMenu.trigger.contains(event.target)) dismissMenu();
  });
  const themeTrigger = root.querySelector('[data-mobile-theme]');
  function themePreference() {
    try { const value = localStorage.getItem('lific_theme'); return ['system', 'light', 'dark'].includes(value) ? value : 'system'; }
    catch { return 'system'; }
  }
  function updateThemeLabel() {
    if (!themeTrigger) return;
    const label = `Choose theme, current: ${themePreference()}`;
    themeTrigger.setAttribute('aria-label', label); themeTrigger.title = label;
  }
  if (themeTrigger) {
    updateThemeLabel();
    listen(themeTrigger, 'click', () => showMenu(themeTrigger, ['System', 'Light', 'Dark'].map(label => ({label, run() {
      const control = document.createElement('select');
      control.dataset.tcPreference = 'theme'; control.hidden = true;
      const option = document.createElement('option'); option.value = label.toLowerCase(); control.append(option);
      document.body.append(control); control.value = option.value;
      control.dispatchEvent(new Event('change', {bubbles: true}));
      control.remove(); updateThemeLabel();
    }}))));
    listen(window, 'storage', updateThemeLabel);
  }

  const controller = () => window.lificTopcoatShell?.projectController?.();
  let renameForm = null;
  let renameOperation = 0;
  function groupTrigger(id) {
    return [...root.querySelectorAll('[data-mobile-group-action]')].find(button => button.dataset.mobileGroupAction === String(id));
  }
  function reportCatalogError(message) {
    let alert = root.querySelector('[data-mobile-error]');
    if (!alert) {
      alert = document.createElement('p'); alert.dataset.mobileError = '';
      alert.setAttribute('role', 'alert'); root.append(alert);
    }
    alert.textContent = message;
  }
  async function saveOrder(kind, id, delta) {
    const app = controller(); if (!app) return;
    try {
      if (kind === 'project') {
        const group = app.snapshot.groups.find(row => row.project_ids.includes(id));
        const siblings = app.snapshot.projects.filter(row => group ? group.project_ids.includes(row.id) : !app.snapshot.groups.some(item => item.project_ids.includes(row.id)));
        const ids = window.LificTopcoatProjects.moveBy(siblings.map(row => row.id), id, delta);
        await app.reorderProjects(window.LificTopcoatProjects.mergeProjectOrder(app.snapshot, group?.id ?? null, ids));
      }
      else await app.reorderGroups(window.LificTopcoatProjects.moveBy(app.snapshot.groups.map(row => row.id), id, delta));
    } catch (error) {
      reportCatalogError(`${kind === 'project' ? 'Project' : 'Group'} order wasn't saved: ${error.message}`);
    }
  }
  function beginRename(group) {
    renameForm?.remove();
    const form = document.createElement('form'); form.className = 'tc-mobile__rename';
    renameForm = form;
    const input = document.createElement('input'); input.setAttribute('aria-label', 'Group name');
    input.value = group.name; input.required = true; input.maxLength = 80;
    const save = document.createElement('button'); save.type = 'submit'; save.textContent = 'Save';
    const cancel = document.createElement('button'); cancel.type = 'button'; cancel.textContent = 'Cancel';
    const alert = document.createElement('p'); alert.setAttribute('role', 'alert'); alert.hidden = true;
    form.append(input, save, cancel, alert); root.append(form);
    const operation = ++renameOperation;
    const finish = () => { if (renameForm !== form) return; ++renameOperation; renameForm = null; form.remove(); groupTrigger(group.id)?.focus(); };
    cancel.addEventListener('click', finish);
    form.addEventListener('keydown', event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); finish(); } });
    form.addEventListener('submit', async event => {
      event.preventDefault(); if (save.disabled) return;
      const app = controller(); if (!app) return;
      save.disabled = true; alert.hidden = true;
      try { await app.renameGroup(group.id, input.value.trim()); if (renameOperation === operation) finish(); }
      catch (error) { if (renameOperation === operation) { alert.textContent = error.message; alert.hidden = false; input.focus(); } }
      finally { save.disabled = false; }
    });
    input.focus(); input.select();
  }
  function actionButton(label, data, id, items) {
    const button = document.createElement('button'); button.type = 'button';
    button.className = 'tc-mobile__actions'; button.textContent = '…';
    button.dataset[data] = String(id); button.setAttribute('aria-label', `Actions for ${label}`);
    button.setAttribute('aria-haspopup', 'menu'); button.setAttribute('aria-expanded', 'false');
    button.addEventListener('click', () => showMenu(button, items()));
    return button;
  }
  let projectMenuProvider = null;
  function projectRow(item) {
    if (publicProject) return projectButton(item);
    const row = document.createElement('div'); row.className = 'tc-mobile__catalog-row';
    const action = actionButton(item.name, 'mobileProjectAction', item.id, () => {
      const app = controller();
      const snapshot = app?.snapshot ?? catalog;
      const group = snapshot.groups.find(row => row.project_ids.includes(item.id));
      const rows = snapshot.projects.filter(project => group ? group.project_ids.includes(project.id) : !snapshot.groups.some(row => row.project_ids.includes(project.id)));
      const index = rows.findIndex(project => project.id === item.id);
      const custom = projectMenuProvider ? projectMenuProvider(item) : [
        ...snapshot.groups.filter(row => row.id !== group?.id).map(row => ({label:`Move to ${row.name}`,run:()=>app?.assignProject(item.id,row.id).catch(error=>reportCatalogError(error.message))})),
        ...(group ? [{label:'Remove from group',run:()=>app?.assignProject(item.id,null).catch(error=>reportCatalogError(error.message))}] : []),
        {label:'New group…',run:()=>beginCreateGroup(item.id)},
      ];
      return [...custom,
        {label: 'Move up', disabled: index <= 0, run: () => saveOrder('project', item.id, -1)},
        {label: 'Move down', disabled: index < 0 || index >= rows.length - 1, run: () => saveOrder('project', item.id, 1)}];
    });
    row.append(projectButton(item), action); return row;
  }
  const createTrigger = root.querySelector('[data-mobile-create]');
  if (createTrigger) listen(createTrigger, 'click', () => showMenu(createTrigger, [
    {label: 'New project', run: () => navigateTo('/projects/new')},
    {label: 'Import project', run: () => navigateTo('/projects/import')},
    {label: 'New group', run() { beginCreateGroup(); }},
  ]));
  function beginCreateGroup(projectId = null) {
    const app = controller(); if (!app) return;
    const form = document.createElement('form'); form.className = 'tc-mobile__rename';
    const input = document.createElement('input'); input.setAttribute('aria-label', 'New group name'); input.required = true;
    const save = document.createElement('button'); save.type = 'submit'; save.textContent = 'Create group';
    const cancel = document.createElement('button'); cancel.type = 'button'; cancel.textContent = 'Cancel';
    form.append(input, save, cancel); root.append(form); input.focus();
    const close = () => { form.remove(); createTrigger.focus(); };
    cancel.addEventListener('click', close);
    form.addEventListener('keydown', event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); } });
    form.addEventListener('submit', async event => { event.preventDefault(); if(save.disabled)return; save.disabled=true; try { const result = await app.createGroup(input.value.trim());
      if (projectId !== null && result?.result?.id) await app.assignProject(projectId, result.result.id); close(); } catch(error) { reportCatalogError(error.message); input.focus(); } finally {save.disabled=false;} });
  }

  let catalog = {generation: -1, projects: [], groups: []};
  const sameProject = (left, right) => typeof left === 'string' && typeof right === 'string' && left.toLowerCase() === right.toLowerCase();
  function project(identifier) {
    if (publicProject) return sameProject(identifier, publicProject)
      ? {identifier: publicProject, name: publicProject, emoji: null} : null;
    return catalog.projects.find(item => sameProject(item.identifier, identifier)) || null;
  }
  function projectButton(item) {
    const button = document.createElement('button');
    button.type = 'button';
    button.dataset.mobileProjectTrigger = item.identifier;
    button.dataset.currentProject = String(sameProject(activeProject, item.identifier));
    button.setAttribute('aria-label', `Open ${item.name} navigation`);
    const icon = document.createElement('span');
    icon.className = 'tc-mobile__icon';
    icon.setAttribute('aria-hidden', 'true');
    const renderedIcon = window.LificProjectIcons?.projectIcon(document, item.emoji);
    if (renderedIcon && typeof renderedIcon !== 'string') icon.append(renderedIcon);
    else icon.textContent = renderedIcon || item.identifier.slice(0, 2);
    const text = document.createElement('span');
    text.className = 'tc-mobile__project-text';
    const name = document.createElement('span');
    name.textContent = item.name;
    const identifier = document.createElement('span');
    identifier.className = 'tc-mobile__identifier';
    identifier.textContent = item.identifier;
    text.append(name, identifier);
    const chevron = document.createElement('span');
    chevron.setAttribute('aria-hidden', 'true');
    chevron.textContent = '›';
    button.append(icon, text, chevron);
    return button;
  }
  function renderCatalog() {
    const list = panel.querySelector('[data-mobile-project-list]');
    const focused = list.contains(document.activeElement) ? document.activeElement.dataset.mobileProjectTrigger : null;
    const fragment = document.createDocumentFragment();
    const assigned = new Set();
    const projects = publicProject ? [project(publicProject)] : catalog.projects;
    if (!publicProject) {
      for (const group of [...catalog.groups].sort((a, b) => a.sort_order - b.sort_order)) {
        const heading = document.createElement('h3');
        heading.textContent = group.name;
        const groupRow = document.createElement('div'); groupRow.className = 'tc-mobile__catalog-row';
        const action = actionButton(group.name, 'mobileGroupAction', group.id, () => {
          const rows = controller()?.snapshot.groups ?? catalog.groups;
          const index = rows.findIndex(row => row.id === group.id);
          return [{label:'Rename',run:()=>beginRename(group)},
            {label:'Move up',disabled:index<=0,run:()=>saveOrder('group',group.id,-1)},
            {label:'Move down',disabled:index<0||index>=rows.length-1,run:()=>saveOrder('group',group.id,1)},
            {label:'Delete group',run:()=>controller()?.deleteGroup(group.id).catch(error=>reportCatalogError(error.message))}];
        });
        groupRow.append(heading, action); fragment.append(groupRow);
        for (const id of group.project_ids) {
          const item = projects.find(item => item.id === id);
          if (item && !assigned.has(id)) { assigned.add(id); fragment.append(projectRow(item)); }
        }
      }
    }
    for (const item of projects) if (!assigned.has(item.id)) fragment.append(projectRow(item));
    list.replaceChildren(fragment);
    panel.querySelector('[data-mobile-empty]').hidden = projects.length !== 0;
    if (focused && opened && level === 'root') {
      const replacement = [...list.querySelectorAll('button')].find(button => sameProject(button.dataset.mobileProjectTrigger, focused));
      (replacement || focusable(root)[0] || panel).focus();
    }
  }
  const destinations = [['overview', 'Overview'], ['issues', 'Issues'], ['graph', 'Graph'], ['modules', 'Modules'], ['pages', 'Pages'], ['files', 'Files'], ['plans', 'Plans'], ['activity', 'Activity'], ['insights', 'Insights']];
  function renderProject(identifier) {
    const item = project(identifier);
    panel.querySelector('[data-mobile-project-name]').textContent = item?.name || 'Project unavailable';
    panel.querySelector('[data-mobile-project-identifier]').textContent = item?.identifier || identifier || '';
    const nav = panel.querySelector('[data-mobile-destinations]');
    const focusedSlug = nav.contains(document.activeElement) ? document.activeElement.dataset.mobileSlug : null;
    const fragment = document.createDocumentFragment();
    if (item) for (const [slug, label] of destinations) {
      if (publicProject && !['issues', 'pages'].includes(slug)) continue;
      const link = document.createElement('a');
      const logical = `${publicProject ? '/public' : ''}/${item.identifier}/${slug}`;
      link.href = window.LificTopcoatRouting?.href(logical) ?? `${document.body.dataset.lificBasePath ?? ''}${logical}`;
      link.dataset.mobileDestination = '';
      link.dataset.mobileSlug = slug;
      link.textContent = label;
      if (sameProject(activeProject, item.identifier) && activePage === slug) link.setAttribute('aria-current', 'page');
      fragment.append(link);
    }
    nav.replaceChildren(fragment);
    panel.querySelector('[data-mobile-unavailable]').hidden = !!item;
    if (focusedSlug && opened && level === 'project') {
      const replacement = [...nav.querySelectorAll('a')].find(link => link.dataset.mobileSlug === focusedSlug);
      (replacement || focusable(projectPane)[0] || panel).focus();
    }
  }
  function setCatalog(next) {
    if (publicProject || !next || !Number.isSafeInteger(next.generation) || next.generation <= catalog.generation ||
        !Array.isArray(next.projects) || !Array.isArray(next.groups)) return false;
    // Read-only summaries from the frozen catalog; no project mutation API.
    catalog = {generation: next.generation, projects: next.projects.filter(item => item &&
      Number.isSafeInteger(item.id) && typeof item.identifier === 'string' && projectPattern.test(item.identifier) && typeof item.name === 'string').map(item => ({...item})),
    groups: next.groups.filter(group => group && typeof group.name === 'string' &&
      Number.isFinite(group.sort_order) && Array.isArray(group.project_ids)).map(group => ({...group, project_ids: [...group.project_ids]}))};
    renderCatalog();
    if (selectedProject) renderProject(selectedProject);
    return true;
  }

  let opened = false;
  let level = 'root';
  let selectedProject = null;
  let restoreFocus = null;
  let projectTrigger = null;
  let isolation = null;
  let bodyOverflow = '';
  const savedInert = new Map();
  const focusable = pane => [...pane.querySelectorAll('button:not(:disabled), a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex="0"]')]
    .filter(element => !element.closest('[inert]') && element.getClientRects().length);
  function focusPane() { (focusable(level === 'project' ? projectPane : root)[0] || panel).focus(); }
  function isolate() {
    let child = panel;
    while (child.parentElement) {
      for (const sibling of child.parentElement.children) {
        if (!(sibling instanceof HTMLElement) || sibling === child || sibling.matches('script, style, [role="menu"]')) continue;
        if (!savedInert.has(sibling)) savedInert.set(sibling, sibling.inert);
        sibling.inert = true;
      }
      child = child.parentElement;
      if (child === document.body) break;
    }
    panel.setAttribute('aria-modal', String(!contextMenuOpen()));
  }
  function releaseIsolation() {
    isolation?.disconnect();
    isolation = null;
    for (const [element, inert] of savedInert) element.inert = inert;
    savedInert.clear();
    document.body.style.overflow = bodyOverflow;
  }
  function present(entry) {
    if (entry && desktop.matches) { close(); return; }
    const wasOpen = opened;
    const oldLevel = level;
    if (!entry) { dismissMenu(); renameForm?.remove(); renameForm = null; ++renameOperation; }
    if (!entry || oldLevel !== (entry.depth === 2 ? 'project' : 'root')) resetGesture();
    if (entry && !wasOpen) {
      restoreFocus = document.activeElement instanceof HTMLElement &&
        ![document.body, document.documentElement].includes(document.activeElement)
        ? document.activeElement : triggers[0] || null;
      bodyOverflow = document.body.style.overflow;
      document.body.style.overflow = 'hidden';
      isolate();
      isolation = new MutationObserver(isolate);
      isolation.observe(document.body, {childList: true, subtree: true, attributes: true, attributeFilter: ['hidden', 'aria-hidden', 'style']});
      panel.dataset.snap = 'true';
    }
    opened = !!entry;
    level = entry?.depth === 2 ? 'project' : 'root';
    if (entry?.project) { selectedProject = entry.project; renderProject(selectedProject); }
    panel.dataset.level = level;
    panel.dataset.open = String(opened);
    panel.inert = !opened;
    panel.setAttribute('aria-hidden', String(!opened));
    root.inert = !opened || level !== 'root';
    projectPane.inert = !opened || level !== 'project';
    root.setAttribute('aria-hidden', String(root.inert));
    projectPane.setAttribute('aria-hidden', String(projectPane.inert));
    for (const trigger of triggers) trigger.setAttribute('aria-expanded', String(opened));
    if (opened) {
      const row = projectTrigger?.isConnected ? projectTrigger : [...root.querySelectorAll('[data-mobile-project-trigger]')]
        .find(element => sameProject(element.dataset.mobileProjectTrigger, selectedProject));
      if (wasOpen && oldLevel === 'project' && level === 'root' && row) row.focus();
      else focusPane();
      requestAnimationFrame(() => requestAnimationFrame(() => { delete panel.dataset.snap; }));
    } else if (wasOpen) {
      releaseIsolation();
      const target = restoreFocus;
      restoreFocus = null;
      if (target?.isConnected && !target.closest('[inert]') && target.getClientRects().length) target.focus();
    }
  }

  let emittingNavigation = false;
  function navigateTo(href, mode = 'push') {
    if (typeof href !== 'string' || !href.startsWith('/') || href.startsWith('//') || !['push', 'replace'].includes(mode)) return false;
    const destination = new URL(href, location.origin);
    if (destination.origin !== location.origin) return false;
    if (publicProject) {
      if (!destination.pathname.startsWith('/public/')) return false;
      const allowed = destination.pathname.match(/^\/public\/([A-Za-z][A-Za-z0-9_-]*)\/(issues(?:\/[A-Za-z][A-Za-z0-9_-]*-\d+)?|board|pages(?:\/\d+)?)$/i);
      if (!allowed || !sameProject(allowed[1], publicProject)) return false;
    }
    close(() => {
      emittingNavigation = true;
      try { panel.dispatchEvent(new CustomEvent('lific:navigate', {bubbles: true, detail: {href, history: mode}})); }
      finally { emittingNavigation = false; }
    });
    return true;
  }
  listen(window, 'lific:navigate', () => { if (!emittingNavigation) routeChanged(); });
  listen(window, 'lific:route-change', routeChanged);
  listen(window, 'lific:project-catalog', event => setCatalog(event.detail));
  listen(window, 'pageshow', () => { const entry = current(); present(entry?.depth ? entry : null); });
  listen(window, 'pagehide', () => { pending = null; });
  listen(desktop, 'change', () => { if (desktop.matches && current()?.depth) close(); });
  for (const trigger of triggers) listen(trigger, 'click', () => { projectTrigger = null; openAt(trigger.dataset.mobileOpen || null); });
  listen(panel, 'click', event => {
    const target = event.target instanceof Element ? event.target : null;
    if (!target) return;
    if (target.closest('[data-mobile-close]')) { close(); return; }
    if (target.closest('[data-mobile-back]')) { back(); return; }
    const projectRow = target.closest('[data-mobile-project-trigger]');
    if (projectRow) { projectTrigger = projectRow; openAt(projectRow.dataset.mobileProjectTrigger); return; }
    const link = target.closest('a[data-mobile-destination]');
    if (link && event.button === 0 && !event.metaKey && !event.ctrlKey && !event.shiftKey && !event.altKey) {
      event.preventDefault();
      const href = link.getAttribute('href');
      const base = document.body.dataset.lificBasePath;
      const logical = window.LificTopcoatRouting?.path(href) ?? (base && href.startsWith(`${base}/`) ? href.slice(base.length) : href);
      navigateTo(logical);
    }
  });
  const contextMenuOpen = () => [...document.querySelectorAll('[role="menu"]')].some(menu =>
    !menu.inert && menu.getAttribute('aria-hidden') !== 'true' && menu.getClientRects().length);
  listen(window, 'keydown', event => {
    if (!opened || event.defaultPrevented || contextMenuOpen()) return;
    if (event.key === 'Escape') {
      event.preventDefault(); event.stopPropagation();
      if (level === 'project') back(); else close();
    } else if (event.key === 'Tab') {
      const pane = level === 'project' ? projectPane : root;
      const items = focusable(pane);
      const first = items[0], last = items.at(-1);
      if (!first) { event.preventDefault(); panel.focus(); return; }
      if (event.shiftKey ? document.activeElement === first || !pane.contains(document.activeElement)
        : document.activeElement === last || !pane.contains(document.activeElement)) {
        event.preventDefault(); (event.shiftKey ? last : first).focus();
      }
    }
  });
  listen(window, 'focusin', event => {
    if (opened && !contextMenuOpen() && !(level === 'project' ? projectPane : root).contains(event.target)) focusPane();
  });

  let gesture = null;
  function resetGesture() {
    const ended = gesture;
    gesture = null;
    delete panel.dataset.dragging;
    panel.style.removeProperty('--tc-mobile-progress');
    panel.style.removeProperty('--tc-mobile-dismiss');
    if (ended && panel.hasPointerCapture(ended.id)) panel.releasePointerCapture(ended.id);
  }
  listen(panel, 'pointerdown', event => {
    if (!opened || event.pointerType === 'mouse' || !event.isPrimary || gesture || contextMenuOpen() ||
        event.target.closest('input, textarea, select, [contenteditable="true"]')) return;
    gesture = {id: event.pointerId, x: event.clientX, y: event.clientY, time: event.timeStamp, claimed: false, level};
  });
  listen(panel, 'pointermove', event => {
    if (gesture?.id !== event.pointerId) return;
    const dx = event.clientX - gesture.x, dy = event.clientY - gesture.y;
    if (!gesture.claimed) {
      if (Math.abs(dy) > 12 && Math.abs(dy) > Math.abs(dx)) { resetGesture(); return; }
      if (Math.abs(dx) < 12 || Math.abs(dx) <= Math.abs(dy) * 1.2) return;
      if (gesture.level === 'project' ? dx <= 0 : dx >= 0) { resetGesture(); return; }
      gesture.claimed = true;
      panel.dataset.dragging = 'true';
      panel.setPointerCapture(event.pointerId);
    }
    const width = panel.getBoundingClientRect().width || innerWidth;
    if (gesture.level === 'project') panel.style.setProperty('--tc-mobile-progress', String(Math.min(1, Math.max(0, 1 - dx / width))));
    else panel.style.setProperty('--tc-mobile-dismiss', `${Math.min(width, Math.max(0, -dx))}px`);
  });
  listen(panel, 'pointerup', event => {
    if (gesture?.id !== event.pointerId) return;
    const ended = gesture;
    const dx = event.clientX - ended.x;
    const width = panel.getBoundingClientRect().width || innerWidth;
    const distance = ended.level === 'project' ? dx : -dx;
    const commit = ended.claimed && distance > 0 && (distance / width > 0.28 || distance / Math.max(1, event.timeStamp - ended.time) > 0.45);
    resetGesture();
    if (commit) { if (ended.level === 'project') back(); else close(); }
  });
  listen(panel, 'pointercancel', event => { if (gesture?.id === event.pointerId) resetGesture(); });
  // Touch starts with implicit capture on the clicked child. Transferring
  // capture to the panel emits a bubbling loss on that child; it does not
  // mean that the panel's gesture was canceled.
  listen(panel, 'lostpointercapture', event => {
    if (event.target === panel && gesture?.id === event.pointerId) resetGesture();
  });

  window.lificMobileNavigation = {
    openAt, close, navigateTo, routeChanged, setCatalog,
    setProjectMenuProvider(provider) { projectMenuProvider = typeof provider === 'function' ? provider : null; },
    destroy() {
      pending = null;
      traversing = false;
      resetGesture();
      present(null);
      for (const remove of removers) remove();
      delete window.lificMobileNavigation;
    },
  };
  try { setCatalog(JSON.parse(panel.dataset.mobileCatalog || '{}')); } catch { /* An unavailable catalog leaves root navigation usable. */ }
  if (publicProject) renderCatalog();
  const restored = current();
  present(restored?.depth ? restored : null);
})();
